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
use std::process::{Command, Stdio};

use rustix::termios::{ControlModes, InputModes, LocalModes};
use support::pty::{modes, open_slave, Pty, Session, TerminalState};
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
