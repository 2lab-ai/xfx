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

use rustix::process::Signal;
use rustix::termios::{ControlModes, InputModes, LocalModes};
use support::pty::{modes, open_slave, Pty, Session, TerminalState, Wait};
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

/// Requires the raw-mode bits upstream sets, read from the child's own terminal
/// while it runs (`shell_runtime.zig:108-138`).
fn assert_raw(state: TerminalState) {
    for mode in [
        LocalModes::ECHO,
        LocalModes::ICANON,
        LocalModes::IEXTEN,
        LocalModes::ISIG,
    ] {
        assert!(!state.local.contains(mode), "{mode:?} is still set");
    }
    for mode in [
        InputModes::IXON,
        InputModes::ICRNL,
        InputModes::BRKINT,
        InputModes::INPCK,
        InputModes::ISTRIP,
    ] {
        assert!(!state.input.contains(mode), "{mode:?} is still set");
    }
    assert!(state.control.contains(ControlModes::CS8), "CS8 is not set");
    assert_eq!(state.min, 1, "VMIN");
    assert_eq!(state.time, 0, "VTIME");
}

#[test]
fn the_tui_positively_enters_raw_mode_and_owns_the_normal_buffer() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = Session::spawn_without_taking_the_terminal(&pty, tui(&sandbox));
    session.wait_for(READY);

    assert_raw(modes(&pty));

    let text = session.text();
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

    session.type_bytes(&[0x04]);
    assert_eq!(session.wait_exit().code(), Some(0));
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
    let text = session.text();
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
    let text = session.text();
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
    assert_no_mode_bytes(&session.text());
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
    assert_no_mode_bytes(&session.text());
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
    let text = session.text();
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
    session.wait_for(READY);
    assert_raw(modes(&pty));

    // And it is a live session afterwards rather than a survivor: Ctrl-D still
    // leaves, and the exit still gives the terminal back byte for byte.
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
