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
//!
//! What the session then owns is a **band**: a divider, the composer, and a
//! hint row at the bottom of the screen ([`layout`]), repainted whole inside one
//! synchronized frame ([`frame`]) whenever something asks for one
//! ([`render_request`]), by a loop that waits on a fixed tick ([`event_loop`]).
//! Everything above the divider stays the terminal's own document.
//!
//! The screen those rows are measured against is standard **output**'s, because
//! that is where they are drawn; the line discipline is standard input's,
//! because that is where input arrives. [`term::window_size`] carries the whole
//! of that ruling, including what a session whose two ends are different
//! terminals gets instead of a cursor report.

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
fn session(config: &crate::config::RuntimeConfig) -> io::Result<ExitCode> {
    // The matrix row for a startup that fails before the terminal is touched:
    // nothing has been captured, nothing has been set, and the exit must write
    // no restore bytes for state it never took.
    #[cfg(feature = "fault-injection")]
    if fault::injected(fault::Fault::BeforeRaw) {
        return Err(io::Error::other("the session store could not be opened"));
    }

    // A screen too small for a band is refused **before** the terminal is
    // touched, and that is the whole of the ordering argument: a refusal
    // discovered after raw mode would have to give a terminal back that it had
    // no reason to take, and a band painted onto a screen that cannot hold one
    // writes over the user's shell output and then clears it on the way out.
    let (rows, columns) = term::window_size();
    if layout::solve(rows, columns, layout::INITIAL_INPUT_ROWS).is_none() {
        return Err(io::Error::other(too_small(rows, columns)));
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

    // The band is made here rather than in `hold` for one reason: the row the
    // exit clears from is the top of what the band **painted**, and the band is
    // the only thing that knows whether it painted anything. A session that
    // left before its first frame -- a Ctrl-D that shared a read with the
    // cursor report, or a startup that failed on this side of raw mode -- hands
    // back `None`, and the exit moves no cursor and erases nothing, exactly as
    // it did before there was a band at all. Made before the failure paths
    // below so that every one of them asks the same question.
    let mut band = frame::Band::new();

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
        term::shutdown(band.painted_top())?;
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
    let held = hold(config, tmux, &wakeup, blocked, &mut band);
    let restored = term::shutdown(band.painted_top());
    // The terminal is back, so the signals go back too -- before `wakeup` is
    // dropped, because the handlers were handed its write end and outlive it.
    signals::release();
    // The first failure wins, and the restore was attempted either way: a
    // screen error that happened while xfx still had the terminal is the one
    // worth reporting, and a terminal left raw is worse than either.
    held.and_then(|code| restored.map(|()| code))
}

/// Why a screen cannot hold a band, in the words the refusal is reported in.
fn too_small(rows: u16, columns: u16) -> String {
    let (min_rows, min_columns) = (layout::MIN_ROWS, layout::MIN_COLS);
    format!(
        "this terminal is {rows}x{columns}, which is too small for xfx's band; \
         it needs at least {min_rows} rows and {min_columns} columns"
    )
}

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
fn hold(
    config: &crate::config::RuntimeConfig,
    tmux: bool,
    wakeup: &signals::Wakeup,
    blocked: signals::Blocked,
    band: &mut frame::Band,
) -> io::Result<ExitCode> {
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
    // pushed above the band, and how big the screen is decides where the band
    // goes. Both are measured here, after the mode set and before anything is
    // drawn, for the same reason the handler installation is: they read and
    // write, either can fail, and a failure inside `hold` travels back through
    // the caller's restore instead of escaping above it with the terminal still
    // raw.
    let mut cursor = probe::CursorProbe::new();
    let screen = settle_screen(
        || {
            // A terminal that does not answer within the deadline is treated as
            // row 1 -- xfx starts at the bottom of what it can prove rather
            // than painting over something it cannot see.
            let cursor_row = cursor
                .read_reply(Instant::now() + probe::DEADLINE)?
                .map_or(UNKNOWN_CURSOR_ROW, |(row, _column)| row);
            let (rows, columns) = term::window_size();
            Ok(Screen {
                cursor_row,
                rows,
                columns,
            })
        },
        |screen| push_scrollback(screen.cursor_row, screen.rows),
        signals::take_winch,
    )?;

    // Solved from the screen the push settled on rather than from the one the
    // caller refused on: those are the same numbers on every ordinary launch,
    // and a resize in between is exactly the case `settle_screen` exists to
    // answer.
    let geometry = layout::solve(screen.rows, screen.columns, layout::INITIAL_INPUT_ROWS)
        .ok_or_else(|| io::Error::other(too_small(screen.rows, screen.columns)))?;

    // What the user typed while the query was in flight was read by the probe,
    // off a terminal that was already raw, and no second read will produce it
    // again. It is handed to the loop, which routes it exactly as it routes
    // anything read afterwards: a Ctrl-D is a Ctrl-D whichever read happened to
    // take it.
    let deferred = cursor.take_deferred();
    let mut shell = shell::Shell::new(config, geometry);
    event_loop::run(
        &mut shell,
        band,
        &event_loop::Launch {
            wakeup,
            held: &held,
            tmux,
            deferred: &deferred,
        },
    )
}

/// The one measurement a launch is computed from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Screen {
    /// The row the shell left the cursor on, one-based.
    cursor_row: u16,
    rows: u16,
    columns: u16,
}

/// How many times a launch will measure and push before it gives up on
/// settling.
const MEASUREMENTS: usize = 3;

/// The cursor row a launch uses when it cannot know where the cursor is.
///
/// The top of the screen, because it is the reading that claims the least:
/// nothing is above row 1, so nothing is pushed and nothing is scrolled on a
/// guess. It is what a terminal that never answers `CSI 6n` gets, and what a
/// launch that could not be measured without a resize landing in the middle of
/// it gets too.
const UNKNOWN_CURSOR_ROW: u16 = 1;

/// Measures the screen and pushes the shell's output above the band into
/// scrollback, **as one transaction**.
///
/// The launch reads two numbers -- where the cursor is, and how big the screen
/// is -- and computes the scrollback push and the band's geometry from both. A
/// `SIGWINCH` invalidates both: the terminal reflows, the cursor moves, and a
/// push aimed at the bottom row of the screen that *was* scrolls the wrong
/// amount of the screen that *is*. Nothing else re-runs the push -- this phase
/// does not re-layout on resize at all, and the event loop takes the flag and
/// drops it -- so the whole of that race has to be closed here.
///
/// Closing it means the validation has to cover the **push**, not just the
/// measurement, and it has to cover *every* push including the last:
///
/// * A resize *during the measurement* is the cheap case. Nothing has been
///   written, so there is nothing to compensate for: measure again.
/// * A resize *during a push* is the one a measurement-only check misses. The
///   push cannot be taken back -- those rows are in the terminal's scrollback
///   now -- but it can be **repeated**, and repeating it is safe in the only
///   direction that matters: everything above the old cursor has already left
///   the screen, so a second push moves blank rows. Pushing more than was there
///   costs blank rows; pushing less opens the band on top of the shell's own
///   output.
/// * The **last** attempt gets the reading that assumes the worst -- the cursor
///   treated as being on the bottom row, so the whole screen goes into
///   scrollback -- and is still post-checked like every other one. A final push
///   that returned unchecked would hand back a geometry the flag had already
///   invalidated, and the event loop would then take that flag and drop it: the
///   resize would be neither compensated for nor visible to anything else.
///
/// Every turn reads the flag with `take_winch`, so a resize this function
/// compensates for is **consumed** here and cannot be seen again and dropped as
/// if nothing had been done about it.
///
/// **When even the last attempt is invalidated, the launch claims nothing.** It
/// takes one fresh measurement for the geometry, consumes the flag with it, and
/// reports [`UNKNOWN_CURSOR_ROW`] -- pushing nothing further, because every
/// number it has about what is on that screen has been contradicted. That is
/// the one path on which xfx may open its band over rows the shell wrote, and
/// it is still the safe side of the trade: a push computed from numbers already
/// known to be wrong scrolls the wrong amount, which can cover the same rows
/// *and* tear the alignment of everything it did move. It takes a terminal
/// being resized continuously through the whole launch to reach, and the next
/// launch measures a screen that has stopped moving.
///
/// The sampler, the push, and the flag are parameters because a `SIGWINCH`
/// cannot honestly be made to land inside a real hundred-millisecond launch
/// window on demand. The ordering is therefore proven against injected ones
/// (`tests` below); the real screen, the real push, and the real flag are wired
/// in by the three lines in `hold` above, and nothing else calls this.
fn settle_screen(
    mut sample: impl FnMut() -> io::Result<Screen>,
    mut push: impl FnMut(&Screen) -> io::Result<()>,
    mut resized: impl FnMut() -> bool,
) -> io::Result<Screen> {
    // Whatever the flag remembers from before the first sample is about a
    // screen this call is about to measure for itself.
    resized();
    for attempt in 1..=MEASUREMENTS {
        let mut screen = sample()?;
        if resized() {
            // It changed while it was being measured. Nothing has been written
            // yet, so there is nothing to compensate for: measure again.
            continue;
        }
        if attempt == MEASUREMENTS {
            // The last word xfx gets, so it is the one that assumes the least
            // about what survived.
            screen.cursor_row = screen.rows;
        }
        push(&screen)?;
        if !resized() {
            return Ok(screen);
        }
        // It changed while the push was on the wire. Nothing about this screen
        // can be claimed any more, so the next attempt starts from a fresh one.
    }
    let mut screen = sample()?;
    resized();
    screen.cursor_row = UNKNOWN_CURSOR_ROW;
    Ok(screen)
}

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
// Called from `event_loop::collect_facts`, which is where every fact the loop
// wakes up to is turned into work, and once from `hold` for a session that was
// stopped before it ever waited.
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

mod editor;
mod event_loop;
#[cfg(feature = "fault-injection")]
mod fault;
mod frame;
mod input;
mod layout;
mod panic;
mod probe;
mod render_request;
mod shell;
mod signals;
mod term;
mod transcript;
mod wrap;

#[cfg(test)]
mod tests {
    use super::*;

    use std::cell::{Cell, RefCell};

    /// What the launch did, in order.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Step {
        Measured(Screen),
        Pushed(Screen),
    }

    /// A screen that answers with a scripted set of samples and a scripted set
    /// of resize answers, and records every measurement and every push.
    ///
    /// The order is the whole property: a push that ran against a sample the
    /// flag had already invalidated, and a push that was never re-checked after
    /// the flag went up underneath it, both produce a `Screen` and differ only
    /// in what they did to the terminal on the way.
    struct Launch {
        samples: Vec<Screen>,
        taken: Cell<usize>,
        /// One answer per `resized()` call, in order. The first is the clearing
        /// read `settle_screen` makes before its first sample.
        resizes: Vec<bool>,
        asked: Cell<usize>,
        steps: RefCell<Vec<Step>>,
    }

    impl Launch {
        fn new(samples: &[Screen], resizes: &[bool]) -> Self {
            Self {
                samples: samples.to_vec(),
                taken: Cell::new(0),
                resizes: resizes.to_vec(),
                asked: Cell::new(0),
                steps: RefCell::new(Vec::new()),
            }
        }

        fn measure(&self) -> io::Result<Screen> {
            let taken = self.taken.get();
            self.taken.set(taken + 1);
            let screen = *self
                .samples
                .get(taken)
                .unwrap_or_else(|| panic!("the launch took {} samples", taken + 1));
            self.steps.borrow_mut().push(Step::Measured(screen));
            Ok(screen)
        }

        fn push(&self, screen: &Screen) -> io::Result<()> {
            self.steps.borrow_mut().push(Step::Pushed(*screen));
            Ok(())
        }

        fn resized(&self) -> bool {
            let asked = self.asked.get();
            self.asked.set(asked + 1);
            *self
                .resizes
                .get(asked)
                .unwrap_or_else(|| panic!("the launch read the resize flag {} times", asked + 1))
        }

        fn settle(&self) -> io::Result<Screen> {
            settle_screen(
                || self.measure(),
                |screen| self.push(screen),
                || self.resized(),
            )
        }

        fn steps(&self) -> Vec<Step> {
            self.steps.borrow().clone()
        }

        fn pushes(&self) -> Vec<Screen> {
            self.steps()
                .into_iter()
                .filter_map(|step| match step {
                    Step::Pushed(screen) => Some(screen),
                    Step::Measured(_) => None,
                })
                .collect()
        }

        /// Requires that the whole script was used.
        ///
        /// Not tidiness: a trailing answer nobody read is a flag nobody
        /// checked, which is exactly the defect these cases exist to pin. The
        /// fake panics loudly when the script runs *short*, so between the two
        /// the number of reads is nailed from both sides.
        fn assert_script_spent(&self) {
            assert_eq!(
                self.asked.get(),
                self.resizes.len(),
                "the launch left {} resize answer(s) unread, so a flag it should \
                 have checked went unchecked",
                self.resizes.len() - self.asked.get()
            );
            assert_eq!(
                self.taken.get(),
                self.samples.len(),
                "the launch left a scripted sample unused"
            );
        }
    }

    fn screen(cursor_row: u16, rows: u16) -> Screen {
        Screen {
            cursor_row,
            rows,
            columns: 80,
        }
    }

    #[test]
    fn a_quiet_launch_measures_once_and_pushes_once() {
        let launch = Launch::new(&[screen(7, 24)], &[false, false, false]);
        assert_eq!(launch.settle().expect("settle"), screen(7, 24));
        assert_eq!(
            launch.steps(),
            vec![Step::Measured(screen(7, 24)), Step::Pushed(screen(7, 24))]
        );
        launch.assert_script_spent();
    }

    #[test]
    fn a_resize_during_the_measurement_is_answered_before_anything_is_pushed() {
        // The cheap half of the race: the numbers describe a screen that no
        // longer exists, and nothing has been written yet, so the stale sample
        // must never reach the push at all.
        let launch = Launch::new(
            &[screen(7, 24), screen(11, 40)],
            &[false, true, false, false],
        );
        assert_eq!(launch.settle().expect("settle"), screen(11, 40));
        assert_eq!(
            launch.steps(),
            vec![
                Step::Measured(screen(7, 24)),
                Step::Measured(screen(11, 40)),
                Step::Pushed(screen(11, 40)),
            ],
            "the launch pushed against a screen it had already been told was gone"
        );
        launch.assert_script_spent();
    }

    #[test]
    fn a_resize_during_the_push_is_compensated_for_by_pushing_again() {
        // The half a measurement-only check misses. The push is on the wire
        // when the terminal reflows: it cannot be taken back, so it is redone
        // against the screen that exists, and the geometry the caller gets is
        // the one that last push agrees with.
        let launch = Launch::new(
            &[screen(7, 24), screen(11, 40)],
            &[false, false, true, false, false],
        );
        assert_eq!(launch.settle().expect("settle"), screen(11, 40));
        assert_eq!(
            launch.steps(),
            vec![
                Step::Measured(screen(7, 24)),
                Step::Pushed(screen(7, 24)),
                Step::Measured(screen(11, 40)),
                Step::Pushed(screen(11, 40)),
            ],
            "a resize that landed while the push was on the wire was never made good"
        );
        launch.assert_script_spent();
    }

    #[test]
    fn the_last_attempt_pushes_the_whole_screen_and_is_checked_like_the_others() {
        // Two attempts lost to resizes during their pushes, and a third that
        // settles. The third gets the reading that assumes the worst -- the
        // cursor on the bottom row, so the whole screen goes to scrollback --
        // and is still post-checked, which is what lets it return at all.
        let launch = Launch::new(
            &[screen(7, 24), screen(11, 40), screen(3, 50)],
            &[false, false, true, false, true, false, false],
        );
        let settled = launch.settle().expect("settle");
        assert_eq!(settled, screen(50, 50), "the last attempt guessed");
        assert_eq!(
            launch.pushes(),
            vec![screen(7, 24), screen(11, 40), screen(50, 50)]
        );
        launch.assert_script_spent();
    }

    #[test]
    fn a_resize_during_the_final_push_is_never_returned_as_a_settled_screen() {
        // The case an unchecked final push hides. All three attempts are lost,
        // the third while its whole-screen push is on the wire -- so the
        // geometry that push was computed from is already contradicted, and
        // returning it would hand the band numbers the flag has invalidated
        // *and* leave the loop to take that flag and drop it.
        let launch = Launch::new(
            &[screen(7, 24), screen(11, 40), screen(3, 50), screen(9, 30)],
            &[false, false, true, false, true, false, true, false],
        );
        let settled = launch.settle().expect("settle");
        assert_eq!(
            (settled.rows, settled.columns),
            (30, 80),
            "the launch returned the screen its last push was aimed at, which \
             the flag had already contradicted"
        );
        assert_eq!(
            settled.cursor_row, UNKNOWN_CURSOR_ROW,
            "a launch that could not settle still claimed to know what was \
             above the cursor"
        );
        assert_eq!(
            launch.pushes().last().copied(),
            Some(screen(50, 50)),
            "nothing may be pushed against a screen measured after every \
             number about it was contradicted"
        );
        launch.assert_script_spent();
    }

    #[test]
    fn a_launch_that_can_never_be_measured_pushes_nothing_and_claims_nothing() {
        // A window being dragged for the whole launch: every measurement is
        // contradicted before anything can be written against it, so nothing
        // ever is. The band opens on the freshest geometry there is, and the
        // record says the cursor row is unknown rather than inventing one.
        let launch = Launch::new(
            &[screen(7, 24), screen(11, 40), screen(3, 50), screen(9, 30)],
            &[true, true, true, true, true],
        );
        let settled = launch.settle().expect("settle");
        assert_eq!((settled.rows, settled.columns), (30, 80));
        assert_eq!(settled.cursor_row, UNKNOWN_CURSOR_ROW);
        assert!(
            launch.pushes().is_empty(),
            "the launch scrolled a screen it had no valid measurement of: {:?}",
            launch.steps()
        );
        launch.assert_script_spent();
    }

    #[test]
    fn the_geometry_a_launch_returns_is_always_its_most_recent_measurement() {
        // The property behind every case above, asserted as one: whatever the
        // flag does, the band is solved from the last screen the launch
        // measured -- never from an older one the flag has since contradicted.
        // And whenever the launch pushed at all, its last push agrees with what
        // it returned, unless it is the degraded reading that pushes nothing.
        for resizes in [
            vec![false, false, false],
            vec![false, true, false, false],
            vec![false, false, true, false, false],
            vec![true, false, false, false],
            vec![false, false, true, false, true, false, false],
            vec![false, false, true, false, true, false, true, false],
            vec![true, true, true, true, true],
        ] {
            let launch = Launch::new(
                &[screen(7, 24), screen(11, 40), screen(3, 50), screen(9, 30)],
                &resizes,
            );
            let settled = launch.settle().expect("settle");
            let last_measured = launch
                .steps()
                .into_iter()
                .filter_map(|step| match step {
                    Step::Measured(screen) => Some(screen),
                    Step::Pushed(_) => None,
                })
                .next_back()
                .expect("a launch measures at least once");
            assert_eq!(
                (settled.rows, settled.columns),
                (last_measured.rows, last_measured.columns),
                "with resizes {resizes:?} the band would be solved from a screen \
                 the launch had already re-measured"
            );
            if settled.cursor_row != UNKNOWN_CURSOR_ROW {
                assert_eq!(
                    launch.pushes().last().copied(),
                    Some(settled),
                    "with resizes {resizes:?} the band would open against a \
                     screen the push never scrolled"
                );
            }
        }
    }

    #[test]
    fn a_resize_that_arrived_before_the_launch_is_not_charged_to_it() {
        // A `SIGWINCH` delivered between the handler installation and the first
        // sample set the flag for a screen the sample below measures for
        // itself. Reading it as an interruption would cost a second probe --
        // and a hundred milliseconds -- on a perfectly quiet launch.
        let launch = Launch::new(&[screen(7, 24)], &[true, false, false]);
        assert_eq!(launch.settle().expect("settle"), screen(7, 24));
        launch.assert_script_spent();
    }

    #[test]
    fn a_terminal_that_cannot_be_measured_is_reported_rather_than_guessed_at() {
        let failed = settle_screen(
            || {
                Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "the screen went away",
                ))
            },
            |_| Ok(()),
            || false,
        )
        .expect_err("a broken descriptor is not a screen size");
        assert_eq!(failed.kind(), io::ErrorKind::BrokenPipe);
    }

    #[test]
    fn a_push_that_cannot_be_written_is_reported_rather_than_drawn_over() {
        let failed = settle_screen(
            || Ok(screen(7, 24)),
            |_| {
                Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "the screen went away",
                ))
            },
            || false,
        )
        .expect_err("a push that never reached the terminal is not a settled screen");
        assert_eq!(failed.kind(), io::ErrorKind::BrokenPipe);
    }

    #[test]
    fn a_screen_too_small_for_a_band_is_refused_by_name_and_by_number() {
        // The message a user reads when xfx will not start. It has to say what
        // the terminal is as well as what xfx needs -- "too small" alone leaves
        // them resizing by trial.
        let refusal = too_small(4, 80);
        assert!(refusal.contains("4x80"), "{refusal}");
        assert!(
            refusal.contains(&layout::MIN_ROWS.to_string())
                && refusal.contains(&layout::MIN_COLS.to_string()),
            "the refusal does not say what would be big enough: {refusal}"
        );
    }
}
