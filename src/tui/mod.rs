//! The Phase-1 TUI: a raw-mode shell that owns a band at the bottom of the
//! terminal's **normal** buffer.
//!
//! It is reached by `XFX_TUI=1` on a bare `xfx` and by nothing else, because it
//! is narrower than the line-oriented shell it sits beside; `docs/parity.md`
//! states exactly how much narrower. The UI thread here is the process's main
//! thread and it owns the terminal exclusively: nothing else writes a byte to
//! stdout, which is what makes "what is on the terminal is what this module
//! wrote" a property rather than a hope (`.prd/03-tui-port.md`).

use std::io::{self, IsTerminal, Write};
use std::os::fd::{AsFd, AsRawFd};
use std::process::ExitCode;

use crate::cli::{Cli, Command};

/// The variable that opts a bare invocation into the TUI.
pub const TUI_ENV: &str = "XFX_TUI";

/// Whether this invocation is the one the TUI owns.
///
/// A bare `xfx`, `XFX_TUI=1`, and a real terminal on both ends. Anything else --
/// a subcommand, a pipe, an unset or different value -- is the line-oriented
/// shell or an ordinary command, unchanged.
pub fn should_run(cli: &Cli) -> bool {
    matches!(cli.command, Command::Interactive)
        && std::env::var_os(TUI_ENV).is_some_and(|value| value == "1")
        && io::stdin().is_terminal()
        && io::stdout().is_terminal()
}

/// Runs the TUI on the calling thread, which must be the process's main thread.
pub fn run_blocking(_cli: Cli) -> ExitCode {
    let config = match crate::app::load_config() {
        Ok(config) => config,
        Err(err) => return fail(&format!("{err}")),
    };
    match session(&config) {
        Ok(code) => code,
        Err(err) => fail(&format!("{err}")),
    }
}

/// Takes the terminal, holds it, and gives it back.
///
/// The two descriptors are named separately and stay separate: `termios` is
/// captured from -- and raw mode entered on -- the descriptor input arrives on,
/// while the mode sequences go to the one output leaves by. On an ordinary
/// invocation they are the same terminal; when they are not, restoring the
/// wrong one would leave the input raw.
fn session(_config: &crate::config::RuntimeConfig) -> io::Result<ExitCode> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let saved = term::capture(stdin.as_fd())?;
    term::enter_raw(stdin.as_fd(), &saved)?;
    let tmux = term::under_tmux();
    term::adopt(
        stdin.as_fd().as_raw_fd(),
        stdout.as_fd().as_raw_fd(),
        saved,
        tmux,
    );

    // The terminal is raw from here on, and `term::shutdown` is the only thing
    // that gives it back, so this function must not return before it runs --
    // no `?`, no early return. Everything that can fail happens inside `hold`,
    // which reports its failure rather than escaping with it.
    let held = hold(tmux);
    let restored = term::shutdown(BAND_TOP);
    // The first failure wins, and the restore was attempted either way: a
    // screen error that happened while xfx still had the terminal is the one
    // worth reporting, and a terminal left raw is worse than either.
    held.and(restored)?;
    Ok(ExitCode::SUCCESS)
}

/// The row the exit clears from, once there is a band to clear.
///
/// `None` until Task 6 of this plan solves a real layout: a session that drew
/// nothing has no band top, and clearing from the screen's first row instead
/// would erase a screen xfx never wrote to.
const BAND_TOP: Option<u16> = None;

/// Holds the terminal for the length of the session.
///
/// Writes the mode set and then reads until the user leaves. Its failures are
/// returned rather than acted on, because the caller owns the restore.
fn hold(tmux: bool) -> io::Result<()> {
    let mut out = io::stdout().lock();
    write!(
        out,
        "{}",
        if tmux {
            term::MODE_SET_TMUX
        } else {
            term::MODE_SET
        }
    )?;
    out.flush()?;
    drop(out);

    // One byte of input, and the exit path. The poll loop, the band, and the
    // turn arrive in the later tasks of this plan; Ctrl-D leaving is the
    // shell's own contract and survives all of them.
    let mut byte = [0u8; 1];
    loop {
        match io::Read::read(&mut io::stdin(), &mut byte) {
            // End of input, and Ctrl-D: the shell's own contract is that
            // Ctrl-D leaves, and it survives every later task.
            Ok(0) => return Ok(()),
            Ok(_) if byte[0] == 0x04 => return Ok(()),
            Ok(_) => continue,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(err),
        }
    }
}

/// Reports a failure that stopped the TUI, on a terminal that is still cooked.
fn fail(message: &str) -> ExitCode {
    let _ = writeln!(io::stderr(), "xfx: {message}");
    ExitCode::FAILURE
}

mod term;
