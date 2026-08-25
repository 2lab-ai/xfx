//! One turn of the UI: wait, reconcile, read, reconcile, commit.
//!
//! The shape is upstream's (`event_loop.zig:76-122`) and every part of it is
//! there for a reason the next part depends on:
//!
//! * **Wait with the mask handed to the kernel.** The wait is
//!   [`signals::wait_for_input`], which is `pselect(2)` carrying the mask, and
//!   the [`TICK`] is expressed as its timeout rather than as a `poll`'s. The
//!   invariant that buys: *while the terminal is raw, `SIGTSTP` is either
//!   blocked or the process is inside that call.* A plain `poll` would unmask
//!   and then wait, and a stop delivered between the two would leave a session
//!   parked on a cooked terminal it believes is raw.
//! * **Reconcile before reading and again before painting.** This is the
//!   turn's ordering invariant and it is not decoration:
//!
//!   > No byte is read, and no frame is painted, on a terminal whose state has
//!   > not been reconciled since the last wait.
//!
//!   A `SIGTSTP` can only be delivered *inside* a wait -- the tick wait, or one
//!   of the burst's readiness checks -- and its handler hands the terminal back
//!   **cooked** before stopping the process. The `SIGCONT` that follows only
//!   sets a flag; taking the terminal again is [`collect_facts`]'s job, on this
//!   thread. So reconciling once at the top of a turn is not enough: a stop
//!   delivered in that turn's own wait would be answered only on the *next*
//!   turn, and this one would read from, and paint onto, a terminal the user's
//!   shell has had back since. Hence one reconcile after the wait and one after
//!   the burst -- unconditional, because "was this turn interrupted" is a
//!   second thing to keep true, and two extra atomics and a drained pipe per
//!   tick is not a price worth a second thing to keep true.
//!
//!   The invariant is the **compiler's** to keep, not a comment's:
//!   [`collect_facts`] is the only thing that makes a [`Reconciled`], reading
//!   and painting are the only things that take one, and each takes it by
//!   value. Deleting either reconcile does not produce a subtle repaint on a
//!   cooked terminal; it fails the build.
//! * **Read in a bounded burst.** At most [`READ_BURSTS`] reads of
//!   [`BURST_BYTES`], so a terminal delivering input faster than the band can
//!   be painted cannot starve the paint. Standard input is a *blocking*
//!   descriptor, so each read past the first is preceded by its own readability
//!   check -- a burst that simply read again would park the UI inside a read
//!   with the frame it owes unpainted.
//! * **Commit last, and only if something asked.** An idle tick writes no
//!   bytes at all. A frame the screen refused is owed again, and only up to
//!   [`FRAME_BUDGET`] of wall-clock time: past that the session leaves through
//!   the same restore every other failure travels through. The document
//!   appends the shell owes go out **first, inside the same commit**: an
//!   append scrolls the whole screen -- the band's own rows with it -- so a
//!   frame painted before one would be carried a row up and left there until
//!   something else asked for a repaint.
//!
//! The runtime's events are taken in the same turn, between the read and the
//! second reconcile: they are text, not terminal state, so they belong with
//! [`Shell::settle_input`] rather than inside [`collect_facts`] -- and keeping
//! them out of that function is what lets the exit's drain reconcile and paint
//! without racing itself for the same channel. Nothing wakes the wait for
//! them; the [`TICK`] is what bounds how long an arrived delta waits, which is
//! the same bound it puts on everything else the band shows.
//!
//! Leaving is [`worker::Worker::shutdown`] -- the drain protocol -- on **every**
//! exit path, because the alternative is a runtime thread still parked in a
//! `send().await` on a channel nobody will ever read again.

use std::io::{self, Write};
use std::os::fd::AsFd;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use tokio::sync::mpsc::Receiver;

use super::bridge::{self, UiEvent};
use super::frame::Band;
use super::render_request::Reason;
use super::shell::Shell;
use super::signals::{self, Held, Wakeup};
use super::worker::{self, Worker};

/// The fixed tick. A turn that nothing woke still comes round this often, which
/// is what bounds how stale the band can be.
pub(crate) const TICK: Duration = Duration::from_millis(8);

/// How many reads one turn may take before it paints, whatever is still
/// arriving.
pub(crate) const READ_BURSTS: usize = 32;

/// How much one of those reads may take.
pub(crate) const BURST_BYTES: usize = 128;

/// How long a screen may refuse every frame before the session gives up on it.
///
/// A screen that has taken nothing for half a second is either gone or so far
/// behind that a band is not what the user is missing, and a session that kept
/// handing the frame back to itself would spin forever on a terminal nobody can
/// see -- which is exactly what an unbounded retry hides. Past the budget the
/// failure leaves through `hold`, so `term::shutdown` still runs and the
/// process exits with the error rather than with a hang.
///
/// **Time, not a count of turns**, and the difference is not pedantry.
/// [`TICK`] is the *longest* a turn waits, not the shortest: readable input or
/// a poked wakeup makes a turn immediate, so a budget of N turns is N turns of
/// unknown duration and can be spent in a millisecond under load. The case the
/// budget exists to be generous *for* is a standard output somebody left
/// non-blocking -- a full pipe answers `EAGAIN`, which is backpressure rather
/// than a broken screen -- and "half a second of backpressure" is the sentence
/// that means what it says only if it is measured on a clock. Half a second is
/// also nothing on an exit path, so there is no cost on the other side.
pub(crate) const FRAME_BUDGET: Duration = Duration::from_millis(500);

/// What a frame costs a build that was asked to be too slow to keep up.
///
/// Five ticks, so a run of them is unambiguously slower than a producer that
/// is filling a 256-deep channel -- which is the state the drain protocol is
/// measured in.
#[cfg(feature = "fault-injection")]
const SLOW_FRAME: Duration = Duration::from_millis(40);

/// What the loop needs from the session that started it and cannot re-derive.
///
/// The `Held` is the proof that the stop signal is still blocked outside the
/// waits, and is what those waits hand to the kernel; the rest is the session's
/// own context that a resume and the first turn need.
pub(crate) struct Launch<'a> {
    pub(crate) wakeup: &'a Wakeup,
    pub(crate) held: &'a Held,
    pub(crate) tmux: bool,
    /// What the launch probe read off the terminal and did not use.
    ///
    /// Typed on a terminal that was already raw, and no second read will
    /// produce them: they go through the loop's own routing, in order, before
    /// its first wait.
    pub(crate) deferred: &'a [u8],
}

/// Runs the session until it leaves, and stops the runtime behind it.
///
/// The drain runs on **every** way out of [`session`], including a failure,
/// which is the whole reason the two are separate functions: a runtime thread
/// parked in `send().await` on a channel the UI has stopped reading is a thread
/// that never returns, and the exit that abandoned it would hang in the join
/// rather than in anything a reader could name.
///
/// Which failure is reported is decided here too, and the order is: the
/// session's own failure first -- it is the one that says why the session
/// ended -- then a screen that broke during the drain, then a runtime that
/// could not go on. Anything the drain produced after a session that had
/// already failed is a second symptom of the same thing.
pub(crate) fn run(
    shell: &mut Shell,
    band: &mut Band,
    launch: &Launch<'_>,
    worker: &mut Worker,
    events: &mut Receiver<UiEvent>,
) -> io::Result<ExitCode> {
    let mut failures = FrameFailures::default();
    let outcome = session(shell, band, launch, events, &mut failures);

    // A screen that already refused the session's last frame is not a screen to
    // keep painting into; the events are still applied, because one of them may
    // be the `Fatal` that explains everything.
    let painting = outcome.is_ok();
    let mut broken: Option<io::Error> = None;
    worker.shutdown(events, Instant::now() + worker::DRAIN_DEADLINE, |event| {
        let reconciled = if painting && broken.is_none() {
            match collect_facts(shell, launch) {
                Ok(token) => Some(token),
                Err(err) => {
                    broken = Some(err);
                    None
                }
            }
        } else {
            None
        };
        match reconciled {
            Some(token) => {
                if let Err(err) = drained(
                    shell,
                    band,
                    &mut io::stdout().lock(),
                    &mut failures,
                    Instant::now(),
                    token,
                    event,
                ) {
                    broken = Some(err);
                }
            }
            // Shown but not painted: the screen is gone or has already refused,
            // and the event may still be the `Fatal` that says why.
            None => shell.apply(event),
        }
    });

    let code = outcome?;
    if let Some(err) = broken {
        return Err(err);
    }
    match shell.fatal() {
        // Reported rather than painted, so it lands on a terminal `hold`'s
        // caller has already given back.
        Some(message) => Err(io::Error::other(message.to_string())),
        None => Ok(code),
    }
}

/// One session: wait, reconcile, read, reconcile, commit, until it leaves.
fn session(
    shell: &mut Shell,
    band: &mut Band,
    launch: &Launch<'_>,
    events: &mut Receiver<UiEvent>,
    failures: &mut FrameFailures,
) -> io::Result<ExitCode> {
    // The probe's leftovers are the session's first bytes, and they go into the
    // same decoder every later read does -- `Shell::route_bytes` is that
    // decoder's only door -- so a sequence the user typed across the report's
    // own read is decoded rather than scanned.
    shell.route_bytes(launch.deferred);
    if shell.leaving() {
        // A Ctrl-D that shared a read with the cursor report ends the session
        // before a band is drawn, which is why the exit has a row to clear from
        // only once one has been.
        return Ok(shell.exit_code());
    }

    let stdin = io::stdin();
    let mut buffer = [0u8; BURST_BYTES];
    loop {
        match signals::wait_for_input(
            &[stdin.as_fd(), launch.wakeup.read_fd()],
            Some(TICK),
            launch.held,
        ) {
            Ok(_) => {}
            // A signal, not a failure: the handlers carry no `SA_RESTART`
            // precisely so that a delivered signal ends the wait and the UI
            // thread gets a turn. What it *meant* is the next line's business,
            // and nothing may happen before it.
            Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
            Err(err) => return Err(err),
        }

        // Before a byte is read: a stop delivered in the wait above handed the
        // terminal back cooked, and reading a cooked terminal reads lines the
        // line discipline assembled rather than the keystrokes this session is
        // decoding.
        let reconciled = collect_facts(shell, launch)?;
        read_burst(shell, &stdin, &mut buffer, launch, reconciled)?;
        // What the bytes could not settle by themselves. A lone `ESC` is both a
        // key and the first byte of every sequence, so it is the Escape key only
        // once it has been alone for `input::Decoder::ESC_TIMEOUT`; asking here,
        // every turn, is what makes that a 50 ms wait rather than a wait for the
        // next keystroke. It reads no terminal and writes none, which is why it
        // is outside the two reconciles rather than between them.
        shell.settle_input(Instant::now());
        // What the runtime produced since the last turn. Text, not terminal
        // state, and therefore outside the reconciles for the same reason.
        take_ui_events(shell, events);
        // And what neither the bytes nor the events settled: how deep the
        // submission queue is now, and whether an armed Escape has gone stale.
        // Both are answers only the clock and the other thread produce, so like
        // `settle_input` this is what makes them a tick rather than a wait for
        // the next keystroke.
        shell.settle_band(Instant::now());
        // Before a frame is painted: the burst's own readiness checks are waits
        // too, so a stop could have landed inside one of them. The first token
        // was spent on the read, so this reconcile is not optional -- it is the
        // only way to have one.
        let reconciled = collect_facts(shell, launch)?;
        commit_frame(
            shell,
            band,
            &mut io::stdout().lock(),
            failures,
            Instant::now(),
            reconciled,
        )?;

        // The matrix row for a UI that cannot keep up with what it asked for.
        // A frame that costs this much is what fills the `UiEvent` channel and
        // parks the producer in `send().await`, which is the state the drain
        // protocol exists to get out of.
        #[cfg(feature = "fault-injection")]
        if super::fault::injected(super::fault::Fault::SlowUi) {
            std::thread::sleep(SLOW_FRAME);
        }

        if shell.leaving() {
            return Ok(shell.exit_code());
        }
    }
}

/// Takes what the runtime has produced, in order, and shows it.
///
/// Bounded at the channel's own depth rather than drained until empty: a
/// producer that can fill it faster than the band can be painted would
/// otherwise keep this turn from ever reaching its frame, and a UI that stopped
/// painting because it was busy being told things is the failure this bound
/// exists to prevent. What is left over is taken by the next turn, [`TICK`]
/// later at the latest.
fn take_ui_events(shell: &mut Shell, events: &mut Receiver<UiEvent>) {
    for _ in 0..bridge::UI_EVENTS {
        match events.try_recv() {
            Ok(event) => shell.apply(event),
            // Empty, or a runtime that is gone -- and a runtime that is gone
            // said so with a `Fatal` before its sender dropped, so there is
            // nothing here that has to be inferred from a closed channel.
            Err(_) => return,
        }
    }
}

/// What the drain does with one event it took: show it, and paint what showing
/// it owed.
///
/// **Both halves, and the second one is a contract rather than a courtesy.**
/// Draining alone would be enough to get a session *out* -- it is what frees
/// the producer's permits and lets a parked turn reach its own conclusion -- so
/// deleting the paint here leaves every acceptance case green while silently
/// dropping the tail of an answer and the sentence saying why the turn ended.
/// That is the whole of what a user gets to keep from a turn they interrupted,
/// and Phase 1 never repaints a document row, so what is not written here is
/// gone. Named, and taking its writer, so that is a thing a test can hold.
fn drained(
    shell: &mut Shell,
    band: &mut Band,
    out: &mut impl Write,
    failures: &mut FrameFailures,
    now: Instant,
    reconciled: Reconciled,
    event: UiEvent,
) -> io::Result<()> {
    shell.apply(event);
    commit_frame(shell, band, out, failures, now, reconciled)
}

/// Proof that the terminal's state has been reconciled since the last wait.
///
/// It exists to be a *token*, in the shape [`super::signals::Blocked`] uses for
/// the same kind of promise: [`collect_facts`] is the only thing that can make
/// one, reading and painting are the only things that take one, and each takes
/// it **by value**. So "nothing is read from, and nothing is painted onto, a
/// terminal whose state has not been reconciled since the last wait" is a fact
/// the compiler keeps rather than a comment the next edit can quietly falsify.
///
/// Not `Clone` and not `Copy`, which is the whole point: the second reconcile
/// of a turn cannot be satisfied with the first one's token.
#[must_use]
struct Reconciled;

/// Reads what the terminal has to give, bounded, and routes it.
///
/// `_reconciled` is consumed rather than read: a burst that ran on a terminal
/// the session had not taken back since the last wait would read whatever the
/// line discipline assembled while the user's shell had it.
fn read_burst(
    shell: &mut Shell,
    stdin: &io::Stdin,
    buffer: &mut [u8],
    launch: &Launch<'_>,
    _reconciled: Reconciled,
) -> io::Result<()> {
    for _ in 0..READ_BURSTS {
        // Standard input blocks, so "there was something a moment ago" is not a
        // licence to read again.
        match signals::wait_for_input(&[stdin.as_fd()], Some(Duration::ZERO), launch.held) {
            Ok(true) => {}
            Ok(false) => return Ok(()),
            // A signal landed inside this wait. The turn goes on to its second
            // reconcile, which is where that is answered.
            Err(err) if err.kind() == io::ErrorKind::Interrupted => return Ok(()),
            Err(err) => return Err(err),
        }
        match rustix::io::read(stdin, &mut *buffer) {
            // No writer left on the terminal. There is nothing more to read,
            // ever, and a session with no input is over.
            Ok(0) => {
                shell.leave();
                return Ok(());
            }
            Ok(read) => shell.route_bytes(&buffer[..read]),
            Err(rustix::io::Errno::INTR | rustix::io::Errno::WOULDBLOCK) => return Ok(()),
            Err(err) => return Err(err.into()),
        }
    }
    Ok(())
}

/// Everything that happened while the UI thread was not looking.
///
/// Three facts in this phase. The drain comes first because it is what makes
/// the next wait a wait: the flags below are the *meaning* of a poke, and the
/// byte is only its knock.
fn collect_facts(shell: &mut Shell, launch: &Launch<'_>) -> io::Result<Reconciled> {
    launch.wakeup.drain();

    if signals::take_resumed() {
        // The stop handler gave the terminal back and the shell has had it
        // since. Taking it again -- reinstalling the stop handler, re-entering
        // raw mode, re-announcing -- happens here, before this turn reads or
        // paints anything, and the band it drew is gone from a screen somebody
        // else has been writing to.
        super::resume(launch.tmux)?;
        shell.render.request(Reason::ExternalDamage);
    }

    if signals::take_winch() {
        // Phase 2 item 12 re-solves the layout here and repaints. This phase
        // takes the flag and acts on nothing -- `docs/parity.md` says so -- and
        // takes it rather than leaving it set so that the *launch* measurement,
        // which does act on it, cannot be answered by a resize this loop
        // already saw.
    }

    Ok(Reconciled)
}

/// Writes the document appends this turn owes, and then paints the frame it
/// owes, if it owes one.
///
/// A screen that refused one write is usually still there, so the reasons go
/// back and the next tick tries again -- but only until the run of failures has
/// lasted [`FRAME_BUDGET`]. Then the **first** error of the run is returned,
/// and it travels out through `hold` to `term::shutdown`: the terminal goes
/// back and the process exits with the error, rather than retrying invisibly
/// onto a screen nobody can see.
///
/// `now` is a parameter rather than an `Instant::now()` inside, for the same
/// reason [`super::render_request::RenderRequest::animation_due`]'s is: a
/// budget measured on the process's own clock could only be tested by sleeping
/// through it, and a test that sleeps for half a second is a test that is
/// either slow or flaky.
fn commit_frame(
    shell: &mut Shell,
    band: &mut Band,
    out: &mut impl Write,
    failures: &mut FrameFailures,
    now: Instant,
    _reconciled: Reconciled,
) -> io::Result<()> {
    // `/clear`, and before everything: the screen and its scrollback go, and
    // what is written after this write is written onto a blank terminal. The
    // shell has already dropped the appends it owed against the screen that is
    // being erased, so the loop's only job is the bytes.
    //
    // A refused clear is **not** owed again, for the reason a refused append is
    // not: the shell has already forgotten the rows this was going to erase, so
    // a second attempt would be aimed at a screen the session can no longer
    // describe. It counts against the same budget, which is what ends a session
    // on a screen that is really gone.
    if shell.take_clearing() {
        if let Err(err) = out
            .write_all(super::shell::CLEAR_SCREEN.as_bytes())
            .and_then(|()| out.flush())
        {
            return match failures.failed(err, now) {
                Some(fatal) => Err(fatal),
                None => Ok(()),
            };
        }
    }
    // Before the frame, and before the `begin` below, for two reasons. An
    // append scrolls the screen, so a band painted first ends up a row above
    // where it belongs. And the rows are the *document's*, not the band's: a
    // tick that had nothing to repaint would otherwise hold them forever.
    //
    // A refused append is **not** owed again. Its bytes are a scroll followed
    // by the rows it made room for, and a write that failed partway through one
    // may have moved the screen already: repeating it would put the rows in the
    // document twice, one of them below a blank row. The failure counts against
    // the same budget a frame's does, which is what ends a session on a screen
    // that is really gone.
    for append in shell.take_pending() {
        if let Err(err) = band.append_document(out, append.scroll, &append.rows, &shell.geometry) {
            return match failures.failed(err, now) {
                Some(fatal) => Err(fatal),
                None => Ok(()),
            };
        }
    }
    let Some(attempt) = shell.render.begin() else {
        return Ok(());
    };
    match band.commit(out, &shell.band_rows(), &shell.geometry, shell.cursor()) {
        Ok(()) => {
            failures.succeeded();
            Ok(())
        }
        Err(err) => {
            shell.render.restore(attempt);
            match failures.failed(err, now) {
                Some(fatal) => Err(fatal),
                None => Ok(()),
            }
        }
    }
}

/// A run of frames the screen would not take.
#[derive(Debug, Default)]
struct FrameFailures {
    /// When the run began, on the caller's clock. `None` while the screen is
    /// taking frames.
    began: Option<Instant>,
    /// The failure that began the run, which is the one worth reporting: a
    /// later `EBADF` on a descriptor the first `EIO` already lost says less
    /// about what went wrong.
    first: Option<io::Error>,
}

impl FrameFailures {
    /// A frame landed, so whatever was wrong is over and the budget is whole
    /// again.
    fn succeeded(&mut self) {
        self.began = None;
        self.first = None;
    }

    /// Records a failed frame. `Some` is the error the session must leave with.
    ///
    /// Every kind counts, and there is no carve-out for `Interrupted`, because
    /// there is nothing for one to catch: a frame is `write_all` followed by
    /// `flush`, and **both retry `Interrupted` themselves** -- `write_all`
    /// loops on it by contract, and every buffered writer's `flush_buf` does
    /// the same. A signal landing inside a frame's write therefore never
    /// reaches this function, and a branch for it would be a speculative one
    /// that no test could reach honestly.
    ///
    /// `WouldBlock` does count. A screen that is permanently full is
    /// indistinguishable from one that is gone, and the whole point of the
    /// budget is that neither can hide; [`FRAME_BUDGET`] is where the room for
    /// a screen that is merely behind is given.
    ///
    /// The *first* failure of a run never ends a session, whatever the clock
    /// says: it starts the budget rather than spending it.
    fn failed(&mut self, err: io::Error, now: Instant) -> Option<io::Error> {
        let began = *self.began.get_or_insert(now);
        if self.first.is_none() {
            self.first = Some(err);
        }
        if now.saturating_duration_since(began) < FRAME_BUDGET {
            return None;
        }
        self.first.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::BTreeMap;

    use crate::config::{Environment, RuntimeConfig};

    /// A screen that refuses every write, as Task 2's exit test has one.
    struct BrokenScreen;

    impl Write for BrokenScreen {
        fn write(&mut self, _bytes: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "the screen went away",
            ))
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "the screen went away",
            ))
        }
    }

    /// A screen that refuses `refusals` writes and then works.
    struct FlakyScreen {
        refusals: usize,
        kind: io::ErrorKind,
        written: Vec<u8>,
    }

    impl Write for FlakyScreen {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.refusals > 0 {
                self.refusals -= 1;
                return Err(io::Error::new(self.kind, "not now"));
            }
            self.written.extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// A shell, and the runtime end of the channels it submits through.
    ///
    /// The receivers are kept rather than dropped: dropping one closes its
    /// channel, and a shell whose runtime channel is closed is not the shell
    /// any of these cases mean to paint. Nothing here submits, so they are
    /// held and not read.
    struct Fixture {
        shell: Shell,
        _work: tokio::sync::mpsc::Receiver<crate::tui::bridge::TurnWork>,
        _control: tokio::sync::mpsc::UnboundedReceiver<crate::tui::bridge::TurnControl>,
    }

    impl std::ops::Deref for Fixture {
        type Target = Shell;

        fn deref(&self) -> &Shell {
            &self.shell
        }
    }

    impl std::ops::DerefMut for Fixture {
        fn deref_mut(&mut self) -> &mut Shell {
            &mut self.shell
        }
    }

    fn shell() -> Fixture {
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let config = RuntimeConfig::load_with(
            &Environment::new(Some(home.path().to_path_buf()), BTreeMap::new()),
            workspace.path(),
        )
        .expect("load a configuration");
        let (work, _work, _control) = crate::tui::worker::WorkHandle::detached();
        Fixture {
            shell: Shell::new(
                &config,
                crate::tui::layout::solve(24, 80, 1).expect("a band"),
                work,
            ),
            _work,
            _control,
        }
    }

    /// A screen that takes everything and remembers it.
    fn screen() -> FlakyScreen {
        FlakyScreen {
            refusals: 0,
            kind: io::ErrorKind::BrokenPipe,
            written: Vec::new(),
        }
    }

    #[test]
    fn an_event_the_drain_takes_reaches_the_screen_and_not_only_the_shell() {
        // The shutdown contract's second half. Draining alone is enough to
        // *exit*: it frees the producer's permits, which is what lets a parked
        // turn reach its conclusion, so a drain that received and painted
        // nothing leaves every pty case green -- and drops the tail of the
        // answer and the sentence saying why the turn ended. Phase 1 never
        // repaints a document row, so those bytes are the user's only copy.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = screen();

        drained(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            Instant::now(),
            Reconciled,
            UiEvent::Delta("TAIL-OF-THE-ANSWER".to_string()),
        )
        .expect("the screen took the frame");
        drained(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            Instant::now(),
            Reconciled,
            UiEvent::TurnEnded {
                failure: Some("WHY-THE-TURN-ENDED".to_string()),
            },
        )
        .expect("the screen took the frame");

        let written = String::from_utf8_lossy(&out.written);
        assert!(
            written.contains("TAIL-OF-THE-ANSWER"),
            "the drain took the answer's tail and never wrote it: {written:?}"
        );
        assert!(
            written.contains("WHY-THE-TURN-ENDED"),
            "the drain took the turn's conclusion and never wrote it: {written:?}"
        );
    }

    #[test]
    fn a_fatal_the_drain_takes_is_remembered_rather_than_written_into_the_band() {
        // The one event that is not a document row: it is for a cooked
        // terminal, and the caller prints it after the restore.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = screen();

        drained(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            Instant::now(),
            Reconciled,
            UiEvent::Fatal("A-TURN-CAME-APART".to_string()),
        )
        .expect("the screen took the frame");

        assert_eq!(shell.fatal(), Some("A-TURN-CAME-APART"));
        assert!(
            !String::from_utf8_lossy(&out.written).contains("A-TURN-CAME-APART"),
            "the fatal was painted into a band that is about to come down"
        );
    }

    fn at(start: Instant, millis: u64) -> Instant {
        start + Duration::from_millis(millis)
    }

    fn refused(what: io::ErrorKind) -> io::Error {
        io::Error::new(what, "refused")
    }

    #[test]
    fn a_screen_that_refuses_one_frame_is_asked_again_on_the_next_tick() {
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let start = Instant::now();
        let mut screen = FlakyScreen {
            refusals: 1,
            kind: io::ErrorKind::BrokenPipe,
            written: Vec::new(),
        };

        commit_frame(
            &mut shell,
            &mut band,
            &mut screen,
            &mut failures,
            start,
            Reconciled,
        )
        .expect("one refusal is not the end of a session");
        assert!(
            screen.written.is_empty(),
            "the refused frame was written anyway"
        );
        commit_frame(
            &mut shell,
            &mut band,
            &mut screen,
            &mut failures,
            at(start, 8),
            Reconciled,
        )
        .expect("the second attempt is the one that lands");
        assert!(
            String::from_utf8_lossy(&screen.written).contains("\u{1b}[22;1H"),
            "the frame the first tick owed was never painted: {:?}",
            String::from_utf8_lossy(&screen.written)
        );
    }

    #[test]
    fn a_screen_that_refuses_every_frame_ends_the_session_when_the_budget_runs_out() {
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let start = Instant::now();

        // Every tick of the budget, at the loop's own rate.
        let mut elapsed = 0;
        while elapsed < FRAME_BUDGET.as_millis() as u64 {
            commit_frame(
                &mut shell,
                &mut band,
                &mut BrokenScreen,
                &mut failures,
                at(start, elapsed),
                Reconciled,
            )
            .unwrap_or_else(|err| panic!("the session gave up after {elapsed} ms: {err}"));
            elapsed += TICK.as_millis() as u64;
        }
        let err = commit_frame(
            &mut shell,
            &mut band,
            &mut BrokenScreen,
            &mut failures,
            at(start, FRAME_BUDGET.as_millis() as u64),
            Reconciled,
        )
        .expect_err("a screen that refused every frame for the whole budget was retried anyway");
        assert_eq!(err.kind(), io::ErrorKind::BrokenPipe, "{err}");
    }

    #[test]
    fn a_burst_of_refusals_in_no_time_at_all_does_not_spend_the_budget() {
        // The reason the budget is a clock and not a count of turns: `TICK` is
        // the *longest* a turn waits, not the shortest. A terminal delivering
        // input, or a signal poking the wakeup pipe, makes turns immediate --
        // so a thousand of them can happen inside one millisecond, and a budget
        // of N turns would be gone before the backpressure it exists to
        // tolerate had a chance to clear.
        let mut failures = FrameFailures::default();
        let start = Instant::now();
        for _ in 0..10_000 {
            assert!(
                failures
                    .failed(refused(io::ErrorKind::WouldBlock), start)
                    .is_none(),
                "a burst of refusals inside one instant ended the session"
            );
        }
        // And the clock, not the count, is what ends it.
        assert!(failures
            .failed(refused(io::ErrorKind::WouldBlock), at(start, 499))
            .is_none());
        assert!(failures
            .failed(refused(io::ErrorKind::WouldBlock), at(start, 500))
            .is_some());
    }

    #[test]
    fn the_first_refusal_of_a_run_starts_the_budget_rather_than_spending_it() {
        // Whatever the clock says when it arrives: a session whose first frame
        // failed at some arbitrary process uptime must not exit on that frame.
        let mut failures = FrameFailures::default();
        let late = Instant::now() + Duration::from_secs(3_600);
        assert!(failures
            .failed(refused(io::ErrorKind::BrokenPipe), late)
            .is_none());
    }

    #[test]
    fn a_signal_that_lands_inside_a_frames_write_never_reaches_the_failure_policy() {
        // The reason `failed` has no `Interrupted` branch: `write_all` retries
        // that kind by contract, so a signal arriving mid-frame costs a second
        // `write` and nothing else. This pins the fact the policy rests on --
        // a frame built from a bare `write` instead would land here truncated,
        // and a policy that counted the interruption would be counting a frame
        // that was really on the screen.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut screen = FlakyScreen {
            refusals: 1,
            kind: io::ErrorKind::Interrupted,
            written: Vec::new(),
        };

        commit_frame(
            &mut shell,
            &mut band,
            &mut screen,
            &mut failures,
            Instant::now(),
            Reconciled,
        )
        .expect("an interrupted write is retried by the write itself");
        assert_eq!(
            String::from_utf8_lossy(&screen.written),
            String::from_utf8_lossy(&band.render(
                &shell.band_rows(),
                &shell.geometry,
                shell.cursor()
            )),
            "the interrupted frame reached the screen short"
        );
        assert!(
            failures.began.is_none(),
            "a frame that really landed started a failure budget"
        );
        assert!(
            shell.render.begin().is_none(),
            "a frame that landed was still owed"
        );
    }

    #[test]
    fn a_tick_with_nothing_to_draw_writes_nothing_at_all() {
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let start = Instant::now();
        let mut screen = FlakyScreen {
            refusals: 0,
            kind: io::ErrorKind::BrokenPipe,
            written: Vec::new(),
        };
        commit_frame(
            &mut shell,
            &mut band,
            &mut screen,
            &mut failures,
            start,
            Reconciled,
        )
        .expect("the first frame");
        let after_first = screen.written.len();
        commit_frame(
            &mut shell,
            &mut band,
            &mut screen,
            &mut failures,
            at(start, 8),
            Reconciled,
        )
        .expect("an idle tick");
        assert_eq!(
            screen.written.len(),
            after_first,
            "an idle tick repainted the band"
        );
    }

    #[test]
    fn the_document_append_is_written_before_the_frame_it_scrolls() {
        // The append moves the whole screen up, the band's own rows included.
        // A frame written first is a band drawn on rows the scroll then takes,
        // and nothing repaints it until something else changes.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut screen = FlakyScreen {
            refusals: 0,
            kind: io::ErrorKind::BrokenPipe,
            written: Vec::new(),
        };
        shell.write_transcript("answered");

        commit_frame(
            &mut shell,
            &mut band,
            &mut screen,
            &mut failures,
            Instant::now(),
            Reconciled,
        )
        .expect("the append and the frame");

        let text = String::from_utf8(screen.written).expect("utf-8");
        assert!(
            text.starts_with("\u{1b}[24;1H\n\u{1b}[21;1Hanswered"),
            "the append did not scroll the screen and place the row: {text:?}"
        );
        let appended = text.find("answered").expect("the appended row");
        let frame = text.find("\u{1b}[?2026h").expect("the frame");
        assert!(
            appended < frame,
            "the band was painted before the append that scrolls it: {text:?}"
        );
        assert!(
            text[frame..].contains("\u{1b}[22;1H"),
            "the band was never repainted onto the rows the scroll left it: {text:?}"
        );
    }

    #[test]
    fn an_append_the_screen_refused_is_not_written_a_second_time() {
        // A scroll cannot be replayed: the write that failed may have moved the
        // screen before it did, and a second one would put the row in the
        // document twice with a blank row between. The frame is still owed,
        // because a repaint of the band is the one write that *is* idempotent.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let start = Instant::now();
        let mut screen = FlakyScreen {
            refusals: 1,
            kind: io::ErrorKind::BrokenPipe,
            written: Vec::new(),
        };
        shell.write_transcript("answered");

        commit_frame(
            &mut shell,
            &mut band,
            &mut screen,
            &mut failures,
            start,
            Reconciled,
        )
        .expect("one refusal is not the end of a session");
        assert!(screen.written.is_empty());

        commit_frame(
            &mut shell,
            &mut band,
            &mut screen,
            &mut failures,
            at(start, 8),
            Reconciled,
        )
        .expect("the next tick");
        let text = String::from_utf8(screen.written).expect("utf-8");
        assert!(
            !text.contains("answered"),
            "the refused append was replayed onto a screen that may already \
             have taken it: {text:?}"
        );
        assert!(
            text.contains("\u{1b}[22;1H"),
            "the frame the refused append asked for was never painted: {text:?}"
        );
    }

    #[test]
    fn the_error_a_session_leaves_with_is_the_one_that_started_the_run() {
        // A later failure describes a descriptor the first one already lost.
        let mut failures = FrameFailures::default();
        let start = Instant::now();
        assert!(failures
            .failed(refused(io::ErrorKind::PermissionDenied), start)
            .is_none());
        assert!(failures
            .failed(refused(io::ErrorKind::BrokenPipe), at(start, 499))
            .is_none());
        let fatal = failures
            .failed(refused(io::ErrorKind::BrokenPipe), at(start, 500))
            .expect("the budget ran out");
        assert_eq!(fatal.kind(), io::ErrorKind::PermissionDenied, "{fatal}");
    }

    #[test]
    fn a_frame_that_lands_gives_the_whole_budget_back() {
        // Otherwise a screen that hiccuped for four hundred milliseconds, then
        // worked for an hour, then hiccuped again would end a session that was
        // working the whole time.
        let mut failures = FrameFailures::default();
        let start = Instant::now();
        assert!(failures
            .failed(refused(io::ErrorKind::BrokenPipe), start)
            .is_none());
        assert!(failures
            .failed(refused(io::ErrorKind::BrokenPipe), at(start, 400))
            .is_none());
        failures.succeeded();

        assert!(
            failures
                .failed(refused(io::ErrorKind::BrokenPipe), at(start, 800))
                .is_none(),
            "the budget was not given back by the frame that landed"
        );
        assert!(
            failures
                .failed(refused(io::ErrorKind::BrokenPipe), at(start, 1_299))
                .is_none(),
            "the second run was measured from the first run's start"
        );
        assert!(failures
            .failed(refused(io::ErrorKind::BrokenPipe), at(start, 1_300))
            .is_some());
    }
}
