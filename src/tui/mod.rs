//! The Phase-1 TUI: a raw-mode shell that owns a band at the bottom of the
//! terminal's **normal** buffer.
//!
//! It is reached by `XFX_TUI=1` on a bare `xfx` and by nothing else, because it
//! is narrower than the line-oriented shell it sits beside; `docs/parity.md`
//! states exactly how much narrower. The UI thread here is the process's main
//! thread and it owns the terminal exclusively: nothing else writes a byte to
//! stdout, which is what makes "what is on the terminal is what this module
//! wrote" a property rather than a hope (`.prd/03-tui-port.md`).
//!
//! Startup has one order, and every step of it is there because the step before
//! it opens a window that the step itself closes:
//!
//! > **mask** -> **hook** -> **raw** -> **handlers** -> **announce**
//!
//! The signal mask goes on first, so no owned signal can be *taken* by a
//! default disposition while the terminal is being transformed. The panic hook
//! is armed next, against ownership recorded a statement earlier, so there is
//! no instant in which the terminal is raw with nothing behind a panic. Only
//! then does the terminal become raw. The handlers are installed after that --
//! inside `hold`, where a `sigaction` that fails travels back through the
//! restore instead of escaping above it -- and the mask lifts as they go on.
//! The mode set is announced last, because it is the first byte a terminal sees
//! and every path that could still fail is now behind a restore.
//!
//! The launch probe follows that order rather than joining it: only once the
//! terminal is raw can a `CSI 6n` be asked and its answer read back off
//! standard input, which is what tells the session how much of the shell's
//! output to push above the band (see [`probe`]).

use std::io::{self, IsTerminal, Write};
use std::os::fd::{AsFd, AsRawFd};
use std::process::ExitCode;
use std::time::Instant;

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
    // The matrix row for a startup that fails before the terminal is touched:
    // nothing has been captured, nothing has been set, and the exit must write
    // no restore bytes for state it never took.
    #[cfg(feature = "fault-injection")]
    if fault::injected(fault::Fault::BeforeRaw) {
        return Err(io::Error::other("the session store could not be opened"));
    }

    let stdin = io::stdin();
    let stdout = io::stdout();
    // Made before the terminal is taken and held until after it is given back,
    // and both halves are the reason it is here rather than beside the handler
    // installation it belongs to. Creating it on a terminal that is still
    // cooked makes a pipe this process cannot open an ordinary early return.
    // Outliving `hold` keeps the write end the handlers were handed pointing at
    // this pipe for as long as a handler can run, rather than at a descriptor
    // number the process has since closed and reused.
    let wakeup = signals::Wakeup::new()?;
    // Nothing that owns a signal may be delivered from here until the last
    // handler is installed. Without this the transition below has a window in
    // it: `enter_raw` makes the terminal raw, and until `signals::install` runs
    // -- one `sigaction` at a time -- a `SIGTERM` finds the default
    // disposition and kills xfx with the terminal still raw and nothing left to
    // put it back. The token is consumed by `install` and by nothing else, so
    // the ordering is the compiler's to keep; dropping it early on a failed
    // `capture` restores the mask this thread arrived with.
    let blocked = signals::block_owned()?;
    let saved = term::capture(stdin.as_fd())?;
    let tmux = term::under_tmux();
    // Ownership is recorded, and the panic hook armed, **before** the terminal
    // becomes raw -- so that there is no instant in which the terminal is raw
    // and nothing is behind a panic. The alternative was to prove that the two
    // statements between an earlier `enter_raw` and this hook could not panic,
    // and a proof that has to be re-done by every future edit is not one worth
    // having.
    //
    // Arming it early costs only this: a panic in the sliver before `enter_raw`
    // now writes the *abnormal* restore for screen state that was never set,
    // and puts back a `termios` the terminal already has. Both are harmless --
    // the escape sequence is upstream's deliberately defensive one
    // (`app_lifecycle.zig:36-38`), written on an exit that by definition does
    // not know what is on the screen, and the `tcsetattr` is a no-op. A raw
    // terminal nobody will cook again is not harmless, which is the trade.
    term::adopt(
        stdin.as_fd().as_raw_fd(),
        stdout.as_fd().as_raw_fd(),
        saved.clone(),
        tmux,
    );
    panic::install_hook();
    term::enter_raw(stdin.as_fd(), &saved)?;

    // The matrix row for a panic on a thread that owns nothing. It has to be
    // taken while the terminal is raw, because "the hook left the terminal
    // alone" is only a claim about a terminal that was in a state to be left.
    #[cfg(feature = "fault-injection")]
    if fault::injected(fault::Fault::NonOwnerPanic) {
        fault::panic_off_the_ui_thread();
    }
    // The matrix row for a startup that fails on the far side of raw mode, and
    // the rule this whole hook exists for: a failure after raw mode restores
    // before it reports. The `?` is the restore's own failure, not an escape
    // around it -- `shutdown` has already run either way.
    #[cfg(feature = "fault-injection")]
    if fault::injected(fault::Fault::AfterRaw) {
        term::shutdown(BAND_TOP)?;
        return Err(io::Error::other("the poll set could not be created"));
    }
    // The matrix row for a panic while the terminal is raw. It unwinds past
    // every restore below, which is the point: only the hook can answer it.
    #[cfg(feature = "fault-injection")]
    if fault::injected(fault::Fault::UiFrame) {
        panic!("a frame commit failed");
    }

    // The terminal is raw from here on, and `term::shutdown` is the only thing
    // that gives it back, so this function must not return before it runs --
    // no `?`, no early return. Everything that can fail happens inside `hold`,
    // which reports its failure rather than escaping with it.
    let held = hold(tmux, &wakeup, blocked);
    let restored = term::shutdown(BAND_TOP);
    // The terminal is back, so the signals go back too -- before `wakeup` is
    // dropped, because the handlers were handed its write end and outlive it.
    signals::release();
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
/// Installs the signal handlers, writes the mode set, and then reads until the
/// user leaves. Its failures are returned rather than acted on, because the
/// caller owns the restore.
///
/// The handlers are installed here rather than next to `term::adopt`, where
/// they logically belong, for one reason: `sigaction` can fail, and a `?` above
/// the caller's `hold`/`shutdown` pair would return with the terminal raw and
/// no restore behind it. Installed here the failure travels back through that
/// restore like every other one. It is still the first thing the session does,
/// and the caller's [`signals::block_owned`] token -- passed through to be
/// consumed here -- means the delay costs nothing: no owned signal can be
/// delivered until this call has returned.
fn hold(tmux: bool, wakeup: &signals::Wakeup, blocked: signals::Blocked) -> io::Result<()> {
    // The block lifts inside `install`, so anything held across the transition
    // is delivered there. A held `SIGTSTP` is the case that has to be answered
    // here: it stops the process on the way out of `install`, and the stop
    // handler cooked the terminal before it did. Waiting for the read loop to
    // notice would not work -- it only consults the flag on an `EINTR`, and
    // nothing has been read yet, so no `EINTR` is coming -- and announcing
    // first would put a session that believes it is raw on a cooked terminal.
    let held = signals::install(wakeup, blocked)?;
    if held.stopped_before_the_session_began() {
        resume(tmux)?;
    } else {
        announce(tmux)?;
    }

    // Where the shell left the cursor decides how much of its output has to be
    // pushed above the band, and a terminal that does not answer within the
    // deadline is treated as row 1 -- xfx starts at the bottom of what it can
    // prove rather than painting over something it cannot see.
    //
    // It happens here, after the mode set and before anything is drawn, for the
    // same reason the handler installation does: it reads and writes, either
    // can fail, and a failure inside `hold` travels back through the caller's
    // restore instead of escaping above it with the terminal still raw.
    let mut cursor = probe::CursorProbe::new();
    let start_row = cursor
        .read_reply(Instant::now() + probe::DEADLINE)?
        .map_or(1, |(row, _column)| row);
    let (rows, _columns) = term::window_size();
    push_scrollback(start_row, rows)?;

    // What the user typed while the query was in flight was read by the probe,
    // off a terminal that was already raw, and no second read will produce it
    // again. So it is answered here, before the first wait, by the same rule the
    // loop below applies: a Ctrl-D is a Ctrl-D whichever read happened to take
    // it. Everything else is discarded exactly as the loop discards it, because
    // there is nothing yet that a keystroke could mean.
    if cursor.take_deferred().contains(&END_OF_TRANSMISSION) {
        return Ok(());
    }

    // One byte of input, and the exit path. The poll loop, the band, and the
    // turn arrive in the later tasks of this plan; Ctrl-D leaving is the
    // shell's own contract and survives all of them.
    //
    // The wait is `signals::wait_for_input` rather than a bare blocking read,
    // and that is the whole of the stop contract: `SIGTSTP` is blocked
    // everywhere in this function except inside that call, which lets it in
    // atomically. So a stop can only happen where a resume can be answered --
    // the wait returns `Interrupted`, raw mode is entered again, and only then
    // does the next wait begin. A stop arriving during the `read` below stays
    // pending until the next wait, which is the same thing one instant later.
    let stdin = io::stdin();
    let mut byte = [0u8; 1];
    loop {
        let outcome = match signals::wait_for_input(stdin.as_fd(), &held) {
            Ok(()) => io::Read::read(&mut &stdin, &mut byte),
            Err(err) => Err(err),
        };
        match outcome {
            // End of input, and Ctrl-D: the shell's own contract is that
            // Ctrl-D leaves, and it survives every later task.
            Ok(0) => return Ok(()),
            Ok(_) if byte[0] == END_OF_TRANSMISSION => return Ok(()),
            Ok(_) => continue,
            // A signal, not a failure. The handlers are installed without
            // `SA_RESTART` precisely so that the wait and the read return
            // instead of swallowing the event, and a resume is the one event
            // Phase 1 has work for.
            Err(err) if err.kind() == io::ErrorKind::Interrupted => {
                if signals::take_resumed() {
                    resume(tmux)?;
                }
                continue;
            }
            Err(err) => return Err(err),
        }
    }
}

/// Ctrl-D. With `ISIG` cleared it is a byte like any other, and it is the one
/// byte Phase 1 acts on -- from whichever read took it.
const END_OF_TRANSMISSION: u8 = 0x04;

/// Scrolls the shell's output above the band into scrollback, on the screen.
///
/// The mechanics and the amount both live in [`probe::push`], against a writer
/// it is handed, so that what the push does to a screen is a test rather than a
/// claim; this is the two lines that hand it the real one. Nothing is erased
/// here -- what leaves the top of the screen goes into the terminal's own
/// scrollback, where the user can still reach it.
fn push_scrollback(cursor_row: u16, rows: u16) -> io::Result<()> {
    let mut out = io::stdout().lock();
    probe::push(&mut out, cursor_row, rows)
}

/// Announces the session on the wire.
///
/// The interactive mode set is the first byte a terminal sees from the TUI, and
/// the acceptance suite waits on part of it, so "the session is up" and "these
/// bytes were written" are the same event. One function, so the ordinary path
/// and the resume path cannot drift apart.
fn announce(tmux: bool) -> io::Result<()> {
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
    out.flush()
}

/// What a SIGCONT means, done on the UI thread where it is allowed to allocate.
///
/// Order matters: reinstall the handler first, because the window in which a
/// second `SIGTSTP` would hit the default disposition is open until it is
/// closed (`app_lifecycle.zig:609-620,646-656`). Doing it before raw mode is
/// re-entered is also what keeps that window from needing a mask of its own:
/// the terminal is cooked for the whole of it.
// Moved into `collect_facts` in Task 6, which is where every other fact the
// loop wakes up to is turned into work.
fn resume(tmux: bool) -> io::Result<()> {
    signals::install_tstp()?;
    let stdin = io::stdin();
    // Re-captured rather than remembered: the shell may have changed the
    // terminal while this process was stopped. The write-once original stays
    // what it was -- that is what a handler restores to.
    let current = term::capture(stdin.as_fd())?;
    term::enter_raw(stdin.as_fd(), &current)?;
    announce(tmux)
}

/// Reports a failure that stopped the TUI, on a terminal that is still cooked.
fn fail(message: &str) -> ExitCode {
    let _ = writeln!(io::stderr(), "xfx: {message}");
    ExitCode::FAILURE
}

#[cfg(feature = "fault-injection")]
mod fault;
mod panic;
mod probe;
mod signals;
mod term;
