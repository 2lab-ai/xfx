//! The Phase-1 TUI acceptance suite, on a real pseudoterminal.
//!
//! Every case here is one row of `.prd/03-tui-port.md` §"Acceptance -- terminal
//! state, positively proven". The suite is two-sided on purpose: asserting only
//! that `termios` is byte-identical after exit would pass a build that never
//! entered raw mode at all, which is precisely the regression this whole epic
//! risks introducing.

#![cfg(unix)]

mod support;

use std::fs::OpenOptions;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::Instant;

use rustix::process::Signal;
use rustix::termios::{ControlModes, InputModes, LocalModes};
use support::pty::{modes, open_slave, Pty, Session, TerminalState, Wait, IDLE_POLL, WAIT};
use support::sandbox::Sandbox;

/// A bare `xfx` that opts into the TUI.
fn tui(sandbox: &Sandbox) -> Command {
    let mut command = sandbox.command();
    command.env("XFX_TUI", "1");
    command.env_remove("TMUX");
    command
}

/// The bytes that mean the TUI has taken the terminal.
///
/// Response-only: nothing this suite types can contain it, so waiting for it
/// cannot be satisfied by the pty's echo of the test's own keystrokes.
const READY: &str = "\u{1b}[?2004h";

/// The cursor position query the launch asks the terminal (`CSI 6n`).
///
/// Response-only in the same sense as [`READY`]: it is written by xfx and never
/// typed by this suite, so waiting for it means the query is really on the wire
/// and the reply that follows cannot be racing it.
const PROBE: &str = "\u{1b}[6n";

/// The whole interactive mode sequence, in order (`terminal.zig:4-13`).
///
/// Spelled out here rather than imported: `src/tui/term.rs` is not visible to
/// an integration test, and a test that read the constant it is checking would
/// pass for any sequence the module happened to declare.
const MODE_SET: &str = "\u{1b}[>4;2m\u{1b}[>1u\u{1b}[?2004h\u{1b}[?7l";

/// The same under tmux, with no kitty keyboard push (`terminal.zig:29-34`).
const MODE_SET_TMUX: &str = "\u{1b}[>4;2m\u{1b}[?2004h\u{1b}[?7l";

/// The whole normal-exit restore, in order (`app_lifecycle.zig:39-41`).
const RESTORE: &str = "\u{1b}[>4;0m\u{1b}[<u\u{1b}[?2004l\u{1b}[?7h\u{1b}[?25h";

/// The same under tmux, with no kitty pop.
const RESTORE_TMUX: &str = "\u{1b}[>4;0m\u{1b}[?2004l\u{1b}[?7h\u{1b}[?25h";

/// The restore an exit that is **not** the planned one writes, which leads with
/// `1049l` defensively (`app_lifecycle.zig:36-38`).
///
/// Response-only, like `READY`: no test types these bytes, so waiting for them
/// cannot be satisfied by the pty echoing the suite's own keystrokes.
const ABNORMAL_RESTORE: &str =
    "\u{1b}[?1049l\u{1b}[>4;0m\u{1b}[<u\u{1b}[?2004l\u{1b}[?7h\u{1b}[?25h";

/// Every byte the TUI writes and the line-oriented shell never does.
///
/// A route that is not the TUI's must emit none of them: an invocation that
/// merely *looked* like the classic path while having already stamped the
/// terminal would be the regression these negatives exist to catch.
const TUI_BYTES: [&str; 4] = ["\u{1b}[>4;2m", "\u{1b}[>1u", "\u{1b}[?2004h", "\u{1b}[?7l"];

/// Requires that no byte of the TUI's mode set reached this terminal.
fn assert_no_mode_bytes(text: &str) {
    for bytes in TUI_BYTES {
        assert!(
            !text.contains(bytes),
            "the classic path wrote the TUI mode byte {bytes:?}: {text:?}"
        );
    }
}

/// The local modes raw mode clears (`shell_runtime.zig:108-138`).
///
/// Named once, because [`assert_raw`] and [`is_raw`] have to mean the same
/// thing by "raw": a predicate that drifted from the assertion would let a
/// bounded wait return on a terminal the assertion then rejects.
const RAW_LOCAL_OFF: [LocalModes; 4] = [
    LocalModes::ECHO,
    LocalModes::ICANON,
    LocalModes::IEXTEN,
    LocalModes::ISIG,
];

/// The input modes raw mode clears.
const RAW_INPUT_OFF: [InputModes; 5] = [
    InputModes::IXON,
    InputModes::ICRNL,
    InputModes::BRKINT,
    InputModes::INPCK,
    InputModes::ISTRIP,
];

/// Whether every raw-mode bit upstream sets is set.
fn is_raw(state: &TerminalState) -> bool {
    RAW_LOCAL_OFF
        .iter()
        .all(|mode| !state.local.contains(*mode))
        && RAW_INPUT_OFF
            .iter()
            .all(|mode| !state.input.contains(*mode))
        && state.control.contains(ControlModes::CS8)
        && state.min == 1
        && state.time == 0
}

/// Requires the raw-mode bits upstream sets, read from the child's own terminal
/// while it runs (`shell_runtime.zig:108-138`).
fn assert_raw(state: TerminalState) {
    for mode in RAW_LOCAL_OFF {
        assert!(!state.local.contains(mode), "{mode:?} is still set");
    }
    for mode in RAW_INPUT_OFF {
        assert!(!state.input.contains(mode), "{mode:?} is still set");
    }
    assert!(state.control.contains(ControlModes::CS8), "CS8 is not set");
    assert_eq!(state.min, 1, "VMIN");
    assert_eq!(state.time, 0, "VTIME");
}

/// The child's terminal, once the session has finished taking it back.
///
/// Every other raw-mode assertion in this suite waits on a byte that the child
/// writes *after* the `tcsetattr` in question -- the mode set, which `resume`
/// emits only once raw mode is back (`src/tui/mod.rs:269-270`). That works
/// whenever the wire says which mode set is which. After a stop whose timing
/// the test does not control, it does not: a session stopped *after* it
/// announced itself has already put one mode set on the wire, so waiting for
/// "a mode set" is satisfied by the one from before the stop and the assertion
/// runs while the terminal is still cooked.
///
/// Counting the ones already there does not fix it. The child is frozen when
/// the count is taken, but the harness's reader thread is not, so a mode set
/// written before the stop can still be in flight and be miscounted as the one
/// that comes after it -- which fails in the silent direction.
///
/// So the terminal itself is asked, until it answers. Bounded, and loud when it
/// never does: a session that does not take its terminal back fails here with
/// the state it was last seen in.
fn wait_until_raw(pty: &Pty) -> TerminalState {
    let deadline = Instant::now() + WAIT;
    loop {
        let state = modes(pty);
        if is_raw(&state) {
            return state;
        }
        assert!(
            Instant::now() < deadline,
            "the session never took the terminal back; it is still {state:?}"
        );
        std::thread::sleep(IDLE_POLL);
    }
}

#[test]
fn the_tui_positively_enters_raw_mode_and_owns_the_normal_buffer() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = Session::spawn_without_taking_the_terminal(&pty, tui(&sandbox));
    session.wait_for(READY);

    assert_raw(modes(&pty));

    session.type_bytes(&[0x04]);
    assert_eq!(session.wait_exit().code(), Some(0));

    // Read after the child was reaped and the pty drained, because the three
    // claims below are about bytes the TUI must *never* write and a snapshot
    // taken off a running child cannot tell "never" from "not yet".
    let text = session.settled_text();
    assert!(
        text.contains(MODE_SET),
        "the interactive mode sequence is not on the terminal, in order: {text:?}"
    );
    for mouse in ["\u{1b}[?1000h", "\u{1b}[?1002h", "\u{1b}[?1006h"] {
        assert!(
            !text.contains(mouse),
            "mouse reporting {mouse:?} was enabled"
        );
    }
    assert!(
        !text.contains("\u{1b}[?1049h"),
        "the main surface took the alternate screen"
    );
}

#[test]
fn a_normal_exit_gives_the_terminal_back_byte_for_byte() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let before = modes(&pty);
    let mut session = Session::spawn_without_taking_the_terminal(&pty, tui(&sandbox));
    session.wait_for(READY);
    let during = modes(&pty);
    assert_ne!(before, during, "the TUI never took the terminal at all");

    session.type_bytes(&[0x04]);
    assert_eq!(session.wait_exit().code(), Some(0));

    assert_eq!(before, modes(&pty), "the terminal was left changed");
    let text = session.settled_text();
    assert!(
        text.contains(RESTORE),
        "the restore sequence is not on the terminal, in order: {text:?}"
    );
    assert!(
        !text.contains("\u{1b}[?1049l"),
        "the normal exit restored an alternate screen it never entered"
    );
    // A session that drew no band has no band top to clear from, and clearing
    // from the screen's first row would erase what the user had before xfx ran.
    // Task 6 solves a real band and this becomes an assertion about its top.
    assert!(
        !text.contains("\u{1b}[J"),
        "the exit erased a screen xfx never drew on: {text:?}"
    );
    assert!(
        !text.contains("\u{1b}[1;1H"),
        "the exit moved the cursor over the user's screen: {text:?}"
    );
}

#[test]
fn under_tmux_the_kitty_keyboard_push_is_omitted() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut command = tui(&sandbox);
    command.env("TMUX", "/tmp/tmux-1000/default,1234,0");
    let mut session = Session::spawn_without_taking_the_terminal(&pty, command);
    session.wait_for(READY);
    session.type_bytes(&[0x04]);
    assert_eq!(session.wait_exit().code(), Some(0));
    let text = session.settled_text();
    assert!(
        !text.contains("\u{1b}[>1u"),
        "pushing kitty flags under tmux breaks key input (terminal.zig:29-34)"
    );
    assert!(
        text.contains(MODE_SET_TMUX),
        "the tmux mode sequence is not on the terminal, in order: {text:?}"
    );
    assert!(
        text.contains(RESTORE_TMUX),
        "the tmux restore is not on the terminal, in order: {text:?}"
    );
}

#[test]
fn without_the_variable_a_bare_xfx_is_still_the_line_shell() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let before = modes(&pty);
    let mut session = Session::spawn_without_taking_the_terminal(&pty, sandbox.command());
    session.wait_for("> ");
    assert_eq!(before, modes(&pty), "the line shell changed the terminal");
    session.type_line("/quit");
    assert_eq!(session.wait_exit().code(), Some(0));
    assert_no_mode_bytes(&session.settled_text());
}

// ---------------------------------------------------------------------------
// the route, negatively
// ---------------------------------------------------------------------------
//
// `should_run` is a conjunction of four facts, and a regression in any one of
// them sends an invocation somewhere its caller did not ask for. Each case
// below breaks exactly one of the four and requires the classic path, with no
// byte of the mode set on the terminal.

#[test]
fn any_other_value_of_the_variable_is_the_line_shell() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let before = modes(&pty);
    let mut command = sandbox.command();
    command.env("XFX_TUI", "yes");
    let mut session = Session::spawn_without_taking_the_terminal(&pty, command);
    session.wait_for("> ");
    assert_eq!(
        before,
        modes(&pty),
        "a value that is not `1` took the terminal"
    );
    session.type_line("/quit");
    assert_eq!(session.wait_exit().code(), Some(0));
    assert_no_mode_bytes(&session.settled_text());
}

#[test]
fn a_subcommand_is_never_the_tui_however_the_variable_is_set() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let before = modes(&pty);
    let mut command = tui(&sandbox);
    command.arg("status");
    let mut session = Session::spawn_without_taking_the_terminal(&pty, command);
    assert_eq!(session.wait_exit().code(), Some(0));
    let text = session.settled_text();
    assert!(
        text.contains("[status]"),
        "the command did not run: {text:?}"
    );
    assert_no_mode_bytes(&text);
    assert_eq!(before, modes(&pty), "a subcommand changed the terminal");
}

#[test]
fn a_bare_invocation_whose_input_is_not_a_terminal_is_refused_not_taken() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let before = modes(&pty);
    // Output really is a terminal, so this case turns on the input alone: with
    // the stdin requirement gone the TUI would be entered and would then fail
    // on `/dev/null` with an ioctl error instead of the shell's own refusal.
    // stderr is the one pipe, because `should_run` never looks at it.
    let child = tui(&sandbox)
        .stdin(Stdio::null())
        .stdout(Stdio::from(open_slave(&pty.slave_path)))
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn xfx");
    let output = child.wait_with_output().expect("wait for xfx");

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).expect("stderr is utf-8");
    assert!(
        stderr.contains("interactive terminal"),
        "the refusal must name the requirement: {stderr}"
    );
    assert_no_mode_bytes(&stderr);
    assert_eq!(
        before,
        modes(&pty),
        "the refused invocation touched the terminal its output was on"
    );
}

#[test]
fn a_bare_invocation_whose_output_is_not_a_terminal_is_refused_not_taken() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let before = modes(&pty);
    let slave = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&pty.slave_path)
        .expect("open the pty slave");
    let output = tui(&sandbox)
        .stdin(Stdio::from(slave))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("spawn xfx");

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty(), "stdout must stay empty");
    let stderr = String::from_utf8(output.stderr).expect("stderr is utf-8");
    assert!(
        stderr.contains("interactive terminal"),
        "the refusal must name the requirement: {stderr}"
    );
    assert_no_mode_bytes(&stderr);
    // The refusal happens before anything is written, so the terminal the
    // input came from is untouched -- including its line discipline.
    assert_eq!(
        before,
        modes(&pty),
        "the refused invocation took the terminal"
    );
}

// ---------------------------------------------------------------------------
// the signal contract
// ---------------------------------------------------------------------------
//
// One row per exit path of `.prd/03-tui-port.md` §"Signals". Each case asserts
// two things that only the real contract can satisfy together: the terminal is
// byte-identical to what it was before the child ran, and the child's *own*
// wait status says the signal killed it. A handler that restored the terminal
// and then called `exit(0)` would pass the first and fail the second, and a
// handler that re-raised without restoring would pass the second and fail the
// first.

/// Starts a TUI child that is signalled rather than typed at.
///
/// The child is given a process group of its own **inside this process's
/// session**, which is load-bearing rather than tidy: POSIX requires `SIGTSTP`
/// sent to a member of an *orphaned* process group to be discarded, and a child
/// that merely inherited the test runner's group is orphaned whenever that
/// group's leader is also its session leader -- which is what a `cargo test`
/// launched from a non-interactive shell looks like, and what this suite was
/// first written against, where the stop simply never happened. `setsid` is not
/// the fix but the same bug: a fresh session leader's group has no member whose
/// parent is in the session at all, so it is orphaned by construction. Own
/// group, same session, parent outside it is the shape a job-controlling shell
/// puts a real job in, and the only one in which a stop signal means anything.
fn started(sandbox: &Sandbox, pty: &Pty) -> Session {
    let mut command = tui(sandbox);
    // SAFETY: `setpgid` is async-signal-safe and touches nothing outside the
    // child that is about to `exec`.
    unsafe {
        command.pre_exec(|| {
            rustix::process::setpgid(None, None)?;
            Ok(())
        });
    }
    let session = Session::spawn_without_taking_the_terminal(pty, command);
    session.wait_for(READY);
    session
}

#[test]
fn sigterm_restores_the_line_discipline_and_dies_by_the_signal() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let before = modes(&pty);
    let session = started(&sandbox, &pty);
    session.signal(Signal::TERM);

    let state = session.wait_state("the child to die", |state| !matches!(state, Wait::Running));
    // WIFSIGNALED, not a fabricated exit code: the handler reset the
    // disposition to default and re-raised, so the parent sees a signal death.
    assert_eq!(state, Wait::Signalled(Signal::TERM.as_raw()));
    // Both halves of the restore, because each has a mutant the other misses.
    // `tcsetattr` alone leaves the screen in bracketed paste with autowrap off;
    // the escape bytes alone leave the line discipline raw.
    session.wait_for(ABNORMAL_RESTORE);
    assert_eq!(before, modes(&pty), "only tcsetattr can produce this");
}

#[test]
fn sighup_restores_the_line_discipline_and_dies_by_the_signal() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let before = modes(&pty);
    let session = started(&sandbox, &pty);
    session.signal(Signal::HUP);

    assert_eq!(
        session.wait_state("the child to die", |state| !matches!(state, Wait::Running)),
        Wait::Signalled(Signal::HUP.as_raw())
    );
    session.wait_for(ABNORMAL_RESTORE);
    assert_eq!(before, modes(&pty));
}

#[test]
fn an_external_sigint_is_not_swallowed_because_isig_is_off() {
    // `ISIG` only suppresses *terminal-generated* SIGINT. A `kill -INT` still
    // arrives, and there is no `xfx-interrupt` thread in this session to catch
    // it, so the handler must do the same restore-and-die as TERM.
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let before = modes(&pty);
    let session = started(&sandbox, &pty);
    session.signal(Signal::INT);

    assert_eq!(
        session.wait_state("the child to die", |state| !matches!(state, Wait::Running)),
        Wait::Signalled(Signal::INT.as_raw())
    );
    session.wait_for(ABNORMAL_RESTORE);
    assert_eq!(before, modes(&pty));
}

#[test]
fn a_stop_that_lands_during_startup_still_comes_back_to_a_raw_terminal() {
    // The startup window, at the acceptance level. `SIGTSTP` is blocked from
    // before the terminal is taken until the session is inside its wait, so a
    // stop aimed at a starting xfx cannot be *taken* anywhere in between -- it
    // is either handled by the default disposition before the block goes on, or
    // held and delivered inside the first wait. Both are legitimate; what may
    // never happen is coming back from the stop onto a cooked terminal that the
    // session believes is raw.
    //
    // The sync point is the child's own process group, which `started` sets in
    // `pre_exec` immediately before `exec`: once the parent can see it, the
    // child has just begun and has certainly not announced itself yet. It is an
    // honest edge -- the parent observes a fact the kernel already had, and the
    // product emits nothing for the test's benefit. It does **not** pin which of
    // the two paths above was taken, and no edge inside the process could: the
    // window has no observable boundary, which is the reason it needed a mask
    // rather than a check. What it pins is the outcome, which is the claim.
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let before = modes(&pty);

    let mut command = tui(&sandbox);
    // SAFETY: `setpgid` is async-signal-safe and touches only this child.
    unsafe {
        command.pre_exec(|| {
            rustix::process::setpgid(None, None)?;
            Ok(())
        });
    }
    let session = Session::spawn_without_taking_the_terminal(&pty, command);

    // Its own process group is also what makes the stop deliverable at all: a
    // child still in the runner's group is in an orphaned one, and POSIX
    // discards a stop sent there.
    let pid = session.pid();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while rustix::process::getpgid(Some(pid)).ok() != Some(pid) {
        assert!(
            std::time::Instant::now() < deadline,
            "the child never took a process group of its own"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    session.signal(Signal::TSTP);
    assert!(
        matches!(
            session.wait_state("the child to stop", |state| matches!(
                state,
                Wait::Stopped(_)
            )),
            Wait::Stopped(_)
        ),
        "a stop aimed at a starting session was swallowed"
    );

    session.signal(Signal::CONT);
    // Not `wait_for(READY)`: which mode set that would match depends on whether
    // the stop landed before or after the first one, and this test deliberately
    // does not control that. See `wait_until_raw`.
    assert_raw(wait_until_raw(&pty));

    // And it is a live session afterwards rather than a survivor: Ctrl-D still
    // leaves, and the exit still gives the terminal back byte for byte. Typing
    // it only once the terminal is raw is part of the same fix -- a Ctrl-D sent
    // into a still-cooked terminal is eaten by the line discipline, and the
    // session waits for input that was already spent.
    session.type_bytes(&[0x04]);
    // `Exited`, not `!Running`: this child has been resumed, so the kernel still
    // has a `Continued` notification standing for it, and `!Running` would be
    // satisfied by that the instant it was asked -- reading the terminal before
    // the exit had restored anything. The harness models the two apart for
    // exactly this reason.
    assert_eq!(
        session.wait_state("the child to exit", |state| matches!(
            state,
            Wait::Exited(_)
        )),
        Wait::Exited(0)
    );
    assert_eq!(before, modes(&pty), "the terminal was left changed");
}

#[test]
fn sigtstp_really_stops_the_process_with_the_terminal_given_back() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let before = modes(&pty);
    let session = started(&sandbox, &pty);

    session.signal(Signal::TSTP);
    assert_eq!(
        session.wait_state("the child to stop", |state| matches!(
            state,
            Wait::Stopped(_)
        )),
        Wait::Stopped(Signal::STOP.as_raw()),
        "only raise(SIGSTOP) after SIG_DFL produces a genuine stop"
    );
    session.wait_for(ABNORMAL_RESTORE);
    assert_eq!(before, modes(&pty), "a stopped job left the terminal raw");

    session.signal(Signal::CONT);
    session.wait_for_count(READY, 2);
    assert_raw(modes(&pty));

    // The reinstall gate. Without it the second SIGTSTP hits the default
    // disposition and stops the process with the terminal still raw.
    session.signal(Signal::TSTP);
    assert_eq!(
        session.wait_state("the second stop", |state| matches!(state, Wait::Stopped(_))),
        Wait::Stopped(Signal::STOP.as_raw())
    );
    assert_eq!(
        before,
        modes(&pty),
        "the SIGTSTP handler was not reinstalled on resume"
    );
}

// ---------------------------------------------------------------------------
// the launch, on a screen that was not empty
// ---------------------------------------------------------------------------
//
// The band opens at the bottom of the *normal* buffer, which the shell has
// already been writing to. These three cases are the whole of what that costs:
// the session asks the terminal where the cursor is, it treats what shares that
// read with the answer as the user's, and it starts below what was already
// there.

#[test]
fn the_launch_probe_asks_where_the_cursor_is_and_does_not_eat_the_answer() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = Session::spawn_without_taking_the_terminal(&pty, tui(&sandbox));
    // The pty answers nothing on its own, so the harness plays the terminal:
    // the query must be on the wire before the reply is typed back.
    session.wait_for(PROBE);
    session.type_bytes(b"\x1b[7;1R");
    session.wait_for(READY);

    // The reply was consumed, not routed: the session is still alive and still
    // leaves on Ctrl-D, which a decoder desynchronized by six stray bytes
    // would not do.
    session.type_bytes(&[0x04]);
    assert_eq!(session.wait_exit().code(), Some(0));
}

#[test]
fn a_keystroke_that_arrives_with_the_answer_is_deferred_rather_than_swallowed() {
    // The reply and a Ctrl-D in **one** write, which is what a user who pressed
    // a key while the query was in flight really produces: both land in the
    // read the probe is doing, and the terminal will never deliver either
    // again. A session that dropped what it did not parse would sit here until
    // the harness gave up.
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = Session::spawn_without_taking_the_terminal(&pty, tui(&sandbox));
    session.wait_for(PROBE);
    session.type_bytes(b"\x1b[7;1R\x04");
    assert_eq!(session.wait_exit().code(), Some(0));
}

#[test]
fn shell_output_from_before_the_launch_is_still_readable_afterwards() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    // Fixed before the child is spawned, so the row the push moves to is the
    // terminal's own answer rather than the 24x80 a pty with no size falls
    // back to -- and so the assertion below is about geometry.
    pty.resize(24, 80);
    let mut command = Command::new("/bin/sh");
    command.arg("-c").arg(format!(
        "printf 'PRIOR-OUTPUT-MARKER\\n'; exec {}",
        env!("CARGO_BIN_EXE_xfx")
    ));
    // The same controlled environment `Sandbox::command` builds, plus the
    // opt-in.
    sandbox.apply_env(&mut command);
    command.env("XFX_TUI", "1");

    let mut session = Session::spawn_without_taking_the_terminal(&pty, command);
    session.wait_for(PROBE);
    session.type_bytes(b"\x1b[2;1R");
    session.wait_for(READY);
    session.type_bytes(&[0x04]);
    assert_eq!(session.wait_exit().code(), Some(0));

    let text = session.settled_text();
    assert!(
        text.contains("PRIOR-OUTPUT-MARKER"),
        "the shell's output was erased: {text:?}"
    );
    let marker = text.find("PRIOR-OUTPUT-MARKER").expect("the marker");
    let band = text.find(READY).expect("the band");
    assert!(
        marker < band,
        "xfx painted over output that was there first"
    );
    // Row 2 of the 24 this terminal really has: one line of shell output above
    // the cursor, so the push is a move to the bottom row and exactly one
    // newline. The move is the load-bearing half -- a linefeed from row 2
    // scrolls nothing at all -- so the bytes are asserted whole and in order.
    // Counted after the mode set, because the shell's own newline is on the
    // terminal before xfx starts.
    let after = &text[band..];
    assert!(
        after.contains("\u{1b}[24;1H"),
        "the push never moved the cursor to the bottom margin, so nothing scrolled: {text:?}"
    );
    let to_bottom = after.find("\u{1b}[24;1H").expect("the move to the bottom");
    let first_newline = after.find('\n').expect("the newline that scrolls");
    assert!(
        to_bottom < first_newline,
        "the newline was written before the cursor reached the bottom margin: {text:?}"
    );
    assert_eq!(
        after.matches('\n').count(),
        1,
        "the push was not the row the terminal reported: {text:?}"
    );
}

// ---------------------------------------------------------------------------
// the failures a shipped build cannot produce
// ---------------------------------------------------------------------------
//
// Two rows of the restoration matrix need the binary to fail on purpose: a
// panic while the terminal is raw, and an initialization that dies on one side
// or the other of raw mode. Nothing a user can type produces either, so they
// come from the `fault-injection` cargo feature, which is off by default and
// therefore absent from every shipped build -- and from this suite except in
// the one CI run that turns it on.

#[cfg(feature = "fault-injection")]
mod faults {
    use super::*;

    /// A TUI invocation that has been asked to fail at `fault`.
    fn faulty(sandbox: &Sandbox, fault: &str) -> Command {
        let mut command = tui(sandbox);
        command.env("XFX_TUI_FAULT", fault);
        command
    }

    #[test]
    fn a_panic_on_the_ui_thread_restores_before_the_message_is_printed() {
        let sandbox = Sandbox::new();
        let pty = Pty::open();
        let before = modes(&pty);
        let mut session =
            Session::spawn_without_taking_the_terminal(&pty, faulty(&sandbox, "ui-frame"));
        let status = session.wait_exit();
        assert!(!status.success(), "a panic exited zero");

        let text = session.settled_text();
        let restore = text.find("\u{1b}[?2004l").expect("the restore ran");
        let message = text
            .find("panicked")
            .expect("the panic message reached the terminal");
        assert!(
            restore < message,
            "the message was painted into a torn band instead of onto a cooked terminal: {text:?}"
        );
        assert_eq!(before, modes(&pty));
    }

    #[test]
    fn a_panic_off_the_ui_thread_leaves_the_terminal_to_its_owner() {
        // The other half of the hook's contract, and the half that a
        // single-threaded phase would otherwise leave untested: the restore is
        // the *owner's* to perform. A hook that ran for any thread would cook
        // this terminal while the thread actually holding it still believed it
        // was raw -- the same double-writer bug the single-writer rule exists
        // to prevent, arriving through the one path that is not a signal.
        let sandbox = Sandbox::new();
        let pty = Pty::open();
        let before = modes(&pty);
        let mut session =
            Session::spawn_without_taking_the_terminal(&pty, faulty(&sandbox, "non-owner-panic"));

        // The session joins the worker before it goes on, so the report on the
        // terminal means the panic is over rather than in flight.
        session.wait_until("the worker's panic to be reported", |text| {
            text.contains("panicked")
        });
        // The claim, read from the terminal itself rather than from the stream.
        assert_raw(modes(&pty));

        // And what follows is a live session rather than a survivor: it still
        // announces, still leaves on Ctrl-D, and still gives the terminal back.
        session.wait_for(READY);
        session.type_bytes(&[0x04]);
        assert_eq!(session.wait_exit().code(), Some(0));

        let text = session.settled_text();
        // The two restores are distinguishable, which is what makes this
        // assertion sharp: only the abnormal one leads with `1049l`, and the
        // owner's ordinary exit never writes it. Its presence anywhere in the
        // whole settled stream means something restored that owned nothing.
        assert!(
            !text.contains("\u{1b}[?1049l"),
            "a thread that owned nothing ran the abnormal restore: {text:?}"
        );
        assert!(
            text.contains(RESTORE),
            "the owner's own exit did not restore: {text:?}"
        );
        assert_eq!(before, modes(&pty), "the terminal was left changed");
    }

    #[test]
    fn a_failure_after_raw_mode_still_gives_the_terminal_back() {
        let sandbox = Sandbox::new();
        let pty = Pty::open();
        let before = modes(&pty);
        let mut session =
            Session::spawn_without_taking_the_terminal(&pty, faulty(&sandbox, "after-raw"));
        assert_eq!(session.wait_exit().code(), Some(1));
        assert!(
            session.settled_text().contains("xfx: "),
            "a half-initialized TUI said nothing"
        );
        assert_eq!(
            before,
            modes(&pty),
            "a half-initialized TUI left a raw terminal"
        );
    }

    #[test]
    fn a_failure_before_raw_mode_writes_no_restore_bytes_at_all() {
        let sandbox = Sandbox::new();
        let pty = Pty::open();
        let before = modes(&pty);
        let mut session =
            Session::spawn_without_taking_the_terminal(&pty, faulty(&sandbox, "before-raw"));
        assert_eq!(session.wait_exit().code(), Some(1));

        // Settled, not snapshotted: this case is entirely about bytes that must
        // never appear, and "not there yet" would satisfy every one of the
        // assertions below.
        let text = session.settled_text();
        assert!(
            text.contains("xfx: "),
            "the refusal never reached the terminal, so the absences below prove nothing: {text:?}"
        );
        for sequence in ["\u{1b}[?2004l", "\u{1b}[?7h", "\u{1b}[<u"] {
            assert!(
                !text.contains(sequence),
                "restored {sequence:?} that was never set: {text:?}"
            );
        }
        assert_eq!(before, modes(&pty));
    }
}
