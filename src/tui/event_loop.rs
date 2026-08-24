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

use std::io::{self, Write};
use std::os::fd::AsFd;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use super::frame::Band;
use super::render_request::Reason;
use super::shell::Shell;
use super::signals::{self, Held, Wakeup};

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

/// Runs the session until it leaves.
pub(crate) fn run(shell: &mut Shell, band: &mut Band, launch: &Launch<'_>) -> io::Result<ExitCode> {
    // The probe's leftovers are the session's first bytes, and they go into the
    // same decoder every later read does -- `Shell::route_bytes` is that
    // decoder's only door -- so a sequence the user typed across the report's
    // own read is decoded rather than scanned.
    shell.route_bytes(launch.deferred);
    if shell.leaving() {
        // A Ctrl-D that shared a read with the cursor report ends the session
        // before a band is drawn, which is why the exit has a row to clear from
        // only once one has been.
        return Ok(ExitCode::SUCCESS);
    }

    let stdin = io::stdin();
    let mut buffer = [0u8; BURST_BYTES];
    let mut failures = FrameFailures::default();
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
        // Before a frame is painted: the burst's own readiness checks are waits
        // too, so a stop could have landed inside one of them. The first token
        // was spent on the read, so this reconcile is not optional -- it is the
        // only way to have one.
        let reconciled = collect_facts(shell, launch)?;
        commit_frame(
            shell,
            band,
            &mut io::stdout().lock(),
            &mut failures,
            Instant::now(),
            reconciled,
        )?;

        if shell.leaving() {
            return Ok(ExitCode::SUCCESS);
        }
    }
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

    fn shell() -> Shell {
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let config = RuntimeConfig::load_with(
            &Environment::new(Some(home.path().to_path_buf()), BTreeMap::new()),
            workspace.path(),
        )
        .expect("load a configuration");
        Shell::new(
            &config,
            crate::tui::layout::solve(24, 80, 1).expect("a band"),
        )
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
