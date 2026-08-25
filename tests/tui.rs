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
use serde_json::Value;
use support::fake_gateway::FakeGateway;
use support::pty::{modes, open_slave, Pty, Session, TerminalState, Wait, IDLE_POLL, WAIT};
use support::sandbox::Sandbox;

/// A bare `xfx` that opts into the TUI.
fn tui(sandbox: &Sandbox) -> Command {
    let mut command = sandbox.command();
    command.env("XFX_TUI", "1");
    command.env_remove("TMUX");
    command
}

/// A TUI wired to a scripted local Gateway.
fn tui_with(sandbox: &Sandbox, gateway: &FakeGateway) -> Command {
    let mut command = sandbox.command_with(gateway);
    command.env("XFX_TUI", "1");
    command.env_remove("TMUX");
    command
}

/// The text of every user message in a captured request body.
///
/// The same reduction `tests/interactive.rs` performs on the line-oriented
/// path, because the claim is the same one: what the composer held is what the
/// provider was asked.
fn user_messages(body: &Value) -> Vec<String> {
    body["prompt"]
        .as_array()
        .expect("the request carries a prompt")
        .iter()
        .filter(|message| message["role"] == "user")
        .filter_map(|message| message["content"].as_array())
        .flat_map(|parts| parts.iter())
        .filter_map(|part| part["text"].as_str())
        .map(str::to_string)
        .collect()
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

/// The first bytes of a band frame: synchronized output on, cursor hidden.
///
/// Response-only, like [`READY`].
const FRAME_BEGIN: &str = "\u{1b}[?2026h\u{1b}[?25l";

/// The last bytes of one. Waiting on **this** rather than on any byte inside
/// the frame is what makes "a band is on the screen" a fact rather than a race:
/// a needle that matched a row's `CUP` would be satisfied by half a frame.
const FRAME_END: &str = "\u{1b}[?2026l\u{1b}[?25h";

/// The band's top row on a 24-row screen: the divider, and therefore the row
/// the exit clears from.
///
/// Spelled out rather than imported, for the same reason [`MODE_SET`] is:
/// `src/tui/layout.rs` is not visible to an integration test, and a test that
/// read the number it is checking would pass for any layout the module happened
/// to solve.
const BAND_TOP: &str = "\u{1b}[22;1H";

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
    // The band's rows are the screen's, so the size is fixed before the child
    // is spawned and the row the exit clears from is arithmetic rather than
    // whatever window the developer happened to have open.
    pty.resize(24, 80);
    let before = modes(&pty);
    let mut session = Session::spawn_without_taking_the_terminal(&pty, tui(&sandbox));
    session.wait_for(READY);
    let during = modes(&pty);
    assert_ne!(before, during, "the TUI never took the terminal at all");
    // Ctrl-D is typed only once a whole frame is on the wire. Typed before
    // that it lands in the launch probe's own read, the session leaves without
    // ever drawing a band, and this case would be asserting about the exit of a
    // session that had nothing to clear -- which is the *next* test.
    session.wait_for(FRAME_END);

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
    // A band was drawn, so the exit clears from **its** top row downward:
    // upstream's own order (`app_lifecycle.zig:578-593`), and the reason the
    // band leaves no wreckage on a screen the shell goes on using.
    assert!(
        text.contains(&format!("{RESTORE}{BAND_TOP}\u{1b}[J")),
        "the exit did not clear from the band's top row, after the restore: {text:?}"
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
fn in_its_own_process_group(sandbox: &Sandbox, pty: &Pty) -> Session {
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

    // Waited for rather than assumed: a stop aimed before `setpgid` has landed
    // goes to the runner's group. It is also an honest sync point -- the parent
    // observes a fact the kernel already had, and the product emits nothing for
    // the test's benefit.
    let pid = session.pid();
    let deadline = Instant::now() + WAIT;
    while rustix::process::getpgid(Some(pid)).ok() != Some(pid) {
        assert!(
            Instant::now() < deadline,
            "the child never took a process group of its own"
        );
        std::thread::sleep(IDLE_POLL);
    }
    session
}

/// The same, once it has announced itself and painted its band.
fn started(sandbox: &Sandbox, pty: &Pty) -> Session {
    let session = in_its_own_process_group(sandbox, pty);
    session.wait_for(READY);
    // And its band, so that every case below starts from a session that has
    // painted exactly one frame. Counting frames is only sound when the test
    // controls how many preceded it, and this is where that control comes from.
    session.wait_for(FRAME_END);
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

    // Its own process group is also what makes the stop deliverable at all: a
    // child still in the runner's group is in an orphaned one, and POSIX
    // discards a stop sent there.
    let session = in_its_own_process_group(&sandbox, &pty);

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
    // The band comes back too. While the process was stopped the terminal was
    // the shell's, and whatever it wrote is on the rows the band had; a session
    // that only re-entered raw mode would resume onto a screen with no band on
    // it and no reason to draw one. The count is exact because `started` waited
    // for the first frame and nothing between then and here asks for another.
    session.wait_for_count(FRAME_END, 2);

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

#[test]
fn a_stop_before_the_first_frame_paints_only_after_the_terminal_is_taken_back() {
    // The ordering the loop's two reconciles exist for. Every other resume case
    // in this suite starts from a session that has already painted, so none of
    // them can see a frame that is *still owed* when the terminal is handed
    // back -- and that is precisely the frame a loop which reconciled once, at
    // the top of a turn, would paint onto a cooked terminal before answering
    // the resume.
    //
    // It is deterministic rather than a race, by construction. The stop is
    // aimed while the launch probe is still waiting for a cursor report this
    // test never sends, and `SIGTSTP` is blocked for the whole of that wait --
    // so it is *held* until the loop's first tick wait, which is strictly
    // before the loop's first commit. The session is therefore stopped owing
    // the frame it has not yet painted, and the assertion below confirms that
    // rather than trusting it.
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    pty.resize(24, 80);
    let before = modes(&pty);
    let session = in_its_own_process_group(&sandbox, &pty);

    session.wait_for(PROBE);
    session.signal(Signal::TSTP);
    assert_eq!(
        session.wait_state("the child to stop", |state| matches!(
            state,
            Wait::Stopped(_)
        )),
        Wait::Stopped(Signal::STOP.as_raw())
    );
    assert_eq!(before, modes(&pty), "a stopped job left the terminal raw");
    assert!(
        !session.text().contains(FRAME_BEGIN),
        "the session painted before it was stopped, so this case is not the \
         one it claims to be"
    );

    session.signal(Signal::CONT);
    session.wait_for(FRAME_END);

    let text = session.text();
    let resumed = text
        .match_indices(READY)
        .nth(1)
        .expect("the resume's mode set")
        .0;
    let painted = text.find(FRAME_BEGIN).expect("the first frame");
    assert!(
        resumed < painted,
        "the band was painted before the resume took the terminal back, so the \
         frame landed on a cooked terminal the user's shell owned: {text:?}"
    );
    assert_raw(wait_until_raw(&pty));

    session.type_bytes(&[0x04]);
    assert_eq!(
        session.wait_state("the child to exit", |state| matches!(
            state,
            Wait::Exited(_)
        )),
        Wait::Exited(0)
    );
    assert_eq!(before, modes(&pty), "the terminal was left changed");
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
    pty.resize(24, 80);
    let mut session = Session::spawn_without_taking_the_terminal(&pty, tui(&sandbox));
    session.wait_for(PROBE);
    session.type_bytes(b"\x1b[7;1R\x04");
    assert_eq!(session.wait_exit().code(), Some(0));

    // And it left before it drew, which is the half of the exit contract the
    // test above cannot reach: with no band there is no top row to clear from,
    // and clearing from the screen's first row would erase what the shell put
    // there before xfx ran. Read after the child was reaped, because this is a
    // claim about bytes that were never written.
    let text = session.settled_text();
    assert!(
        !text.contains(FRAME_BEGIN),
        "a session that left in the probe still painted a band: {text:?}"
    );
    assert!(
        !text.contains("\u{1b}[J"),
        "the exit erased a screen xfx never drew on: {text:?}"
    );
}

#[test]
fn a_keystroke_that_arrived_with_the_answer_is_typed_rather_than_scanned() {
    // The other half of the deferred-bytes contract, and the half a session
    // that only looked for a Ctrl-D in them would fail: what the probe read is
    // fed to the session's decoder, in arrival order, ahead of everything the
    // loop reads afterwards -- so a character typed while the query was in
    // flight is in the composer, in the order it was typed.
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    pty.resize(24, 80);
    let mut session = Session::spawn_without_taking_the_terminal(&pty, tui(&sandbox));
    session.wait_for(PROBE);
    session.type_bytes(b"\x1b[7;1Rhi");
    session.wait_for("> hi\u{1b}[24;1H");

    session.type_bytes(&[0x15, 0x04]);
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
    // The band, not the mode set: `READY` is written before the probe and would
    // be satisfied by the announce this test is already past.
    session.wait_for(FRAME_END);
    session.type_bytes(&[0x04]);
    assert_eq!(session.wait_exit().code(), Some(0));

    let text = session.settled_text();
    assert!(
        text.contains("PRIOR-OUTPUT-MARKER"),
        "the shell's output was erased: {text:?}"
    );
    let marker = text.find("PRIOR-OUTPUT-MARKER").expect("the marker");
    let announced = text.find(READY).expect("the session's announce");
    assert!(
        marker < announced,
        "xfx painted over output that was there first"
    );
    // Row 2 of the 24 this terminal really has: one line of shell output above
    // the cursor, so the push is a move to the bottom row and exactly one
    // newline. The move is the load-bearing half -- a linefeed from row 2
    // scrolls nothing at all -- so the bytes are asserted whole and in order.
    // Counted after the mode set, because the shell's own newline is on the
    // terminal before xfx starts.
    let after = &text[announced..];
    // Bounded at the first frame: the band writes to the bottom row too, and
    // the exit writes a newline of its own, so a slice that ran to the end of
    // the session would be counting the push and the paint together.
    let launch = &after[..after.find(FRAME_BEGIN).expect("the band's first frame")];
    assert!(
        launch.contains("\u{1b}[24;1H"),
        "the push never moved the cursor to the bottom margin, so nothing scrolled: {text:?}"
    );
    let to_bottom = launch.find("\u{1b}[24;1H").expect("the move to the bottom");
    let first_newline = launch.find('\n').expect("the newline that scrolls");
    assert!(
        to_bottom < first_newline,
        "the newline was written before the cursor reached the bottom margin: {text:?}"
    );
    assert_eq!(
        launch.matches('\n').count(),
        1,
        "the push was not the row the terminal reported: {text:?}"
    );
}

// ---------------------------------------------------------------------------
// the band
// ---------------------------------------------------------------------------
//
// The session owns three rows at the bottom of the normal buffer and repaints
// all three inside one synchronized frame. These two cases are the whole of
// what that promises: a band appears where the geometry says it does, and a
// screen that cannot hold one is refused by name rather than painted over.

#[test]
fn the_band_is_painted_at_the_bottom_inside_one_synchronized_frame() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    pty.resize(24, 80);
    let mut session = Session::spawn_without_taking_the_terminal(&pty, tui(&sandbox));
    session.wait_for(READY);
    session.wait_for(FRAME_END);

    session.type_bytes(&[0x04]);
    assert_eq!(session.wait_exit().code(), Some(0));

    // Counted on the settled terminal rather than on a live snapshot: "no frame
    // was left open" is a claim about the whole session, and a snapshot taken
    // mid-frame would report an imbalance the session does not have.
    let text = session.settled_text();
    let open = text.matches(FRAME_BEGIN).count();
    let close = text.matches(FRAME_END).count();
    assert!(open > 0, "no frame was synchronized: {text:?}");
    assert_eq!(open, close, "a synchronized frame was left open");

    // Everything below is asserted *inside* the first frame, because the exit
    // also moves the cursor to the band's top row and erases from it: bytes
    // outside the frame would prove the exit ran, not that a band was drawn.
    let begins = text.find(FRAME_BEGIN).expect("a synchronized frame");
    let ends = text[begins..].find(FRAME_END).expect("a closed frame") + begins;
    let frame = &text[begins..ends];
    assert!(
        frame.contains(&format!("{BAND_TOP}\u{1b}[J")),
        "the frame did not clear the band before painting it: {frame:?}"
    );
    for (row, what) in [
        ("\u{1b}[22;1H", "the divider"),
        ("\u{1b}[23;1H", "the composer"),
        ("\u{1b}[24;1H", "the hint row"),
    ] {
        assert!(
            frame.contains(row),
            "{what} was never placed at {row:?}: {frame:?}"
        );
    }
    // The caret, after the composer's two-cell prompt marker on row 23. It is
    // the last thing the frame places, which is what leaves the terminal's own
    // cursor where the user is typing rather than where the paint ended.
    assert!(
        frame.ends_with("\u{1b}[23;3H"),
        "the frame did not end by placing the caret in the composer: {frame:?}"
    );
    // The band is the bottom of the screen and nothing above it: a row placed
    // in the document would be painting over the terminal's own scrollback.
    for row in 1..=21 {
        assert!(
            !frame.contains(&format!("\u{1b}[{row};1H")),
            "the band painted on document row {row}: {frame:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// the composer
// ---------------------------------------------------------------------------
//
// What the band's middle rows are *for*: text typed into a raw terminal by a
// session that owns every keystroke, edited by grapheme, wrapped by the band,
// and capped so that a long draft cannot eat the terminal's own document.
//
// The needles here are frame-local byte strings rather than bare text, because
// everything the session ever painted is still in the harness's buffer: a
// needle that were merely a prefix of an earlier frame's composer row would be
// satisfied by that frame and would assert nothing at all.

#[test]
fn typing_appears_in_the_composer_and_the_cursor_follows_it() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    pty.resize(24, 80);
    let mut session = Session::spawn_without_taking_the_terminal(&pty, tui(&sandbox));
    session.wait_for(READY);

    // The composer row, and the hint row's own placement right after it: the
    // pair is what makes this the whole row rather than a prefix of a longer
    // one.
    session.type_bytes("hello \u{d55c}\u{ae00}".as_bytes());
    session.wait_for("> hello \u{d55c}\u{ae00}\u{1b}[24;1H");
    // Backspace removes the whole grapheme, not a byte of it.
    session.type_bytes(&[0x7f]);
    session.wait_for("> hello \u{d55c}\u{1b}[24;1H");

    // C-a: the caret goes home, which is the composer's first column -- two
    // cells of prompt marker in, on the composer's own row.
    session.type_bytes(&[0x01]);
    session.wait_for("> hello \u{d55c}\u{1b}[24;1H\u{1b}[23;3H");
    // C-d with text under the caret: a forward delete, and the session stays.
    session.type_bytes(&[0x04]);
    session.wait_for("> ello \u{d55c}\u{1b}[24;1H");
    assert!(matches!(session.state(), Wait::Running));

    // C-e to the end, C-u to kill the line back to its start, and C-d on the
    // empty composer leaves. The kill takes the line the caret is on, so the
    // move is part of clearing it rather than decoration.
    session.type_bytes(&[0x05, 0x15, 0x04]);
    assert_eq!(session.wait_exit().code(), Some(0));
}

#[test]
fn the_composer_stops_growing_at_half_the_content_area() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    // content_bottom 9 => at most 5 composer rows, so a band whose divider is
    // row 6 is a composer at its cap.
    pty.resize(12, 20);
    let mut session = Session::spawn_without_taking_the_terminal(&pty, tui(&sandbox));
    session.wait_for(READY);

    for _ in 0..8 {
        session.type_bytes(b"0123456789012345678");
        session.type_bytes(&[0x0a]); // C-j: a newline in the composer
    }
    // The band at its cap: the divider on row 6, and the erase that begins
    // every frame from there. Sixteen rows of draft are in the composer and it
    // is showing five of them.
    session.wait_for("\u{1b}[6;1H\u{1b}[J");
    session.wait_for("\u{1b}[12;1H"); // the hint row is still the last row
    let text = session.text();
    for row in 1..=5 {
        assert!(
            !text.contains(&format!("\u{1b}[{row};1H")),
            "the composer grew past its cap and painted over the transcript \
             on row {row}: {text:?}"
        );
    }

    // Submitting is what empties a draft this tall -- a kill takes one line --
    // and an empty composer is what Ctrl-D leaves from.
    session.type_bytes(&[0x0d]);
    session.type_bytes(&[0x04]);
    assert_eq!(session.wait_exit().code(), Some(0));

    // And what was submitted is in the terminal's own document, above the band.
    let text = session.settled_text();
    assert!(
        text.contains("0123456789012345678"),
        "the submitted draft never reached the document: {text:?}"
    );
}

#[test]
fn a_terminal_too_small_for_a_band_is_refused_with_a_reason() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    pty.resize(4, 80);
    let before = modes(&pty);
    let mut session = Session::spawn_without_taking_the_terminal(&pty, tui(&sandbox));
    assert_eq!(session.wait_exit().code(), Some(1));

    let text = session.settled_text();
    assert!(
        text.contains("xfx: "),
        "a terminal too small was refused without saying so: {text:?}"
    );
    assert!(
        text.contains("4x80"),
        "the refusal does not say what the terminal is: {text:?}"
    );
    // Refused *before* the terminal was taken, which is the whole of the
    // ordering: not one byte of the mode set, and a line discipline nobody
    // touched. A refusal discovered after raw mode would have to give back a
    // terminal it had no reason to take.
    assert_no_mode_bytes(&text);
    assert!(
        !text.contains(FRAME_BEGIN),
        "a band was painted onto a screen that cannot hold one: {text:?}"
    );
    assert_eq!(before, modes(&pty), "the terminal was left changed");
}

// ---------------------------------------------------------------------------
// a turn
// ---------------------------------------------------------------------------

#[test]
fn a_submitted_prompt_runs_one_turn_and_the_answer_lands_in_the_document() {
    let gateway = FakeGateway::start(vec![support::fake_gateway::Reply::Sse(
        support::fake_gateway::content_only(&["MARKER-TURN-", "ONE"]),
    )]);
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    pty.resize(24, 80);
    let mut session =
        Session::spawn_without_taking_the_terminal(&pty, tui_with(&sandbox, &gateway));
    session.wait_for(READY);

    session.type_bytes(b"say the marker");
    session.type_bytes(&[0x0d]);
    session.wait_for("MARKER-TURN-ONE");

    // The prompt xfx actually sent carries what was typed -- the screen looking
    // right is not evidence that anything was sent.
    let body = gateway.only_request().json();
    let sent = user_messages(&body);
    assert!(
        sent.iter()
            .any(|message| message.contains("say the marker")),
        "the composer's text never reached the wire: {sent:?}"
    );

    session.type_bytes(&[0x04]);
    assert_eq!(session.wait_exit().code(), Some(0));
    assert_eq!(sandbox.session_ids().len(), 1, "the turn was not recorded");
}

#[test]
fn a_second_prompt_may_wait_and_a_third_is_refused_with_its_text_kept() {
    // Against a **running** worker, which is the only place the claim can be
    // tested: the work channel's one slot is emptied the instant the runtime
    // takes the prompt, so from here on the channel says "room" for the whole
    // length of the turn. A session that asked it would accept every later
    // prompt silently and run each one as a surprise when the last ended.
    //
    // What the session holds instead is `worker::WORK_LIMIT`: the turn in
    // flight, and one prompt waiting where the band says `queued 1`. The third
    // is refused on the hint row with its text left in the composer.
    //
    // `SseThenHang` is what keeps the first turn running: the body arrives, the
    // connection does not close, and the turn is still in flight while the
    // other two prompts are typed.
    let gateway = FakeGateway::start(vec![support::fake_gateway::Reply::SseThenHang(vec![
        support::fake_gateway::sse_body(&[support::fake_gateway::text_delta(
            "d",
            "FIRST-TURN-RUNNING",
        )]),
    ])]);
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    pty.resize(24, 80);
    let mut session =
        Session::spawn_without_taking_the_terminal(&pty, tui_with(&sandbox, &gateway));
    session.wait_for(READY);

    session.type_bytes(b"first\r");

    // Response-only: the answer's own text, so the turn is provably under way
    // rather than merely submitted.
    session.wait_for("FIRST-TURN-RUNNING");

    session.type_bytes(b"second\r");
    // Response-only as well: nothing this test types contains it, and the band
    // is the only thing that writes it. A queue nobody can see is the "queued
    // into a surprise" this row exists to rule out.
    session.wait_for("queued 1");

    session.type_bytes(b"third\r");
    session.wait_for("one prompt is already queued");
    // The draft was not thrown away with the refusal. Raw mode has `ECHO`
    // clear, so these six cells on the terminal are the composer painting them
    // and not the line discipline echoing what was typed.
    session.wait_for("third");

    session.type_bytes(&[0x03, 0x03]);
    assert_eq!(session.wait_exit().code(), Some(130));

    assert_eq!(
        gateway.request_count(),
        1,
        "a prompt the user was refused, or one they had to wait for, reached \
         the wire anyway"
    );
    let sent = user_messages(&gateway.only_request().json());
    assert!(
        !sent.iter().any(|message| message.contains("third")),
        "the refused prompt is in the one request that was made: {sent:?}"
    );
}

#[test]
fn ctrl_c_is_a_byte_that_cancels_the_turn_and_a_second_one_exits_130() {
    // The row of the restoration matrix that no signal reaches: `ISIG` is
    // clear, so a typed Ctrl-C generates nothing and arrives as `0x03` for the
    // decoder. This case and `an_external_sigint_is_not_swallowed_because_isig_is_off`
    // together are what prove the two paths are disjoint -- one kills the
    // process by the signal, the other is a keystroke the session answers.
    let gateway = FakeGateway::start(vec![support::fake_gateway::Reply::SseThenHang(vec![
        support::fake_gateway::sse_body(&[support::fake_gateway::text_delta(
            "a",
            "MARKER-STREAMING",
        )]),
    ])]);
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    pty.resize(24, 80);
    let before = modes(&pty);
    let mut session =
        Session::spawn_without_taking_the_terminal(&pty, tui_with(&sandbox, &gateway));
    session.wait_for(READY);
    session.type_bytes(b"start something long\r");
    session.wait_for("MARKER-STREAMING");

    session.type_bytes(&[0x03]);
    // Response-only: `app::INTERRUPT_NOTICE`'s own words, which nothing here
    // types.
    session.wait_for("stopping the turn");
    // No signal was delivered, so the child is still alive and the byte did the
    // work. Read without consuming anything, so asking does not change the
    // answer for the exit below.
    assert!(matches!(session.state(), Wait::Running));

    session.type_bytes(&[0x03]);
    let status = session.wait_exit();
    assert_eq!(
        status.code(),
        Some(130),
        "a session the user interrupted did not say so in its status"
    );
    assert_eq!(before, modes(&pty));
}

#[test]
fn an_interrupt_drops_the_prompt_that_was_waiting_rather_than_running_it_next() {
    // The contract in one line: after the interrupt notice, nothing else
    // starts. The runtime is held by a turn that never ends -- `SseThenHang`
    // writes its body and does not hang up -- so the second prompt is provably
    // *waiting* rather than run and forgotten, and the gateway is scripted with
    // a second reply nobody should ever see.
    let gateway = FakeGateway::start(vec![
        support::fake_gateway::Reply::SseThenHang(vec![support::fake_gateway::sse_body(&[
            support::fake_gateway::text_delta("d", "FIRST-TURN-RUNNING"),
        ])]),
        support::fake_gateway::Reply::Sse(support::fake_gateway::content_only(&[
            "MARKER-THE-QUEUE-RAN-ANYWAY",
        ])),
    ]);
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    pty.resize(24, 80);
    let mut session =
        Session::spawn_without_taking_the_terminal(&pty, tui_with(&sandbox, &gateway));
    session.wait_for(READY);
    session.type_bytes(b"first\r");
    session.wait_for("FIRST-TURN-RUNNING");
    session.type_bytes(b"second\r");
    session.wait_for("queued 1");

    session.type_bytes(&[0x03]);
    // Response-only, and both halves of the keystroke: the turn was asked to
    // stop, and what was queued behind it goes too.
    session.wait_for("stopping the turn");
    session.wait_for("dropping what was queued");
    // Still alive: one Ctrl-C is a cancellation, not an exit.
    assert!(matches!(session.state(), Wait::Running));

    // The band has stopped announcing a queue that no longer exists. Asserted
    // as the *frame* an empty hint row makes -- the row written at 24;1 with
    // nothing on it before the caret goes back to the composer -- and counted
    // from the whole settled stream below rather than waited for, because an
    // empty hint row is also what every frame before the queue looked like.
    session.type_bytes(&[0x04]);
    assert_eq!(session.wait_exit().code(), Some(0));
    let text = session.settled_text();
    let queued = text.rfind("queued 1").expect("the queue was announced");
    let cleared = text
        .rfind("\u{1b}[24;1H\u{1b}[23;3H")
        .expect("a frame with an empty hint row and an empty composer");
    assert!(
        queued < cleared,
        "the band was still saying `queued 1` after the interrupt dropped it"
    );

    assert_eq!(
        gateway.request_count(),
        1,
        "the queued prompt started by itself after the user stopped everything"
    );
    assert!(
        !text.contains("MARKER-THE-QUEUE-RAN-ANYWAY"),
        "the second reply was delivered, so the queued prompt ran: {text:?}"
    );
}

#[test]
fn the_interrupt_that_stopped_one_turn_does_not_end_the_session_on_the_next() {
    // The gesture must not outlive the turn it was about. Two turns, each of
    // them stopped by a Ctrl-C: the one below the second answer is the *first*
    // Ctrl-C of a new turn, so it has to cancel that turn rather than exit 130.
    //
    // **Both turns hang deliberately, and that is what makes the case
    // deterministic rather than a race.** The first reply used to be an
    // ordinary one that finished on its own, and the test then leaned on the
    // Ctrl-C landing in the gap between the answer appearing on the terminal
    // and the turn concluding. Task 13 made that gap the other sign: an answer
    // is *released* over several frames now, so by the time its last character
    // is on the screen the turn behind it has long since ended, and a Ctrl-C
    // there throws the draft away instead of stopping anything. A turn that is
    // provably still running is the state the gesture is about.
    let hanging = |text: &str| {
        support::fake_gateway::Reply::SseThenHang(vec![support::fake_gateway::sse_body(&[
            support::fake_gateway::text_delta("d", text),
        ])])
    };
    let gateway = FakeGateway::start(vec![hanging("MARKER-ONE"), hanging("MARKER-TWO")]);
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    pty.resize(24, 80);
    let mut session =
        Session::spawn_without_taking_the_terminal(&pty, tui_with(&sandbox, &gateway));
    session.wait_for(READY);

    session.type_bytes(b"ask one\r");
    session.wait_for("MARKER-ONE");
    session.type_bytes(&[0x03]);
    session.wait_for("stopping the turn");

    session.type_bytes(b"ask two\r");
    session.wait_for("MARKER-TWO");
    session.type_bytes(&[0x03]);
    session.wait_for_count("stopping the turn", 2);
    assert!(
        matches!(session.state(), Wait::Running),
        "the first Ctrl-C of a new turn exited the session, because the \
         session still remembered being asked to stop the last one"
    );

    // And the session is still a session: it leaves the ordinary way.
    session.type_bytes(&[0x04]);
    assert_eq!(session.wait_exit().code(), Some(0));
}

/// One answer, long enough that releasing it takes many frames.
///
/// `MIN_CPS` is 400 bytes a second, so this much text is about eight tenths of
/// a second of stream whatever the backlog rule computes -- a margin the
/// assertions below are made inside, rather than a race run against.
///
/// **Words rather than one long run of characters**, and that is a property of
/// the instrument rather than decoration: an answer is wrapped into rows and
/// each row is placed with its own `CUP`, so a needle that straddled a row
/// boundary would never appear on the wire whole and a `wait_for` on it could
/// only ever time out. Wrapping moves a word down whole, so a marker that is
/// one word is on one row whatever the answer's length works out to be.
fn long_answer(head: &str, tail: &str) -> String {
    format!("{head} {} {tail}", "xx ".repeat(100))
}

#[test]
fn a_streamed_answer_is_released_over_frames_rather_than_dumped_in_one() {
    // The pacer, on a real terminal. A provider does not stream at a human
    // rate: it sends a burst, then nothing, then a bigger burst. A UI that
    // appended each burst the instant it arrived shows an answer as a series
    // of jumps, and this one shows it as a stream.
    //
    // The claim is made in the one direction a terminal can prove it: the
    // **head of a single delta is on the screen while its tail is not**. There
    // is no arrangement of buffers in which that is true of text appended
    // whole, and the gap it is asserted inside is three quarters of a second
    // of pacing rather than a scheduling accident.
    //
    // `SseThenHang` keeps the turn running, so the 200 ms drain deadline is not
    // what is being measured here.
    let gateway = FakeGateway::start(vec![support::fake_gateway::Reply::SseThenHang(vec![
        support::fake_gateway::sse_body(&[support::fake_gateway::text_delta(
            "a",
            &long_answer("MARKER-HEAD", "MARKER-TAIL"),
        )]),
    ])]);
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    pty.resize(24, 80);
    let mut session =
        Session::spawn_without_taking_the_terminal(&pty, tui_with(&sandbox, &gateway));
    session.wait_for(READY);
    session.type_bytes(b"stream\r");

    // Response-only, and the first eleven bytes of the delta.
    session.wait_for("MARKER-HEAD");
    assert!(
        !session.text().contains("MARKER-TAIL"),
        "the whole delta was on the terminal the instant its first byte was: \
         the answer was appended rather than released"
    );

    // And the rest of it arrives -- pacing is a delay, not a filter.
    session.wait_for("MARKER-TAIL");

    session.type_bytes(&[0x04]);
    assert_eq!(session.wait_exit().code(), Some(0));
}

#[test]
fn an_escape_sequence_in_an_answer_reaches_the_terminal_disarmed() {
    // What the band is protected by, end to end. The text of an answer is a
    // provider's, and the rows it becomes are written straight to a terminal
    // that would *obey* what is in them -- `\x1b[2J` erases the screen,
    // `\x1b[?1049h` takes the alternate buffer this TUI promises never to
    // touch, an OSC retitles the window.
    //
    // Two doors, and this proves the pair rather than either: the `ESC` is
    // turned into a space at the channel every `UiEvent` crosses
    // (`tui::bridge::inert`), and a row is stripped of everything but a colour
    // before it is placed (`tui::frame::row_text`). So the bytes arrive -- they
    // are quoted here as ordinary text, which is exactly the evidence that they
    // were delivered and not merely lost -- and the terminal executes none of
    // them.
    let gateway = FakeGateway::start(vec![support::fake_gateway::Reply::Sse(
        support::fake_gateway::content_only(&["BEFORE-\u{1b}[2J\u{1b}[?1049h\u{1b}[31m-AFTER"]),
    )]);
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    pty.resize(24, 80);
    let mut session =
        Session::spawn_without_taking_the_terminal(&pty, tui_with(&sandbox, &gateway));
    session.wait_for(READY);
    session.type_bytes(b"say something dangerous\r");
    session.wait_for("-AFTER");

    session.type_bytes(&[0x04]);
    assert_eq!(session.wait_exit().code(), Some(0));
    let text = session.settled_text();
    // The answer is there, with its escape bytes disarmed into spaces.
    assert!(
        text.contains("[2J [?1049h [31m-AFTER"),
        "the answer did not arrive at all, so nothing above is evidence: {text:?}"
    );
    // And not one of the three sequences was ever written as a sequence.
    for sequence in ["\u{1b}[2J", "\u{1b}[?1049h", "\u{1b}[31m"] {
        assert!(
            !text.contains(sequence),
            "a provider's {sequence:?} reached the terminal and was obeyed: {text:?}"
        );
    }
}

#[test]
fn leaving_mid_stream_does_not_take_the_rest_of_the_answer_with_it() {
    // The other half of pacing, and the one that would be silent. Text held
    // back for a clock is text the band can come down on top of: Phase 1 never
    // repaints a document row, so an answer still in the queue when the session
    // exits is an answer the user never gets. Every way out flushes it whole --
    // this is the way out that has *nothing to drain*, because the runtime had
    // already handed everything over before the user typed Ctrl-D.
    let gateway = FakeGateway::start(vec![support::fake_gateway::Reply::SseThenHang(vec![
        support::fake_gateway::sse_body(&[support::fake_gateway::text_delta(
            "a",
            &long_answer("MARKER-BEGAN", "MARKER-ENDED"),
        )]),
    ])]);
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    pty.resize(24, 80);
    let before = modes(&pty);
    let mut session =
        Session::spawn_without_taking_the_terminal(&pty, tui_with(&sandbox, &gateway));
    session.wait_for(READY);
    session.type_bytes(b"stream\r");
    session.wait_for("MARKER-BEGAN");

    session.type_bytes(&[0x04]);
    assert_eq!(session.wait_exit().code(), Some(0));
    let text = session.settled_text();
    assert!(
        text.contains("MARKER-ENDED"),
        "the exit took the rest of the answer with it: {text:?}"
    );
    assert_eq!(before, modes(&pty), "the terminal was left changed");
}

#[test]
fn what_the_backpressure_held_back_is_drained_and_painted_on_the_way_out() {
    // The drain's other half, end to end, and the bound is what makes it
    // reachable on demand. An answer bigger than `event_loop::PACED_BACKLOG`
    // stops being taken off the channel part-way through -- that is the whole
    // point of the bound -- so at the moment the user leaves there are real
    // `UiEvent`s still queued behind a UI that deliberately stopped listening.
    // Those are the events the shutdown drain exists to collect, and collecting
    // them is not enough: they have to be **shown** and **painted**, or an exit
    // that looked clean drops the end of an answer the provider had already
    // finished sending.
    //
    // **Few, large deltas on purpose.** The channel is `bridge::UI_EVENTS` deep,
    // so an answer split into thousands of small ones parks the producer -- and
    // then quitting cancels the turn and the rest of the answer was never sent
    // at all, which is a different (and documented) story. Twenty of eight
    // kilobytes all fit, so the provider is provably *finished* while the UI
    // still holds only the first eight of them.
    //
    // Deterministic rather than raced: the queue drains at `pacer::MAX_CPS`,
    // 5000 bytes a second, so a backlog at the 64 KiB mark stays over it for
    // thirteen seconds and the keystroke below lands inside the first tenth of
    // one.
    let block = "answer ".repeat(1170);
    let mut deltas: Vec<Value> = vec![support::fake_gateway::text_delta(
        "a",
        &format!("MARKER-BEGAN {block}"),
    )];
    deltas.extend((0..18).map(|_| support::fake_gateway::text_delta("a", &block)));
    deltas.push(support::fake_gateway::text_delta("a", "MARKER-ENDED"));
    deltas.push(support::fake_gateway::finish("stop"));
    let gateway = FakeGateway::start(vec![support::fake_gateway::Reply::Sse(
        support::fake_gateway::sse_body(&deltas),
    )]);
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    pty.resize(24, 80);
    let before = modes(&pty);
    let mut session =
        Session::spawn_without_taking_the_terminal(&pty, tui_with(&sandbox, &gateway));
    session.wait_for(READY);
    session.type_bytes(b"stream more than the bound\r");
    session.wait_for("MARKER-BEGAN");

    session.type_bytes(&[0x04]);
    assert_eq!(session.wait_exit().code(), Some(0));
    let text = session.settled_text();
    assert!(
        text.contains("MARKER-ENDED"),
        "the events the bound left on the channel were drained and thrown \
         away rather than written"
    );
    assert_eq!(before, modes(&pty), "the terminal was left changed");
}

#[test]
fn a_double_escape_clears_the_composer_and_warns_before_it_does() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    pty.resize(24, 80);
    let mut session = Session::spawn_without_taking_the_terminal(&pty, tui(&sandbox));
    session.wait_for(READY);

    session.type_bytes(b"a draft worth keeping");
    session.wait_for("a draft worth keeping");

    session.type_bytes(&[0x1b]);
    // Response-only: the hint row's warning, which nothing here types. Waiting
    // for it also proves the Escape has *settled* -- a lone ESC is the Escape
    // key only after 50 ms of quiet -- so the second one below is a second
    // keystroke rather than the tail of this one.
    session.wait_for("esc again to clear");

    session.type_bytes(&[0x1b]);
    // The composer's row, empty, immediately followed by the `CUP` for the row
    // below it: the marker with nothing after it is what an empty composer
    // paints and a composer holding a draft cannot.
    session.wait_for("\u{1b}[23;1H> \u{1b}[24;1H");

    session.type_bytes(&[0x04]);
    assert_eq!(session.wait_exit().code(), Some(0));
}

#[test]
fn every_slash_command_is_answered_by_the_session_rather_than_by_the_model() {
    // plan:109 -- the TUI answers exactly the six `interactive::SLASH_COMMANDS`
    // and nothing else. The gateway is scripted with a reply nobody should ever
    // see: a command that reached the provider would both show that marker and
    // leave a request behind.
    let gateway = FakeGateway::start(vec![support::fake_gateway::Reply::Sse(
        support::fake_gateway::content_only(&["MARKER-A-COMMAND-WAS-ASKED"]),
    )]);
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    pty.resize(24, 80);
    let mut session =
        Session::spawn_without_taking_the_terminal(&pty, tui_with(&sandbox, &gateway));
    session.wait_for(READY);

    // Each command with a needle out of its own answer, none of which the test
    // types: `/help`'s summary line, `/version`'s channel, the model report,
    // `/new`'s sentence, `/clear`'s promise, and a name that is not one of the
    // six.
    session.type_bytes(b"/help\r");
    session.wait_for("list these commands");
    session.type_bytes(b"/version\r");
    session.wait_for("xfx 0.");
    session.type_bytes(b"/model\r");
    session.wait_for("[shell] model=");
    session.type_bytes(b"/new\r");
    session.wait_for("starts a fresh conversation");
    session.type_bytes(b"/notacommand\r");
    session.wait_for("is not an xfx command");
    session.type_bytes(b"/clear\r");
    session.wait_for("the conversation is kept");

    // The sixth, and the reason it leaves this test rather than a Ctrl-D: all
    // six names are then literally pinned against the zero-request assertion
    // below rather than five of them.
    session.type_bytes(b"/quit\r");
    assert_eq!(session.wait_exit().code(), Some(0));
    assert_eq!(
        gateway.request_count(),
        0,
        "a slash command was sent to the provider as a prompt"
    );
    let text = session.settled_text();
    assert!(
        !text.contains("MARKER-A-COMMAND-WAS-ASKED"),
        "the provider answered something, so a command became a prompt: {text:?}"
    );
}

/// The three sequences `/clear` writes, in order (`interactive.rs:85`).
///
/// Spelled out rather than imported, like [`MODE_SET`]: `src/tui/shell.rs` is
/// not visible to an integration test, and a test that read the constant it is
/// checking would pass for any sequence the module happened to declare.
/// Response-only -- no test types an escape sequence -- so waiting for it means
/// the bytes are really on the wire.
const CLEAR_SCREEN: &str = "\u{1b}[H\u{1b}[2J\u{1b}[3J";

#[test]
fn clear_erases_the_scrollback_the_answers_live_in_and_not_only_the_screen() {
    // `3J` is the one that carries the weight here. An answer that has scrolled
    // past the band is in the *terminal's own* scrollback and xfx never
    // repaints it, so a `/clear` that erased the visible screen alone would
    // leave the whole transcript one wheel-turn away -- and would mean
    // something different on this surface than the same command means on the
    // line-oriented one.
    let gateway = FakeGateway::start(vec![support::fake_gateway::Reply::Sse(
        support::fake_gateway::content_only(&["MARKER-BEFORE-THE-CLEAR"]),
    )]);
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    pty.resize(24, 80);
    let mut session =
        Session::spawn_without_taking_the_terminal(&pty, tui_with(&sandbox, &gateway));
    session.wait_for(READY);
    session.type_bytes(b"say the marker\r");
    session.wait_for("MARKER-BEFORE-THE-CLEAR");

    session.type_bytes(b"/clear\r");
    session.wait_for(CLEAR_SCREEN);
    session.wait_for("the conversation is kept");

    session.type_bytes(&[0x04]);
    assert_eq!(session.wait_exit().code(), Some(0));

    let text = session.settled_text();
    let erased = text
        .find(CLEAR_SCREEN)
        .expect("the erase reached the screen");
    let kept = text
        .rfind("the conversation is kept")
        .expect("the promise reached the screen");
    assert!(
        erased < kept,
        "the row that says the conversation was kept was erased by the clear \
         it was written for: {text:?}"
    );
    assert_eq!(
        text.matches(CLEAR_SCREEN).count(),
        1,
        "the screen was erased more than once for one /clear: {text:?}"
    );
}

#[test]
fn quit_leaves_the_way_ctrl_d_does() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    pty.resize(24, 80);
    let before = modes(&pty);
    let mut session = Session::spawn_without_taking_the_terminal(&pty, tui(&sandbox));
    session.wait_for(READY);

    session.type_bytes(b"/quit\r");

    assert_eq!(session.wait_exit().code(), Some(0));
    let text = session.settled_text();
    assert!(
        text.contains(RESTORE),
        "the ordinary restore did not run on the way out of /quit: {text:?}"
    );
    assert_eq!(before, modes(&pty));
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

    use std::time::Duration;

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
    fn a_worker_panic_arrives_as_data_and_the_ui_restores_before_printing_it() {
        // The double-writer bug this rules out is invisible to a test that only
        // asserts "it exited".
        let gateway = FakeGateway::start(vec![support::fake_gateway::Reply::Sse(
            support::fake_gateway::content_only(&["unused"]),
        )]);
        let sandbox = Sandbox::new();
        let pty = Pty::open();
        pty.resize(24, 80);
        let before = modes(&pty);
        let mut command = tui_with(&sandbox, &gateway);
        command.env("XFX_TUI_FAULT", "worker-turn");
        let mut session = Session::spawn_without_taking_the_terminal(&pty, command);
        session.wait_for(READY);
        session.type_bytes(b"anything\r");

        let status = session.wait_exit();
        assert!(!status.success(), "a worker panic exited zero");
        // Settled rather than snapshotted: the ordering below is about the last
        // bytes the process writes, and a snapshot taken the instant it was
        // reaped can be missing them.
        let text = session.settled_text();
        // **Exactly once**, and this is the assertion that makes the ordering
        // one below mean anything. The runtime thread's panic runs the process
        // panic hook *before* it is caught, so a hook that reported it would
        // put a first copy on a terminal that is still raw and mid-frame -- and
        // an ordering test that looked for the *last* copy would be satisfied
        // by the UI's own, laundering the raw one it was written to catch.
        assert_eq!(
            text.matches("a turn panicked").count(),
            1,
            "the panic was reported twice: once by a thread that owns no \
             terminal, and once by the UI: {text:?}"
        );
        let restore = text.find("\u{1b}[?2004l").expect("the UI restored");
        let message = text
            .find("a turn panicked")
            .expect("the panic reached the user");
        assert!(
            restore < message,
            "the panic was printed into a raw terminal: {text:?}"
        );
        assert_eq!(before, modes(&pty));
    }

    #[test]
    fn quitting_mid_stream_with_a_slow_ui_still_exits_and_still_publishes_the_log() {
        // A turn that is still running when the user leaves, against a server
        // that has said everything it is going to say and will not hang up.
        //
        // **What this proves is the deadline, and it is worth being exact about
        // that.** The runtime thread does not conclude here: the transport is
        // awaiting bytes from a socket that will never produce another one, and
        // the cancellation it polls is read *between* reads, so a quiet socket
        // never delivers it -- `gateway/` is outside this plan's boundary and
        // the awaitable half of the pair reaches only the `UiEvent` channel.
        // Measured: the session log this leaves records the turn as
        // `outcome=unfinished`, which is the honest word for it.
        //
        // So the property under test is that none of that can hold the user's
        // terminal: the UI drains what the worker did produce, gives up on the
        // rest at `worker::DRAIN_DEADLINE`, closes the channel, waits out a
        // bounded join, and restores -- leaving a readable session log and a
        // `termios` byte for byte what it was. The drain's other half, the one
        // where the worker *does* conclude and the UI keeps receiving until it
        // says so, is proven deterministically in
        // `worker::tests::the_drain_keeps_receiving_until_the_turn_says_it_is_over`;
        // a pty cannot be made to interleave those two threads on demand.
        let mut deltas: Vec<Value> = (0..2000)
            .map(|n| support::fake_gateway::text_delta("d", &format!("chunk-{n} ")))
            .collect();
        deltas.push(support::fake_gateway::finish("stop"));
        let gateway = FakeGateway::start(vec![support::fake_gateway::Reply::SseThenHang(vec![
            support::fake_gateway::sse_body(&deltas),
        ])]);
        let sandbox = Sandbox::new();
        let pty = Pty::open();
        pty.resize(24, 80);
        let before = modes(&pty);
        let mut command = tui_with(&sandbox, &gateway);
        command.env("XFX_TUI_FAULT", "slow-ui");
        let mut session = Session::spawn_without_taking_the_terminal(&pty, command);
        session.wait_for(READY);
        session.type_bytes(b"stream a lot\r");
        session.wait_for("chunk-0");

        session.type_bytes(&[0x04]);
        let started = Instant::now();
        assert_eq!(session.wait_exit().code(), Some(0), "the drain deadlocked");
        // Near what the protocol actually promises rather than an order of
        // magnitude above it: `worker::DRAIN_DEADLINE` (2 s) plus `JOIN_GRACE`
        // (250 ms) plus room for a loaded machine's scheduling. A ten-second
        // bound would pass a deadline that had quietly stopped being enforced.
        // Measured here: 2.6-2.8 s.
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the drain outlived its deadline: {:?}",
            started.elapsed()
        );
        assert_eq!(before, modes(&pty));

        // A torn manifest is worse than a slow exit: the session must still read.
        let listed = Command::new(env!("CARGO_BIN_EXE_xfx"))
            .arg("session")
            .arg("last")
            .current_dir(&sandbox.workspace)
            .env("HOME", &sandbox.home)
            .output()
            .expect("read the session back");
        assert!(
            listed.status.success(),
            "the session log was left unreadable: {listed:?}"
        );
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
