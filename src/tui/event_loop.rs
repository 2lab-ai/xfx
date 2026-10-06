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

use std::io;
use std::os::fd::AsFd;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use tokio::sync::mpsc::Receiver;

use super::approval_readiness::Outcome;
use super::bridge::{self, UiEvent};
use super::deliver::{Emit, RawTty, Sink};
use super::frame::{Band, Commit, Retint};
use super::render_request::Reason;
use super::shell::{Resize, Shell};
use super::signals::{self, Held, Wakeup};
use super::transcript::Landed;
use super::worker::{self, Worker};

/// The fixed tick. A turn that nothing woke still comes round this often, which
/// is what bounds how stale the band can be.
pub(crate) const TICK: Duration = Duration::from_millis(8);

/// How many reads one turn may take before it paints, whatever is still
/// arriving.
pub(crate) const READ_BURSTS: usize = 32;

/// How much one of those reads may take.
pub(crate) const BURST_BYTES: usize = 128;

/// How much answer text may be waiting to be released before the loop stops
/// taking more of it off the channel.
///
/// The one number that bounds `super::pacer::Pacer`'s queue, and it is here
/// rather than there because backpressure is the loop's to apply: what stops
/// the queue growing is the channel filling, and only the loop decides whether
/// to empty the channel.
///
/// Sized against the ceiling rather than against a screen: `pacer::MAX_CPS` is
/// 5000 bytes a second, so this is about thirteen seconds of reading already in
/// hand. Past that the provider is producing faster than anybody can read it
/// and the right thing is to stop taking it, not to buffer a megabyte for a
/// band that shows one row at a time.
///
/// # What the bound really is
///
/// **`PACED_BACKLOG + bridge::DELTA_SLICE`**, and it is a number rather than a
/// hope. Two halves make it finite and neither is enough alone:
///
/// * This one stops the loop taking **the next** event once the mark is
///   reached, asked between events rather than once per batch. Checking per
///   batch was worth up to [`bridge::UI_EVENTS`] deltas of overshoot; checking
///   per event is worth one event.
/// * `bridge::DELTA_SLICE` is what makes "one event" a size. A `UiEvent` is
///   indivisible once it is off the channel -- `Shell::apply` cannot refuse
///   what it has already been handed -- and nothing downstream promises a delta
///   is small, so the division happens at the **ingress**, where the producer
///   can still await a permit between pieces.
///
/// Nothing is dropped to achieve it, which is the constraint the whole
/// arrangement is shaped by: dropping the tail of an answer to respect a
/// buffering number would be the one failure this module is built to prevent.
/// The peak is asserted rather than asserted-about, in
/// `the_queue_never_grows_past_the_bound_however_the_answer_arrives`.
///
/// The one input it is not exact for is a text whose first **indivisible** unit
/// -- one grapheme cluster, one escape sequence -- is itself larger than a
/// piece, which `bridge::slices` hands over whole rather than cutting in half
/// or hanging on. It documents that ruling and why the corner is degenerate.
pub(crate) const PACED_BACKLOG: usize = 64 * 1024;

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

    // Everything an exit owes, in one call, with the two things this function
    // has that a test cannot make -- a terminal to reconcile and a runtime to
    // drain -- passed in as what they are.
    let broken = shut_down(
        shell,
        band,
        &mut RawTty::stdout(),
        &mut failures,
        outcome.is_ok(),
        Shutdown {
            reconcile: |shell, band| collect_facts(shell, band, launch, Instant::now()),
            drain: |taken| worker.shutdown(events, Instant::now() + worker::DRAIN_DEADLINE, taken),
            size: super::term::reported_window_size,
        },
    );

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
        let reconciled = collect_facts(shell, band, launch, Instant::now())?;
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
        let reconciled = collect_facts(shell, band, launch, Instant::now())?;
        commit_frame(
            shell,
            band,
            &mut RawTty::stdout(),
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
        // And bounded a second way, which is where the pacer's queue gets its
        // bound. `Shell` releases answer text against a clock, so taking events
        // faster than it releases them would grow a `String` to the length of
        // the whole answer with nothing to stop it. Leaving them on the channel
        // instead fills it, parks the runtime in its `send().await`, and stops
        // the socket being read -- backpressure to the provider rather than
        // memory here. It cannot wedge: the pacer always drains at
        // `pacer::MIN_CPS` or better, so the door opens again on its own.
        //
        // **Inside the loop, and that is the whole of what makes it a bound.**
        // Asked once before the batch it was a bound on the depth *entered*
        // with: a batch that began under the mark could take
        // [`bridge::UI_EVENTS`] more deltas past it, so the number meant
        // nothing about how much is really in hand. Asked here it is worth one
        // event of overshoot, which is the least any policy that may not drop
        // an event can be ([`PACED_BACKLOG`]).
        if shell.paced_backlog() >= PACED_BACKLOG {
            return;
        }
        match events.try_recv() {
            Ok(event) => shell.apply(event),
            // Empty, or a runtime that is gone -- and a runtime that is gone
            // said so with a `Fatal` before its sender dropped, so there is
            // nothing here that has to be inferred from a closed channel.
            Err(_) => return,
        }
    }
}

/// The three things an exit borrows from its caller.
///
/// One parameter rather than three because they are one role -- what the caller
/// can still do while the session comes down -- and because each of them is a
/// thing `run` alone can supply: a reconcile that takes the terminal back, a
/// drain that reaches the worker thread, and a reader of the process's own
/// window size. Named fields rather than a row of positional closures, so a
/// call site says which is which.
struct Shutdown<R, D, S>
where
    R: FnMut(&mut Shell, &mut Band) -> io::Result<Reconciled>,
    D: FnOnce(&mut dyn FnMut(UiEvent)),
    S: FnOnce() -> (u16, u16),
{
    /// Takes the terminal back before anything is read or painted.
    reconcile: R,
    /// What the runtime still has to say.
    drain: D,
    /// The screen's size, for the winch an exit may still owe an answer to
    /// ([`resolve_resize_on_exit`]).
    size: S,
}

/// The whole of an exit: what the runtime still has to say, and what the pacer
/// is still holding.
///
/// Extracted from [`run`] rather than written inside it, and the reason is the
/// second half. A drain that received nothing is the commonest exit there is --
/// a turn that concluded while its answer was still being released leaves an
/// empty channel behind it -- and it is exactly the case in which the flush
/// below is the *only* thing that can put the rest of that answer on the
/// terminal. Inside `run` that branch could not be reached by any test in this
/// process: `run` needs a worker thread, a signal token and the process's real
/// standard output. Out here the two facts it cannot fabricate are parameters,
/// so a test scripts a drain that carries nothing and a screen that remembers
/// what it was given, and `run`'s own wiring is one call with nothing in it to
/// get wrong.
///
/// Returns the screen error that ended it, if a screen did.
fn shut_down<R, D, S>(
    shell: &mut Shell,
    band: &mut Band,
    out: &mut impl Sink,
    failures: &mut FrameFailures,
    painting: bool,
    caller: Shutdown<R, D, S>,
) -> Option<io::Error>
where
    R: FnMut(&mut Shell, &mut Band) -> io::Result<Reconciled>,
    D: FnOnce(&mut dyn FnMut(UiEvent)),
    S: FnOnce() -> (u16, u16),
{
    let Shutdown {
        mut reconcile,
        drain,
        size,
    } = caller;
    // Before anything is written, and once: a winch nobody has resolved yet
    // would otherwise hold every write below -- the drain's frames and the
    // pacer's tail alike -- until a deadline this session will not live to see.
    let mut broken: Option<io::Error> = None;
    if painting {
        resolve_resize_on_exit(shell, band, size);
        // And then the plane, before a single byte of the drain is written. A
        // session can end with a question still up -- a `Fatal` from the
        // runtime, a second Ctrl-C, a supervisor's signal answered by the drain
        // -- and everything below belongs on the plane the user's shell is on.
        // Nobody is left to answer the question, so the plane is given back
        // rather than waited on; the tool call it belonged to is refused by the
        // prompter's own shutdown path (`super::approval::TuiPrompter`).
        if band.on_alternate() {
            shell.release_screen();
            if let Err(err) = paint_alternate(shell, band, out, failures, Instant::now()) {
                broken = Some(err);
            }
        }
    }
    // A screen that already refused the session's last frame is not a screen to
    // keep painting into; the events are still applied, because one of them may
    // be the `Fatal` that explains everything.
    drain(&mut |event| {
        let reconciled = if painting && broken.is_none() {
            match reconcile(shell, band) {
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
                if let Err(err) = drained(shell, band, out, failures, Instant::now(), token, event)
                {
                    broken = Some(err);
                }
            }
            // Shown but not painted: the screen is gone or has already refused,
            // and the event may still be the `Fatal` that says why.
            None => shell.apply(event),
        }
    });

    // And what the pacer still holds, for the exit whose drain carried nothing
    // at all. Phase 1 never repaints a document row, so this is the last moment
    // those bytes can reach the terminal.
    if painting && broken.is_none() && shell.paced_backlog() > 0 {
        match reconcile(shell, band) {
            Ok(token) => {
                if let Err(err) = flushed(shell, band, out, failures, Instant::now(), token) {
                    broken = Some(err);
                }
            }
            Err(err) => broken = Some(err),
        }
    }
    broken
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
    out: &mut impl Sink,
    failures: &mut FrameFailures,
    now: Instant,
    reconciled: Reconciled,
    event: UiEvent,
) -> io::Result<()> {
    shell.apply(event);
    flushed(shell, band, out, failures, now, reconciled)
}

/// What an exit owes the stream it is still holding: the text, and the frame
/// that text asks for.
///
/// **Flushed rather than paced.** A delta goes into the pacer like every other
/// one, and the pacer releases text against a clock that an exiting loop is no
/// longer ticking -- so an exit that only applied its events would free the
/// producer's permits, come down cleanly, and leave the tail of the answer in a
/// queue nobody will ever tick again. Phase 1 never repaints a document row, so
/// that text would simply be gone. Pacing an exit would be the wrong thing even
/// if it worked: the session is over, and the reader is owed the words rather
/// than the rhythm.
///
/// Called from **both** ways out, because they are different ways out. The
/// drain calls it per event, so an interrupted turn's tail is painted as it is
/// taken; [`run`] calls it once after the drain, for the session whose turn had
/// already concluded and whose drain therefore had nothing to carry.
fn flushed(
    shell: &mut Shell,
    band: &mut Band,
    out: &mut impl Sink,
    failures: &mut FrameFailures,
    now: Instant,
    reconciled: Reconciled,
) -> io::Result<()> {
    shell.flush_paced();
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
///
/// `now` is a parameter for the reason [`commit_frame`]'s is: the resize
/// deadline below is a monotonic time, and a debounce measured on the process's
/// own clock could only be tested by sleeping through it.
fn collect_facts(
    shell: &mut Shell,
    band: &mut Band,
    launch: &Launch<'_>,
    now: Instant,
) -> io::Result<Reconciled> {
    launch.wakeup.drain();

    if signals::take_resumed() {
        // The stop handler gave the terminal back and the shell has had it
        // since. Taking it again -- reinstalling the stop handler, re-entering
        // raw mode, re-announcing -- happens here, before this turn reads or
        // paints anything, and the band it drew is gone from a screen somebody
        // else has been writing to.
        super::resume(launch.tmux)?;
        // The handler that stopped this process wrote the abnormal restore, and
        // that sequence leads with `1049l` whichever plane was on the screen
        // (`super::term::abnormal_restore`); the mode set `resume` has just
        // re-announced carries no `1049h`. So a session that was reviewing a
        // change on the approval plane is on the normal buffer now, and the
        // band's own record is the only thing that still says otherwise. Told
        // here, the frame below takes the plane again from scratch; untold, it
        // would repaint the approval screen onto the buffer the user was given
        // back -- or, if nothing on it had changed, paint nothing at all and
        // leave the session with neither a band nor a question on the screen.
        adopt_resume(shell, band, signals::take_plane_restored());
        // And the palette, which the damage above does not cover. A stop hands
        // the terminal back with the theme subscription turned off
        // (`super::term::abnormal_restore`'s `?2031l`), so for however long the
        // user's shell had it this session heard nothing -- and the one thing
        // they may have done in that time is switch their system theme. The
        // mode set `resume` just re-announced subscribes again, which covers
        // every change from **now** on; what it cannot do is say what was
        // missed, so the session asks. Armed rather than written here:
        // `collect_facts` reconciles state and the paint tick is what writes
        // (`Shell::resync_theme`).
        shell.resync_theme();
    }

    // Taken rather than left set, so that the *launch* measurement -- which
    // acts on the same flag -- cannot be answered by a resize this loop already
    // saw. The band is not re-solved here: a drag is a burst of these, and what
    // it starts is a deadline ([`super::render_request::RESIZE_DEBOUNCE`]).
    if signals::take_winch() {
        shell.render.mark_resize(now);
    }
    resolve_resize(shell, band, now, super::term::reported_window_size);
    // **The post-write check**, and this is the point it belongs at: the winch
    // taken a few lines above is the one that landed *inside* the last tick's
    // write, so a frame proven here is a frame no signal has invalidated since.
    // Before the drain it would promote a composition the terminal has already
    // reflowed away from.
    shell.reconcile_approval();

    Ok(Reconciled)
}

/// What a resume owes the band, given whether a handler really gave the plane
/// back.
///
/// **The provenance is the whole of this function.** `SIGCONT`'s handler sets
/// the resume flag for *any* continue (`super::signals`'s `flag_resumed`, and
/// its own `Held::stopped_before_the_session_began` says so): the `SIGTSTP`
/// this session handles, an uncatchable `SIGSTOP` that ran no handler at all,
/// and an operator's bare `kill -CONT` on a process that was never stopped.
/// Only the first of those wrote the abnormal restore, and only the abnormal
/// restore writes the `1049l` that gives the alternate screen back.
///
/// Re-entering raw mode and re-announcing the mode set on any of the three is
/// harmless, because both are idempotent. Taking the plane again is not:
/// `1049h` saves the normal buffer a second time -- over the save that is
/// holding the user's real screen -- and leaves the session two enters against
/// one leave, so the exit gives back a buffer that is not the one it took.
///
/// `plane_restored` is a parameter rather than a call to
/// `signals::take_plane_restored` for the reason every reading in this module is
/// one: "a continue nothing restored takes no plane back" is a claim about a
/// signal that cannot honestly be delivered inside a unit test.
///
/// **The shell's half is here too, rather than beside the call**, because it is
/// the same event: somebody else has had the terminal, so every row on it --
/// the band's and the document's alike -- is a claim this session can no longer
/// make. Written as one seam so a test can reach the whole of what a resume
/// does to this side without a signal, and so the damage cannot be raised by a
/// road that misses what it means to the rest of the session.
fn adopt_resume(shell: &mut Shell, band: &mut Band, plane_restored: bool) {
    if plane_restored {
        band.plane_given_back();
    }
    // Through the shell's own door rather than by asking for the reason
    // directly: a resume is not only a frame the band owes, it is the moment
    // this session stops being able to say what is on **any** row -- the
    // document's included -- and `external_damage` is where both halves of that
    // are recorded together.
    shell.external_damage();
}

/// Re-solves the band from the terminal's size, once the debounce is out.
///
/// `size` is a parameter rather than [`super::term::reported_window_size`] for
/// the reason every screen in this module is one: "a burst of winches costs one
/// resolve" is a claim about how often a question is asked, and a function that
/// asked the process's own terminal could only be tested by resizing the
/// developer's window.
///
/// **It is the *reported* size and never `term::window_size`.** That one
/// answers a terminal that will not say its size with the 24x80 a launch has to
/// start from, and a running session handed that number moves its band to rows
/// 22-24 of a screen that is nothing of the kind. Here a reading that says
/// nothing arrives as `(0, 0)` and `Shell::resize` refuses it.
///
/// The band's shadow is forgotten **here** rather than left to
/// `Reason::ExternalDamage`, because the two facts are not the same size: the
/// shadow has to be re-sized to the screen that now exists, and the geometry
/// the shell just adopted is the only thing that knows what that is. What is
/// asked for is one [`Reason::Resize`]; the whole repaint is the band's
/// `damaged` flag, which survives until a frame really lands.
fn resolve_resize(
    shell: &mut Shell,
    band: &mut Band,
    now: Instant,
    size: impl FnOnce() -> (u16, u16),
) {
    if !shell.render.take_resize(now) {
        return;
    }
    adopt_resize(shell, band, size);
}

/// The same, at the exit, **without waiting for the deadline**.
///
/// The one moment the debounce cannot be allowed to run: nothing is written
/// while a winch is outstanding ([`Shell::blind`]), the resolve is up to
/// [`super::render_request::RESIZE_DEBOUNCE`] away, and a user who drags a
/// window and then presses Ctrl-D lands inside that window. The exit is the
/// last moment the answer they were reading can reach the terminal at all --
/// this phase never repaints a document row -- so the signal is answered now.
///
/// It is not the debounce being broken. The debounce is there so that a drag
/// costs one re-solve rather than one per signal, and there is no drag left to
/// protect: one measurement, and then the session is over. An exit with nothing
/// outstanding measures nothing, so an ordinary shutdown is unchanged.
///
/// A screen that turns out to hold no band is still silent afterwards, and that
/// is the same trade the running session makes for the harder reason: an append
/// is a scroll, and what it pushes into native scrollback at coordinates that
/// mean nothing can never be taken back.
fn resolve_resize_on_exit(shell: &mut Shell, band: &mut Band, size: impl FnOnce() -> (u16, u16)) {
    if !shell.render.force_resize() {
        return;
    }
    adopt_resize(shell, band, size);
}

/// Measures the screen and re-solves the band from it.
fn adopt_resize(shell: &mut Shell, band: &mut Band, size: impl FnOnce() -> (u16, u16)) {
    let (rows, cols) = size();
    if let Resize::Repaint(geometry) = shell.resize(rows, cols) {
        // A terminal that changed size re-wrapped its own document by rules xfx
        // does not model, so every cell the shadow describes is a claim about a
        // screen that no longer exists.
        band.invalidate(geometry.rows, geometry.cols);
        shell.damage_told();
    }
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
    out: &mut impl Sink,
    failures: &mut FrameFailures,
    now: Instant,
    _reconciled: Reconciled,
) -> io::Result<()> {
    // **The theme query, above every exit below.** It carries no coordinates
    // and moves no cell, so it means the same thing on the borrowed buffer and
    // on a screen no band fits on -- and those are exactly the two states a
    // session can sit in for minutes at a time. Written after either of the
    // returns below it would be a resync that lands when the question is
    // answered or the window is made big enough again, which is the moment it
    // stops being worth anything.
    if shell.take_theme_query() {
        let query = super::theme::MODE_QUERY.as_bytes();
        if let Err(emit) = check_theme_query(query, band.on_alternate())
            .map_err(Emit::rejected)
            .and_then(|()| out.emit(query))
        {
            // The same disposal `/clear` gets, for the same reasons.
            // `Rejected` and `ZeroProgress` moved no byte, so the question is
            // handed back and offered again next tick under the existing
            // budget; `Partial` is fatal on the spot and is not re-armed --
            // there is no next tick for it to survive into.
            //
            // **And a query that lands does not mend the budget.** A frame is
            // what proves the screen is taking bytes; seven bytes with no cells
            // behind them are not, and calling `succeeded` here would let a
            // session that has refused every frame for `FRAME_BUDGET` reset the
            // run each time a resync happened to slip through.
            let rearm = matches!(emit, Emit::Rejected(_) | Emit::ZeroProgress(_));
            return match disposed(emit, failures, now) {
                Some(fatal) => Err(fatal),
                None => {
                    if rearm {
                        shell.restore_theme_query();
                    }
                    Ok(())
                }
            };
        }
    }
    // `/clear`, and before everything: the screen and its scrollback go, and
    // what is written after this write is written onto a blank terminal. The
    // shell has already dropped the appends it owed against the screen that is
    // being erased, so the loop's only job is the bytes.
    //
    // A refused clear is handled the same way a refused append is
    // (`commit_document` below): `Rejected` and `ZeroProgress` moved no bytes,
    // so the intent is handed back (`Shell::restore_clearing`) and retried
    // next tick, under the existing budget. `Partial` is fatal on the spot and
    // is not re-armed -- there is no next tick for it to survive into.
    // Whose screen it is, before anything is written onto it. A question the
    // band cannot show is reviewed on the terminal's other buffer, and both the
    // frames that live there and the one write that gives the plane back are
    // this function's -- nothing below may run while the terminal is on it.
    //
    // **Except what the primary plane already owed, which is paid before the
    // plane changes hands.** One tick takes a *batch* of events
    // ([`take_ui_events`]) and composes one frame afterwards, so the row a tool
    // call put in the document and the question that follows it are both
    // applied before anything is written -- and a frame routed straight to the
    // other buffer leaves that row owed ([`paint_alternate`]), which on this
    // surface means never: it lands only when the plane comes back, minutes
    // later, under an answer to a question whose tool call left no trace.
    // Splitting the two events across ticks hides it, which is what local
    // scheduling and one CI runner did while two faster ones did not.
    //
    // It is a **barrier rather than a delay**: what is owed is written and then
    // the plane is taken, in this same tick, so a question still costs an extra
    // write only when there was really something owed.
    //
    // **And what is owed is the whole primary plane, not only its rows.** An
    // append is a scroll: it moves the band up out from under the coordinates
    // the last frame put it at, and `1049h` saves the buffer it leaves --
    // document rows, and a hole where the band was. That is the screen the user
    // is handed back when the question is answered, and it is what run
    // 33260998206 (exact head 3c89ea2) showed on both arm64 runners: the saved
    // primary buffer held the prompt and three tool rows with nothing below
    // them. So the rows are paid, and then the band frame they moved.
    let taking_the_plane =
        !band.on_alternate() && shell.screen_owner() == super::shell::ScreenOwner::Approval;
    if band.on_alternate() || taking_the_plane {
        // Not on a screen no band fits on: there, `blind` says the row numbers
        // are claims about a screen that may not exist, and an append is a
        // scroll that cannot be taken back. Those rows keep waiting for a
        // screen, exactly as they do without a question.
        //
        // And not when the plane owes nothing: a question that arrives on a
        // screen whose band is exactly where the last frame put it is still
        // entered in **one write**, which is what keeps the transition from
        // flickering. [`Band::owes_primary_frame`] is the second half of the
        // question because a refused band frame leaves the rows landed and the
        // band moved, and `owes_document` alone would call that tick square.
        if taking_the_plane
            && !shell.blind()
            && (shell.owes_document() || band.owes_primary_frame())
        {
            // **The whole of what the primary plane is owed, in the order the
            // primary tick pays it**: the rows first, then the band the rows
            // scrolled out from under. Paying only the document leaves the
            // band where it no longer is, and `1049h` saves *that* -- a buffer
            // carrying the document and a hole where the band was, which is the
            // screen the user is handed back when the question is answered.
            //
            // The same two functions the ordinary tick below calls, not a
            // painter of this branch's own: a second one would be the copy that
            // forgot the invalidate, or the cursor, or the budget.
            if !commit_document(shell, band, out, failures, now)? {
                // The write was refused and counted. The rows stay owed and so
                // does the plane: the next tick offers both again, in order.
                return Ok(());
            }
            if !commit_band(shell, band, out, failures, now)? {
                // The rows landed and the band did not. The plane stays where
                // it is for the same reason: what `1049h` would save is a
                // screen this session cannot describe.
                return Ok(());
            }
        }
        return paint_alternate(shell, band, out, failures, now);
    }
    if shell.take_clearing() {
        // The one vector this loop writes itself, so the check sits here --
        // ownership stays where it is: the shell decides that a clear is owed
        // and what it means to its own state, and the loop writes it. What is
        // declared is the whole of `/clear`: a blank screen, the caret at its
        // origin, and the scrollback erased, which is the one place in the TUI
        // that erases one.
        let cleared = super::shell::CLEAR_SCREEN.as_bytes();
        if let Err(emit) = check_cleared(cleared, &shell.geometry)
            .map_err(Emit::rejected)
            .and_then(|()| out.emit(cleared))
        {
            // `take_clearing` already consumed the intent above, so a
            // `Rejected` or `ZeroProgress` -- neither of which moved a byte --
            // has to hand it back or `/clear` is forgotten for good: nothing
            // else ever sets the flag again. A `Partial` is not re-armed: it
            // is fatal below on every path (`disposed`), so there is no next
            // tick for it to survive into, and re-arming here would be a flag
            // this session never reads again.
            let rearm = matches!(emit, Emit::Rejected(_) | Emit::ZeroProgress(_));
            return match disposed(emit, failures, now) {
                Some(fatal) => Err(fatal),
                None => {
                    if rearm {
                        shell.restore_clearing();
                    }
                    Ok(())
                }
            };
        }
    }
    // **And nothing at all on a screen no band fits on.** Every write below
    // addresses a row by number, out of a geometry that still describes the
    // screen the band was last solved for; the terminal has since reported one
    // it does not fit on, and a terminal answers a `CUP` past its last row by
    // clamping it -- silently. The frame would land on the bottom row on top of
    // itself, and the append is worse, because an append is a **scroll** and
    // what leaves the top of the screen is in the terminal's native scrollback
    // for good: there is no later frame that can take it back.
    //
    // Nothing is taken and nothing is begun, so nothing is dropped either. The
    // rows the document is owed stay owed and the reasons stay pending, and
    // both are answered by the frame `Shell::resize` asks for when the screen
    // can hold a band again -- which is also the only thing that clears this
    // ([`Shell::blind`]). The `/clear` above is deliberately not held: it
    // carries no coordinates, so it means the same thing on any screen, and the
    // shell has already forgotten the rows it erases.
    if shell.blind() {
        return Ok(());
    }
    // Before every write below, and before the appends themselves: a band that
    // has grown has taken rows the **document** is holding, and the frame under
    // it opens with `CUP(band_top,1)` + `ED`. Carried up first, those rows leave
    // the top of the screen into the terminal's own scrollback, where they are
    // still the user's; left where they are, they are erased by the very next
    // frame -- and this phase never repaints a document row, so they are gone.
    //
    // It has to be here rather than inside either writer, because either can be
    // the one that runs first: a turn that starts with nothing to append still
    // grows the band, and an append that follows one would otherwise place its
    // own row on top of the row the growth had not yet moved.
    //
    // A refused carry is owed again -- nothing is recorded until it lands
    // (`Band::carry_document`) -- and it counts against the same budget every
    // other refused write does.
    if !commit_document(shell, band, out, failures, now)? {
        return Ok(());
    }
    commit_band(shell, band, out, failures, now)?;
    Ok(())
}

/// What the theme query says it does: **nothing at all** except ask.
///
/// One query, no cell, no caret and no mode. The screen is one cell because a
/// vector with no footprint needs no more: a query that moved a cell is refused
/// by the emptiness check whatever size the model is, which is the same
/// argument [`super::probe`]'s launch queries make.
///
/// **The plane is the one the terminal is really on, and this is the one vector
/// where that cannot be proved by anything being refused.** `Intent::Queries`
/// licenses no row on *either* buffer and holds the caret to the one the seed
/// had, so seven bytes that touch neither are accepted against either seed --
/// the plane is passed because it is the true statement about where these bytes
/// land, not because a wrong one would be caught here. Stated rather than left
/// implied, and asserted as independence rather than as a guard
/// (`a_theme_query_is_accepted_on_either_plane_and_moves_no_cell_on_either`), so
/// that nobody reads a passing check as proof the plane was considered.
fn check_theme_query(bytes: &[u8], on_alternate: bool) -> io::Result<()> {
    let plane = if on_alternate {
        super::check::PlaneKind::Alternate
    } else {
        super::check::PlaneKind::Primary
    };
    super::check::preflight(
        &super::check::TerminalModel::seed_modes(1, 1, super::check::ModeSet::fresh(), 0, plane),
        bytes,
        &super::check::Declared::new(
            super::check::Intent::Queries(&[super::check::QueryId::ThemeMode]),
            super::check::Footprint::none(plane),
        ),
    )?;
    Ok(())
}

/// What `/clear` says it does: a blank screen, a caret at its origin, and the
/// scrollback erased.
///
/// The one vector this loop writes itself, so the declaration lives here --
/// ownership stays where it is, with the shell deciding that a clear is owed and
/// what it means to the shell's own state. The scrollback erase is asserted as a
/// flag and nothing further: no grid models a terminal's scrollback, and this is
/// the only emitter in the TUI that erases one.
fn check_cleared(bytes: &[u8], geometry: &super::layout::Geometry) -> io::Result<()> {
    super::check::preflight(
        &super::check::TerminalModel::seed_foreign(geometry.rows, geometry.cols, None),
        bytes,
        &super::check::Declared::new(
            super::check::Intent::Cleared,
            super::check::Footprint::new(
                super::check::PlaneKind::Primary,
                vec![super::check::Seg::Erase(1..=geometry.rows)],
            ),
        ),
    )?;
    Ok(())
}

/// The band's own frame on the **primary** plane: the rows the session wants
/// under the document, and nothing else.
///
/// `Ok(true)` when the screen has what the band owes it -- including when
/// nothing asked for a frame, which is a screen that already had one --  and
/// `Ok(false)` when the write was refused and counted.
///
/// A function of its own for the reason [`commit_document`] is one: **two**
/// callers need it and a second copy would drift. The ordinary primary tick
/// ends here, and the barrier that hands the terminal to a question runs it
/// first -- because an append is a scroll, and a plane taken straight after one
/// saves a buffer with the document on it and a hole where the band was.
fn commit_band(
    shell: &mut Shell,
    band: &mut Band,
    out: &mut impl Sink,
    failures: &mut FrameFailures,
    now: Instant,
) -> io::Result<bool> {
    let Some(attempt) = shell.render.begin() else {
        return Ok(true);
    };
    // The matrix row for a terminal that takes part of a frame and then stops.
    // Armed here, where the first frame of the session is about to be built,
    // and performed by the sink against the real descriptor.
    #[cfg(feature = "fault-injection")]
    super::fault::arm_partial_frame();
    if attempt.damaged() {
        // Something that is not this band wrote on the screen -- a resume that
        // handed the terminal to the shell and took it back, a `/clear` that
        // erased it, a Ctrl-L asking for exactly this. The shadow is a claim
        // about what is on those rows and that claim is now false about all of
        // them, so the frame below is a whole one rather than a difference from
        // a screen that no longer exists.
        band.invalidate(shell.geometry.rows, shell.geometry.cols);
        // And a readiness receipt is the same kind of claim about the same
        // rows.
        shell.invalidate_approval();
        // The band has been told, so its own `damaged` is the fact from here
        // on and the shell's record of an untold one is spent
        // ([`super::shell::Shell::damage_told`]).
        shell.damage_told();
    } else if failures.pending_recovery {
        // A tick right behind a recovered tear, on a shadow that is already
        // correct: nothing about the rows is unknown the way damage leaves
        // them, so `invalidate` would only discard what recovery just
        // rebuilt. `force_redraw` asks for less -- a real write next, not a
        // trusted `Commit::NoChange` -- which is exactly the one thing this
        // tick still owes.
        band.force_redraw(&shell.geometry);
    }
    // Bound **once**, and before the intent: the rows are what is about to be
    // written, and a second `band_rows()` call after the composition would be a
    // second reading of a shell the first one may have changed.
    let rows = shell.band_rows();
    let cursor = shell.cursor();
    shell.intend_approval();
    match band.commit(out, &rows, &shell.geometry, cursor) {
        // A frame that wrote nothing is a frame the screen already had, so the
        // budget is whole for the same reason a delivered one leaves it whole
        // -- and the two are told apart here, because a zero-byte repaint may
        // keep a receipt and may never mint one
        // ([`super::approval_readiness::Readiness::landed`]).
        Ok(landed) => {
            shell.approval_landed(match landed {
                Commit::Painted => Outcome::Painted,
                Commit::NoChange => Outcome::Unchanged,
            });
            // Only a frame that actually went out re-verifies the screen.
            // `Commit::NoChange` wrote nothing -- on an ordinary tick that is
            // fine, the screen already held this, but on the tick
            // `pending_recovery` forced (`force_redraw` above) it would mean
            // reading a diff against an already-correct shadow as if it were
            // proof the terminal is still there, which is exactly the bug a
            // recovered tear's budget must not have. That is scoped to a
            // recovery actually pending: an ordinary `NoChange` tick, with no
            // recovery owed, still resets the budget exactly as it did
            // before that gate existed -- `landed == Commit::Painted` alone
            // would swallow that reset too.
            if landed == Commit::Painted || !failures.pending_recovery {
                failures.succeeded();
            }
            Ok(true)
        }
        Err(emit) => {
            // A refused write may have left half a frame on the screen.
            shell.approval_write_failed();
            shell.render.restore(attempt);
            if !matches!(emit, Emit::Partial { .. }) {
                return match disposed(emit, failures, now) {
                    Some(fatal) => Err(fatal),
                    None => Ok(false),
                };
            }
            // A torn primary-band frame: a fixed cleanup vector, then the same
            // frame rebuilt from an erased shadow, in this same call
            // (`Band::recover_primary`), under the policy every recoverable
            // tear shares ([`recover_tear`]). Recovered, the screen holds a
            // whole, correct band again, but this tick still reports `false`
            // -- no `approval_landed`, no `succeeded` -- so readiness and the
            // budget are restored only by a later frame this loop verifies on
            // its own account. `pending_recovery` is what that later frame
            // reads (this function's own top, on the next call) to make sure
            // it really is one.
            recover_tear(emit, failures, now, "primary-band frame", || {
                band.recover_primary(out, &rows, &shell.geometry, cursor)
                    .map(|_| ())
            })
            .map(|()| false)
        }
    }
}

/// What a torn repaint whose whole content is still in hand costs: the one
/// policy both recoverable tears -- the primary band's frame ([`commit_band`])
/// and a repaint of the alternate plane the session already owns
/// ([`paint_alternate`]) -- are under, written once so the two cannot drift.
///
/// `torn` is the `Emit::Partial` the repaint ended in, and `recover` is the
/// caller's one attempt: a fixed cleanup vector and the same repaint rebuilt,
/// in this same call. `what` names the repaint in the message a failed
/// recovery leaves with.
///
/// * **Spent on the original failure exactly as a refused write is**
///   (`failures.record`, never `succeeded`), so a screen that keeps tearing
///   still runs the budget out even though each individual tear is recovered.
/// * **A tear found already past the budget ends the session on its own
///   error**, not the run's possibly different, already-repaired `first`, and
///   gets no recovery attempt either -- checked before the error moves into
///   `record`.
/// * **One attempt.** Recovered, `pending_recovery` is raised so the caller's
///   next tick forces a real write rather than trusting a cache or a shadow
///   recovery itself just rebuilt; readiness and the budget come back only from
///   that frame. Whatever failed the cleanup or the rebuild is fatal on the
///   spot, marked [`Partial`](super::diagnostic::Reason::Partial) the way the
///   tear itself would have been -- and still names the original tear, since
///   that is the failure this session could not get past. Its text is kept
///   before the error moves, so the message can say what the *first* tear was
///   and not only the second.
///
/// `Ok(())` when the tear was recovered; `Err` is the error the session leaves
/// with.
fn recover_tear(
    torn: Emit,
    failures: &mut FrameFailures,
    now: Instant,
    what: &str,
    recover: impl FnOnce() -> Result<(), Emit>,
) -> io::Result<()> {
    let original = torn.into_error();
    let original_text = original.to_string();
    if failures.expired(now) {
        return Err(super::diagnostic::mark(
            super::diagnostic::Reason::Partial,
            original,
        ));
    }
    // Ordinary first-failure bookkeeping for a tear still inside the budget;
    // `expired` above already rules out exhaustion here.
    failures.record(original, now);
    match recover() {
        Ok(()) => {
            failures.pending_recovery = true;
            Ok(())
        }
        Err(recovery) => Err(super::diagnostic::mark(
            super::diagnostic::Reason::Partial,
            io::Error::other(format!(
                "a torn {what} ({original_text}) could not be recovered: {recovery}",
                recovery = recovery.into_error(),
            )),
        )),
    }
}

/// What the **document** is owed: the rows a growing band pushed up, and the
/// appends the session has not written yet.
///
/// `Ok(true)` when everything owed has landed, `Ok(false)` when a write was
/// refused and counted against the frame budget -- which ends the caller's tick
/// rather than this function, because what may be attempted after a refused
/// write is the caller's question and not this one's.
///
/// A function of its own because there are **two** callers and they must not
/// drift: the ordinary primary frame below, and the barrier above that pays the
/// document before the plane changes hands. Written twice, the second copy
/// would be the one that forgot the carry, or the order, or that a
/// `Rejected`/`ZeroProgress` append is dropped instead of retried.
fn commit_document(
    shell: &mut Shell,
    band: &mut Band,
    out: &mut impl Sink,
    failures: &mut FrameFailures,
    now: Instant,
) -> io::Result<bool> {
    // **The recolour a theme report owes the rows already on the screen, and
    // it goes first.** It writes no scroll, so it cannot move anything the two
    // writers below are about to address -- and ahead of them a row that is
    // carried off the top of the screen leaves in the palette the user has
    // now rather than the one they had.
    //
    // Only from here, which is the ordinary primary tick and the barrier that
    // pays the primary plane before a question takes the other one: on the
    // alternate plane this is never called at all, so the debt waits, and on a
    // screen no band fits on `commit_frame` returns above it for the reason
    // every other coordinate does.
    //
    // Nothing is taken, so nothing is re-armed: the debt is cleared only by a
    // call that settled it, and a refused one leaves it owed under the same
    // budget as every other refused write. It is not a frame and does not count
    // as one -- `failures.succeeded` stays where the frames are.
    if shell.owes_retint() {
        match band.retint_document(out, &shell.palette(), &shell.geometry) {
            // Settled: the cells were repainted, or the screen already held
            // them, or this session owns no document row on it.
            Ok(Retint::Settled(_)) => shell.retint_paid(),
            // Deferred, and the tick **goes on**. The band cannot describe the
            // screen yet, so the debt stands -- and the frame below is exactly
            // what makes the next tick able to answer it, so returning early
            // here would starve the repair that settles this.
            Ok(Retint::Deferred) => {}
            Err(emit) => {
                return match disposed(emit, failures, now) {
                    Some(fatal) => Err(fatal),
                    None => Ok(false),
                }
            }
        }
    }
    if let Err(emit) = band.carry_document(out, &shell.geometry) {
        return match disposed(emit, failures, now) {
            Some(fatal) => Err(fatal),
            None => Ok(false),
        };
    }
    // Before the frame, and before the `begin` below, for two reasons. An
    // append scrolls the screen, so a band painted first ends up a row above
    // where it belongs. And the rows are the *document's*, not the band's: a
    // tick that had nothing to repaint would otherwise hold them forever.
    //
    // **One operation at a time, and each one measured as it is written.** The
    // shell hands over rows rather than text ([`Shell::emit_document_front`]),
    // and it builds them from the width the screen has at the moment of the
    // call -- so a write refused on one tick and retried on the next, across a
    // resize, is re-wrapped for the screen it really lands on. Taking the whole
    // batch up front, as this did, froze every row at the width the delta
    // happened to arrive at: the painter clips them to the columns there are
    // now, and this phase never repaints a document row, so the overhang was
    // gone from the session rather than from the frame.
    //
    // Whether a refused operation is owed again depends on its `Emit`.
    // `Rejected` and `ZeroProgress` both moved no bytes -- the terminal is
    // exactly as it was -- so it and everything behind it stay owed, under the
    // existing (unchanged) frame budget below, and the next attempt measures
    // them again. `Partial` already put a prefix of its vector on the terminal,
    // so it is fatal on the spot (`disposed` sends it straight to `Some`,
    // bypassing the budget): its bytes are never replayed and the state it
    // would have produced is never adopted, while what was queued behind it --
    // never offered to the terminal, so it cannot have moved it -- is kept.
    // Dropped here that suffix would be gone for good.
    //
    // The geometry is copied once, before the loop: the shell is borrowed for
    // each call, and nothing between the measurement and the write may change
    // the screen it is about -- which is exactly the promise being kept here.
    let geometry = shell.geometry;
    while let Some(landed) = shell.emit_document_front(|append| {
        match band.append_document(out, append.scroll, &append.rows, &geometry) {
            Ok(()) => Landed::All,
            Err(emit) => {
                if matches!(emit, Emit::Partial { .. }) {
                    Landed::Prefix(emit)
                } else {
                    Landed::None(emit)
                }
            }
        }
    }) {
        if let Err(emit) = landed {
            return match disposed(emit, failures, now) {
                Some(fatal) => Err(fatal),
                None => Ok(false),
            };
        }
    }
    // **A scroll is a reason for a frame.** The rows that just landed moved the
    // band's own out from under the coordinates the last frame put them at, so
    // the band is a row above where it belongs until one repaints it. Nothing
    // else raises this: the appends came from events that asked for a frame in
    // every case seen so far, which is exactly the kind of "in every case seen
    // so far" that fails on one runner and not the others.
    if band.owes_primary_frame() {
        shell.render.request(Reason::Transcript);
    }
    Ok(true)
}

/// Everything that happens while the question owns the terminal's other buffer,
/// and the one write that gives it back.
///
/// **Three transitions and no fourth**, decided by comparing what the terminal
/// is really showing ([`Band::on_alternate`], a record of delivered bytes) with
/// whose screen the session wants it to be (`Shell::screen_owner`, a record of
/// intent). Reading only the second would enter a plane twice if a question
/// arrived on two consecutive ticks, and leave one that was never entered.
///
/// Nothing the *primary* plane owes is written here -- not a `/clear`, not a
/// document append, not a band frame. An append is a scroll, and what leaves the
/// top of the alternate buffer is gone for good: the terminal hands that buffer
/// back on the way out. So those stay owed, exactly as they do on a screen no
/// band fits on ([`commit_frame`]), and they land the moment the plane comes
/// back.
fn paint_alternate(
    shell: &mut Shell,
    band: &mut Band,
    out: &mut impl Sink,
    failures: &mut FrameFailures,
    now: Instant,
) -> io::Result<()> {
    use super::shell::ScreenOwner;

    match (band.on_alternate(), shell.screen_owner()) {
        // The question was answered -- or the session is coming down with one
        // still up. **One write**, and the ownership moves only after it: the
        // leave, the hidden cursor and the whole band repainted are one vector
        // precisely so that no terminal can present the gap between them.
        (true, ScreenOwner::Primary) => {
            let cursor = shell.cursor();
            // A frame the output check refuses is not written, and it takes the
            // same road a refused write does: into the budget, counted once,
            // with the plane still owed. There is no new failure policy here.
            let frame = match band.restore_primary(&shell.band_rows(), &shell.geometry, cursor) {
                Ok(frame) => frame,
                // A builder does no I/O, so its failure is a refusal and is
                // said so here rather than inferred downstream.
                Err(refusal) => {
                    return match disposed(Emit::rejected(refusal), failures, now) {
                        Some(fatal) => Err(fatal),
                        None => Ok(()),
                    }
                }
            };
            match out.emit(frame.bytes()) {
                Ok(()) => {
                    band.frame_landed(&frame, &shell.geometry, cursor);
                    debug_assert!(!band.on_alternate());
                    // The band on the primary plane is now exactly what this
                    // frame painted, so whatever asked for a frame has had one.
                    let _ = shell.render.begin();
                    failures.succeeded();
                    Ok(())
                }
                Err(emit) => match disposed(emit, failures, now) {
                    Some(fatal) => Err(fatal),
                    None => Ok(()),
                },
            }
        }
        // A change the band cannot show: take the plane and paint the whole
        // surface onto it in the same frame.
        (false, ScreenOwner::Approval) => {
            let rows = shell.screen_rows();
            let cursor = shell.screen_cursor();
            shell.intend_approval();
            let frame = match band.enter_alternate(&rows, &shell.geometry, cursor) {
                Ok(frame) => frame,
                Err(refusal) => {
                    // Exactly the branch a refused write takes below: the
                    // question keeps its plane, and the tick is counted.
                    shell.approval_write_failed();
                    return match disposed(Emit::rejected(refusal), failures, now) {
                        Some(fatal) => Err(fatal),
                        None => Ok(()),
                    };
                }
            };
            match out.emit(frame.bytes()) {
                Ok(()) => {
                    band.frame_landed(&frame, &shell.geometry, cursor);
                    shell.approval_landed(Outcome::Painted);
                    let _ = shell.render.begin();
                    failures.succeeded();
                    // The matrix row for a panic while the **other** plane is
                    // owned, and it is taken exactly here for two reasons. The
                    // entering frame has been written, flushed and recorded, so
                    // the terminal really is on the buffer the hook has to give
                    // back -- injected a statement earlier it would prove
                    // nothing the `ui-frame` row does not. And it is before the
                    // loop reads another byte, so no answer can race it: the
                    // question is up and unanswerable, which is the state a
                    // panic here leaves a user in.
                    #[cfg(feature = "fault-injection")]
                    if super::fault::injected(super::fault::Fault::AlternatePanic) {
                        panic!("the approval screen panicked");
                    }
                    Ok(())
                }
                Err(emit) => {
                    shell.approval_write_failed();
                    match disposed(emit, failures, now) {
                        Some(fatal) => Err(fatal),
                        None => Ok(()),
                    }
                }
            }
        }
        // Already there: a marker that moved, or a screen that changed size.
        (true, ScreenOwner::Approval) => {
            let Some(attempt) = shell.render.begin() else {
                return Ok(());
            };
            if failures.pending_recovery {
                // A tick right behind a recovered tear of this plane
                // (`Band::recover_alternate`), whose cache is exactly what the
                // rebuild painted -- so left alone, this repaint would be the
                // empty "already holds this" one below, which proves nothing
                // about a screen that just tore and can never mint the receipt
                // the tear took. The alternate counterpart of `commit_band`'s
                // `force_redraw`, read off the same flag.
                //
                // Only an alternate recovery can still be pending here: a
                // primary one is settled before this arm is ever reached,
                // because the frame that takes the plane is a whole write
                // that calls `succeeded` when it lands.
                band.force_alternate_repaint();
            }
            let rows = shell.screen_rows();
            let cursor = shell.screen_cursor();
            shell.intend_approval();
            let frame = match band.repaint_alternate(&rows, &shell.geometry, cursor) {
                Ok(frame) => frame,
                Err(refusal) => {
                    shell.approval_write_failed();
                    shell.render.restore(attempt);
                    return match disposed(Emit::rejected(refusal), failures, now) {
                        Some(fatal) => Err(fatal),
                        None => Ok(()),
                    };
                }
            };
            // The screen already holds this, which is the commonest frame while
            // a person is reading a change: the band's animation asks for one
            // twice a second and nothing on this plane has moved. It keeps a
            // receipt of this very composition and mints none, which is what
            // stops an unwritten repaint granting a question nobody has seen.
            if frame.bytes().is_empty() {
                shell.approval_landed(Outcome::Unchanged);
                // Not reached while a recovery is pending: the force above
                // drops the one cache that can make a repaint empty, so the
                // tick that owes a verified write always has bytes to write.
                // The guard is kept for the day that stops holding -- an empty
                // repaint is not known to prove anything about a recovery, and
                // must not clear one out from under the tick that still owes
                // it.
                if !failures.pending_recovery {
                    failures.succeeded();
                }
                return Ok(());
            }
            // The matrix row for a terminal that tears a repaint of the plane
            // this session already owns. Armed here -- past the empty return,
            // right before the one emit of a repaint that has bytes -- so the
            // vector it lands on is this repaint and never the `1049h` that
            // took the plane or the `1049l` that gives it back.
            #[cfg(feature = "fault-injection")]
            super::fault::arm_partial_alternate();
            match out.emit(frame.bytes()) {
                Ok(()) => {
                    band.frame_landed(&frame, &shell.geometry, cursor);
                    shell.approval_landed(Outcome::Painted);
                    failures.succeeded();
                    Ok(())
                }
                Err(emit) => {
                    shell.approval_write_failed();
                    shell.render.restore(attempt);
                    if !matches!(emit, Emit::Partial { .. }) {
                        return match disposed(emit, failures, now) {
                            Some(fatal) => Err(fatal),
                            None => Ok(()),
                        };
                    }
                    // A torn repaint of a plane this session already owns: no
                    // scroll, no plane transition and no native scrollback, and
                    // the whole of what it meant is the surface in hand. So it
                    // takes the road a torn primary band does
                    // ([`recover_tear`]): the fixed cleanup vector, then the
                    // same surface repainted whole, in this same call
                    // (`Band::recover_alternate`). Recovered, the question is on
                    // the screen whole again, and still no receipt is minted --
                    // `approval_write_failed` above took it, and only the forced
                    // repaint on a later tick may give it back.
                    recover_tear(emit, failures, now, "alternate-plane repaint", || {
                        band.recover_alternate(out, &rows, &shell.geometry, cursor)
                    })
                }
            }
        }
        // Not this function's turn; `commit_frame` guards against reaching here.
        (false, ScreenOwner::Primary) => Ok(()),
    }
}

/// Failure disposition for every write except the two recoverable repaints'
/// `Partial` -- `commit_band`'s primary band and `paint_alternate`'s repaint of
/// a plane it already owns -- which [`recover_tear`] answers instead.
///
/// Two roads, and which one a failure takes is settled by one question: is any
/// byte of that vector already on the terminal?
///
/// * A refusal and a zero-progress write left the screen exactly as it was, so
///   the vector is still owed and the session may offer it again. They spend
///   the frame budget, which is the policy that was already here: a screen that
///   refuses everything for [`FRAME_BUDGET`] ends the session, and one that
///   refuses a frame and then takes the next costs nothing at all.
/// * A partial write did not. The terminal took a prefix of a vector and then
///   stopped, so what reached it may be incomplete -- and the whole vector is
///   the only thing this session could offer again, which would put that
///   prefix on the screen twice. There is nothing to
///   re-offer and nothing to wait for, so it ends the session on the spot,
///   whatever the budget says. `Some` is always the error the session leaves
///   with, and the count is carried inside it rather than in any state that
///   outlives the failure.
///
/// `WouldBlock` is on whichever road its position puts it: a terminal that was
/// full before it took anything is a screen that is merely behind, and the
/// budget is where the room for that is given ([`super::deliver`]).
fn disposed(emit: Emit, failures: &mut FrameFailures, now: Instant) -> Option<io::Error> {
    match emit {
        // P3-DIAGNOSTIC: marked here for every `Partial` except the two
        // recoverable repaints', which `recover_tear` marks itself before
        // this function is ever reached for them -- together they are the
        // whole match, neither guessed from an error's text.
        prefix @ Emit::Partial { .. } => Some(super::diagnostic::mark(
            super::diagnostic::Reason::Partial,
            prefix.into_error(),
        )),
        Emit::Rejected(err) | Emit::ZeroProgress(err) => failures
            .failed(err, now)
            .map(|err| super::diagnostic::mark(super::diagnostic::Reason::Exhausted, err)),
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
    /// A tear [`Band::recover_primary`] or [`Band::recover_alternate`] fixed in
    /// the same call, still owed a tick that proves it on its own account.
    ///
    /// Set the moment recovery lands, by [`recover_tear`] -- the one policy
    /// both recovering arms call. Read at the top of the arm that tore, on
    /// every later call, to force the next write there to be a real one
    /// rather than trust what recovery itself just rebuilt: [`commit_band`]
    /// asks [`Band::force_redraw`] for a frame a diff cannot skip, and
    /// `paint_alternate`'s already-there arm asks
    /// [`Band::force_alternate_repaint`] for a repaint the cache cannot
    /// empty. Cleared only by [`FrameFailures::succeeded`], the one function
    /// that ever clears it: while this is set, a zero-byte repaint on either
    /// plane (primary [`Commit::NoChange`] or alternate already-there) cannot
    /// clear it -- neither one is the independently painted frame that
    /// actually re-verifies the screen. A successful, independently painted
    /// primary frame or a successful nonempty alternate write both call
    /// `succeeded` unconditionally and do clear it.
    pending_recovery: bool,
}

impl FrameFailures {
    /// A frame landed, so whatever was wrong is over and the budget is whole
    /// again.
    fn succeeded(&mut self) {
        self.began = None;
        self.first = None;
        self.pending_recovery = false;
    }

    /// True if a failure landing at `now` would already find the run's
    /// [`FRAME_BUDGET`] spent. The one comparison against `FRAME_BUDGET`
    /// this type makes; [`failed`](Self::failed) below defers to it rather
    /// than repeating the threshold. Read-only: `began` is read, not
    /// `get_or_insert`ed, so calling this never itself starts the clock.
    fn expired(&self, now: Instant) -> bool {
        self.began
            .is_some_and(|began| now.saturating_duration_since(began) >= FRAME_BUDGET)
    }

    /// Starts the run's clock, and its `first` failure, if this is the first
    /// one seen -- the bookkeeping half of [`failed`](Self::failed), split
    /// out for a caller that has already checked [`expired`](Self::expired)
    /// itself and so has no `Option` outcome left to handle: exhaustion is
    /// impossible here by construction, not by an unreachable branch.
    fn record(&mut self, err: io::Error, now: Instant) {
        self.began.get_or_insert(now);
        if self.first.is_none() {
            self.first = Some(err);
        }
    }

    /// Records a failed frame. `Some` is the error the session must leave with.
    ///
    /// Every kind that reaches it counts, and there is no carve-out for
    /// `Interrupted`, because there is nothing for one to catch: an emit
    /// retries that kind itself ([`super::deliver`]), so a signal landing
    /// between two of a frame's syscalls costs one more syscall and never
    /// arrives here.
    ///
    /// `WouldBlock` does count -- when it is the reason a vector went out
    /// whole-or-not-at-all. A screen that is permanently full is
    /// indistinguishable from one that is gone, and the whole point of the
    /// budget is that neither can hide; [`FRAME_BUDGET`] is where the room for
    /// a screen that is merely behind is given. A `WouldBlock` that arrived
    /// *after* the terminal took part of a vector never reaches this function
    /// at all: [`disposed`] ends the session on it.
    ///
    /// The *first* failure of a run never ends a session, whatever the clock
    /// says: it starts the budget rather than spending it.
    fn failed(&mut self, err: io::Error, now: Instant) -> Option<io::Error> {
        self.record(err, now);
        if self.expired(now) {
            self.first.take()
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::super::deliver::{emit_counted, RawWrite};

    use std::collections::BTreeMap;

    use tokio::sync::mpsc;

    // The debounce the loop's own cases advance a clock past. Imported rather
    // than pinned because these cases are not the ones that claim what it is --
    // `render_request::tests::the_resize_debounce_is_fifty_milliseconds` is --
    // and a second literal here would have to be kept correct beside it.
    use super::super::render_request::RESIZE_DEBOUNCE;
    use super::super::transcript::Append;

    use crate::config::{Environment, RuntimeConfig};

    /// A screen that refuses every write, as Task 2's exit test has one.
    struct BrokenScreen;

    impl RawWrite for BrokenScreen {
        fn write_once(&mut self, _bytes: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "the screen went away",
            ))
        }
    }

    impl Sink for BrokenScreen {
        fn emit(&mut self, bytes: &[u8]) -> Result<(), Emit> {
            emit_counted(self, bytes)
        }
    }

    /// A screen that refuses `refusals` writes and then works.
    struct FlakyScreen {
        refusals: usize,
        kind: io::ErrorKind,
        written: Vec<u8>,
    }

    impl RawWrite for FlakyScreen {
        fn write_once(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.refusals > 0 {
                self.refusals -= 1;
                return Err(io::Error::new(self.kind, "not now"));
            }
            self.written.extend_from_slice(bytes);
            Ok(bytes.len())
        }
    }

    impl Sink for FlakyScreen {
        fn emit(&mut self, bytes: &[u8]) -> Result<(), Emit> {
            emit_counted(self, bytes)
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

    impl Fixture {
        /// Everything the document owes, landed onto a terminal that takes
        /// everything, as the appends it was offered.
        ///
        /// What [`commit_document`] does with a sink that never refuses, for
        /// the cases that are about *what* was owed rather than about what the
        /// terminal did with it. The rows are measured at the width the shell
        /// has when this is called, which is the property under test wherever a
        /// resize sits between the text and this call.
        fn take_pending(&mut self) -> Vec<Append> {
            let mut offered = Vec::new();
            while self
                .shell
                .emit_document_front(|append| {
                    offered.push(append.clone());
                    Landed::<std::convert::Infallible>::All
                })
                .is_some()
            {}
            offered
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
                crate::tui::theme::Palette {
                    mode: crate::tui::theme::Mode::Dark,
                    depth: crate::tui::theme::Depth::Ansi256,
                },
                false,
                work,
            ),
            _work,
            _control,
        }
    }

    /// A screen that takes everything and remembers it.
    /// What an empty composer's first row begins with.
    ///
    /// Pinned as a literal rather than imported from `super::super::shell`, for
    /// the reason every needle in this crate's tests is: a test that read the
    /// constant it is checking would pass for whatever that module happened to
    /// declare.
    const COMPOSER_MARKER: &str = "> ";

    fn screen() -> FlakyScreen {
        FlakyScreen {
            refusals: 0,
            kind: io::ErrorKind::BrokenPipe,
            written: Vec::new(),
        }
    }

    #[test]
    fn the_loop_stops_taking_events_while_the_stream_it_already_has_is_deep_enough() {
        // Where the pacer's queue gets its bound, and the only place it could:
        // `Shell::apply` cannot refuse an event -- by the time it is called the
        // event is already off the channel -- so the refusal has to be *not
        // taking it*. Left there, the channel fills, the runtime parks in its
        // `send().await`, and the socket stops being read. That is
        // backpressure to the provider; the alternative is a `String` here
        // growing to the length of the whole answer.
        let mut shell = shell();
        let (events, mut receiver) = tokio::sync::mpsc::channel(bridge::UI_EVENTS);
        shell.apply(UiEvent::Delta("x".repeat(PACED_BACKLOG)));
        let waiting = UiEvent::Delta("STILL-ON-THE-CHANNEL".to_string());
        events
            .try_send(waiting.clone())
            .expect("room on the channel");

        take_ui_events(&mut shell, &mut receiver);
        assert_eq!(
            receiver.try_recv().ok(),
            Some(waiting),
            "the loop took an event onto a stream that is already deep enough"
        );

        // And the door opens again on its own, which is what makes this
        // backpressure rather than a wedge: nothing outside has to notice.
        shell.flush_paced();
        events
            .try_send(UiEvent::Delta("TAKEN".to_string()))
            .expect("room on the channel");
        take_ui_events(&mut shell, &mut receiver);
        assert!(
            receiver.try_recv().is_err(),
            "the loop was still refusing events after the stream had drained"
        );
    }

    #[test]
    fn the_bound_is_asked_between_events_rather_than_once_a_batch() {
        // The defect a batch-level check hides. `take_ui_events` may consume
        // `UI_EVENTS` messages in one turn, so a check made only on the way in
        // is a statement about the depth the batch *started* at: a session one
        // byte under the mark could take 256 more deltas past it, and the
        // number would be describing a queue that no longer exists.
        //
        // Asked between events, the overshoot is one event -- which is the
        // least it can be for a policy that may not drop one.
        let mut shell = shell();
        let (events, mut receiver) = tokio::sync::mpsc::channel(bridge::UI_EVENTS);
        let delta = PACED_BACKLOG / 4;
        for index in 0..8 {
            events
                .try_send(UiEvent::Delta("x".repeat(delta)))
                .unwrap_or_else(|_| panic!("room for delta {index}"));
        }

        take_ui_events(&mut shell, &mut receiver);

        assert!(
            shell.paced_backlog() >= PACED_BACKLOG,
            "the loop stopped before it had to: {}",
            shell.paced_backlog()
        );
        assert!(
            shell.paced_backlog() < PACED_BACKLOG + delta,
            "the loop crossed the mark and kept going: {}",
            shell.paced_backlog()
        );
        // and what it did not take is still there to be taken -- refusing is
        // not dropping
        let mut left = 0usize;
        while let Ok(UiEvent::Delta(text)) = receiver.try_recv() {
            left += text.len();
        }
        assert_eq!(
            shell.paced_backlog() + left,
            delta * 8,
            "an event went missing between the channel and the stream"
        );
    }

    #[test]
    fn an_event_is_taken_whole_however_big_it_is_and_the_door_shuts_behind_it() {
        // What this half of the bound can and cannot do, stated rather than
        // hidden. A `UiEvent` is indivisible once it is off the channel and
        // this phase may not drop one, so whatever one carries is taken in
        // full -- and what the loop does about it is shut the door behind it.
        // That leaves the overshoot at exactly one event, which is why the
        // other half (`bridge::DELTA_SLICE`) exists: it is what turns "one
        // event" into a size.
        let mut shell = shell();
        let (events, mut receiver) = tokio::sync::mpsc::channel(bridge::UI_EVENTS);
        let huge = PACED_BACKLOG * 2 + 1;
        events
            .try_send(UiEvent::Delta("x".repeat(huge)))
            .expect("room on the channel");
        let next = UiEvent::Delta("BEHIND-THE-BIG-ONE".to_string());
        events.try_send(next.clone()).expect("room on the channel");

        take_ui_events(&mut shell, &mut receiver);

        assert_eq!(
            shell.paced_backlog(),
            huge,
            "the delta was truncated to fit a buffering number"
        );
        assert_eq!(
            receiver.try_recv().ok(),
            Some(next),
            "the loop kept taking events with twice the bound already in hand"
        );
    }

    #[test]
    fn the_queue_never_grows_past_the_bound_however_the_answer_arrives() {
        // The claim the two halves add up to, measured rather than argued. A
        // third of a megabyte of answer is put on the channel exactly as
        // `UiEventSink` would put it there, and the most the UI ever holds is
        // asserted against the bound.
        //
        // Two shapes, because they stress different halves: an answer that
        // arrived in **one frame** (divided at the ingress, which is the case
        // `bridge::DELTA_SLICE` exists for) and one that arrived as a **stream
        // of small deltas** (many events, which is what the per-event check is
        // for). The peak is read after every take rather than at the end,
        // because the end is the one moment it is guaranteed to be low.
        //
        // A coarse clock on purpose: a turn a second releases `pacer::MAX_CPS`
        // and drains this in a few hundred turns. The bound is a property of
        // the arithmetic and not of the tick length, and running it at the
        // real 8 ms would spend seven minutes proving the same thing.
        let answer = "word ".repeat(70_000);
        let one_frame: Vec<String> = bridge::slices(&answer)
            .into_iter()
            .map(str::to_string)
            .collect();
        let a_stream: Vec<String> = answer
            .as_bytes()
            .chunks(37)
            .map(|piece| String::from_utf8(piece.to_vec()).expect("ascii"))
            .collect();

        for shape in [one_frame, a_stream] {
            let mut shell = shell();
            let (events, mut receiver) = tokio::sync::mpsc::channel(bridge::UI_EVENTS);
            let mut queued = shape;
            queued.reverse();
            let start = Instant::now();
            let (mut peak, mut released, mut turn) = (0usize, 0usize, 0u64);

            while !queued.is_empty() || shell.paced_backlog() > 0 {
                while let Some(piece) = queued.pop() {
                    if events.try_send(UiEvent::Delta(piece.clone())).is_err() {
                        // A full channel is the runtime parked in its send,
                        // which is the backpressure this is all about.
                        queued.push(piece);
                        break;
                    }
                }
                take_ui_events(&mut shell, &mut receiver);
                peak = peak.max(shell.paced_backlog());
                turn += 1;
                shell.settle_band(start + Duration::from_millis(turn * 1000));
                released += shell
                    .take_pending()
                    .into_iter()
                    .map(|append| append.rows.len())
                    .sum::<usize>();
                assert!(turn < 100_000, "the queue never drained");
            }

            assert!(
                peak <= PACED_BACKLOG + bridge::DELTA_SLICE,
                "the queue reached {peak} bytes, past the bound of {}",
                PACED_BACKLOG + bridge::DELTA_SLICE
            );
            assert!(peak > PACED_BACKLOG, "the bound was never approached");
            assert!(released > 0, "nothing was ever released");
        }
    }

    #[test]
    fn a_tick_that_changed_nothing_leaves_the_wire_alone() {
        // Item 11's no-op skip, at the seam that decides it. Every reason the
        // loop can raise ends up here, and several of them are raised by things
        // that changed no cell -- an animation phase turning over, a keystroke
        // the decoder absorbed, a runtime event that produced no text. A frame
        // for one of those is a whole-band repaint on a link that may be a
        // serial line.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = screen();
        let now = Instant::now();

        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            now,
            Reconciled,
        )
        .expect("the first frame");
        assert!(
            !out.written.is_empty(),
            "the session painted nothing at all, so this case proves nothing"
        );

        out.written.clear();
        shell.render.request(Reason::Animation);
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            now,
            Reconciled,
        )
        .expect("the idle tick");
        assert!(
            out.written.is_empty(),
            "a tick that changed nothing repainted the band: {:?}",
            String::from_utf8_lossy(&out.written)
        );
    }

    #[test]
    fn an_ordinary_no_change_tick_still_clears_a_budget_no_recovery_owns() {
        // `xfx-recovery-budget-narrowing.md`'s own scope: gating
        // `commit_band`'s `failures.succeeded()` on `landed ==
        // Commit::Painted` alone (the prior round's recovery fix) also
        // swallowed the reset on an ordinary, non-recovery `NoChange` tick --
        // which at base `e94b933` reset unconditionally on any `Ok(landed)`.
        // This is a genuine production path, not a seam: a plain counted
        // failure (`disposed`, no tear, no `pending_recovery`) followed by an
        // unrelated redundant tick must still clear the budget.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = screen();
        let now = Instant::now();

        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            now,
            Reconciled,
        )
        .expect("the first frame");
        assert!(
            !out.written.is_empty(),
            "the session painted nothing at all, so this case proves nothing"
        );

        assert!(
            failures
                .failed(refused(io::ErrorKind::WouldBlock), now)
                .is_none(),
            "a single counted failure ended the session on its own"
        );
        assert!(
            !failures.pending_recovery,
            "a plain counted failure must never set recovery pending"
        );

        out.written.clear();
        shell.render.request(Reason::Animation);
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            now,
            Reconciled,
        )
        .expect("the idle tick");
        assert!(
            out.written.is_empty(),
            "the idle tick repainted the band: {:?}",
            String::from_utf8_lossy(&out.written)
        );

        assert!(
            failures.began.is_none(),
            "an ordinary NoChange tick with no recovery pending did not reset the budget's clock"
        );
        assert!(
            failures.first.is_none(),
            "an ordinary NoChange tick with no recovery pending kept the counted failure"
        );
    }

    #[test]
    fn a_screen_somebody_else_wrote_on_is_repainted_whole() {
        // `Reason::ExternalDamage` is the one reason a painter cannot answer by
        // painting a difference: the shadow is a claim about what is on those
        // rows, and a resume that handed the terminal to the shell, a `/clear`
        // that erased it, or a Ctrl-L each mean that claim is false about all
        // of them. Asked with nothing else changed, so a frame that came back
        // came back for this reason and no other.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = screen();
        let now = Instant::now();

        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            now,
            Reconciled,
        )
        .expect("the first frame");
        assert!(
            String::from_utf8_lossy(&out.written).contains(COMPOSER_MARKER),
            "the session painted no composer, so this case proves nothing"
        );

        out.written.clear();
        shell.render.request(Reason::ExternalDamage);
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            now,
            Reconciled,
        )
        .expect("the frame after the damage");
        let text = String::from_utf8_lossy(&out.written);
        assert!(
            text.contains(COMPOSER_MARKER),
            "a damaged screen was diffed against a shadow that describes it no \
             longer, so the band was never put back: {text:?}"
        );
    }

    #[test]
    fn a_clear_makes_the_next_frame_a_whole_one() {
        // `/clear` erases the screen behind the diff's back: the shadow still
        // holds the band it last painted, so a frame that trusted it would find
        // nothing changed and leave the user looking at an empty terminal for
        // the rest of the session.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = screen();
        let now = Instant::now();

        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            now,
            Reconciled,
        )
        .expect("the first frame");
        let painted = String::from_utf8_lossy(&out.written).into_owned();
        assert!(
            painted.contains(COMPOSER_MARKER),
            "the first frame painted no composer, so this case proves nothing: {painted:?}"
        );

        // Through the command the user really types, because the clear is the
        // shell's decision and a test that set the flag itself would prove
        // nothing about the path that sets it.
        shell.route_bytes(b"/clear\r");

        out.written.clear();
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            now,
            Reconciled,
        )
        .expect("the frame after the clear");
        let text = String::from_utf8_lossy(&out.written);
        assert!(
            text.contains(super::super::shell::CLEAR_SCREEN),
            "the clear never reached the screen: {text:?}"
        );
        assert!(
            text.contains(COMPOSER_MARKER),
            "the band was not repainted onto the screen the clear emptied: {text:?}"
        );
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
    fn an_exit_whose_drain_carried_nothing_still_writes_what_the_pacer_held() {
        // The exit the drain **cannot** cover, and the commonest one there is.
        // A turn that concluded while its answer was still being released
        // leaves an empty channel behind it: the drain takes no event, the
        // per-event flush never runs, and what the pacer is still holding is an
        // answer the user was in the middle of reading. Phase 1 never repaints
        // a document row, so the flush at the end of `shut_down` is the last
        // moment those bytes can reach the terminal at all.
        //
        // The drain is a parameter, so "carried nothing" is a fact this case
        // states rather than one it has to arrange on a real runtime -- where
        // it is not arrangeable at all: a turn's conclusion is one of the lines
        // held behind the stream, so nothing on the terminal says the channel
        // has gone quiet.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = screen();
        shell.apply(UiEvent::Delta("TAIL-NOBODY-DRAINED".to_string()));
        shell.apply(UiEvent::TurnEnded { failure: None });
        assert!(
            shell.paced_backlog() > 0,
            "the stream was empty, so this case proves nothing"
        );

        let broken = shut_down(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            true,
            Shutdown {
                reconcile: |_, _| Ok(Reconciled),
                drain: |_taken| {
                    // The runtime had already said everything and gone quiet.
                },
                size: || panic!("an exit with no winch outstanding measured the screen"),
            },
        );

        assert!(broken.is_none(), "the screen refused: {broken:?}");
        assert!(
            String::from_utf8_lossy(&out.written).contains("TAIL-NOBODY-DRAINED"),
            "the band came down on text the runtime had already handed over"
        );
    }

    #[test]
    fn an_exit_whose_drain_carried_an_event_writes_that_and_the_stream_with_it() {
        // The other half of the same call, so neither is wired by accident: an
        // event taken during the drain is shown *and* painted, and the stream
        // it arrived on top of goes out with it rather than a frame later.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = screen();
        shell.apply(UiEvent::Delta("ALREADY-STREAMING".to_string()));

        let broken = shut_down(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            true,
            Shutdown {
                reconcile: |_, _| Ok(Reconciled),
                drain: |taken| {
                    taken(UiEvent::TurnEnded {
                        failure: Some("WHY-THE-TURN-ENDED".to_string()),
                    });
                },
                size: || panic!("an exit with no winch outstanding measured the screen"),
            },
        );

        assert!(broken.is_none(), "the screen refused: {broken:?}");
        let written = String::from_utf8_lossy(&out.written);
        assert!(
            written.contains("ALREADY-STREAMING"),
            "the drain dropped the answer it was draining on top of: {written:?}"
        );
        assert!(
            written.contains("WHY-THE-TURN-ENDED"),
            "the drain took the turn's conclusion and never wrote it: {written:?}"
        );
    }

    #[test]
    fn an_exit_onto_a_screen_that_has_already_refused_paints_nothing_more() {
        // `painting` is the session's own verdict on the terminal, and an exit
        // that ignored it would hand a frame to a screen the session has just
        // given up on. The events are still applied, because one of them may be
        // the `Fatal` that explains everything.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = screen();
        shell.apply(UiEvent::Delta("NOT-FOR-A-DEAD-SCREEN".to_string()));

        let broken = shut_down(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            false,
            Shutdown {
                reconcile: |_, _| panic!("a screen the session gave up on was reconciled"),
                drain: |taken| taken(UiEvent::Fatal("A-TURN-CAME-APART".to_string())),
                size: || panic!("a screen the session gave up on was measured"),
            },
        );

        assert!(broken.is_none());
        assert!(out.written.is_empty(), "a frame reached a refused screen");
        assert_eq!(shell.fatal(), Some("A-TURN-CAME-APART"));
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

    /// A screen that takes `prefix` bytes of what it is offered, fails once,
    /// and takes everything after that.
    ///
    /// The failure is *not* permanent, and that is what makes the cases below
    /// discriminating: a screen that refused everything afterwards would leave
    /// "the loop stopped offering" and "the screen kept refusing" telling the
    /// same story on the wire. Here, anything a post-failure offer carries
    /// **lands**, so bytes past the prefix in `written` are proof that
    /// something was offered after a prefix had already reached the terminal.
    struct HalfDeaf {
        prefix: usize,
        failed: bool,
        written: Vec<u8>,
    }

    impl HalfDeaf {
        fn taking(prefix: usize) -> Self {
            Self {
                prefix,
                failed: false,
                written: Vec::new(),
            }
        }
    }

    impl RawWrite for HalfDeaf {
        fn write_once(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.failed || bytes.is_empty() {
                self.written.extend_from_slice(bytes);
                return Ok(bytes.len());
            }
            if self.prefix > 0 {
                let taken = self.prefix.min(bytes.len());
                self.prefix -= taken;
                self.written.extend_from_slice(&bytes[..taken]);
                return Ok(taken);
            }
            self.failed = true;
            Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "the screen stopped taking bytes",
            ))
        }
    }

    impl Sink for HalfDeaf {
        fn emit(&mut self, bytes: &[u8]) -> Result<(), Emit> {
            emit_counted(self, bytes)
        }
    }

    /// A screen that takes `prefix` bytes of the first vector it is offered
    /// and then refuses everything for good, including a later synchronous
    /// recovery attempt's own fixed cleanup vector.
    ///
    /// [`HalfDeaf`] heals after its one failure, which is what
    /// [`Band::recover_primary`]'s cleanup vector finds and lands on. This one
    /// never does, so it stands in for the tear P3-COMMIT's containment is
    /// actually about: one no fixed reset this build knows can undo.
    struct IrrecoverablyTorn {
        prefix: usize,
        written: Vec<u8>,
        /// Every vector [`Sink::emit`] was asked to send, whole, in the order
        /// it was offered -- distinct from `written`, which is only the bytes
        /// this screen actually *took*. A cleanup vector refused outright is
        /// one entry here and none of it in `written`; recording only
        /// `written` cannot tell that apart from a cleanup that was never
        /// offered at all.
        offered: Vec<Vec<u8>>,
    }

    impl IrrecoverablyTorn {
        fn taking(prefix: usize) -> Self {
            Self {
                prefix,
                written: Vec::new(),
                offered: Vec::new(),
            }
        }
    }

    impl RawWrite for IrrecoverablyTorn {
        fn write_once(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.prefix > 0 {
                let taken = self.prefix.min(bytes.len());
                self.prefix -= taken;
                self.written.extend_from_slice(&bytes[..taken]);
                return Ok(taken);
            }
            Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "the screen stopped taking bytes for good",
            ))
        }
    }

    impl Sink for IrrecoverablyTorn {
        fn emit(&mut self, bytes: &[u8]) -> Result<(), Emit> {
            self.offered.push(bytes.to_vec());
            emit_counted(self, bytes)
        }
    }

    #[test]
    fn a_frame_the_screen_took_only_part_of_ends_the_session_rather_than_being_offered_again() {
        // **The containment this unit exists for.** A refused frame is a frame
        // the terminal never saw, and the budget may offer it again. A frame
        // the terminal took a *prefix* of is a terminal holding half a vector,
        // and P3-COMMIT gives this build exactly one fixed thing to try on
        // that: `Band::recover_primary`. `IrrecoverablyTorn` refuses that
        // attempt too, so this stays the case recovery cannot be assumed to
        // fix -- there is no vector this session can write that is *known* to,
        // and re-offering the whole one would send the prefix twice. So the
        // tick does not end -- the session does.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut screen = IrrecoverablyTorn::taking(12);

        let err = commit_frame(
            &mut shell,
            &mut band,
            &mut screen,
            &mut failures,
            Instant::now(),
            Reconciled,
        )
        .expect_err(
            "a frame the screen took part of, with no recovery available, ends the session",
        );

        assert_eq!(
            screen.written.len(),
            12,
            "something was offered after the prefix: {:?}",
            String::from_utf8_lossy(&screen.written)
        );
        assert!(
            err.to_string().contains("12"),
            "the failure does not say how much the screen accepted: {err}"
        );
    }

    #[test]
    fn a_partial_frame_on_the_way_out_still_lets_the_drain_collect_what_the_runtime_has() {
        // The exit's two halves are independent: a screen that took half a
        // frame ends the painting, and the drain still runs -- the event it
        // carries may be the `Fatal` that explains the session.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = HalfDeaf::taking(12);
        shell.apply(UiEvent::Delta("HALF-A-FRAME".to_string()));

        let broken = shut_down(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            true,
            Shutdown {
                reconcile: |_, _| Ok(Reconciled),
                drain: |taken| taken(UiEvent::Fatal("A-TURN-CAME-APART".to_string())),
                size: || panic!("an exit with no winch outstanding measured the screen"),
            },
        );

        let broken = broken.expect("a screen that took half a frame was reported as sound");
        assert!(
            broken.to_string().contains("12"),
            "the exit's failure does not say how much the screen accepted: {broken}"
        );
        assert_eq!(
            shell.fatal(),
            Some("A-TURN-CAME-APART"),
            "the drain stopped collecting because the screen had failed"
        );
        assert_eq!(
            out.written.len(),
            12,
            "a frame was painted after the screen took half of one: {:?}",
            String::from_utf8_lossy(&out.written)
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
        // The reason `failed` has no `Interrupted` branch: an emit retries that
        // kind itself, so a signal arriving mid-frame costs one more syscall
        // and nothing else. This pins the fact the policy rests on -- a frame
        // built from a bare `write` instead would land on the screen truncated,
        // and a policy that counted the interruption would be counting a frame
        // that was really on the screen.
        //
        // Judged against the *same* frame on a screen that was not interrupted,
        // rather than against a rebuilt one: a frame is a difference from what
        // the terminal already holds, so rebuilding it after the fact would
        // rebuild it against a shadow the frame has already advanced and
        // compare two things that were never the same claim.
        let mut twin = shell();
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut interrupted = FlakyScreen {
            refusals: 1,
            kind: io::ErrorKind::Interrupted,
            written: Vec::new(),
        };
        commit_frame(
            &mut shell,
            &mut band,
            &mut interrupted,
            &mut failures,
            Instant::now(),
            Reconciled,
        )
        .expect("an interrupted write is retried by the write itself");

        let mut undisturbed = screen();
        commit_frame(
            &mut twin,
            &mut Band::new(),
            &mut undisturbed,
            &mut FrameFailures::default(),
            Instant::now(),
            Reconciled,
        )
        .expect("the same frame, uninterrupted");

        assert!(
            !undisturbed.written.is_empty(),
            "neither screen was painted, so this case proves nothing"
        );
        assert_eq!(
            String::from_utf8_lossy(&interrupted.written),
            String::from_utf8_lossy(&undisturbed.written),
            "the interrupted frame reached the screen short"
        );
        assert_eq!(interrupted.refusals, 0, "the refusal was never spent");
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
    fn a_zero_progress_append_refusal_is_retried_exactly_once_next_tick() {
        // Superseded rationale, kept as a note rather than deleted silently:
        // this case used to assert the opposite -- that a refused append was
        // *never* replayed, on the theory that a refusal of unknown shape may
        // have moved the screen before it failed. `FlakyScreen`'s refusal is a
        // `ZeroProgress` (`deliver::classify`'s own accounting: nothing was
        // taken before it failed), so that theory does not reach it -- the
        // terminal is exactly as it was, and an append never offered to it is
        // still owed. Dropping it, as the old code did by always resuming
        // after the failed index, was not the safety the comment claimed: it
        // was the row gone from the session for good.
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
        assert_eq!(
            text.matches("answered").count(),
            1,
            "a zero-progress refusal must land its row exactly once, not zero \
             and not twice: {text:?}"
        );
        assert!(
            text.contains("\u{1b}[22;1H"),
            "the frame the refused append asked for was never painted: {text:?}"
        );
    }

    #[test]
    fn a_zero_progress_refusal_and_what_was_ahead_of_it_both_land_next_tick_in_order() {
        // Superseded rationale, kept as a note: this case used to assert that
        // the refused row ("refused") was gone for good and only what was
        // queued behind it ("behind it") survived. That was true only because
        // the old code resumed the batch *after* the failed index for every
        // kind of refusal, including the two (`Rejected`, `ZeroProgress`) that
        // provably moved no bytes. Both are owed again now, oldest first: the
        // one that failed and the one that was never offered.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let start = Instant::now();
        let mut screen = FlakyScreen {
            refusals: 1,
            kind: io::ErrorKind::BrokenPipe,
            written: Vec::new(),
        };
        // Two document writes, because one whole line each is what the shell
        // owes per line of an answer: the first is refused, the second is never
        // tried.
        shell.write_transcript("refused\n");
        shell.write_transcript("behind it\n");

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
        assert_eq!(
            text.matches("refused").count(),
            1,
            "the append that failed with zero progress must land exactly once: {text:?}"
        );
        assert!(
            text.contains("behind it"),
            "an append the terminal was never offered was dropped with the one \
             it was behind: {text:?}"
        );
        let refused_at = text.find("refused").expect("the retried refusal");
        let behind_at = text.find("behind it").expect("the append behind it");
        assert!(
            refused_at < behind_at,
            "the retried batch did not keep its original order: {text:?}"
        );
    }

    /// A [`Sink`] scripted at the `emit` boundary itself, unlike
    /// [`FlakyScreen`]'s [`RawWrite`] one.
    ///
    /// The seam exists for one `Emit` variant `FlakyScreen` cannot produce
    /// from outside `frame.rs`: `Rejected` is minted by `Band`'s own preflight
    /// check, and the tamper hook that makes a real one fail
    /// (`Band::tamper_with`) is private to that module. This does not
    /// exercise the checker -- every vector it is handed still passes it, so
    /// `preflight` always returns `Ok` here -- it exercises what the batch
    /// loop in `commit_document` does with the `Emit` it is given, which is
    /// the same question for a real refusal and a scripted one.
    struct ScriptedSink {
        answers: std::collections::VecDeque<Result<(), Emit>>,
        calls: Vec<Vec<u8>>,
    }

    impl ScriptedSink {
        fn new(answers: impl IntoIterator<Item = Result<(), Emit>>) -> Self {
            Self {
                answers: answers.into_iter().collect(),
                calls: Vec::new(),
            }
        }
    }

    impl Sink for ScriptedSink {
        fn emit(&mut self, bytes: &[u8]) -> Result<(), Emit> {
            self.calls.push(bytes.to_vec());
            self.answers.pop_front().unwrap_or(Ok(()))
        }
    }

    #[test]
    fn a_rejected_append_in_the_middle_of_a_batch_is_retried_with_what_follows_it_and_not_what_already_landed(
    ) {
        // The cross-product the take-the-whole-batch shape makes possible to
        // get wrong: a batch of three, the first of which already landed on a
        // *previous* tick's own terms, is not the same case as a refusal
        // discovered mid-batch on the tick that offers all three together.
        // This is the second: one `commit_frame` call offers `[first, second,
        // third]`, `second` is refused, and the assertion is that `first` --
        // already on the terminal -- is never reoffered, while `second` and
        // `third` are, in order, on the very next tick.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let start = Instant::now();
        let mut screen = ScriptedSink::new([
            Ok(()),
            Err(Emit::Rejected(io::Error::other(
                "the output check refused this vector",
            ))),
        ]);

        shell.write_transcript("first\n");
        shell.write_transcript("second\n");
        shell.write_transcript("third\n");

        commit_frame(
            &mut shell,
            &mut band,
            &mut screen,
            &mut failures,
            start,
            Reconciled,
        )
        .expect("a rejection within budget is not fatal");
        assert_eq!(
            screen.calls.len(),
            2,
            "the batch must stop at the refusal, not run past it or fall short: {:?}",
            screen.calls
        );

        commit_frame(
            &mut shell,
            &mut band,
            &mut screen,
            &mut failures,
            at(start, 8),
            Reconciled,
        )
        .expect("the retried tail of the batch");
        // From the second tick on -- everything up to and including the
        // rejection is the first tick's own and already checked above; a band
        // frame call may follow the retried appends once they all land, and
        // is not itself part of what this case is about.
        let retried: Vec<String> = screen.calls[2..]
            .iter()
            .map(|call| String::from_utf8_lossy(call).into_owned())
            .collect();
        assert!(
            retried.iter().any(|call| call.contains("second")),
            "the rejected append was not retried: {retried:?}"
        );
        assert!(
            retried.iter().any(|call| call.contains("third")),
            "what was queued behind the rejected append was not retried: {retried:?}"
        );
        assert!(
            !retried.iter().any(|call| call.contains("first")),
            "an append that already landed on the terminal was reoffered: {retried:?}"
        );
        assert_eq!(
            screen
                .calls
                .iter()
                .filter(|call| String::from_utf8_lossy(call).contains("first"))
                .count(),
            1,
            "an append that already landed must appear exactly once across the whole run: {:?}",
            screen.calls
        );
    }

    #[test]
    fn a_partial_append_ends_the_session_and_only_what_was_never_offered_stays_owed() {
        // The sibling of the case above, for the kind of failure the resume
        // point does *not* move for: a `Partial` already put bytes on the
        // terminal, so it is not owed again -- only what follows it, which the
        // terminal was never offered at all, survives. And unlike a
        // `Rejected` or a `ZeroProgress`, it is fatal on the spot rather than
        // spending the budget (`disposed`), because there is no vector this
        // session could write that is known to complete the one that tore.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut screen = ScriptedSink::new([
            Ok(()),
            Err(Emit::Partial {
                delivered: 3,
                cause: io::Error::new(io::ErrorKind::BrokenPipe, "gone mid-write"),
            }),
        ]);

        shell.write_transcript("first\n");
        shell.write_transcript("second\n");
        shell.write_transcript("third\n");

        let err = commit_frame(
            &mut shell,
            &mut band,
            &mut screen,
            &mut failures,
            Instant::now(),
            Reconciled,
        )
        .expect_err("a partial write ends the session rather than being retried");
        assert!(err.to_string().contains('3'), "{err}");

        let untried = shell.take_pending();
        assert_eq!(
            untried.len(),
            1,
            "only the append queued behind the torn one should still be owed: {untried:?}"
        );
        assert!(
            untried[0].rows.iter().any(|row| row.contains("third")),
            "the wrong append survived the partial failure: {untried:?}"
        );
    }

    #[test]
    fn a_zero_progress_clear_is_retried_next_tick_and_text_queued_meanwhile_survives() {
        // `take_clearing` consumes the flag before the write is even
        // attempted, so a `ZeroProgress` refusal -- which moved no bytes --
        // has nowhere else to leave the intent but `restore_clearing`. Left
        // unrearmed, `/clear` is silently forgotten: nothing else ever sets
        // `clearing` again.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let start = Instant::now();
        let mut screen = FlakyScreen {
            refusals: 1,
            kind: io::ErrorKind::BrokenPipe,
            written: Vec::new(),
        };

        shell.route_bytes(b"/clear\r");
        assert!(
            shell.owes_document(),
            "the clear's own notice was not queued as a document row"
        );

        commit_frame(
            &mut shell,
            &mut band,
            &mut screen,
            &mut failures,
            start,
            Reconciled,
        )
        .expect("a zero-progress refusal of the clear is not fatal");
        assert!(
            screen.written.is_empty(),
            "the refused clear must not have reached the terminal"
        );

        // Text queued strictly between the refusal and the retry: the case
        // `restore_clearing` exists to keep separate from `clear_screen`,
        // which would drop this along with everything else `/clear` already
        // discarded once.
        shell.write_transcript("fresh text\n");

        commit_frame(
            &mut shell,
            &mut band,
            &mut screen,
            &mut failures,
            at(start, 8),
            Reconciled,
        )
        .expect("the retried clear");

        let text = String::from_utf8(screen.written).expect("utf-8");
        assert_eq!(
            text.matches("\u{1b}[H\u{1b}[2J\u{1b}[3J").count(),
            1,
            "the clear vector must land exactly once, not zero and not twice: {text:?}"
        );
        let clear_at = text
            .find("\u{1b}[H\u{1b}[2J\u{1b}[3J")
            .expect("the retried clear");
        let notice_at = text
            .find("cleared the screen")
            .expect("the clear's own notice");
        let fresh_at = text
            .find("fresh text")
            .expect("text queued while the clear was still owed was dropped with it");
        assert!(
            clear_at < notice_at && notice_at < fresh_at,
            "the retried clear did not keep its order relative to what followed it: {text:?}"
        );
    }

    #[test]
    fn a_partial_clear_ends_the_session_and_the_intent_is_not_rearmed() {
        // The sibling that does not resume: a `Partial` clear already put
        // some of its vector on the terminal, so `disposed` ends the session
        // on it rather than counting it against the budget -- and re-arming
        // `clearing` would be a flag a session that is about to end never
        // reads again.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut screen = HalfDeaf::taking(3);

        shell.route_bytes(b"/clear\r");

        let err = commit_frame(
            &mut shell,
            &mut band,
            &mut screen,
            &mut failures,
            Instant::now(),
            Reconciled,
        )
        .expect_err("a clear the terminal took part of ends the session rather than being retried");
        assert!(err.to_string().contains('3'), "{err}");
        assert!(
            !shell.take_clearing(),
            "a partial clear left the intent armed as if nothing had reached the terminal"
        );
    }

    // -----------------------------------------------------------------------
    // the theme query a resume owes
    // -----------------------------------------------------------------------

    /// `DSR ? 996 n`, spelled here rather than imported for the reason every
    /// needle in this module's tests is: a test that read the constant it is
    /// checking would pass for whatever `super::super::theme` declared.
    const THEME_QUERY: &str = "\u{1b}[?996n";

    #[test]
    fn a_resume_asks_the_terminal_which_way_round_it_is_now() {
        // The gap mode 2031 cannot close. A stop hands the terminal back with
        // the subscription off, so whatever the user did to their theme while
        // their shell had it was said to nobody; re-announcing on the way back
        // subscribes to the *next* change and cannot report the one that was
        // missed. Armed by the resume and written by the tick, once.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = screen();
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            Instant::now(),
            Reconciled,
        )
        .expect("the first frame");
        out.written.clear();

        shell.resync_theme();
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            Instant::now(),
            Reconciled,
        )
        .expect("the tick that carries the query");
        let text = String::from_utf8_lossy(&out.written).to_string();
        assert_eq!(
            text.matches(THEME_QUERY).count(),
            1,
            "the resync did not reach the terminal exactly once: {text:?}"
        );

        out.written.clear();
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            Instant::now(),
            Reconciled,
        )
        .expect("the tick after it");
        let text = String::from_utf8_lossy(&out.written).to_string();
        assert!(
            !text.contains(THEME_QUERY),
            "one resume put a query on every tick that followed it: {text:?}"
        );
    }

    #[test]
    fn a_theme_query_reaches_a_screen_no_band_fits_on() {
        // The state a resync is worth the most in, and the one every write
        // below it refuses to make: on a screen the band cannot be painted on,
        // `commit_frame` returns without addressing a row, because a row number
        // out of a geometry that no longer describes the screen is a `CUP` the
        // terminal clamps and an append that cannot be taken back. The query
        // carries no coordinates at all, so it means the same thing on any
        // screen -- and a session can sit in this state for as long as somebody
        // leaves the window small.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = screen();
        shell.render.mark_resize(Instant::now());
        assert!(shell.blind(), "this case does not start where it means to");

        shell.resync_theme();
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            Instant::now(),
            Reconciled,
        )
        .expect("the tick");

        assert_eq!(
            String::from_utf8_lossy(&out.written),
            THEME_QUERY,
            "a blind tick wrote something other than the query, or nothing at all"
        );
    }

    #[test]
    fn a_theme_query_is_accepted_on_either_plane_and_moves_no_cell_on_either() {
        // What the declaration can and cannot prove, asserted rather than
        // implied. These bytes touch no cell on the primary buffer and none on
        // the borrowed one, so the output check accepts them against a seed
        // that names either -- which is why `check_theme_query`'s plane is a
        // true statement about where the bytes land rather than a guard. A
        // future edit that gave this vector a cell would find exactly one of
        // these two accepting it.
        for on_alternate in [false, true] {
            check_theme_query(THEME_QUERY.as_bytes(), on_alternate)
                .unwrap_or_else(|err| panic!("on_alternate={on_alternate}: {err}"));
        }
        // And the emptiness is what carries it: a vector that placed a cell is
        // refused on both, so "accepted on either" is a statement about *this*
        // vector and not about the intent being lax.
        for on_alternate in [false, true] {
            let with_a_cell = format!("{THEME_QUERY}\u{1b}[1;1Hx");
            check_theme_query(with_a_cell.as_bytes(), on_alternate)
                .expect_err("a query vector that wrote a cell");
        }
    }

    #[test]
    fn a_question_on_the_other_plane_does_not_hold_the_theme_query_back() {
        // A question the band cannot show can own the terminal for minutes
        // while somebody reads a change, which is exactly long enough for a
        // background to move under it. `commit_frame` hands the rest of the
        // tick to `paint_alternate` and returns, so a query written after that
        // branch would land when the question was answered -- and the answer is
        // the moment the resync stops being worth anything.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = CountingScreen::default();
        on_the_alternate_plane(&mut shell, &mut band, &mut out, &mut failures);
        assert!(band.on_alternate(), "the plane was never taken");
        out.written.clear();

        shell.resync_theme();
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            Instant::now(),
            Reconciled,
        )
        .expect("the tick while the question is up");

        let text = String::from_utf8_lossy(&out.written).to_string();
        assert!(
            text.starts_with(THEME_QUERY),
            "the query did not go out above the plane's own frame: {text:?}"
        );
        assert_eq!(
            shell.screen_owner(),
            ScreenOwner::Approval,
            "the query gave the question's plane away"
        );
    }

    #[test]
    fn a_refused_theme_query_is_still_owed_on_the_next_tick() {
        // `take_theme_query` consumes the flag before the write is attempted,
        // so a refusal that moved no byte has nowhere to leave the intent but
        // `restore_theme_query` -- and nothing else ever sets it again for a
        // resume that has already been adopted. The same disposal `/clear`
        // gets, and the same reason.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let start = Instant::now();
        let mut out = FlakyScreen {
            refusals: 1,
            kind: io::ErrorKind::BrokenPipe,
            written: Vec::new(),
        };

        shell.resync_theme();
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            start,
            Reconciled,
        )
        .expect("a refused query is not fatal");
        assert!(
            out.written.is_empty(),
            "the refused query reached the terminal"
        );

        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            at(start, 8),
            Reconciled,
        )
        .expect("the retried query");
        let text = String::from_utf8_lossy(&out.written).to_string();
        assert_eq!(
            text.matches(THEME_QUERY).count(),
            1,
            "the query was forgotten, or asked twice: {text:?}"
        );
    }

    #[test]
    fn a_partial_theme_query_ends_the_session_and_is_not_rearmed() {
        // The sibling that does not resume. A terminal that took part of the
        // vector holds a fragment of an escape sequence, and offering the whole
        // thing again would put that fragment on the screen twice; `disposed`
        // ends the session on it, and re-arming would be a flag nobody reads.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = HalfDeaf::taking(3);

        shell.resync_theme();
        let err = commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            Instant::now(),
            Reconciled,
        )
        .expect_err("a query the terminal took part of ends the session");
        assert!(err.to_string().contains('3'), "{err}");
        assert!(
            !shell.take_theme_query(),
            "a partial query left the intent armed as if nothing had reached the terminal"
        );
    }

    #[test]
    fn a_theme_query_that_lands_does_not_mend_the_frame_budget() {
        // The budget is a claim about **frames**: a screen that has refused
        // every one of them for `FRAME_BUDGET` ends the session rather than
        // retrying invisibly. Seven bytes with no cells behind them are not
        // evidence against that, and a `succeeded` here would let a session
        // whose screen is gone reset the run every time a resync slipped
        // through -- which is the one failure the budget exists to make
        // impossible to hide.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = screen();
        let start = Instant::now();
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            start,
            Reconciled,
        )
        .expect("the first frame");
        assert!(
            shell.render.begin().is_none(),
            "the session still owes a frame, so the tick below would land one \
             and mend the budget for a reason this case is not about"
        );

        // A run of refusals begins, and then a query lands in the middle of it.
        assert!(failures
            .failed(refused(io::ErrorKind::BrokenPipe), start)
            .is_none());
        shell.resync_theme();
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            at(start, 100),
            Reconciled,
        )
        .expect("the query");

        // The run is still the one that began at `start`, so the budget is
        // spent on schedule rather than restarted.
        assert!(
            failures
                .failed(refused(io::ErrorKind::BrokenPipe), at(start, 501))
                .is_some(),
            "a query that landed mended a budget the screen had not"
        );
    }

    // -----------------------------------------------------------------------
    // the document rows a theme report finds already on the screen
    // -----------------------------------------------------------------------
    //
    // The greys and the report are spelled out rather than imported, for the
    // reason every needle in this module's tests is: an expectation built from
    // the accessor under test passes for whatever that accessor says.

    /// What a terminal with mode 2031 sends when its background goes light,
    /// and back.
    const WENT_LIGHT: &[u8] = b"\x1b[?997;2n";
    const WENT_DARK: &[u8] = b"\x1b[?997;1n";

    /// The answer text's own grey, dark (`theme::DARK`'s `255`) and light
    /// (`theme::LIGHT`'s `235`).
    const BODY_DARK: &str = "\u{1b}[38;5;255m";
    const BODY_LIGHT: &str = "\u{1b}[38;5;235m";
    /// What xfx's own lines are painted in, in both modes.
    const NOTICE_DARK: &str = "\u{1b}[38;5;250m";
    const NOTICE_LIGHT: &str = "\u{1b}[38;5;241m";

    /// One tick of the loop's writer, onto a screen that takes everything.
    fn tick(
        shell: &mut Fixture,
        band: &mut Band,
        out: &mut FlakyScreen,
        failures: &mut FrameFailures,
        now: Instant,
    ) -> String {
        out.written.clear();
        commit_frame(shell, band, out, failures, now, Reconciled).expect("a tick");
        String::from_utf8(out.written.clone()).expect("a frame is text and escapes")
    }

    /// A session whose document holds the echo of a prompt, one finished
    /// answer row and one of xfx's own lines -- every byte of it delivered.
    ///
    /// The band is painted **first**, because that is the frame that sizes the
    /// band's shadow: rows appended before a session has drawn anything land on
    /// the screen without the band ever recording them, which is a session with
    /// no document it can describe rather than the one these cases are about.
    fn a_document_on_the_screen(
        shell: &mut Fixture,
        band: &mut Band,
        out: &mut FlakyScreen,
        failures: &mut FrameFailures,
        now: Instant,
    ) {
        tick(shell, band, out, failures, now);
        shell.route_bytes(b"hello\r");
        shell.apply(UiEvent::Delta("a streamed answer\n".to_string()));
        shell.apply(UiEvent::Notice("[tool] read_file ok".to_string()));
        shell.flush_paced();
        let landed = tick(shell, band, out, failures, now);
        for wanted in [
            format!("{BODY_DARK}a streamed answer"),
            format!("{NOTICE_DARK}[tool] read_file ok"),
        ] {
            assert!(
                landed.contains(&wanted),
                "the document this case is about never reached the screen: {landed:?}"
            );
        }
    }

    #[test]
    fn a_theme_report_recolours_the_document_rows_that_are_still_on_the_screen() {
        // The whole of what U-B is for. A background that went light leaves
        // every answer row already on the screen painted in the dark palette's
        // near-white -- on white -- and this phase repaints no document row, so
        // without this they stay that way for as long as the session lasts.
        //
        // What may move is the colour around a row and nothing inside one: the
        // text, its place and the user's own echo are what they were.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = screen();
        let now = Instant::now();
        a_document_on_the_screen(&mut shell, &mut band, &mut out, &mut failures, now);

        shell.route_bytes(WENT_LIGHT);
        let written = tick(&mut shell, &mut band, &mut out, &mut failures, now);

        assert!(
            written.contains(&format!("{BODY_LIGHT}a streamed answer")),
            "the answer on the screen was not repainted for the terminal the \
             user has now: {written:?}"
        );
        assert!(
            written.contains(&format!("{NOTICE_LIGHT}[tool] read_file ok")),
            "xfx's own line kept the palette the session started in: {written:?}"
        );
        assert!(
            !written.contains('\n'),
            "the recolour scrolled the screen, which puts a row into native \
             scrollback that nothing can take back: {written:?}"
        );
        assert!(
            !written.contains("hello"),
            "the recolour rewrote the user's own echo, which this surface \
             never painted a colour onto: {written:?}"
        );
        assert!(
            !written.contains("\u{1b}[2J")
                && !written.contains("\u{1b}[3J")
                && !written.contains("\u{1b}[?1049"),
            "the recolour erased a screen or moved a plane: {written:?}"
        );
    }

    #[test]
    fn a_report_that_flipped_and_flipped_back_before_it_was_paid_costs_nothing() {
        // The debt carries no palette of its own -- it is "the document, in
        // whatever palette is in force when it is paid" -- so a terminal that
        // reported light and dark again inside one tick is a terminal whose
        // document is already right. A debt that remembered an old-to-new pair
        // would repaint every cell twice and end on the wrong grey.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = screen();
        let now = Instant::now();
        a_document_on_the_screen(&mut shell, &mut band, &mut out, &mut failures, now);

        shell.route_bytes(WENT_LIGHT);
        shell.route_bytes(WENT_DARK);
        let written = tick(&mut shell, &mut band, &mut out, &mut failures, now);

        assert!(
            !written.contains("an answer") && !written.contains("[tool] ran"),
            "a report that changed nothing in the end still rewrote the \
             document: {written:?}"
        );
        assert!(
            !shell.owes_retint(),
            "the recolour was left owed after the tick that settled it"
        );
    }

    #[test]
    fn a_recolour_the_screen_refused_is_offered_again_on_the_next_tick() {
        // Nothing takes the debt, so nothing has to re-arm it: a refused
        // vector moved no byte, and the next tick offers exactly the same one
        // -- under the same frame budget every other refused write spends.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = screen();
        let now = Instant::now();
        a_document_on_the_screen(&mut shell, &mut band, &mut out, &mut failures, now);

        shell.route_bytes(WENT_LIGHT);
        out.refusals = 1;
        let refused = tick(&mut shell, &mut band, &mut out, &mut failures, now);
        assert!(
            refused.is_empty(),
            "the refused tick wrote something: {refused:?}"
        );
        assert!(
            shell.owes_retint(),
            "a refused recolour was recorded as if it had landed"
        );

        let written = tick(&mut shell, &mut band, &mut out, &mut failures, now);
        assert!(
            written.contains(&format!("{BODY_LIGHT}a streamed answer")),
            "the recolour the screen refused was never offered again: {written:?}"
        );
        assert!(!shell.owes_retint());

        // And a report that arrives while one is still owed **moves** what is
        // owed rather than queueing behind it: the debt is the document in the
        // palette that is in force, so a retry writes where the terminal is
        // now rather than where it was when the refusal happened.
        shell.route_bytes(WENT_DARK);
        out.refusals = 1;
        tick(&mut shell, &mut band, &mut out, &mut failures, now);
        shell.route_bytes(WENT_LIGHT);
        let converged = tick(&mut shell, &mut band, &mut out, &mut failures, now);
        assert!(
            !converged.contains(&format!("{BODY_DARK}a streamed answer")),
            "the retry wrote the palette of the report that was refused rather \
             than the one the terminal has now: {converged:?}"
        );
    }

    #[test]
    fn a_recolour_the_terminal_took_part_of_ends_the_session_rather_than_being_replayed() {
        // The road every document vector is on. A terminal that took a prefix
        // and stopped holds a fragment of an escape sequence, and the whole
        // vector is the only thing this session could offer again -- which
        // would put that fragment on the screen twice. There is no recovery
        // and no replay for one of these: `disposed` ends the session on the
        // spot, whatever the budget says.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = screen();
        let now = Instant::now();
        a_document_on_the_screen(&mut shell, &mut band, &mut out, &mut failures, now);

        shell.route_bytes(WENT_LIGHT);
        let mut torn = HalfDeaf::taking(3);
        let err = commit_frame(
            &mut shell,
            &mut band,
            &mut torn,
            &mut failures,
            now,
            Reconciled,
        )
        .expect_err("a recolour the terminal took part of ends the session");
        assert!(err.to_string().contains('3'), "{err}");
        // The vector the terminal tore is **this** one: a tick that had
        // recoloured nothing would have settled the debt on its way past.
        assert!(
            shell.owes_retint(),
            "the torn vector was not the recolour, so this case proves nothing"
        );
    }

    #[test]
    fn a_report_while_a_question_owns_the_other_plane_waits_for_the_plane_to_come_back() {
        // A document row cannot be written on the buffer a question borrowed:
        // the terminal hands that buffer back on the way out, so those bytes
        // would be lost, and the rows they were meant for are still in the dark
        // palette when the user comes back to them. The debt waits instead.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = screen();
        let now = Instant::now();
        a_document_on_the_screen(&mut shell, &mut band, &mut out, &mut failures, now);

        shell.apply(a_change_too_big_for_the_band());
        assert_eq!(
            shell.screen_owner(),
            ScreenOwner::Approval,
            "the question did not take the plane, so this case proves nothing"
        );
        tick(&mut shell, &mut band, &mut out, &mut failures, now);
        shell.reconcile_approval();

        shell.route_bytes(WENT_LIGHT);
        let borrowed = tick(&mut shell, &mut band, &mut out, &mut failures, now);
        assert!(
            !borrowed.contains("an answer"),
            "the document was repainted onto the buffer the question borrowed: \
             {borrowed:?}"
        );
        assert!(
            shell.owes_retint(),
            "the recolour was dropped while the question was up"
        );

        // Answered: the plane comes back under a whole repaint of the band,
        // and the tick after it pays what the primary plane still owes.
        shell.route_bytes(b"3");
        tick(&mut shell, &mut band, &mut out, &mut failures, now);
        let back = tick(&mut shell, &mut band, &mut out, &mut failures, now);
        assert!(
            back.contains(&format!("{BODY_LIGHT}a streamed answer")),
            "the recolour never landed after the plane came back: {back:?}"
        );
    }

    #[test]
    fn a_clear_and_a_report_in_one_tick_never_bring_the_erased_document_back() {
        // The ordering that makes this a real hazard: `/clear` erases the
        // screen *before* the document writer runs, and the band is told what
        // that means only when the frame is built, after it. A recolour paid in
        // between would repaint the erased answer back onto a screen the user
        // emptied -- and it must not matter which of the two the batch carried
        // first.
        for report_first in [true, false] {
            let mut shell = shell();
            let mut band = Band::new();
            let mut failures = FrameFailures::default();
            let mut out = screen();
            let now = Instant::now();
            a_document_on_the_screen(&mut shell, &mut band, &mut out, &mut failures, now);

            if report_first {
                shell.route_bytes(WENT_LIGHT);
                shell.route_bytes(b"/clear\r");
            } else {
                shell.route_bytes(b"/clear\r");
                shell.route_bytes(WENT_LIGHT);
            }
            for tick_number in 0..2 {
                let written = tick(&mut shell, &mut band, &mut out, &mut failures, now);
                assert!(
                    !written.contains("an answer") && !written.contains("[tool] ran"),
                    "tick {tick_number} with the report {} the clear brought \
                     erased history back: {written:?}",
                    if report_first { "before" } else { "after" }
                );
            }
        }
    }

    #[test]
    fn a_resize_and_a_report_in_one_tick_never_repaint_the_screen_that_was() {
        // The same hazard on the other road. A terminal that changed size
        // re-wrapped its own document by rules this crate does not model, so
        // every cell the shadow describes is a claim about a screen that is
        // gone -- and a recolour is a claim made out of exactly those cells.
        for report_first in [true, false] {
            let mut shell = shell();
            let mut band = Band::new();
            let mut failures = FrameFailures::default();
            let mut out = screen();
            let now = Instant::now();
            a_document_on_the_screen(&mut shell, &mut band, &mut out, &mut failures, now);

            if report_first {
                shell.route_bytes(WENT_LIGHT);
                adopt_resize(&mut shell, &mut band, || (30, 100));
            } else {
                adopt_resize(&mut shell, &mut band, || (30, 100));
                shell.route_bytes(WENT_LIGHT);
            }
            for tick_number in 0..2 {
                let written = tick(&mut shell, &mut band, &mut out, &mut failures, now);
                assert!(
                    !written.contains("an answer") && !written.contains("[tool] ran"),
                    "tick {tick_number} with the report {} the resize repainted \
                     a screen that no longer exists: {written:?}",
                    if report_first { "before" } else { "after" }
                );
            }
        }
    }

    #[test]
    fn a_recolour_armed_before_a_resume_never_repaints_the_screen_somebody_else_had() {
        // A stop hands the terminal to the user's shell and a continue takes it
        // back, so every row on it -- the document's included -- is a claim
        // this session can no longer make. A recolour still owed from before
        // that is owed against a screen somebody else has been writing on, and
        // the band is only *told* when the frame is built, which is after the
        // recolour would have been paid.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = screen();
        let now = Instant::now();
        a_document_on_the_screen(&mut shell, &mut band, &mut out, &mut failures, now);

        shell.route_bytes(WENT_LIGHT);
        assert!(shell.owes_retint(), "the report armed nothing to defend");
        // The same screen the session was stopped on: nothing here is a resize.
        adopt_resume(&mut shell, &mut band, false);

        for tick_number in 0..2 {
            let written = tick(&mut shell, &mut band, &mut out, &mut failures, now);
            assert!(
                !written.contains("a streamed answer") && !written.contains("[tool] read_file ok"),
                "tick {tick_number} after a resume repainted the document out of \
                 a shadow describing a screen somebody else had: {written:?}"
            );
        }
    }

    #[test]
    fn a_report_that_arrives_behind_a_resume_owes_no_recolour_at_all() {
        // The other order, and the one the arming guard is for: the report is
        // decoded after the resume was adopted and before any frame has told
        // the band what that means, so the shadow it would be paid from still
        // describes the screen that was.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = screen();
        let now = Instant::now();
        a_document_on_the_screen(&mut shell, &mut band, &mut out, &mut failures, now);

        adopt_resume(&mut shell, &mut band, false);
        shell.route_bytes(WENT_LIGHT);
        assert!(
            !shell.owes_retint(),
            "a report behind a resume owed a recolour of a screen this session \
             can no longer describe"
        );
        let written = tick(&mut shell, &mut band, &mut out, &mut failures, now);
        assert!(
            !written.contains("a streamed answer") && !written.contains("[tool] read_file ok"),
            "the tick after a resume brought the old document back: {written:?}"
        );
    }

    #[test]
    fn a_recolour_deferred_by_damage_is_settled_by_the_frame_that_repairs_the_screen() {
        // Deferred is not the same answer as done. While the band cannot
        // describe the screen the recolour is **kept** -- and the tick goes on,
        // because the frame below it is exactly what makes the screen
        // describable again, and an early return here would starve the repair
        // that settles it. It terminates rather than looping: once that frame
        // has landed the band owns no document row of its own, and the debt is
        // settled with no bytes at all.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = screen();
        let now = Instant::now();
        a_document_on_the_screen(&mut shell, &mut band, &mut out, &mut failures, now);

        // Ctrl-L on a tick the screen refuses: the band is told -- its own
        // `damaged` is raised -- and the frame that would clear it never lands.
        shell.route_bytes(&[0x0c]);
        out.refusals = 1;
        tick(&mut shell, &mut band, &mut out, &mut failures, now);

        shell.route_bytes(WENT_LIGHT);
        assert!(
            shell.owes_retint(),
            "the report armed nothing, so this case proves nothing"
        );
        let repairing = tick(&mut shell, &mut band, &mut out, &mut failures, now);
        assert!(
            !repairing.contains("a streamed answer"),
            "a deferred recolour was paid out of a shadow that is not this \
             screen's: {repairing:?}"
        );
        assert!(
            repairing.contains("\u{1b}[?2026h"),
            "the tick that deferred the recolour never reached the frame that \
             repairs the screen: {repairing:?}"
        );
        assert!(
            shell.owes_retint(),
            "a deferred recolour was recorded as done"
        );

        // The frame has landed, so the next tick can answer: the band owns no
        // document row now, and settling the debt costs nothing.
        let settling = tick(&mut shell, &mut band, &mut out, &mut failures, now);
        assert!(
            settling.is_empty(),
            "settling a deferred recolour wrote to the screen: {settling:?}"
        );
        assert!(
            !shell.owes_retint(),
            "the debt outlived the tick that settled it"
        );
        let idle = tick(&mut shell, &mut band, &mut out, &mut failures, now);
        assert!(idle.is_empty(), "the session kept writing: {idle:?}");
    }

    #[test]
    fn a_report_in_the_middle_of_a_stream_leaves_nothing_in_the_old_palette() {
        // The unfinished line is on the screen like any other document row, so
        // it is recoloured like one -- and what the pacer releases behind the
        // report is written in the palette that is in force when it lands
        // (U-A's `Transcript::emit_front`). The two halves must not disagree:
        // a session that recoloured only the finished rows would leave the row
        // the answer is still growing on in the greys of the terminal that was.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = screen();
        let now = Instant::now();
        tick(&mut shell, &mut band, &mut out, &mut failures, now);
        shell.apply(UiEvent::Delta("half an answer".to_string()));
        shell.flush_paced();
        let dark = tick(&mut shell, &mut band, &mut out, &mut failures, now);
        assert!(
            dark.contains(&format!("{BODY_DARK}half an answer")),
            "the unfinished line never reached the screen: {dark:?}"
        );

        shell.route_bytes(WENT_LIGHT);
        shell.apply(UiEvent::Notice("[tool] ran".to_string()));
        shell.apply(UiEvent::Delta(" and the rest".to_string()));
        shell.flush_paced();
        let written = tick(&mut shell, &mut band, &mut out, &mut failures, now);

        assert!(
            written.contains(&format!("{BODY_LIGHT}half an answer")),
            "the row the answer is still growing on kept the old palette: \
             {written:?}"
        );
        assert!(
            written.contains(NOTICE_LIGHT) && written.contains("[tool] ran"),
            "the notice that arrived with the report is not in the palette it \
             landed in: {written:?}"
        );
        assert!(
            // The notice is named rather than its grey alone: the light
            // palette paints the **divider** in the same 250 that a dark
            // notice is, and the divider is the band's own row.
            !written.contains(BODY_DARK) && !written.contains(&format!("{NOTICE_DARK}[tool] ran")),
            "a row went to the screen in the palette the session has left: \
             {written:?}"
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

    // -----------------------------------------------------------------------
    // the screen changing size under the loop
    // -----------------------------------------------------------------------

    /// Erase from the cursor to the end of the screen: the one sequence that
    /// tells a whole frame from a difference.
    ///
    /// Pinned as a literal rather than imported from `super::super::frame`, for
    /// the reason every needle in this crate's tests is.
    const ERASE_BELOW: &str = "\u{1b}[J";

    /// A window size a test hands the loop, and a count of how often it was
    /// asked for one.
    ///
    /// A closure rather than `term::reported_window_size`, for the reason every screen
    /// in this module is a parameter: "the loop reads the terminal's size once
    /// per burst of winches" is a claim about how often a question is asked,
    /// and a function that asked the process's own terminal could only be
    /// tested by resizing the developer's window.
    fn sized(size: (u16, u16), asked: &std::cell::Cell<usize>) -> impl FnOnce() -> (u16, u16) + '_ {
        move || {
            asked.set(asked.get() + 1);
            size
        }
    }

    #[test]
    fn a_burst_of_winches_costs_one_resolve() {
        // A person dragging a window edge produces a `SIGWINCH` per row it
        // passes through, and each one would otherwise cost a `TIOCGWINSZ`, a
        // re-solve, a re-wrap of the tail and a whole-screen repaint. The
        // debounce is a **deadline**, never a sleep: the UI thread is the only
        // reader of the terminal, and a thread asleep in the resize path is a
        // session that has stopped reading its keyboard.
        let mut shell = shell();
        let mut band = Band::new();
        let asked = std::cell::Cell::new(0usize);
        let start = Instant::now();

        for at in [0, 10, 20, 30] {
            shell.render.mark_resize(start + Duration::from_millis(at));
        }
        resolve_resize(&mut shell, &mut band, start, sized((30, 100), &asked));
        assert_eq!(
            asked.get(),
            0,
            "the loop read the terminal before it was due"
        );

        resolve_resize(
            &mut shell,
            &mut band,
            start + RESIZE_DEBOUNCE,
            sized((30, 100), &asked),
        );
        assert_eq!(asked.get(), 1, "the burst was not resolved");
        assert_eq!(shell.geometry.rows, 30);

        resolve_resize(
            &mut shell,
            &mut band,
            start + Duration::from_secs(60),
            sized((30, 100), &asked),
        );
        assert_eq!(asked.get(), 1, "the burst was resolved twice");
    }

    #[test]
    fn zero_by_zero_after_launch_is_no_new_information() {
        // A screen the terminal will not describe is not a screen of 24x80.
        // At launch that fallback is the only number there is; here there is a
        // band on a screen whose size is known, and replacing it would move the
        // band for a measurement that said nothing.
        let mut shell = shell();
        let mut band = Band::new();
        let asked = std::cell::Cell::new(0usize);
        let start = Instant::now();
        let before = shell.geometry;
        // The first frame every session owes, so what is left pending below is
        // whatever this case put there.
        let _ = shell.render.begin();

        shell.render.mark_resize(start);
        resolve_resize(
            &mut shell,
            &mut band,
            start + RESIZE_DEBOUNCE,
            sized((0, 0), &asked),
        );
        assert_eq!(asked.get(), 1);
        assert_eq!(shell.geometry, before, "the band was solved from nothing");
        assert!(
            shell.render.begin().is_none(),
            "a measurement that said nothing asked for a frame"
        );
    }

    #[test]
    fn resize_invalidates_the_shadow_for_one_full_repaint() {
        // The Task 1 seam, and the reason a resize may not be a difference: the
        // shadow is a claim about what is on the terminal's rows, and a
        // terminal that changed size re-wrapped its own document by rules xfx
        // does not model. The frame after one is therefore the whole band --
        // opened with the erase the Phase-1 painter wrote on every frame -- and
        // exactly one of them.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = screen();
        let asked = std::cell::Cell::new(0usize);
        let start = Instant::now();

        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            start,
            Reconciled,
        )
        .expect("the first frame");
        out.written.clear();

        shell.render.mark_resize(start);
        resolve_resize(
            &mut shell,
            &mut band,
            start + RESIZE_DEBOUNCE,
            sized((30, 100), &asked),
        );
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            start,
            Reconciled,
        )
        .expect("the frame after the resize");

        let text = String::from_utf8(out.written.clone()).expect("utf-8");
        assert!(
            text.contains(ERASE_BELOW),
            "the frame after a resize was a difference from a screen that no \
             longer exists: {text:?}"
        );
        assert!(
            text.contains(&"\u{2500}".repeat(100)),
            "the divider was not repainted across the wider screen: {text:?}"
        );

        // And exactly one of them: the damage is answered by the frame that
        // paid for it, so the tick after it is silent again.
        out.written.clear();
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            start,
            Reconciled,
        )
        .expect("the tick after the repaint");
        assert!(
            out.written.is_empty(),
            "the resize left the band repainting whole for ever: {:?}",
            String::from_utf8_lossy(&out.written)
        );
    }

    // -------------------------------------------------------------------
    // text that has not landed is wrapped for the screen it lands on
    // -------------------------------------------------------------------
    //
    // The defect these were written red against: rows were measured when the
    // delta *arrived* and held as frozen strings until a tick could write
    // them. A write refused on one tick is retried on the next
    // (`commit_document`), and a screen that narrowed in between got rows
    // wrapped for the screen it used to be -- which
    // `Band::append_document` -> `render_append` -> `place` -> `row_text` ->
    // `clip` (`frame.rs:1533,1156,1626,1696,1758`) cuts to `geometry.cols`
    // **as it is now**. A row wrapped to 80 columns and clipped to 40 does not
    // get a second, narrower row for what fell off the end, and Phase 1 never
    // repaints a document row: the overhang was gone from the session rather
    // than from the frame.
    //
    // The repair is that the transcript keeps the **text** until the terminal
    // takes it and measures it inside the write itself
    // (`Transcript::emit_front`), so the width a row is wrapped at is always
    // the width it is written at. Only text the terminal has really seen is
    // left alone by a resize, which is the boundary `resize_unfinished`
    // already drew for the open tail.

    #[test]
    fn a_refusal_that_outlives_a_narrowing_resize_is_rewrapped_for_the_screen_the_retry_lands_on() {
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let start = Instant::now();
        let mut screen = FlakyScreen {
            refusals: 1,
            kind: io::ErrorKind::BrokenPipe,
            written: Vec::new(),
        };

        // One finished line, three distinct marker blocks so a partial loss
        // names exactly what went missing. At the fixture's 80-column start
        // this single (space-free) word hard-wraps at cols 80 -- word rule
        // `wrap.rs:213-221` -- into exactly `"A"*40 + "B"*40` and `"C"*10`,
        // with no clipping yet: 80 columns holds 80 cells.
        let a = "A".repeat(40);
        let b = "B".repeat(40);
        let c = "C".repeat(10);
        shell.write_transcript(&format!("{a}{b}{c}\n"));

        commit_frame(
            &mut shell,
            &mut band,
            &mut screen,
            &mut failures,
            start,
            Reconciled,
        )
        .expect("a zero-progress refusal is not fatal");
        assert!(
            screen.written.is_empty(),
            "the refusal must not have moved a single byte -- otherwise this \
             case is testing `Partial`, not `ZeroProgress`"
        );

        // The screen narrows to 40 columns strictly between the refusal and
        // the retry -- the same window `resize_unfinished` answers for the
        // *tail*, and which this text, never offered to any terminal, is
        // entitled to as well.
        shell.render.mark_resize(start);
        let asked = std::cell::Cell::new(0usize);
        resolve_resize(
            &mut shell,
            &mut band,
            start + RESIZE_DEBOUNCE,
            sized((24, 40), &asked),
        );

        // After the debounce the resize was resolved at, because that is the
        // order the loop can really produce: a retry timed before the resize
        // it is retrying across is a clock no session runs on, and the budget
        // `commit_frame` measures against would be reading it backwards.
        commit_frame(
            &mut shell,
            &mut band,
            &mut screen,
            &mut failures,
            at(start, 58),
            Reconciled,
        )
        .expect("the retry");

        let text = String::from_utf8(screen.written).expect("utf-8");
        // Not asserted through `frame::clip`/`row_text` -- that would be the
        // clipping helper grading its own homework. This reads the literal
        // bytes the sink received, the same way every other case in this
        // module tells a landed row from a dropped one.
        for (marker, what) in [
            (&a, "the first 40 columns, which no width ever moved"),
            (
                &b,
                "the middle 40, which only a re-wrap for the narrower screen \
                 can place -- clipping the 80-column row loses it for good",
            ),
            (&c, "the tail of the line, on a row of its own"),
        ] {
            assert_eq!(
                text.matches(marker.as_str()).count(),
                1,
                "{what}: expected exactly once in {text:?}"
            );
        }
        // And in the order the line was written in. Three rows that each
        // landed once but in the wrong order is a different defect with the
        // same marker count.
        let (at_a, at_b, at_c) = (
            text.find(a.as_str()).expect("the first marker"),
            text.find(b.as_str()).expect("the second marker"),
            text.find(c.as_str()).expect("the third marker"),
        );
        assert!(
            at_a < at_b && at_b < at_c,
            "the re-wrapped rows did not keep the order of the line they came \
             from: {text:?}"
        );
    }

    #[test]
    fn text_queued_before_the_first_frame_is_wrapped_for_the_screen_that_frame_paints() {
        // The same guarantee without a refusal in it: a session whose terminal
        // reports its real size only after the first delta has been applied --
        // a `SIGWINCH` a pty delivers on attach, or a multiplexer settling.
        // Nothing has been offered to any terminal yet, so every row is owed at
        // the width the first frame is painted at.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut screen = screen();
        let start = Instant::now();

        let a = "A".repeat(40);
        let b = "B".repeat(40);
        let c = "C".repeat(10);
        shell.write_transcript(&format!("{a}{b}{c}\n"));

        shell.render.mark_resize(start);
        let asked = std::cell::Cell::new(0usize);
        resolve_resize(
            &mut shell,
            &mut band,
            start + RESIZE_DEBOUNCE,
            sized((24, 40), &asked),
        );

        commit_frame(
            &mut shell,
            &mut band,
            &mut screen,
            &mut failures,
            at(start, 58),
            Reconciled,
        )
        .expect("the first frame");

        let text = String::from_utf8(screen.written).expect("utf-8");
        for marker in [&a, &b, &c] {
            assert_eq!(
                text.matches(marker.as_str()).count(),
                1,
                "a row of the first line was clipped or repeated: {text:?}"
            );
        }
    }

    #[test]
    fn a_refusal_that_outlives_a_widening_resize_is_rewrapped_for_the_screen_the_retry_lands_on() {
        // The other direction, and it fails differently: a narrowing loses
        // text, a widening leaves the answer in a column of the width it was
        // queued at while the screen around it is twice that. Both are the same
        // defect -- rows measured against a screen that is not the one they are
        // written on.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let start = Instant::now();
        let asked = std::cell::Cell::new(0usize);
        let mut screen = FlakyScreen {
            refusals: 1,
            kind: io::ErrorKind::BrokenPipe,
            written: Vec::new(),
        };

        // Narrow first, with nothing owed: this is the width the text is
        // queued at and has nothing to do with the width it lands at.
        shell.render.mark_resize(start);
        resolve_resize(
            &mut shell,
            &mut band,
            start + RESIZE_DEBOUNCE,
            sized((24, 40), &asked),
        );

        let a = "A".repeat(40);
        let b = "B".repeat(40);
        let c = "C".repeat(10);
        shell.write_transcript(&format!("{a}{b}{c}\n"));

        commit_frame(
            &mut shell,
            &mut band,
            &mut screen,
            &mut failures,
            at(start, 58),
            Reconciled,
        )
        .expect("a zero-progress refusal is not fatal");
        assert!(screen.written.is_empty(), "the refusal moved bytes");

        shell.render.mark_resize(at(start, 100));
        resolve_resize(
            &mut shell,
            &mut band,
            at(start, 100) + RESIZE_DEBOUNCE,
            sized((24, 80), &asked),
        );

        commit_frame(
            &mut shell,
            &mut band,
            &mut screen,
            &mut failures,
            at(start, 160),
            Reconciled,
        )
        .expect("the retry");

        let text = String::from_utf8(screen.written).expect("utf-8");
        assert_eq!(
            text.matches(format!("{a}{b}").as_str()).count(),
            1,
            "the 80 columns the screen now has were written as two 40-column \
             rows -- the text was wrapped for a screen it did not land on: \
             {text:?}"
        );
        assert_eq!(
            text.matches(c.as_str()).count(),
            1,
            "the rest of the line did not land exactly once: {text:?}"
        );
    }

    #[test]
    fn a_write_that_landed_is_not_remeasured_when_the_one_behind_it_is() {
        // The cross-product of the two mechanisms: a batch whose first write
        // lands and whose second is refused, with a resize before the retry.
        // What already reached the terminal is the terminal's -- at the width
        // it was written at, never offered again -- and only what did not is
        // measured against the new screen. A repair that re-measured "what is
        // owed" without knowing where the batch stopped would write the first
        // row twice.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let start = Instant::now();
        let asked = std::cell::Cell::new(0usize);
        let mut screen = ScriptedSink::new([
            Ok(()),
            Err(Emit::ZeroProgress(io::Error::new(
                io::ErrorKind::WouldBlock,
                "the screen took nothing",
            ))),
        ]);

        let a = "A".repeat(40);
        let b = "B".repeat(40);
        let c = "C".repeat(10);
        shell.write_transcript("LANDED\n");
        shell.write_transcript(&format!("{a}{b}{c}\n"));

        commit_frame(
            &mut shell,
            &mut band,
            &mut screen,
            &mut failures,
            start,
            Reconciled,
        )
        .expect("a zero-progress refusal is not fatal");
        assert_eq!(
            screen.calls.len(),
            2,
            "the batch must stop at the refusal: {:?}",
            screen.calls
        );

        shell.render.mark_resize(start);
        resolve_resize(
            &mut shell,
            &mut band,
            start + RESIZE_DEBOUNCE,
            sized((24, 40), &asked),
        );
        commit_frame(
            &mut shell,
            &mut band,
            &mut screen,
            &mut failures,
            at(start, 58),
            Reconciled,
        )
        .expect("the retry");

        let whole: String = screen
            .calls
            .iter()
            .map(|call| String::from_utf8_lossy(call).into_owned())
            .collect();
        assert_eq!(
            whole.matches("LANDED").count(),
            1,
            "a write the terminal already took was offered again: {whole:?}"
        );
        let retried: String = screen.calls[2..]
            .iter()
            .map(|call| String::from_utf8_lossy(call).into_owned())
            .collect();
        for marker in [&a, &b, &c] {
            assert_eq!(
                retried.matches(marker.as_str()).count(),
                1,
                "the refused write was not re-measured for the narrower \
                 screen: {retried:?}"
            );
        }
    }

    #[test]
    fn without_a_resize_the_same_zero_progress_retry_keeps_every_marker() {
        // The positive control for the case above: same refusal, same finished
        // line, no resize in between. If this ever failed, the case above
        // would be proving something about the assertion method (a plain
        // substring search on the emitted bytes) rather than about the
        // resize -- this pins the method down first.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let start = Instant::now();
        let mut screen = FlakyScreen {
            refusals: 1,
            kind: io::ErrorKind::BrokenPipe,
            written: Vec::new(),
        };

        let a = "A".repeat(40);
        let b = "B".repeat(40);
        let c = "C".repeat(10);
        shell.write_transcript(&format!("{a}{b}{c}\n"));

        commit_frame(
            &mut shell,
            &mut band,
            &mut screen,
            &mut failures,
            start,
            Reconciled,
        )
        .expect("a zero-progress refusal is not fatal");
        assert!(screen.written.is_empty());

        commit_frame(
            &mut shell,
            &mut band,
            &mut screen,
            &mut failures,
            at(start, 8),
            Reconciled,
        )
        .expect("the retry");

        let text = String::from_utf8(screen.written).expect("utf-8");
        for marker in [&a, &b, &c] {
            assert!(
                text.contains(marker.as_str()),
                "without a resize in between, the retry must keep every \
                 marker: {text:?}"
            );
        }
    }

    #[test]
    fn a_second_delta_of_a_still_open_line_recomputes_its_scroll_from_the_tail_a_resize_just_remeasured(
    ) {
        // Characterization, not a defect: this is the mechanism the case
        // above shows a *finished* line has none of. While a line is still
        // open, `Transcript::painted` (`transcript.rs`'s own field, not
        // anything `Shell` tracks) is the row count `resize_unfinished`
        // keeps current, and every further `push` measures its `scroll`
        // against *that*, not against columns or the append's own row count
        // alone -- which is exactly the state a fix for the case above would
        // have to reconstruct for rows that already left the tail.
        // `layout::MIN_COLS` is 20, so every width below is a real,
        // fittable screen and not `Resize::TooSmall` silently declining to
        // touch the transcript at all.
        let mut shell = shell();

        // The fixture starts at 80 columns. "A" * 40 fits one row there,
        // painted = 1.
        shell.write_transcript(&"A".repeat(40));
        let first = shell.take_pending();
        assert_eq!(first.len(), 1, "one push, one append: {first:?}");
        assert_eq!(first[0].rows, vec!["A".repeat(40)]);
        assert_eq!(first[0].scroll, 1, "the only row this line has, freshly");

        // Narrows to 20 columns strictly before the next delta.
        // `resize_unfinished` re-wraps the *open* tail now, not later:
        // 40 `A`s at 20 columns is two rows, so painted becomes 2 -- a fact
        // this narrowing alone produced, before the next delta adds a
        // single byte.
        shell.resize(24, 20);

        // A second, unrelated-length delta on the same still-open line: 25
        // `B`s land after the 40 `A`s, wrapped fresh as one 65-character
        // tail at 20 columns -- four rows in total (20, 20, 20, 5). A fix
        // that read only "how many rows does this delta's own text make"
        // (4, the same as the true total) or "how many columns are there
        // now" (20, which says nothing about how much of the tail a
        // previous width already accounted for) has no way to recover that
        // two of those four rows were already counted painted by the
        // resize, before this delta existed.
        shell.write_transcript(&"B".repeat(25));
        let second = shell.take_pending();
        assert_eq!(second.len(), 1, "one push, one append: {second:?}");
        assert_eq!(
            second[0].rows,
            vec![
                "A".repeat(20),
                "A".repeat(20),
                "B".repeat(20),
                "B".repeat(5),
            ],
            "the tail is re-wrapped whole, from both deltas, at the new width"
        );
        assert_eq!(
            second[0].scroll, 2,
            "two new rows beyond the two the resize had already re-measured \
             as painted -- not four, which is this delta's own row total, \
             and not a function of columns alone, which cannot distinguish \
             this session's `painted = 2` from a session that had painted \
             nothing at this same width"
        );
    }

    #[test]
    fn a_screen_that_shrank_leaves_the_exit_a_row_it_can_clear_from() {
        // `Band::painted_top` is what `term::shutdown` clears from, and after a
        // resize it is a row number in the screen that was. A session that
        // shrank and then left before its next frame landed would hand the exit
        // a row below the terminal's last one.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = screen();
        let asked = std::cell::Cell::new(0usize);
        let start = Instant::now();

        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            start,
            Reconciled,
        )
        .expect("the first frame");
        assert_eq!(band.painted_top(), Some(22));

        shell.render.mark_resize(start);
        resolve_resize(
            &mut shell,
            &mut band,
            start + RESIZE_DEBOUNCE,
            sized((10, 40), &asked),
        );
        assert!(
            band.painted_top().is_some_and(|top| top <= 10),
            "the exit would clear from row {:?} of a ten-row screen",
            band.painted_top()
        );
    }

    #[test]
    fn a_winch_that_changed_nothing_costs_no_frame() {
        // A terminal that reports its size for a font change, and the second
        // winch of a burst whose first one already resolved. A repaint for
        // either would make an idle session's cost a function of how often its
        // terminal talks about itself.
        let mut shell = shell();
        let mut band = Band::new();
        let asked = std::cell::Cell::new(0usize);
        let start = Instant::now();
        let _ = shell.render.begin();

        shell.render.mark_resize(start);
        resolve_resize(
            &mut shell,
            &mut band,
            start + RESIZE_DEBOUNCE,
            sized((24, 80), &asked),
        );
        assert_eq!(asked.get(), 1, "the terminal was never asked");
        assert!(
            shell.render.begin().is_none(),
            "a resize to the size the screen already is asked for a frame"
        );
    }

    #[test]
    fn a_screen_no_band_fits_on_is_not_written_on_at_all() {
        // The window `Resize::TooSmall` opens. The geometry still describes the
        // screen the band was last solved for and the screen is smaller than
        // that, so every row the band would paint is addressed at a coordinate
        // the terminal no longer has -- and a terminal answers a `CUP` past its
        // last row by **clamping** it, silently, so the whole band lands on the
        // bottom row on top of itself. Nothing else asks for that frame; a
        // keystroke does.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = screen();
        let asked = std::cell::Cell::new(0usize);
        let start = Instant::now();

        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            start,
            Reconciled,
        )
        .expect("the first frame");
        assert!(!out.written.is_empty(), "the band never painted at all");

        shell.render.mark_resize(start);
        resolve_resize(
            &mut shell,
            &mut band,
            start + RESIZE_DEBOUNCE,
            sized((4, 80), &asked),
        );
        assert_eq!(shell.geometry.rows, 24, "the band was solved for 4 rows");

        // A keystroke, which is what really asks for a frame in this window.
        out.written.clear();
        shell.route_bytes(b"z");
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            start,
            Reconciled,
        )
        .expect("the tick on a screen too small for the band");
        assert!(
            out.written.is_empty(),
            "the band painted rows 22-24 onto a four-row screen: {:?}",
            String::from_utf8_lossy(&out.written)
        );

        // And it is **owed**, not lost: the screen grows back and the frame the
        // keystroke asked for is painted, with the keystroke in it.
        shell.render.mark_resize(start);
        resolve_resize(
            &mut shell,
            &mut band,
            start + RESIZE_DEBOUNCE,
            sized((24, 80), &asked),
        );
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            start,
            Reconciled,
        )
        .expect("the frame after the screen grew back");
        let text = String::from_utf8(out.written.clone()).expect("utf-8");
        assert!(
            text.contains("> z"),
            "the frame the keystroke asked for never arrived: {text:?}"
        );
    }

    #[test]
    fn what_the_document_is_owed_waits_for_a_screen_to_be_written_on() {
        // The half that cannot be taken back. A document append is a **scroll**
        // followed by the rows it made room for, and a row that leaves the top
        // of the screen is in the terminal's native scrollback for good -- so an
        // append placed at coordinates the screen does not have does not merely
        // look wrong, it puts the wrong thing somewhere nothing can ever
        // rewrite. Held instead, exactly as a refused one is.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = screen();
        let asked = std::cell::Cell::new(0usize);
        let start = Instant::now();

        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            start,
            Reconciled,
        )
        .expect("the first frame");

        shell.render.mark_resize(start);
        resolve_resize(
            &mut shell,
            &mut band,
            start + RESIZE_DEBOUNCE,
            sized((4, 80), &asked),
        );

        out.written.clear();
        shell.write_transcript("ANSWER-WITH-NO-SCREEN-TO-PUT-IT-ON");
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            start,
            Reconciled,
        )
        .expect("the tick on a screen too small for the band");
        assert!(
            out.written.is_empty(),
            "an append was scrolled into a terminal it does not fit: {:?}",
            String::from_utf8_lossy(&out.written)
        );

        shell.render.mark_resize(start);
        resolve_resize(
            &mut shell,
            &mut band,
            start + RESIZE_DEBOUNCE,
            sized((24, 80), &asked),
        );
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            start,
            Reconciled,
        )
        .expect("the frame after the screen grew back");
        let text = String::from_utf8(out.written.clone()).expect("utf-8");
        assert!(
            text.contains("ANSWER-WITH-NO-SCREEN-TO-PUT-IT-ON"),
            "the answer was dropped rather than held: {text:?}"
        );
    }

    #[test]
    fn a_screen_that_holds_the_band_is_never_blind() {
        // The bound on the rule above, and the reason it is derived rather than
        // flagged: a session whose screen holds its band must not be able to
        // reach the state that paints nothing. Every reading that is not
        // `TooSmall` leaves the geometry describing the screen.
        let mut shell = shell();
        let mut band = Band::new();
        let asked = std::cell::Cell::new(0usize);
        let start = Instant::now();
        assert!(!shell.blind(), "a fresh session cannot be painted on");

        for size in [(30, 100), (24, 80), (6, 20), (40, 132)] {
            shell.render.mark_resize(start);
            resolve_resize(
                &mut shell,
                &mut band,
                start + RESIZE_DEBOUNCE,
                sized(size, &asked),
            );
            assert!(
                !shell.blind(),
                "{size:?} holds a band and was treated as a screen that does not"
            );
        }
    }

    #[test]
    fn a_winch_nobody_has_resolved_yet_holds_the_screen_too() {
        // The window the debounce itself opens, and it is the same defect one
        // tick earlier. A `SIGWINCH` means the terminal has **already** changed
        // size; the band is not re-solved for `RESIZE_DEBOUNCE` afterwards, so
        // for that whole interval every row number the band would write is a
        // coordinate out of the screen that *was*. A frame there lands clamped
        // onto the bottom row, and an append there scrolls -- into native
        // scrollback, where nothing can take it back.
        //
        // Withholding is not the same as acting on the signal at once, which is
        // the thing the debounce exists to prevent: nothing is measured, nothing
        // is re-solved, and the frame is simply owed until the deadline comes
        // round.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = screen();
        let asked = std::cell::Cell::new(0usize);
        let start = Instant::now();

        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            start,
            Reconciled,
        )
        .expect("the first frame");

        shell.render.mark_resize(start);
        out.written.clear();
        shell.route_bytes(b"z");
        shell.write_transcript("ANSWER-MID-DRAG");
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            start + Duration::from_millis(8),
            Reconciled,
        )
        .expect("a tick inside the debounce");
        assert!(
            out.written.is_empty(),
            "the band wrote to a screen whose size it has been told is stale: {:?}",
            String::from_utf8_lossy(&out.written)
        );
        assert_eq!(asked.get(), 0, "this case never measured anything");

        // And nothing is lost by the wait: the resolve comes round and both the
        // keystroke and the answer are still owed.
        resolve_resize(
            &mut shell,
            &mut band,
            start + RESIZE_DEBOUNCE,
            sized((30, 100), &asked),
        );
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            start + RESIZE_DEBOUNCE,
            Reconciled,
        )
        .expect("the frame the resize asked for");
        let text = String::from_utf8(out.written.clone()).expect("utf-8");
        assert!(
            text.contains("ANSWER-MID-DRAG"),
            "the answer released mid-drag was dropped: {text:?}"
        );
        assert!(
            text.contains("> z"),
            "the keystroke typed mid-drag was dropped: {text:?}"
        );
    }

    #[test]
    fn an_exit_with_a_winch_still_pending_measures_once_and_writes_the_tail() {
        // The hole the withheld-write rule opens at the one moment it cannot be
        // made up later. Nothing is written while a `SIGWINCH` is outstanding,
        // because until the band is re-solved every row number is a coordinate
        // out of the screen that was -- and the resolve is deliberately
        // `RESIZE_DEBOUNCE` away. A user who drags a window and then presses
        // Ctrl-D lands inside that window, and the exit is the **last** moment
        // the answer they were reading can reach the terminal at all: Phase 1
        // never repaints a document row.
        //
        // So an exit answers an outstanding winch *now*, deadline or not. That
        // is not the debounce being broken: the debounce exists to stop a drag
        // costing a re-solve per signal, and there is no drag left to protect
        // -- there is one measurement and then the session is over.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = screen();
        let asked = std::cell::Cell::new(0usize);
        shell.apply(UiEvent::Delta("TAIL-BEHIND-A-PENDING-WINCH".to_string()));
        shell.apply(UiEvent::TurnEnded { failure: None });
        assert!(shell.paced_backlog() > 0, "the stream was empty");

        // The winch, and no tick between it and the exit.
        shell.render.mark_resize(Instant::now());
        assert!(shell.blind(), "this case does not start where it means to");

        let broken = shut_down(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            true,
            Shutdown {
                reconcile: |_, _| Ok(Reconciled),
                drain: |_taken| {},
                size: sized((30, 100), &asked),
            },
        );

        assert!(broken.is_none(), "the screen refused: {broken:?}");
        assert_eq!(asked.get(), 1, "the exit never measured the screen");
        let written = String::from_utf8_lossy(&out.written).to_string();
        assert!(
            written.contains("TAIL-BEHIND-A-PENDING-WINCH"),
            "the band came down on an answer the runtime had already handed over: {written:?}"
        );
        // And at the coordinates the screen really has: the hint row of a
        // thirty-row screen is row 30, not row 24.
        assert!(
            written.contains("\u{1b}[30;1H"),
            "the tail was written at the coordinates of the screen that was: {written:?}"
        );
        assert!(
            !shell.blind(),
            "the exit left the session still refusing to write"
        );
    }

    #[test]
    fn an_exit_onto_a_screen_no_band_fits_on_writes_nothing_at_all() {
        // The exception the rule above must keep. A window dragged below the
        // smallest screen a band fits on is not a coordinate problem the exit
        // can measure its way out of: there is nowhere to put the answer. The
        // forced measurement happens, finds a screen that cannot hold a band,
        // and the session comes down silent -- which is the same trade the
        // running session makes, and for the harder reason: an append is a
        // scroll, and what it pushes into native scrollback at coordinates that
        // mean nothing can never be taken back.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = screen();
        let asked = std::cell::Cell::new(0usize);
        shell.apply(UiEvent::Delta("TAIL-WITH-NOWHERE-TO-GO".to_string()));
        shell.apply(UiEvent::TurnEnded { failure: None });
        shell.render.mark_resize(Instant::now());

        let broken = shut_down(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            true,
            Shutdown {
                reconcile: |_, _| Ok(Reconciled),
                drain: |_taken| {},
                size: sized((4, 80), &asked),
            },
        );

        assert!(broken.is_none(), "the screen refused: {broken:?}");
        assert_eq!(asked.get(), 1, "the exit never measured the screen");
        assert!(
            out.written.is_empty(),
            "the exit scrolled an answer into a terminal it does not fit: {:?}",
            String::from_utf8_lossy(&out.written)
        );
        assert!(shell.blind(), "a screen no band fits on was written on");
    }

    #[test]
    fn an_exit_with_no_winch_pending_measures_nothing() {
        // The bound on the forced resolve: it answers an **outstanding** signal
        // and does not invent one. An exit that measured unconditionally would
        // re-solve the band from a fresh reading on every shutdown, which is a
        // whole repaint on the way out for a screen nothing said had changed.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = screen();
        let asked = std::cell::Cell::new(0usize);
        shell.apply(UiEvent::Delta("ORDINARY-TAIL".to_string()));
        shell.apply(UiEvent::TurnEnded { failure: None });

        let broken = shut_down(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            true,
            Shutdown {
                reconcile: |_, _| Ok(Reconciled),
                drain: |_taken| {},
                size: sized((30, 100), &asked),
            },
        );

        assert!(broken.is_none(), "the screen refused: {broken:?}");
        assert_eq!(
            asked.get(),
            0,
            "an exit measured a screen nobody said had changed"
        );
        assert!(
            String::from_utf8_lossy(&out.written).contains("ORDINARY-TAIL"),
            "the ordinary exit stopped writing what the pacer held"
        );
    }

    #[test]
    fn a_turn_that_starts_does_not_erase_the_row_the_document_was_just_given() {
        // The controller found this as a scenario-13 flake: two builds, the same
        // facts, and a document that differed by the prompt echo. The race is
        // between two things the loop does not order:
        //
        //   * a submitted line is echoed into the document **immediately**
        //     (`Shell::say` -> `Mark::Line`), and an append places the document's
        //     newest row at `band_top - 1`;
        //   * the turn it started is announced by the **runtime** a moment later
        //     (`UiEvent::TurnStarted`), and the activity row that appears grows
        //     the band by one -- so `band_top` becomes the row that echo is on.
        //
        // Every frame opens with `CUP(band_top,1)` + `ED`, so whichever run lost
        // that race had the row the user submitted erased: off the screen, and
        // never in scrollback, because this phase never repaints a document row.
        // The band has to carry the document up out of its way first, and the
        // carry has to be on the wire **before** the frame that takes the row.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = screen();
        let now = Instant::now();

        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            now,
            Reconciled,
        )
        .expect("the first frame");
        let idle_top = shell.geometry.band_top();

        // The echo, written the instant the line is submitted and while no turn
        // is running: it lands on the bottom row of the document.
        shell.route_bytes(b"say hello\r");
        out.written.clear();
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            now,
            Reconciled,
        )
        .expect("the frame that appends the echo");
        let echoed = String::from_utf8_lossy(&out.written).into_owned();
        assert!(
            echoed.contains(&format!("\u{1b}[{};1Hsay hello", idle_top - 1)),
            "the echo was not appended to the bottom document row: {echoed:?}"
        );

        // And now the runtime says the turn started, so the band grows onto it.
        shell.apply(UiEvent::TurnStarted);
        shell.settle_band(now);
        let running_top = shell.geometry.band_top();
        assert_eq!(
            running_top + 1,
            idle_top,
            "the turn's row did not grow the band, so this case proves nothing"
        );

        out.written.clear();
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            now,
            Reconciled,
        )
        .expect("the frame the turn's row asked for");
        let written = String::from_utf8_lossy(&out.written).into_owned();

        let scrolled = written
            .find(&format!("\u{1b}[{};1H\n", shell.geometry.rows))
            .unwrap_or_else(|| {
                panic!("the band took a document row and carried nothing up: {written:?}")
            });
        // Whatever the frame writes on the band's new top -- the activity row
        // itself on a diffed frame, an `ED` on a whole one -- has to come after
        // the carry. Either way that row was the document's a moment ago.
        let taken = written
            .find(&format!("\u{1b}[{running_top};1H"))
            .unwrap_or_else(|| panic!("the frame never painted the band's new top: {written:?}"));
        assert!(
            scrolled < taken,
            "the document was carried up after the band had already taken its row: {written:?}"
        );
        assert!(
            !written.contains("say hello"),
            "the carry repainted a document row, which this phase never does: {written:?}"
        );
    }

    #[test]
    fn a_band_that_did_not_grow_scrolls_the_document_for_nothing() {
        // The bound, and it is the expensive direction to get wrong: a scroll
        // cannot be taken back -- what leaves the top of the screen is in the
        // terminal's own scrollback for good -- so a carry on a band that took
        // no row would walk the user's document up one row per frame.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = screen();
        let now = Instant::now();

        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            now,
            Reconciled,
        )
        .expect("the first frame");
        shell.route_bytes(b"say hello\r");
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            now,
            Reconciled,
        )
        .expect("the frame that appends the echo");

        out.written.clear();
        shell.render.request(Reason::ExternalDamage);
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            now,
            Reconciled,
        )
        .expect("a frame on a band that did not grow");
        let written = String::from_utf8_lossy(&out.written).into_owned();
        assert!(
            !written.contains(&format!("\u{1b}[{};1H\n", shell.geometry.rows)),
            "a band that took no row scrolled the document anyway: {written:?}"
        );
    }

    // -----------------------------------------------------------------------
    // the other plane, and the one write that gives it back
    // -----------------------------------------------------------------------

    use super::super::shell::ScreenOwner;

    /// `CSI ? 1049 h` / `l`, spelled here rather than imported.
    /// What the band's hint row says about the model, and therefore the
    /// cheapest proof that the **band** is on the screen rather than only the
    /// document. The same needle `scripts/smoke-tui.sh` reads the saved primary
    /// buffer for.
    const HINT_MODEL: &str = "glm-5.2";

    const ENTERS_ALTERNATE: &str = "\u{1b}[?1049h";
    const LEAVES_ALTERNATE: &str = "\u{1b}[?1049l";

    /// A screen that counts the vectors, not the bytes and not the syscalls.
    ///
    /// The one-write invariant is about **vectors**: a sampled snapshot of a
    /// pty that happened to look atomic satisfies nothing, because a terminal
    /// presents whatever it has whenever it is scheduled to. So the seam counts
    /// `emit`, which is what the loop is required to call exactly once for the
    /// restore. It counts no syscalls, and cannot: one emit may cost several
    /// short writes on a busy descriptor without the loop having offered
    /// anything twice (`super::super::deliver`), and the old `write_all`
    /// override here existed only to hide that difference behind `Write`.
    #[derive(Default)]
    struct CountingScreen {
        calls: usize,
        written: Vec<u8>,
    }

    impl Sink for CountingScreen {
        fn emit(&mut self, bytes: &[u8]) -> Result<(), Emit> {
            self.calls += 1;
            self.written.extend_from_slice(bytes);
            Ok(())
        }
    }

    /// A question the band can ask on its own rows: no diff, so
    /// `ApprovalSurface::for_request` keeps it inline.
    ///
    /// This module's own, because the shell's fixtures are private to the
    /// shell's tests -- and a second copy of a request is a copy of data, not of
    /// behaviour.
    fn a_question() -> crate::permission::ApprovalRequest {
        crate::permission::ApprovalRequest {
            tool: "edit_file",
            target: "notes.txt".to_string(),
            summary: "edit `notes.txt`: replace \"alpha\" with \"beta\"".to_string(),
            always_scope:
                "allow every future edit_file to `notes.txt` for the rest of this session"
                    .to_string(),
            diff: None,
        }
    }

    /// The envelope that question reaches the shell in.
    fn asked_inline() -> UiEvent {
        UiEvent::Approval(crate::tui::approval::ApprovalAsked {
            id: crate::tui::approval_readiness::ApprovalId(1),
            request: a_question(),
        })
    }

    /// A second, different question that still fits the band -- content the
    /// panel genuinely differs on (target, summary), not a repeat of
    /// [`asked_inline`]'s own. `Shell::ask` replaces the one pending panel
    /// rather than queuing a second, so this is what stands in for "the
    /// document changed" without leaving the primary plane or touching the
    /// document at all.
    fn a_second_question_inline() -> UiEvent {
        UiEvent::Approval(crate::tui::approval::ApprovalAsked {
            id: crate::tui::approval_readiness::ApprovalId(2),
            request: crate::permission::ApprovalRequest {
                tool: "edit_file",
                target: "readme.md".to_string(),
                summary: "edit `readme.md`: replace \"old\" with \"new\"".to_string(),
                always_scope: "allow every future edit_file to `readme.md`".to_string(),
                diff: None,
            },
        })
    }

    /// A question about a change no band summary could show.
    fn a_change_too_big_for_the_band() -> UiEvent {
        UiEvent::Approval(crate::tui::approval::ApprovalAsked {
            id: crate::tui::approval_readiness::ApprovalId(1),
            request: crate::permission::ApprovalRequest {
                tool: "edit_file",
                target: "notes.txt".to_string(),
                summary: "edit `notes.txt`: replace \"alpha\" with \"beta\"".to_string(),
                always_scope: "allow every future edit_file to `notes.txt`".to_string(),
                diff: Some(crate::permission::ApprovalDiff {
                    before: "a".repeat(4_000),
                    after: "b".repeat(4_000),
                }),
            },
        })
    }

    /// A session whose first frame is on the screen and whose second is the
    /// approval plane.
    fn on_the_alternate_plane(
        shell: &mut Fixture,
        band: &mut Band,
        out: &mut CountingScreen,
        failures: &mut FrameFailures,
    ) {
        commit_frame(shell, band, out, failures, Instant::now(), Reconciled)
            .expect("the first frame");
        shell.apply(a_change_too_big_for_the_band());
        assert_eq!(
            shell.screen_owner(),
            ScreenOwner::Approval,
            "the question did not take the plane, so this case proves nothing"
        );
        out.written.clear();
        out.calls = 0;
        commit_frame(shell, band, out, failures, Instant::now(), Reconciled)
            .expect("the frame that takes the plane");
        // What the next tick does with the frame that just landed
        // (`collect_facts`). Without it the question is on the plane and has no
        // receipt, so an affirmative typed by any case below is refused -- which
        // is correct, and is a different case from the ones here.
        shell.reconcile_approval();
    }

    /// A screen that takes `prefix` bytes of the first vector it is offered,
    /// fails once, and takes everything after that -- the shape a terminal
    /// holding a synchronized frame's prefix leaves for whatever this loop
    /// tries next, real accounting and all (`super::super::deliver`).
    struct TornScreen {
        prefix: usize,
        failed: bool,
        written: Vec<u8>,
    }

    impl TornScreen {
        fn taking(prefix: usize) -> Self {
            Self {
                prefix,
                failed: false,
                written: Vec::new(),
            }
        }
    }

    /// Refuses the very first byte of every vector, always with the same
    /// `errno` -- one no other fixture in this file shares -- so a case that
    /// needs a run's *first* failure and a later tear to be provably
    /// different errors, not just different variants, can tell them apart by
    /// more than luck.
    struct RefusesFromTheFirstByte;

    impl RawWrite for RefusesFromTheFirstByte {
        fn write_once(&mut self, _bytes: &[u8]) -> io::Result<usize> {
            Err(io::Error::from_raw_os_error(libc::EACCES))
        }
    }

    impl Sink for RefusesFromTheFirstByte {
        fn emit(&mut self, bytes: &[u8]) -> Result<(), Emit> {
            emit_counted(self, bytes)
        }
    }

    impl RawWrite for TornScreen {
        fn write_once(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.failed {
                self.written.extend_from_slice(bytes);
                return Ok(bytes.len());
            }
            if self.prefix > 0 {
                let taken = self.prefix.min(bytes.len());
                self.prefix -= taken;
                self.written.extend_from_slice(&bytes[..taken]);
                return Ok(taken);
            }
            self.failed = true;
            Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "the screen stopped taking bytes",
            ))
        }
    }

    impl Sink for TornScreen {
        fn emit(&mut self, bytes: &[u8]) -> Result<(), Emit> {
            emit_counted(self, bytes)
        }
    }

    // P3-COMMIT RED, written and run before `Band::recover_primary` existed:
    // a screen that tears the first primary-band frame must not end the
    // session -- `commit_band` is expected to recover in this same call and
    // report `false` (no readiness this tick), not fail. Against the
    // unmodified code this fails, because every `Emit::Partial` reaches
    // `disposed`, which is unconditionally fatal.
    #[test]
    fn a_torn_primary_band_frame_is_recovered_in_the_same_call() {
        let mut fixture = shell();
        fixture.shell.apply(asked_inline());
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut screen = TornScreen::taking(5);

        let landed = commit_band(
            &mut fixture.shell,
            &mut band,
            &mut screen,
            &mut failures,
            Instant::now(),
        )
        .expect("a torn primary-band frame is recovered in the same call, not fatal");

        assert!(
            !landed,
            "a recovered frame must not itself grant readiness this tick"
        );
    }

    /// A screen that tears the first vector at `prefix`, heals for exactly one
    /// vector after that -- the recovery cleanup a caller may try next -- and
    /// then refuses everything again, so a rebuild written right behind the
    /// cleanup finds a screen that is gone once more.
    struct HealsOnlyForTheCleanupVector {
        prefix: usize,
        stage: u8,
        written: Vec<u8>,
    }

    impl HealsOnlyForTheCleanupVector {
        fn taking(prefix: usize) -> Self {
            Self {
                prefix,
                stage: 0,
                written: Vec::new(),
            }
        }
    }

    impl RawWrite for HealsOnlyForTheCleanupVector {
        fn write_once(&mut self, bytes: &[u8]) -> io::Result<usize> {
            match self.stage {
                0 => {
                    if self.prefix > 0 {
                        let taken = self.prefix.min(bytes.len());
                        self.prefix -= taken;
                        self.written.extend_from_slice(&bytes[..taken]);
                        return Ok(taken);
                    }
                    self.stage = 1;
                    Err(io::Error::new(
                        io::ErrorKind::BrokenPipe,
                        "the screen stopped taking bytes",
                    ))
                }
                1 => {
                    self.stage = 2;
                    self.written.extend_from_slice(bytes);
                    Ok(bytes.len())
                }
                _ => Err(io::Error::new(io::ErrorKind::BrokenPipe, "gone again")),
            }
        }
    }

    impl Sink for HealsOnlyForTheCleanupVector {
        fn emit(&mut self, bytes: &[u8]) -> Result<(), Emit> {
            emit_counted(self, bytes)
        }
    }

    #[test]
    fn a_torn_frame_whose_cleanup_vector_is_refused_ends_the_session() {
        // Cleanup, specifically. `IrrecoverablyTorn` never heals -- it takes
        // `prefix` bytes of the first vector it is offered and then refuses
        // every vector after that, forever, including the fixed cleanup
        // vector `recover_primary` tries next. That alone does not tell a
        // cleanup refusal apart from a refusal further downstream (a rebuild
        // that was tried and lost too), so `offered` -- every whole vector
        // this screen was asked to send, not just the bytes it kept -- is
        // asserted directly below: exactly two vectors reached it, the torn
        // frame and then the cleanup, and the cleanup is the *last* one,
        // which is what proves no rebuild was ever attempted behind it.
        let mut fixture = shell();
        fixture.shell.apply(asked_inline());
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut screen = IrrecoverablyTorn::taking(5);

        let err = commit_band(
            &mut fixture.shell,
            &mut band,
            &mut screen,
            &mut failures,
            Instant::now(),
        )
        .expect_err("a cleanup vector the screen refuses ends the session, not just the tick");
        // The exact wording `Emit::Partial`'s `Display` produces
        // (`super::super::deliver::describe_prefix`), not a bare digit a
        // wider match could also find in an address or an errno.
        assert!(
            err.to_string().contains("accepted 5 bytes"),
            "the original tear's own accepted count was lost: {err}"
        );
        assert_eq!(
            screen.offered.len(),
            2,
            "a vector was offered after the cleanup, so a rebuild was attempted too: \
             {:?}",
            screen.offered
        );
        assert_eq!(
            screen.offered[1],
            super::super::check::RECOVERY_CLEANUP.as_bytes(),
            "the second vector offered was not the fixed cleanup vector"
        );
        assert_eq!(
            screen.written.len(),
            5,
            "the cleanup vector put bytes on the screen before it was refused, so it did \
             not fail before any of its own landed: {:?}",
            String::from_utf8_lossy(&screen.written)
        );
    }

    #[test]
    fn a_torn_frame_recovered_by_cleanup_but_lost_again_on_the_rebuild_ends_the_session() {
        // Redraw, specifically: the cleanup vector lands, and the rebuild
        // `recover_primary` writes right behind it does not.
        let mut fixture = shell();
        fixture.shell.apply(asked_inline());
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut screen = HealsOnlyForTheCleanupVector::taking(5);

        let err = commit_band(
            &mut fixture.shell,
            &mut band,
            &mut screen,
            &mut failures,
            Instant::now(),
        )
        .expect_err("a rebuild the screen refuses after a landed cleanup ends the session");
        // Same exact wording as the cleanup-refusal case above, not a bare
        // digit: this is the *original* tear's own count, still named after
        // the cleanup landed and the rebuild behind it failed too.
        assert!(
            err.to_string().contains("accepted 5 bytes"),
            "the original tear's own accepted count was lost: {err}"
        );
        assert_eq!(
            screen.stage, 2,
            "the cleanup vector was not the one vector that actually landed"
        );
    }

    #[test]
    fn recovery_makes_exactly_one_attempt_even_though_the_healed_screen_could_take_a_second() {
        // Zero-recovery recursion. `TornScreen` heals after its first failure,
        // so if `recover_primary` (or its caller) ever tried a second sweep,
        // this screen would happily take it -- the only thing that can catch
        // that is counting vectors, not just checking the outcome is `Ok`.
        let mut fixture = shell();
        fixture.shell.apply(asked_inline());
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut torn = TornScreen::taking(5);

        commit_band(
            &mut fixture.shell,
            &mut band,
            &mut torn,
            &mut failures,
            Instant::now(),
        )
        .expect("a torn frame is recovered in one call");

        // The recovery wrote exactly two vectors: the fixed cleanup, then the
        // one rebuilt frame. A second attempt would be a third.
        let text = String::from_utf8(torn.written).expect("utf-8");
        assert_eq!(
            text.matches(super::super::check::RECOVERY_CLEANUP).count(),
            1,
            "the cleanup vector was not written exactly once: {text:?}"
        );
        assert_eq!(
            text.matches("\u{1b}[?2026h\u{1b}[?25l").count(),
            1,
            "the rebuilt frame was not written exactly once, so more than one attempt was made: \
             {text:?}"
        );
    }

    #[test]
    fn the_budgets_clock_does_not_reset_across_a_successful_recovery() {
        // A recovered tear is spent on the ORIGINAL failure exactly as a
        // refused write is: `failures.failed`, never `failures.succeeded`.
        // So a screen that keeps tearing frames still runs the budget out,
        // even though every individual tear is recovered -- which is only
        // true if the clock a first tear starts survives a recovered one.
        let mut fixture = shell();
        fixture.shell.apply(asked_inline());
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut screen = TornScreen::taking(5);

        commit_band(
            &mut fixture.shell,
            &mut band,
            &mut screen,
            &mut failures,
            Instant::now(),
        )
        .expect("a torn frame is recovered in one call");

        assert!(
            failures.began.is_some(),
            "a successful recovery reset the budget's clock as though nothing had failed"
        );
        assert!(
            failures.first.is_some(),
            "a successful recovery discarded the failure the budget would report if it ran out"
        );
    }

    #[test]
    fn a_same_shape_tick_after_a_recovered_tear_does_not_reset_the_budget_and_a_second_tear_still_exhausts_it(
    ) {
        // The regression the single-call assertions above cannot see:
        // `shell.render.restore` re-arms the exact reasons a recovered tear's
        // own failed attempt carried, unconditionally, in `commit_band`'s
        // `Err(emit)` arm -- so the very next tick offers the *same* pending
        // frame again, driven by nothing this test does by hand. That tick
        // is not guaranteed to land: a screen that tears again before any
        // frame has been independently painted must still be charged against
        // the *original* tear's clock, not a fresh one -- `failures.failed`
        // is `get_or_insert`, but only a test that drives two real tears in a
        // row, both through `commit_band` itself, can see whether the second
        // one actually goes through that path rather than accidentally
        // resetting it.
        let mut fixture = shell();
        fixture.shell.apply(asked_inline());
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut first_tear = TornScreen::taking(5);

        let t0 = Instant::now();
        commit_band(
            &mut fixture.shell,
            &mut band,
            &mut first_tear,
            &mut failures,
            t0,
        )
        .expect("a torn frame is recovered in one call");
        let began = failures
            .began
            .expect("the tear did not start the budget's clock");
        assert!(
            failures.first.is_some(),
            "the tear's own error was not kept"
        );

        // The next tick: no manual `band.invalidate`, no new content -- the
        // same shape, offered again only because `restore` re-armed it, and
        // `FrameFailures::pending_recovery` forces it through
        // `Band::force_redraw` rather than a trusted no-op diff. This second
        // screen tears too, before that write can land -- so this tick is a
        // second failure, not an independently painted frame, and must not
        // touch the clock the first tear started.
        let mut second_tear = TornScreen::taking(5);
        commit_band(
            &mut fixture.shell,
            &mut band,
            &mut second_tear,
            &mut failures,
            t0,
        )
        .expect("the second tear is recovered in the same call, same as the first");
        assert_eq!(
            failures.began,
            Some(began),
            "a second tear before any frame independently landed reset the budget's clock"
        );
        assert!(
            failures.first.is_some(),
            "a second tear before any frame independently landed discarded the original failure"
        );
        assert!(
            failures.pending_recovery,
            "a recovered second tear must still owe the next tick a real, verified write"
        );

        // A third tear, on genuinely different content
        // (`a_second_question_inline`, not a repeat of either tick above), at
        // exactly the first tear's own clock plus the whole budget. This is
        // fatal only if neither tick above ever reset that clock.
        fixture.shell.apply(a_second_question_inline());
        let mut third_tear = TornScreen::taking(5);
        let err = commit_band(
            &mut fixture.shell,
            &mut band,
            &mut third_tear,
            &mut failures,
            began + FRAME_BUDGET,
        )
        .expect_err(
            "a third tear at the first tear's clock plus the whole budget did not exhaust it",
        );
        assert_eq!(
            err.kind(),
            io::ErrorKind::BrokenPipe,
            "the third tear was not charged against the first tear's own clock: {err}"
        );
    }

    // P3-DIAGNOSTIC RED, Trinity R2: `FrameFailures::failed` only ever hands
    // back the run's *first* failure. When the budget expires on a tear
    // (`Emit::Partial`) that arrives behind an earlier, unrelated first
    // failure, the unmodified branch reported that first failure's own kind
    // under `Reason::Exhausted` -- losing the tear that actually ended this
    // tick, both in the returned `io::Error` and in the independent report
    // `diagnostic::report` writes from it.
    #[test]
    fn a_budget_expired_by_an_earlier_failure_still_reports_the_current_tear_as_partial() {
        let mut fixture = shell();
        fixture.shell.apply(asked_inline());
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let t0 = Instant::now();

        // The run's first failure: `ZeroProgress`, `EACCES`/`PermissionDenied`
        // -- a kind and errno no other case in this test needs, so a report
        // that leaked it back out could not be mistaken for the tear below.
        commit_band(
            &mut fixture.shell,
            &mut band,
            &mut RefusesFromTheFirstByte,
            &mut failures,
            t0,
        )
        .expect("the first failure of a run never ends the session");
        assert_eq!(
            failures.first.as_ref().map(io::Error::kind),
            Some(io::ErrorKind::PermissionDenied),
            "the fixture this case depends on did not start the budget the way it expects"
        );

        // A torn primary-band frame, at the first failure's own clock plus
        // the whole budget: the budget is already spent when this tear
        // arrives, so no recovery may be attempted on it either.
        let mut tear = TornScreen::taking(5);
        let err = commit_band(
            &mut fixture.shell,
            &mut band,
            &mut tear,
            &mut failures,
            t0 + FRAME_BUDGET,
        )
        .expect_err("a tear at the first failure's clock plus the whole budget did not exhaust it");

        assert_eq!(
            err.kind(),
            io::ErrorKind::BrokenPipe,
            "the exhausted budget reported the run's first, unrelated failure's kind instead \
             of the current tear's own: {err}"
        );
        assert!(
            err.to_string().contains("accepted 5 bytes"),
            "the current tear's own accepted count was lost behind the run's first failure: {err}"
        );
        assert_eq!(
            tear.written.len(),
            5,
            "a cleanup or rebuild vector was offered to an already-exhausted budget: {:?}",
            String::from_utf8_lossy(&tear.written)
        );

        // The independent record this session's exit writes must agree: the
        // *current* tear's kind, under `Reason::Partial` -- not the first
        // failure's kind under `Reason::Exhausted` -- and still exactly the
        // closed four-key schema, with none of the tear's own words in it.
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let config = RuntimeConfig::load_with(
            &Environment::new(Some(home.path().to_path_buf()), BTreeMap::new()),
            workspace.path(),
        )
        .expect("load a configuration against a fresh home");
        super::super::diagnostic::report(&config, &err).expect("the report was written");
        let body = std::fs::read_to_string(home.path().join(".xfx").join("last-tui-error.json"))
            .expect("read the report");
        let parsed: serde_json::Value =
            serde_json::from_str(&body).unwrap_or_else(|err| panic!("not valid JSON: {err}"));
        // The exact object, not a substring check: the current tear's own
        // kind under `Reason::Partial`, a wrapped `Prefix` errno of `null`,
        // and nothing else -- no schema key an incidental digit or word
        // check could miss, and no room for the tear's own "accepted 5
        // bytes" sentence to hide in an unchecked extra field.
        assert_eq!(
            parsed,
            serde_json::json!({
                "schema": 1,
                "reason": "partial",
                "error_kind": "BrokenPipe",
                "errno": null,
            }),
            "the report is not exactly the expected object: {body:?}"
        );
    }

    #[test]
    fn a_recovered_tick_grants_no_readiness_and_the_next_ordinary_frame_does() {
        // No readiness grant on recovery, and the next normal repaint still
        // verifies it the ordinary way -- the same causal shape as
        // `a_refused_frame_grants_no_readiness_and_the_next_one_does`, driven
        // by a recovered tear instead of a plain refusal.
        let mut fixture = shell();
        fixture.shell.apply(asked_inline());
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut screen = TornScreen::taking(5);

        commit_band(
            &mut fixture.shell,
            &mut band,
            &mut screen,
            &mut failures,
            Instant::now(),
        )
        .expect("a torn frame is recovered in one call");
        fixture.shell.reconcile_approval();
        assert!(
            !fixture
                .shell
                .approval_ready(crate::tui::approval_readiness::ApprovalId(1)),
            "a recovered tear granted a receipt on the same tick it tore on"
        );

        // No manual invalidate and no new content: the recovery already
        // rebuilt the band correctly, so a tick that asked for the very same
        // content again would have nothing left to paint by its own diff
        // alone. What drives this next tick for real is `commit_band` itself
        // -- `shell.render.restore` re-armed the original reasons in the
        // `Err(emit)` arm the tear took, and `FrameFailures::pending_recovery`
        // (set when recovery landed) forces this call to a real write
        // (`Band::force_redraw`) rather than trust a diff against a shadow
        // that is already correct. Only *that* independently painted frame
        // may grant readiness -- an unchanged repaint could not, by the same
        // rule `a_refused_frame_grants_no_readiness_and_the_next_one_does`
        // rests on.
        let landed2 = commit_band(
            &mut fixture.shell,
            &mut band,
            &mut screen,
            &mut failures,
            Instant::now(),
        )
        .expect("the next ordinary frame lands");
        assert!(
            landed2,
            "the next ordinary frame after a recovered tear did not land"
        );
        fixture.shell.reconcile_approval();
        assert!(
            fixture
                .shell
                .approval_ready(crate::tui::approval_readiness::ApprovalId(1)),
            "the next ordinary frame after a recovered tear still did not grant readiness"
        );
    }

    #[test]
    fn a_refused_frame_grants_no_readiness_and_the_next_one_does() {
        // **The causal boundary, deterministically.** The screen refuses the
        // first frame's bytes, so nothing about the question reached it; the
        // second lands, and only then is there a screen for an affirmative to be
        // given against. No sleep and no timing: the refusal is the cause and
        // the receipt is read directly.
        let mut fixture = shell();
        fixture.shell.apply(asked_inline());
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut screen = FlakyScreen {
            refusals: 1,
            kind: io::ErrorKind::WouldBlock,
            written: Vec::new(),
        };

        commit_band(
            &mut fixture.shell,
            &mut band,
            &mut screen,
            &mut failures,
            Instant::now(),
        )
        .expect("a refused frame is counted, not fatal");
        fixture.shell.reconcile_approval();
        assert!(
            !fixture
                .shell
                .approval_ready(crate::tui::approval_readiness::ApprovalId(1)),
            "bytes the screen refused granted a receipt"
        );

        commit_band(
            &mut fixture.shell,
            &mut band,
            &mut screen,
            &mut failures,
            Instant::now(),
        )
        .expect("the second frame lands");
        fixture.shell.reconcile_approval();
        assert!(fixture
            .shell
            .approval_ready(crate::tui::approval_readiness::ApprovalId(1)));
        assert!(
            !screen.written.is_empty(),
            "nothing was ever written, so this proves nothing"
        );
    }

    #[test]
    fn a_winch_between_the_write_and_the_reconcile_revokes_the_frame() {
        // The window the post-write check exists for. A `SIGWINCH` that lands
        // inside the write is drained on the tick *after* it (`collect_facts`),
        // and `geometry` still reports the old size for the whole debounce -- so
        // a receipt kept here would match those dimensions and grant against a
        // screen that has already reflowed.
        let mut fixture = shell();
        fixture.shell.apply(asked_inline());
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut screen = FlakyScreen {
            refusals: 0,
            kind: io::ErrorKind::WouldBlock,
            written: Vec::new(),
        };

        commit_band(
            &mut fixture.shell,
            &mut band,
            &mut screen,
            &mut failures,
            Instant::now(),
        )
        .expect("the frame lands");
        let winched = Instant::now();
        fixture.shell.render.mark_resize(winched);
        fixture.shell.reconcile_approval();
        assert!(!fixture
            .shell
            .approval_ready(crate::tui::approval_readiness::ApprovalId(1)));

        // And it comes back the ordinary way once the resize has settled and a
        // frame for the screen the user is now looking at has landed. Without
        // this half, a `reconcile` that revoked for ever would pass.
        //
        // Driven through `resolve_resize`, which is what the loop really does
        // with a settled deadline: re-solve the band, invalidate the shadow,
        // and let the next frame be a whole one.
        let settled = winched + RESIZE_DEBOUNCE;
        resolve_resize(&mut fixture.shell, &mut band, settled, || (30, 100));
        commit_band(
            &mut fixture.shell,
            &mut band,
            &mut screen,
            &mut failures,
            settled,
        )
        .expect("the redraw lands");
        fixture.shell.reconcile_approval();
        assert!(
            fixture
                .shell
                .approval_ready(crate::tui::approval_readiness::ApprovalId(1)),
            "a settled resize never earned a new receipt"
        );
    }

    #[test]
    fn a_document_row_owed_when_the_plane_changes_hands_is_written_before_it_does() {
        // **One tick takes a batch of events, and a batch can cross a plane.**
        // `take_ui_events` drains up to `bridge::UI_EVENTS` of them and the tick
        // composes exactly one frame afterwards, so a tool result and the
        // question that follows it are both applied before anything is
        // written -- and that one frame, seeing the question already owns the
        // screen, is routed to the other buffer. What the primary plane owes
        // stays owed (`paint_alternate`), so the tool's own row never reaches
        // the document at all: the user is asked about a change whose tool call
        // left no trace, and the row lands minutes later when the plane comes
        // back.
        //
        // Slow scheduling splits the two events across two ticks and hides it,
        // which is exactly what happened: this passed locally and on one CI
        // runner and failed on the two faster ones (run 33256925472, exact head
        // f45d005 -- Linux and arm64 macOS waited out scenario 20's
        // `[tool] read_file ok` while the alternate screen was already up).
        //
        // Driven through the real drain with both events already in the
        // channel, so the batch is a fact rather than a race: no sleep, no
        // scheduler, nothing this test has to hope for.
        let (events, mut receiver) = mpsc::channel(bridge::UI_EVENTS);
        events
            .try_send(UiEvent::ToolResult {
                call_id: "call-0".to_string(),
                tool: "read_file".to_string(),
                ok: true,
                detail: String::new(),
            })
            .expect("the channel is empty");
        events
            .try_send(a_change_too_big_for_the_band())
            .expect("the channel has room");

        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = CountingScreen::default();
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            Instant::now(),
            Reconciled,
        )
        .expect("the first frame");
        out.written.clear();
        out.calls = 0;

        // The tick's own order (`run`): the batch, then the settle, then the
        // one frame.
        take_ui_events(&mut shell, &mut receiver);
        shell.settle_band(Instant::now());
        assert_eq!(
            shell.screen_owner(),
            ScreenOwner::Approval,
            "the batch did not reach the question, so this case proves nothing"
        );
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            Instant::now(),
            Reconciled,
        )
        .expect("the frame");

        // **Three things in one order**, because paying the document is only
        // the first half of what the primary plane is owed. An append is a
        // *scroll*: it moves the band's own rows up and leaves the band where
        // it no longer is, so a tick that appended and then took the plane
        // hands the terminal a buffer to save that carries the document and a
        // hole where the band was. That is what `1049h` freezes, and what the
        // user is given back when the question is answered.
        let written = String::from_utf8_lossy(&out.written).into_owned();
        let row = written
            .find("[tool] read_file ok")
            .unwrap_or_else(|| panic!("the tool's row never reached the document: {written:?}"));
        let band_row = written
            .find(HINT_MODEL)
            .unwrap_or_else(|| panic!("the band was never repainted: {written:?}"));
        let took = written
            .find(ENTERS_ALTERNATE)
            .unwrap_or_else(|| panic!("the plane was never taken: {written:?}"));
        assert!(
            row < band_row,
            "the band was painted before the append scrolled it: {written:?}"
        );
        assert!(
            band_row < took,
            "the plane changed hands over a band the scroll had moved: {written:?}"
        );
        assert!(band.on_alternate(), "the loop forgot which plane it is on");
        assert!(
            shell.take_pending().is_empty(),
            "the document is still owed rows the plane was taken over"
        );
        assert!(
            shell.render.begin().is_none(),
            "the primary plane is still owed a frame the alternate cannot paint"
        );
    }

    #[test]
    fn the_barrier_is_about_an_owed_row_rather_than_about_the_event_that_made_it() {
        // The fix is a property of the **plane transition**, not of `read_file`
        // and not of a tool call: anything that puts a row in the document in
        // the same batch as the question has the same claim on the primary
        // buffer. A guard written around the event that happened to expose it
        // would leave every other producer -- a notice, a turn's failure line,
        // the sentence a refusal writes -- with the defect intact.
        let (events, mut receiver) = mpsc::channel(bridge::UI_EVENTS);
        events
            .try_send(UiEvent::Notice("xfx: something worth saying".to_string()))
            .expect("the channel is empty");
        events
            .try_send(a_change_too_big_for_the_band())
            .expect("the channel has room");

        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = CountingScreen::default();
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            Instant::now(),
            Reconciled,
        )
        .expect("the first frame");
        out.written.clear();

        take_ui_events(&mut shell, &mut receiver);
        shell.settle_band(Instant::now());
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            Instant::now(),
            Reconciled,
        )
        .expect("the frame");

        let written = String::from_utf8_lossy(&out.written).into_owned();
        let row = written
            .find("something worth saying")
            .unwrap_or_else(|| panic!("the notice never reached the document: {written:?}"));
        let took = written
            .find(ENTERS_ALTERNATE)
            .unwrap_or_else(|| panic!("the plane was never taken: {written:?}"));
        assert!(
            row < took,
            "the plane changed hands with a row owed: {written:?}"
        );
    }

    #[test]
    fn a_refused_row_keeps_the_plane_as_well_as_the_row() {
        // The barrier's own failure path. A write that was refused leaves the
        // document owed, and taking the plane anyway would put the question on
        // the other buffer with the row still waiting -- the very state the
        // barrier exists to prevent, reached through the door it added. So the
        // tick ends: the rows stay owed, the plane stays where it is, and the
        // next tick offers both again in this order.
        // **Two** rows, to prove the order survives a refusal mid-batch, not
        // just that something does. `FlakyScreen`'s refusal is a
        // `ZeroProgress` (`commit_document`): the refused row moved no bytes,
        // so it and the one behind it are both still owed and both land on
        // the next tick, oldest first -- only the second is asserted below,
        // since the first landing exactly once is what the zero-progress
        // append tests above already cover.
        let (events, mut receiver) = mpsc::channel(bridge::UI_EVENTS);
        events
            .try_send(UiEvent::Notice("xfx: a row that will not land".to_string()))
            .expect("the channel is empty");
        events
            .try_send(UiEvent::Notice("xfx: the row behind it".to_string()))
            .expect("the channel has room");
        events
            .try_send(a_change_too_big_for_the_band())
            .expect("the channel has room");

        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        commit_frame(
            &mut shell,
            &mut band,
            &mut CountingScreen::default(),
            &mut failures,
            Instant::now(),
            Reconciled,
        )
        .expect("the first frame");

        take_ui_events(&mut shell, &mut receiver);
        shell.settle_band(Instant::now());
        // A screen that refuses **the append and nothing else**, which is the
        // only writer that can tell the two behaviours apart: one that refused
        // everything would leave the plane untaken whatever the loop decided,
        // because the bytes that take it would be refused too.
        let mut out = FlakyScreen {
            refusals: 1,
            kind: io::ErrorKind::BrokenPipe,
            written: Vec::new(),
        };
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            Instant::now(),
            Reconciled,
        )
        .expect("a refused write is counted, not fatal on its own");

        assert!(
            !band.on_alternate(),
            "the plane was taken over a row the terminal refused"
        );
        assert!(
            !String::from_utf8_lossy(&out.written).contains(ENTERS_ALTERNATE),
            "the plane was offered the terminal over a row that did not land"
        );
        assert!(
            shell.owes_document(),
            "the rows behind the refused one were dropped instead of staying owed"
        );
        assert_eq!(
            shell.screen_owner(),
            ScreenOwner::Approval,
            "the question was forgotten with the frame"
        );
        // And the next tick, onto a screen that works, pays the document and
        // then takes the plane -- in that order, which is the whole rule.
        let mut out = CountingScreen::default();
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            Instant::now(),
            Reconciled,
        )
        .expect("the frame after the refusal");
        let written = String::from_utf8_lossy(&out.written).into_owned();
        let row = written
            .find("the row behind it")
            .unwrap_or_else(|| panic!("the surviving row never landed: {written:?}"));
        let took = written
            .find(ENTERS_ALTERNATE)
            .unwrap_or_else(|| panic!("the plane was never taken: {written:?}"));
        assert!(row < took, "the retry took the plane first: {written:?}");
    }

    #[test]
    fn the_scroll_the_barrier_makes_is_itself_a_reason_for_the_frame_it_pays() {
        // The barrier pays the band by asking [`RenderRequest::begin`] for an
        // attempt, and a tick whose reasons were already taken has none to
        // give. Every event in this suite happens to raise one, which is
        // exactly the kind of coincidence that holds on three runners and not
        // on the fourth -- so the **scroll** raises its own: the append moved
        // the band, and that is a reason for a frame whatever else is pending.
        let (events, mut receiver) = mpsc::channel(bridge::UI_EVENTS);
        events
            .try_send(UiEvent::Notice("xfx: a row that scrolls".to_string()))
            .expect("the channel is empty");
        events
            .try_send(a_change_too_big_for_the_band())
            .expect("the channel has room");

        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        commit_frame(
            &mut shell,
            &mut band,
            &mut CountingScreen::default(),
            &mut failures,
            Instant::now(),
            Reconciled,
        )
        .expect("the first frame");

        take_ui_events(&mut shell, &mut receiver);
        shell.settle_band(Instant::now());
        // Taken and **dropped**, which is the one thing production may never do
        // with an attempt: it is how this case says "a frame has already
        // accounted for everything these events asked for", leaving the tick
        // below with a document to pay and nothing else claiming a frame.
        let _taken_and_dropped = shell.render.begin();

        let mut out = CountingScreen::default();
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            Instant::now(),
            Reconciled,
        )
        .expect("the frame that takes the plane");

        let written = String::from_utf8_lossy(&out.written).into_owned();
        let row = written
            .find("a row that scrolls")
            .unwrap_or_else(|| panic!("the row was never written: {written:?}"));
        let band_row = written.find(HINT_MODEL).unwrap_or_else(|| {
            panic!("the scroll asked for no frame, so the band stayed moved: {written:?}")
        });
        let took = written
            .find(ENTERS_ALTERNATE)
            .unwrap_or_else(|| panic!("the plane was never taken: {written:?}"));
        assert!(
            row < band_row,
            "the band was painted before the scroll: {written:?}"
        );
        assert!(
            band_row < took,
            "the plane changed hands over a band the scroll had moved: {written:?}"
        );
    }

    /// A screen that refuses the one write carrying `needle`, and works either
    /// side of it.
    ///
    /// The band's frame is the only write in a barrier tick that carries the
    /// band's own text: the document's rows are the notice, and the plane's are
    /// the mode sequence. So this refuses **the primary frame and nothing
    /// else** -- the one writer that can tell "the rows landed and the band did
    /// not" apart from "nothing landed".
    struct DeafToTheBand {
        needle: &'static str,
        refused: bool,
        written: Vec<u8>,
    }

    impl RawWrite for DeafToTheBand {
        fn write_once(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if !self.refused && String::from_utf8_lossy(bytes).contains(self.needle) {
                self.refused = true;
                return Err(io::Error::new(io::ErrorKind::BrokenPipe, "not now"));
            }
            self.written.extend_from_slice(bytes);
            Ok(bytes.len())
        }
    }

    impl Sink for DeafToTheBand {
        fn emit(&mut self, bytes: &[u8]) -> Result<(), Emit> {
            emit_counted(self, bytes)
        }
    }

    #[test]
    fn a_refused_band_keeps_the_plane_too() {
        // The second half of the barrier's failure path, and the reason the
        // barrier pays the band at all. The rows landed, so the screen has
        // scrolled and the band is a row above where it belongs; the repaint
        // that would fix it was refused. Taking the plane now would save
        // exactly the buffer this round is about -- document rows, and a hole
        // where the band was -- so the tick ends instead, and the next one
        // repaints before it offers the terminal to the question.
        let (events, mut receiver) = mpsc::channel(bridge::UI_EVENTS);
        events
            .try_send(UiEvent::Notice("xfx: a row that lands".to_string()))
            .expect("the channel is empty");
        events
            .try_send(a_change_too_big_for_the_band())
            .expect("the channel has room");

        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        commit_frame(
            &mut shell,
            &mut band,
            &mut CountingScreen::default(),
            &mut failures,
            Instant::now(),
            Reconciled,
        )
        .expect("the first frame");

        take_ui_events(&mut shell, &mut receiver);
        shell.settle_band(Instant::now());
        let mut out = DeafToTheBand {
            needle: HINT_MODEL,
            refused: false,
            written: Vec::new(),
        };
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            Instant::now(),
            Reconciled,
        )
        .expect("a refused write is counted, not fatal on its own");

        let written = String::from_utf8_lossy(&out.written).into_owned();
        assert!(
            written.contains("a row that lands"),
            "the row the barrier pays first never landed: {written:?}"
        );
        assert!(
            !band.on_alternate(),
            "the plane was taken over a band the screen refused"
        );
        assert!(
            !written.contains(ENTERS_ALTERNATE),
            "the plane was offered the terminal over a band that did not land: {written:?}"
        );
        assert_eq!(
            shell.screen_owner(),
            ScreenOwner::Approval,
            "the question was forgotten with the frame"
        );
        // Taken and put back, which is what `restore` is for: there is no way
        // to ask without taking, and a test that took the frame the retry below
        // is about would be testing its own bookkeeping.
        let owed = shell.render.begin();
        assert!(
            owed.is_some(),
            "the refused frame was counted as delivered instead of owed again"
        );
        if let Some(attempt) = owed {
            shell.render.restore(attempt);
        }

        // And the next tick, onto a screen that works, repaints the band and
        // then takes the plane -- in that order, which is the whole rule.
        let mut out = CountingScreen::default();
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            Instant::now(),
            Reconciled,
        )
        .expect("the frame after the refusal");
        let written = String::from_utf8_lossy(&out.written).into_owned();
        let band_row = written
            .find(HINT_MODEL)
            .unwrap_or_else(|| panic!("the band was never repainted: {written:?}"));
        let took = written
            .find(ENTERS_ALTERNATE)
            .unwrap_or_else(|| panic!("the plane was never taken: {written:?}"));
        assert!(
            band_row < took,
            "the retry took the plane first: {written:?}"
        );
    }

    #[test]
    fn the_barrier_writes_nothing_on_a_screen_no_band_fits_on() {
        // The one place the barrier may not pay what is owed. `blind` means the
        // row numbers are claims about a screen that may not exist, and an
        // append is a **scroll**: what leaves the top of the screen is in the
        // terminal's own scrollback for good, so a row placed by a coordinate
        // the terminal no longer has cannot be taken back by any later frame.
        // Those rows keep waiting for a screen, exactly as they do when no
        // question is pending.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = screen();
        let asked = std::cell::Cell::new(0usize);
        let start = Instant::now();
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            start,
            Reconciled,
        )
        .expect("the first frame");

        shell.render.mark_resize(start);
        resolve_resize(
            &mut shell,
            &mut band,
            start + RESIZE_DEBOUNCE,
            sized((4, 80), &asked),
        );
        assert!(
            shell.blind(),
            "the fixture is not on a screen it must not write"
        );

        shell.apply(UiEvent::Notice("xfx: a row with nowhere to go".to_string()));
        shell.apply(a_change_too_big_for_the_band());
        assert!(
            shell.owes_document(),
            "the fixture owes no row, so this proves nothing"
        );
        out.written.clear();
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            start,
            Reconciled,
        )
        .expect("the frame");

        let written = String::from_utf8_lossy(&out.written).into_owned();
        assert!(
            !written.contains("a row with nowhere to go"),
            "a document row was scrolled onto a screen the band does not fit on: {written:?}"
        );
        assert!(
            shell.owes_document(),
            "the row was taken and dropped instead of staying owed"
        );
    }

    #[test]
    fn a_frame_asked_for_on_a_screen_no_band_fits_on_waits_for_one_that_does() {
        // The other half of the blind rule, and the one the integration suite
        // leans on: not "nothing happened to want a frame", but **a frame was
        // genuinely owed and still nothing was written**. Ctrl-L is the shape
        // of it a user can produce at will -- `Action::Redraw` raises
        // `Reason::ExternalDamage` from inside the approval panel
        // (`super::shell`), so a frame is asked for by a keystroke that answers
        // no question and changes no state.
        //
        // Written because the pty case that used to carry this claim could pass
        // on a session that wanted nothing. It cannot any more, and this is
        // where the claim is deterministic: no signal, no scheduler, no clock
        // but the one this test hands in.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = screen();
        let asked = std::cell::Cell::new(0usize);
        let start = Instant::now();
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            start,
            Reconciled,
        )
        .expect("the first frame");

        shell.render.mark_resize(start);
        resolve_resize(
            &mut shell,
            &mut band,
            start + RESIZE_DEBOUNCE,
            sized((4, 80), &asked),
        );
        assert!(
            shell.blind(),
            "the fixture is not on a screen it must not write"
        );

        out.written.clear();
        shell.render.request(Reason::ExternalDamage);
        // Three ticks, because one proves only that the tick after the resolve
        // is quiet: the rule is that the screen stays unwritten for as long as
        // it cannot hold a band, not that the first frame after the news is.
        for tick in 0..3 {
            commit_frame(
                &mut shell,
                &mut band,
                &mut out,
                &mut failures,
                start + RESIZE_DEBOUNCE + Duration::from_millis(tick * 8),
                Reconciled,
            )
            .expect("a tick on a screen no band fits on");
        }
        assert!(
            out.written.is_empty(),
            "a frame a keystroke asked for was painted onto a screen no band fits on: {:?}",
            String::from_utf8_lossy(&out.written)
        );

        // And it is **held**, not dropped. The screen grows to a different one
        // from the one it left, so what comes back is a re-solve rather than
        // the frame that was owed -- and it lands at the coordinates the
        // terminal really has.
        shell.render.mark_resize(start + RESIZE_DEBOUNCE);
        resolve_resize(
            &mut shell,
            &mut band,
            start + RESIZE_DEBOUNCE + RESIZE_DEBOUNCE,
            sized((30, 100), &asked),
        );
        assert!(!shell.blind(), "the session is still refusing to write");
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            start + RESIZE_DEBOUNCE + RESIZE_DEBOUNCE,
            Reconciled,
        )
        .expect("the frame the screen can hold");

        let written = String::from_utf8_lossy(&out.written).into_owned();
        assert!(
            !written.is_empty(),
            "the frame the small screen held was dropped instead of owed"
        );
        assert!(
            written.contains("\u{1b}[30;1H"),
            "the held frame landed at the coordinates of the screen that was: {written:?}"
        );
    }

    #[test]
    fn a_question_the_band_cannot_show_takes_the_plane_in_one_frame() {
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = CountingScreen::default();

        on_the_alternate_plane(&mut shell, &mut band, &mut out, &mut failures);

        let written = String::from_utf8_lossy(&out.written).into_owned();
        assert_eq!(out.calls, 1, "the enter was not one write: {written:?}");
        assert!(
            written.starts_with(ENTERS_ALTERNATE),
            "the plane was painted before it was taken: {written:?}"
        );
        assert!(
            written.contains("Permission needed"),
            "the plane was taken and the question was not painted on it: {written:?}"
        );
        assert!(band.on_alternate(), "the loop forgot which plane it is on");
    }

    #[test]
    fn a_tick_on_the_other_plane_that_changed_nothing_writes_nothing_at_all() {
        // Scenario 14's claim, on the plane it does not cover. The band asks for
        // a frame twice a second while a turn is running, and a question is up
        // for as long as a person takes to read a change -- so a loop that wrote
        // whatever the band handed it would put a whole unchanged screen on the
        // wire, twice a second, for minutes.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = CountingScreen::default();
        on_the_alternate_plane(&mut shell, &mut band, &mut out, &mut failures);

        shell.render.request(Reason::Animation);
        out.written.clear();
        out.calls = 0;
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            Instant::now(),
            Reconciled,
        )
        .expect("a tick on the other plane");
        assert_eq!(
            out.calls,
            0,
            "a screen the terminal already holds was written again: {:?}",
            String::from_utf8_lossy(&out.written)
        );

        // And a marker that moved is still a frame, so the skip is a skip
        // rather than a surface that stopped painting.
        shell.route_bytes(&[0x1b, b'[', b'B']);
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            Instant::now(),
            Reconciled,
        )
        .expect("the frame the moved marker asked for");
        assert_eq!(out.calls, 1, "the marker moved and nothing was written");
    }

    #[test]
    fn an_empty_alternate_repaint_still_clears_a_budget_no_recovery_owns() {
        // The positive companion to the seam test below: the same empty
        // alternate-plane repaint, but with no primary recovery pending,
        // still resets the budget exactly as it did before the narrowing.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = CountingScreen::default();
        on_the_alternate_plane(&mut shell, &mut band, &mut out, &mut failures);

        let began = Instant::now();
        assert!(
            failures
                .failed(refused(io::ErrorKind::WouldBlock), began)
                .is_none(),
            "a single counted failure ended the session on its own"
        );
        assert!(!failures.pending_recovery, "no recovery is owed here");

        shell.render.request(Reason::Animation);
        out.written.clear();
        out.calls = 0;
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            began,
            Reconciled,
        )
        .expect("a tick on the other plane");
        assert_eq!(
            out.calls, 0,
            "an unchanged alternate screen was written again"
        );

        assert!(
            failures.began.is_none(),
            "an empty alternate repaint with no recovery pending did not reset the budget's clock"
        );
        assert!(
            failures.first.is_none(),
            "an empty alternate repaint with no recovery pending kept the counted failure"
        );
    }

    #[test]
    fn a_pending_recovery_forces_the_owned_plane_to_a_real_repaint_and_only_that_clears_it() {
        // A **constructed test seam**: `pending_recovery` raised by hand on a
        // screen nothing tore. It pins how the already-there arm *reads* the
        // flag, whoever raised it. The real road, a recovered tear of this
        // plane, is the `a_recovered_alternate_tick_grants_no_readiness_…`
        // case below.
        //
        // This case used to assert the opposite half of the same rule: that an
        // **empty** repaint could not clear a pending recovery, because
        // nothing on this arm forced a real write. Something does now
        // (`Band::force_alternate_repaint`), so a tick that owes a verified
        // write never gets to be empty -- and what is pinned instead is that
        // the unchanged screen is written whole once, and that this write,
        // not the tick, is what gives the budget back.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = CountingScreen::default();
        on_the_alternate_plane(&mut shell, &mut band, &mut out, &mut failures);

        let began = Instant::now();
        assert!(
            failures
                .failed(refused(io::ErrorKind::WouldBlock), began)
                .is_none(),
            "a single counted failure ended the session on its own"
        );
        failures.pending_recovery = true; // the seam -- see the comment above

        shell.render.request(Reason::Animation);
        out.written.clear();
        out.calls = 0;
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            began,
            Reconciled,
        )
        .expect("a tick on the other plane");
        let written = String::from_utf8_lossy(&out.written).into_owned();
        assert_eq!(
            out.calls, 1,
            "a tick that owes a verified repaint wrote nothing: {written:?}"
        );
        assert!(
            written.contains("\u{1b}[2J") && written.contains("Permission needed"),
            "the forced repaint is not the whole question: {written:?}"
        );
        assert!(
            !failures.pending_recovery && failures.began.is_none() && failures.first.is_none(),
            "the repaint that landed did not give the budget back"
        );
    }

    /// Where the alternate-plane recovery cases below tear the repaint.
    ///
    /// Long enough to carry the repaint's whole head -- the frame's open, the
    /// `CUP` home and the screen erase -- and the start of its first row, so
    /// "the rebuild is the vector the tear was of" compares more than an escape
    /// introducer.
    const ALTERNATE_TEAR: usize = 48;

    /// A marker that moved on a question that already owns the other plane:
    /// the one thing that asks the `(true, Approval)` arm for a repaint with
    /// bytes in it, and the tick every case below tears.
    fn a_repaint_of_the_owned_plane(
        shell: &mut Fixture,
        band: &mut Band,
        out: &mut impl Sink,
        failures: &mut FrameFailures,
        now: Instant,
    ) -> io::Result<()> {
        shell.route_bytes(&[0x1b, b'[', b'B']);
        commit_frame(shell, band, out, failures, now, Reconciled)
    }

    /// The independent record a session that ended on `err` leaves behind,
    /// parsed: the closed four-key object `diagnostic::report` writes.
    fn reported(err: &io::Error) -> serde_json::Value {
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let config = RuntimeConfig::load_with(
            &Environment::new(Some(home.path().to_path_buf()), BTreeMap::new()),
            workspace.path(),
        )
        .expect("load a configuration against a fresh home");
        super::super::diagnostic::report(&config, err).expect("the report was written");
        let body = std::fs::read_to_string(home.path().join(".xfx").join("last-tui-error.json"))
            .expect("read the report");
        serde_json::from_str(&body).unwrap_or_else(|err| panic!("not valid JSON: {err}: {body:?}"))
    }

    // P3-COMMIT (alternate plane) RED, written and run before the
    // `(true, Approval)` arm had a recovery road: a screen that tears a
    // repaint of the plane this session already owns must not end the session
    // -- the arm is expected to recover in this same call, the way
    // `commit_band` recovers a torn primary band. Against the unmodified code
    // this fails, because that arm hands every `Emit::Partial` to `disposed`,
    // which is unconditionally fatal.
    #[test]
    fn a_torn_repaint_of_the_plane_the_session_owns_is_recovered_in_the_same_call() {
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = CountingScreen::default();
        on_the_alternate_plane(&mut shell, &mut band, &mut out, &mut failures);

        let mut torn = TornScreen::taking(ALTERNATE_TEAR);
        a_repaint_of_the_owned_plane(
            &mut shell,
            &mut band,
            &mut torn,
            &mut failures,
            Instant::now(),
        )
        .expect("a torn repaint of a plane this session already owns is recovered, not fatal");

        let text = String::from_utf8_lossy(&torn.written).into_owned();
        let (prefix, rest) = torn.written.split_at(ALTERNATE_TEAR);
        let cleanup = super::super::check::RECOVERY_CLEANUP.as_bytes();
        assert!(
            rest.starts_with(cleanup),
            "the fixed cleanup vector was not the next thing on the wire after the tear: {text:?}"
        );
        // **The same vector, whole.** The repaint is built from the rows,
        // geometry and cursor the torn attempt was, and nothing in the paint
        // reads the cache recovery dropped -- so the rebuild opens with
        // exactly the bytes the terminal already took and then finishes them.
        let rebuilt = &rest[cleanup.len()..];
        assert!(
            rebuilt.starts_with(prefix) && rebuilt.len() > prefix.len(),
            "the rebuild is not the vector the tear was of: {text:?}"
        );
        let rebuilt = String::from_utf8_lossy(rebuilt).into_owned();
        assert_eq!(
            rebuilt.matches("\u{1b}[?2026h\u{1b}[?25l").count(),
            1,
            "more than one rebuild was written, so more than one attempt was made: {text:?}"
        );
        assert!(
            rebuilt.ends_with("\u{1b}[?2026l\u{1b}[?25h"),
            "the rebuild did not close its frame: {rebuilt:?}"
        );
        assert!(
            rebuilt.contains("\u{1b}[2J") && rebuilt.contains("Permission needed"),
            "the rebuild is not a whole repaint of the question: {rebuilt:?}"
        );
        assert!(
            !text.contains("\u{1b}[?1049"),
            "recovering a repaint moved the terminal between its planes: {text:?}"
        );
        assert!(band.on_alternate(), "recovery lost the plane it was on");
        assert!(
            failures.pending_recovery,
            "a recovered tear does not owe the next tick a verified repaint"
        );
        assert!(
            failures.began.is_some() && failures.first.is_some(),
            "the tear was not spent on the budget the way a refused write is"
        );
    }

    #[test]
    fn a_refused_repaint_of_the_owned_plane_is_still_counted_and_offered_again() {
        // The two roads that moved no byte are not this unit's: a refusal and
        // a zero-progress write leave the screen as it was, so the repaint is
        // owed again under the budget -- and no cleanup vector is ever written
        // for a screen nothing reached.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = CountingScreen::default();
        on_the_alternate_plane(&mut shell, &mut band, &mut out, &mut failures);

        let mut screen = FlakyScreen {
            refusals: 1,
            kind: io::ErrorKind::WouldBlock,
            written: Vec::new(),
        };
        a_repaint_of_the_owned_plane(
            &mut shell,
            &mut band,
            &mut screen,
            &mut failures,
            Instant::now(),
        )
        .expect("a refused repaint is counted, not fatal");
        assert!(
            screen.written.is_empty(),
            "a refused repaint put bytes on the wire"
        );
        assert!(
            !failures.pending_recovery,
            "a write that moved no byte was treated as a recovered tear"
        );

        commit_frame(
            &mut shell,
            &mut band,
            &mut screen,
            &mut failures,
            Instant::now(),
            Reconciled,
        )
        .expect("the repaint offered again lands");
        let written = String::from_utf8_lossy(&screen.written).into_owned();
        assert!(
            written.contains("Permission needed"),
            "the refused repaint was not offered again: {written:?}"
        );
        assert!(
            !written.contains(super::super::check::RECOVERY_CLEANUP),
            "a cleanup vector was written for a screen nothing reached: {written:?}"
        );
        assert!(
            failures.began.is_none(),
            "the repaint that landed did not mend the budget"
        );
    }

    #[test]
    fn a_torn_alternate_repaint_whose_cleanup_vector_is_refused_ends_the_session_naming_the_tear() {
        // Cleanup, specifically: `IrrecoverablyTorn` takes the prefix and then
        // refuses everything, the fixed cleanup vector included -- and
        // `offered` says the cleanup was the last vector it was asked for, so
        // no rebuild was attempted behind a cleanup that never landed.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = CountingScreen::default();
        on_the_alternate_plane(&mut shell, &mut band, &mut out, &mut failures);

        let mut screen = IrrecoverablyTorn::taking(ALTERNATE_TEAR);
        let err = a_repaint_of_the_owned_plane(
            &mut shell,
            &mut band,
            &mut screen,
            &mut failures,
            Instant::now(),
        )
        .expect_err("a cleanup vector the screen refuses ends the session, not just the tick");

        let text = err.to_string();
        assert!(
            text.contains(&format!("accepted {ALTERNATE_TEAR} bytes")),
            "the original tear's own accepted count was lost: {text}"
        );
        assert!(
            text.contains("could not be recovered"),
            "the error does not say a recovery was attempted and failed: {text}"
        );
        assert_eq!(
            screen.offered.len(),
            2,
            "a vector was offered after the refused cleanup, so a rebuild was attempted too: \
             {:?}",
            screen.offered
        );
        assert_eq!(
            screen.offered[1],
            super::super::check::RECOVERY_CLEANUP.as_bytes(),
            "the second vector offered was not the fixed cleanup vector"
        );
        assert_eq!(
            screen.written.len(),
            ALTERNATE_TEAR,
            "the cleanup put bytes on the screen before it was refused: {:?}",
            String::from_utf8_lossy(&screen.written)
        );
        assert_eq!(
            reported(&err)["reason"],
            "partial",
            "an alternate repaint that could not be recovered was not recorded as a tear"
        );
    }

    #[test]
    fn a_torn_alternate_repaint_recovered_by_cleanup_but_lost_on_the_rebuild_ends_the_session() {
        // Rebuild, specifically: the cleanup vector lands, and the repaint
        // written right behind it does not.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = CountingScreen::default();
        on_the_alternate_plane(&mut shell, &mut band, &mut out, &mut failures);

        let mut screen = HealsOnlyForTheCleanupVector::taking(ALTERNATE_TEAR);
        let err = a_repaint_of_the_owned_plane(
            &mut shell,
            &mut band,
            &mut screen,
            &mut failures,
            Instant::now(),
        )
        .expect_err("a rebuild the screen refuses after a landed cleanup ends the session");

        let text = err.to_string();
        assert!(
            text.contains(&format!("accepted {ALTERNATE_TEAR} bytes")),
            "the original tear's own accepted count was lost: {text}"
        );
        assert!(
            text.contains("could not be recovered"),
            "the error does not say a recovery was attempted and failed: {text}"
        );
        assert_eq!(
            screen.stage, 2,
            "the cleanup vector was not the one vector that actually landed"
        );
        assert_eq!(reported(&err)["reason"], "partial");
    }

    #[test]
    fn a_recovered_alternate_tick_grants_no_readiness_and_the_next_tick_forces_a_repaint_that_does()
    {
        // The readiness half. The recovered tick writes a whole, correct
        // question -- and still mints no receipt, because the only frame that
        // may grant one is a frame this loop verifies on its own account. The
        // tick after it is the one that does, and it has to be a **real**
        // repaint: recovery left the cache describing exactly what the rebuild
        // painted, so without something forcing the write the next repaint
        // would be the empty "screen already holds this" one, which keeps a
        // receipt and never mints one -- a question nobody could say yes to.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = CountingScreen::default();
        on_the_alternate_plane(&mut shell, &mut band, &mut out, &mut failures);
        assert!(
            shell.approval_ready(crate::tui::approval_readiness::ApprovalId(1)),
            "the question was never answerable, so losing readiness below proves nothing"
        );

        let mut torn = TornScreen::taking(ALTERNATE_TEAR);
        a_repaint_of_the_owned_plane(
            &mut shell,
            &mut band,
            &mut torn,
            &mut failures,
            Instant::now(),
        )
        .expect("a torn repaint is recovered in one call");
        shell.reconcile_approval();
        assert!(
            !shell.approval_ready(crate::tui::approval_readiness::ApprovalId(1)),
            "a recovered tear granted a receipt on the tick it tore on"
        );

        // No input and no new content: the next tick is driven by the reasons
        // the torn attempt re-armed, and by nothing this case does by hand.
        out.written.clear();
        out.calls = 0;
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            Instant::now(),
            Reconciled,
        )
        .expect("the tick after a recovered tear");
        let written = String::from_utf8_lossy(&out.written).into_owned();
        assert_eq!(
            out.calls, 1,
            "the tick after a recovered tear was not one real repaint: {written:?}"
        );
        assert!(
            written.contains("\u{1b}[2J") && written.contains("Permission needed"),
            "the forced repaint is not a whole repaint of the question: {written:?}"
        );
        shell.reconcile_approval();
        assert!(
            shell.approval_ready(crate::tui::approval_readiness::ApprovalId(1)),
            "the verified repaint after a recovered tear still did not grant readiness"
        );
        assert!(
            !failures.pending_recovery && failures.began.is_none() && failures.first.is_none(),
            "the verified repaint did not give the budget back"
        );
    }

    #[test]
    fn a_tear_on_the_owned_plane_after_the_budget_ran_out_is_fatal_with_no_recovery_attempt() {
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = CountingScreen::default();
        on_the_alternate_plane(&mut shell, &mut band, &mut out, &mut failures);
        let t0 = Instant::now();

        // The run's first failure, of a kind and errno no tear shares.
        a_repaint_of_the_owned_plane(
            &mut shell,
            &mut band,
            &mut RefusesFromTheFirstByte,
            &mut failures,
            t0,
        )
        .expect("the first failure of a run never ends the session");
        assert_eq!(
            failures.first.as_ref().map(io::Error::kind),
            Some(io::ErrorKind::PermissionDenied),
            "the fixture this case depends on did not start the budget the way it expects"
        );

        // The same repaint, still owed, torn at the first failure's clock plus
        // the whole budget: spent before this tear arrives, so no cleanup and
        // no rebuild may be offered, and the session ends on the tear's own
        // error rather than on the run's first.
        let mut tear = TornScreen::taking(ALTERNATE_TEAR);
        let err = commit_frame(
            &mut shell,
            &mut band,
            &mut tear,
            &mut failures,
            t0 + FRAME_BUDGET,
            Reconciled,
        )
        .expect_err("a tear past the budget is fatal");
        assert_eq!(
            err.kind(),
            io::ErrorKind::BrokenPipe,
            "the session left with the run's first failure instead of the tear: {err}"
        );
        assert!(
            err.to_string()
                .contains(&format!("accepted {ALTERNATE_TEAR} bytes")),
            "the tear's own accepted count was lost: {err}"
        );
        assert_eq!(
            tear.written.len(),
            ALTERNATE_TEAR,
            "a cleanup or rebuild was offered to an already-spent budget: {:?}",
            String::from_utf8_lossy(&tear.written)
        );
        assert_eq!(reported(&err)["reason"], "partial");
    }

    #[test]
    fn a_second_tear_on_the_owned_plane_before_a_verified_repaint_still_exhausts_the_budget() {
        // A screen that keeps tearing is not a screen that keeps working:
        // every recovered tear is spent on the clock the first one started,
        // so a third one at that clock plus the budget ends the session.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = CountingScreen::default();
        on_the_alternate_plane(&mut shell, &mut band, &mut out, &mut failures);
        let t0 = Instant::now();

        a_repaint_of_the_owned_plane(
            &mut shell,
            &mut band,
            &mut TornScreen::taking(ALTERNATE_TEAR),
            &mut failures,
            t0,
        )
        .expect("the first tear is recovered");
        let began = failures
            .began
            .expect("the tear did not start the budget's clock");

        // The tick the recovery forces, torn too: a second failure, not a
        // verified repaint, so it may not touch the clock.
        let mut second = TornScreen::taking(ALTERNATE_TEAR);
        commit_frame(
            &mut shell,
            &mut band,
            &mut second,
            &mut failures,
            t0,
            Reconciled,
        )
        .expect("the second tear is recovered in the same call, as the first was");
        assert!(
            String::from_utf8_lossy(&second.written)
                .contains(super::super::check::RECOVERY_CLEANUP),
            "the tick behind a recovery wrote nothing to tear, so this case proves nothing"
        );
        assert_eq!(
            failures.began,
            Some(began),
            "a second tear before any verified repaint reset the budget's clock"
        );
        assert!(
            failures.pending_recovery,
            "a recovered second tear must still owe the next tick a verified repaint"
        );

        let err = a_repaint_of_the_owned_plane(
            &mut shell,
            &mut band,
            &mut TornScreen::taking(ALTERNATE_TEAR),
            &mut failures,
            began + FRAME_BUDGET,
        )
        .expect_err("a third tear at the first tear's clock plus the whole budget is fatal");
        assert_eq!(
            err.kind(),
            io::ErrorKind::BrokenPipe,
            "the third tear was not charged against the first tear's own clock: {err}"
        );
    }

    #[test]
    fn a_torn_frame_that_takes_the_plane_is_still_fatal_with_no_recovery_attempt() {
        // The boundary of the recovery above, pinned from the other side: the
        // `1049h` frame is a plane transition, and a terminal that took part
        // of one may be showing either buffer -- no rebuild is right on both.
        // `TornScreen` heals right after the tear, so a cleanup or a rebuild
        // offered here would land and show up in `written`.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = CountingScreen::default();
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            Instant::now(),
            Reconciled,
        )
        .expect("the first frame");
        shell.apply(a_change_too_big_for_the_band());

        let mut torn = TornScreen::taking(ALTERNATE_TEAR);
        let err = commit_frame(
            &mut shell,
            &mut band,
            &mut torn,
            &mut failures,
            Instant::now(),
            Reconciled,
        )
        .expect_err("a torn plane transition ends the session");
        let written = String::from_utf8_lossy(&torn.written).into_owned();
        assert!(
            written.starts_with(ENTERS_ALTERNATE) && torn.written.len() == ALTERNATE_TEAR,
            "the torn vector was not the frame that takes the plane, or something followed it: \
             {written:?}"
        );
        assert!(
            !err.to_string().contains("could not be recovered"),
            "a recovery was attempted on a plane transition: {err}"
        );
        assert_eq!(reported(&err)["reason"], "partial");
    }

    #[test]
    fn a_torn_frame_that_gives_the_plane_back_is_still_fatal_with_no_recovery_attempt() {
        // The other transition: the `1049l` restore, which leaves the terminal
        // on whichever buffer the fragment did -- the same reason, and the
        // same road out.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = CountingScreen::default();
        on_the_alternate_plane(&mut shell, &mut band, &mut out, &mut failures);
        shell.route_bytes(b"3");
        assert_eq!(
            shell.screen_owner(),
            ScreenOwner::Primary,
            "the answer did not give the plane back, so this case proves nothing"
        );

        let mut torn = TornScreen::taking(ALTERNATE_TEAR);
        let err = commit_frame(
            &mut shell,
            &mut band,
            &mut torn,
            &mut failures,
            Instant::now(),
            Reconciled,
        )
        .expect_err("a torn plane transition ends the session");
        let written = String::from_utf8_lossy(&torn.written).into_owned();
        assert!(
            written.starts_with(LEAVES_ALTERNATE) && torn.written.len() == ALTERNATE_TEAR,
            "the torn vector was not the restore, or something followed it: {written:?}"
        );
        assert!(
            !err.to_string().contains("could not be recovered"),
            "a recovery was attempted on a plane transition: {err}"
        );
        assert_eq!(reported(&err)["reason"], "partial");
    }

    #[test]
    fn a_resume_after_a_recovered_alternate_tear_takes_the_plane_again_in_one_write() {
        // What recovery may not disturb: the primary plane's own records. The
        // torn repaint and its cleanup never left the borrowed plane, so the
        // buffer the terminal saved -- and the caret it saved with it -- are
        // exactly what they were, and a resume at this question still takes
        // the plane back in **one** write rather than first painting a band
        // onto a buffer nothing tore.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = CountingScreen::default();
        on_the_alternate_plane(&mut shell, &mut band, &mut out, &mut failures);
        a_repaint_of_the_owned_plane(
            &mut shell,
            &mut band,
            &mut TornScreen::taking(ALTERNATE_TEAR),
            &mut failures,
            Instant::now(),
        )
        .expect("a torn repaint is recovered in one call");

        band.plane_given_back();
        shell.render.request(Reason::ExternalDamage);
        out.written.clear();
        out.calls = 0;
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            Instant::now(),
            Reconciled,
        )
        .expect("the frame the resume asked for");
        let written = String::from_utf8_lossy(&out.written).into_owned();
        assert_eq!(
            out.calls, 1,
            "the plane was not taken back in one write: {written:?}"
        );
        assert!(
            written.starts_with(ENTERS_ALTERNATE) && written.contains("Permission needed"),
            "the resume did not take the plane and paint the question on it: {written:?}"
        );
        assert!(
            band.on_alternate(),
            "the loop did not record the retaken plane"
        );
    }

    #[test]
    fn a_resume_takes_the_approval_plane_again_before_it_paints_anything_on_it() {
        // The one path on which the terminal changes plane without a frame
        // saying so. A `SIGTSTP` at a question runs `signals`'s
        // `stop_for_job_control`, which writes the abnormal restore --
        // `1049l` first (`term::abnormal_restore`) -- and stops the process on
        // the user's own screen. The resume re-announces the mode set
        // (`super::resume`), and that carries **no** `1049h`.
        //
        // So on the first tick after a resume the session still owns the
        // question and the terminal is on the normal buffer. A loop that read
        // only its own record would take the `(true, Approval)` arm and either
        // write nothing at all -- the screen "already holds this" -- leaving the
        // user staring at a shell with no band and no question, or erase and
        // repaint the whole approval screen **onto the user's own buffer**.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = CountingScreen::default();
        on_the_alternate_plane(&mut shell, &mut band, &mut out, &mut failures);

        // What `collect_facts` does on the tick a resume is noticed, minus the
        // real `termios` and the real standard output: the handler gave the
        // plane back, and the screen is the shell's to describe now.
        band.plane_given_back();
        shell.render.request(Reason::ExternalDamage);

        out.written.clear();
        out.calls = 0;
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            Instant::now(),
            Reconciled,
        )
        .expect("the frame the resume asked for");

        let written = String::from_utf8_lossy(&out.written).into_owned();
        assert!(
            written.starts_with(ENTERS_ALTERNATE),
            "the plane was not taken again before anything was painted: {written:?}"
        );
        assert!(
            written.contains("Permission needed"),
            "the question was not painted back onto the plane: {written:?}"
        );
        // And the erase that opens an alternate paint is inside the plane it
        // belongs to, never on the buffer the user was just given back.
        let took = written.find(ENTERS_ALTERNATE).expect("the enter");
        let erased = written.find("\u{1b}[2J").expect("the erase");
        assert!(
            took < erased,
            "the whole screen was erased on the user's own buffer: {written:?}"
        );
        assert!(
            band.on_alternate(),
            "the loop did not record the retaken plane"
        );
    }

    #[test]
    fn a_continue_no_handler_answered_takes_no_plane_back() {
        // `SIGCONT`'s handler sets the resume flag for **any** continue: a
        // `SIGSTOP`, which is uncatchable and runs no handler at all, and an
        // operator's bare `kill -CONT` on a process that was never stopped, as
        // well as the `SIGTSTP` this session handles (`signals.rs`'s own
        // `Held::stopped_before_the_session_began` says so). On those two the
        // terminal is exactly where it was -- still on the approval plane,
        // because no `1049l` was ever written.
        //
        // Re-entering raw mode and re-announcing the mode set for one of them
        // costs a mode set and is idempotent. Taking the plane again is not:
        // `1049h` saves the normal buffer over the save holding the user's real
        // screen, and leaves the session two enters to one leave.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = CountingScreen::default();
        on_the_alternate_plane(&mut shell, &mut band, &mut out, &mut failures);

        // A continue that no handler answered: the resume flag is set and the
        // plane was never given back.
        adopt_resume(&mut shell, &mut band, false);
        assert!(
            band.on_alternate(),
            "a continue nothing restored took the plane away from the band"
        );

        out.written.clear();
        out.calls = 0;
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            Instant::now(),
            Reconciled,
        )
        .expect("the frame after a continue");

        let written = String::from_utf8_lossy(&out.written).into_owned();
        assert!(
            !written.contains(ENTERS_ALTERNATE),
            "a plane that was never given back was taken a second time: {written:?}"
        );
    }

    #[test]
    fn a_continue_a_handler_did_answer_takes_the_plane_back() {
        // The other half, so the gate is a gate rather than an off switch: the
        // `SIGTSTP` this session handles really does write `1049l` on its way
        // down (`signals.rs`'s `stop_for_job_control`), so the plane really is
        // the terminal's again and has to be taken back.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = CountingScreen::default();
        on_the_alternate_plane(&mut shell, &mut band, &mut out, &mut failures);

        adopt_resume(&mut shell, &mut band, true);
        assert!(
            !band.on_alternate(),
            "the band was not told the handler gave the plane back"
        );

        out.written.clear();
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            Instant::now(),
            Reconciled,
        )
        .expect("the frame the resume asked for");
        let written = String::from_utf8_lossy(&out.written).into_owned();
        assert!(
            written.starts_with(ENTERS_ALTERNATE),
            "the plane a handler gave back was not taken again: {written:?}"
        );
    }

    #[test]
    fn the_event_loop_issues_exactly_one_emit_for_the_restore_frame() {
        // The invariant this seam exists for. Two vectors is two presentations
        // on a terminal that does not implement synchronized output: the plane
        // given back, and then -- a scheduler quantum later -- the band.
        //
        // **Vectors, not syscalls.** The seam counts what this loop decides to
        // offer; a kernel that takes one vector in three short writes has not
        // made the loop write twice, and the case that pins *that* difference
        // lives with the counting itself (`super::super::deliver`).
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = CountingScreen::default();
        on_the_alternate_plane(&mut shell, &mut band, &mut out, &mut failures);

        shell.route_bytes(b"3");
        assert_eq!(
            shell.screen_owner(),
            ScreenOwner::Primary,
            "the answer did not give the plane back"
        );
        out.written.clear();
        out.calls = 0;
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            Instant::now(),
            Reconciled,
        )
        .expect("the frame that gives the plane back");

        let written = String::from_utf8_lossy(&out.written).into_owned();
        assert_eq!(
            out.calls, 1,
            "the restore was offered as {} vectors: {written:?}",
            out.calls
        );
        assert!(
            written.starts_with(LEAVES_ALTERNATE),
            "the restore did not lead with the leave: {written:?}"
        );
        assert!(
            written.contains(COMPOSER_MARKER),
            "the restore gave the plane back and painted no band: {written:?}"
        );
        assert!(
            !band.on_alternate(),
            "the loop still believes it is on the plane it gave back"
        );
    }

    #[test]
    fn restoration_never_shows_an_intermediate_blank_grid() {
        // Every snapshot a terminal can take between the leave and the repaint
        // is one this loop never produces: the two are one vector, so there is
        // no moment at which this session has given the plane back and not
        // painted the band. Asserted as the byte fact that makes it true --
        // one emit, and the band inside it, after the leave.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = CountingScreen::default();
        on_the_alternate_plane(&mut shell, &mut band, &mut out, &mut failures);

        shell.route_bytes(b"1");
        out.written.clear();
        out.calls = 0;
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            Instant::now(),
            Reconciled,
        )
        .expect("the frame that gives the plane back");

        let written = String::from_utf8_lossy(&out.written).into_owned();
        assert_eq!(out.calls, 1);
        let leaves = written.find(LEAVES_ALTERNATE).expect("the leave");
        let band_at = written.find(COMPOSER_MARKER).expect("the band");
        assert!(
            leaves < band_at,
            "the band was painted on the plane being left: {written:?}"
        );
    }

    #[test]
    fn normal_exit_while_alternate_is_owned_restores_the_primary() {
        // A session can end with a question still up -- a `Fatal` from the
        // runtime, a supervisor's `SIGTERM` answered by the drain -- and the
        // exit is then the only thing left that can give the plane back. It
        // happens before the drain writes anything, because everything the
        // drain paints belongs on the primary plane.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = CountingScreen::default();
        on_the_alternate_plane(&mut shell, &mut band, &mut out, &mut failures);
        out.written.clear();
        out.calls = 0;

        let broken = shut_down(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            true,
            Shutdown {
                reconcile: |_, _| Ok(Reconciled),
                drain: |taken| {
                    taken(UiEvent::TurnEnded {
                        failure: Some("WHY-THE-TURN-ENDED".to_string()),
                    });
                },
                size: || panic!("an exit with no winch outstanding measured the screen"),
            },
        );

        assert!(broken.is_none(), "the screen refused: {broken:?}");
        let written = String::from_utf8_lossy(&out.written).into_owned();
        let leaves = written.find(LEAVES_ALTERNATE).expect("the leave");
        let drained_at = written
            .find("WHY-THE-TURN-ENDED")
            .expect("the drain's own row");
        assert!(
            leaves < drained_at,
            "the exit painted the drain's rows on the plane it had not given back: {written:?}"
        );
        assert_eq!(
            written.matches(LEAVES_ALTERNATE).count(),
            1,
            "the exit left the alternate screen more than once: {written:?}"
        );
        assert!(
            !band.on_alternate(),
            "the exit left the session on the other plane"
        );
        assert_eq!(
            shell.screen_owner(),
            ScreenOwner::Primary,
            "the exit gave the screen back and did not say so"
        );
    }

    #[test]
    fn an_exit_that_never_took_the_other_plane_writes_no_leave_at_all() {
        // The bound on the rule above. A `1049l` written by every exit would
        // reset an alternate screen the session never entered, which on a
        // terminal that models one swaps in a buffer the user was not looking
        // at -- and that is what `term::RESTORE` deliberately omits.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = CountingScreen::default();
        shell.apply(UiEvent::Delta("ORDINARY-TAIL".to_string()));
        shell.apply(UiEvent::TurnEnded { failure: None });

        let broken = shut_down(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            true,
            Shutdown {
                reconcile: |_, _| Ok(Reconciled),
                drain: |_taken| {},
                size: || panic!("an exit with no winch outstanding measured the screen"),
            },
        );

        assert!(broken.is_none(), "the screen refused: {broken:?}");
        let written = String::from_utf8_lossy(&out.written).into_owned();
        assert!(
            !written.contains(LEAVES_ALTERNATE),
            "the exit reset an alternate screen it never entered: {written:?}"
        );
    }

    #[test]
    fn a_resize_while_the_other_plane_is_owned_repaints_it_at_the_size_it_now_has() {
        // The alternate screen is the whole terminal, so a screen that changed
        // size is a screen every row of it is now at the wrong coordinates on.
        // And the return recomputes the primary: the band is solved from the
        // geometry the resize adopted, not from the one the question arrived
        // under.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = CountingScreen::default();
        on_the_alternate_plane(&mut shell, &mut band, &mut out, &mut failures);

        let now = Instant::now();
        shell.render.mark_resize(now);
        let asked = std::cell::Cell::new(0usize);
        resolve_resize(
            &mut shell,
            &mut band,
            now + RESIZE_DEBOUNCE + Duration::from_millis(1),
            sized((30, 100), &asked),
        );
        assert_eq!(asked.get(), 1, "the resize was never measured");
        out.written.clear();
        out.calls = 0;
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            Instant::now(),
            Reconciled,
        )
        .expect("the repaint on the other plane");

        let written = String::from_utf8_lossy(&out.written).into_owned();
        assert!(
            !written.contains(ENTERS_ALTERNATE),
            "a repaint took a plane it was already on: {written:?}"
        );
        assert!(
            written.contains("\u{1b}[30;1H"),
            "the alternate screen was not repainted to the screen's last row: {written:?}"
        );

        shell.route_bytes(b"3");
        out.written.clear();
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            Instant::now(),
            Reconciled,
        )
        .expect("the frame that gives the plane back");
        let written = String::from_utf8_lossy(&out.written).into_owned();
        assert!(
            written.contains("\u{1b}[30;1H"),
            "the band came back at the coordinates of the screen that was: {written:?}"
        );
    }

    #[test]
    fn nothing_the_primary_plane_owes_is_written_onto_the_other_one() {
        // A document append is a **scroll** of whatever plane it lands on, and
        // what leaves the top of the alternate screen is gone: the terminal
        // gives that buffer back on the way out. So the rows the document is
        // owed stay owed, exactly as they do on a screen no band fits on.
        let mut shell = shell();
        let mut band = Band::new();
        let mut failures = FrameFailures::default();
        let mut out = CountingScreen::default();
        on_the_alternate_plane(&mut shell, &mut band, &mut out, &mut failures);

        shell.apply(UiEvent::Delta("ANSWER-TEXT-WHILE-DECIDING".to_string()));
        shell.flush_paced();
        out.written.clear();
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            Instant::now(),
            Reconciled,
        )
        .expect("a frame on the other plane");
        let written = String::from_utf8_lossy(&out.written).into_owned();
        assert!(
            !written.contains("ANSWER-TEXT-WHILE-DECIDING"),
            "the document was appended to the plane it does not live on: {written:?}"
        );

        // And it is owed rather than dropped: the answer arrives on the primary
        // plane the moment the question gives it back.
        shell.route_bytes(b"3");
        out.written.clear();
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            Instant::now(),
            Reconciled,
        )
        .expect("the frame that gives the plane back");
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            Instant::now(),
            Reconciled,
        )
        .expect("the frame after it");
        let written = String::from_utf8_lossy(&out.written).into_owned();
        assert!(
            written.contains("ANSWER-TEXT-WHILE-DECIDING"),
            "the document rows a question held back were dropped: {written:?}"
        );
    }

    #[test]
    fn a_clear_that_left_synchronized_output_open_is_refused() {
        // `?2026h` with no close leaves a terminal that supports it presenting
        // nothing at all until somebody else closes it -- a frozen screen, from
        // a vector whose whole declared job is to blank one.
        let geometry = super::super::layout::solve(24, 80, 1).expect("a band");
        let opened = "\u{1b}[?2026h\u{1b}[H\u{1b}[2J\u{1b}[3J";
        let refused =
            check_cleared(opened.as_bytes(), &geometry).expect_err("a clear that froze the screen");
        assert!(
            refused.to_string().contains("synchronized"),
            "the refusal did not name the synchronized output: {refused}"
        );
    }

    #[test]
    fn a_clear_that_kept_the_scrollback_or_a_cell_is_refused() {
        // `/clear` is the one emitter in the TUI that erases a terminal's
        // scrollback, and the only one whose declared intent is a blank screen:
        // a vector missing either half is not the thing the shell asked for.
        let geometry = super::super::layout::solve(24, 80, 1).expect("a band");
        check_cleared(super::super::shell::CLEAR_SCREEN.as_bytes(), &geometry)
            .expect("the clear as it stands");

        let kept_scrollback = "\u{1b}[H\u{1b}[2J";
        let refused =
            check_cleared(kept_scrollback.as_bytes(), &geometry).expect_err("a kept scrollback");
        assert!(
            refused.to_string().contains("scrollback"),
            "the refusal did not name the scrollback: {refused}"
        );

        // An erase of one row rather than the screen: the rows a clear promised
        // to blank are still holding whatever was on them.
        let one_row = "\u{1b}[H\u{1b}[K\u{1b}[3J";
        assert!(check_cleared(one_row.as_bytes(), &geometry).is_err());
    }

    // --- P3-WRAP: the shipped painter, measured -----------------------------
    //
    // `.prd/tui-phase2/loop.md:127` re-defers transcript wrap memoization
    // until "a benchmark exceeds 8 ms/frame at 80x24 or 32 ms/frame at
    // 300x200 ... run that benchmark on the shipped painter; below the
    // threshold the memoization is work with no receipt". The existing
    // `check::tests::a_preflight_costs_a_small_fraction_of_one_frames_budget`
    // is not that benchmark: it times `check::preflight` alone, against a
    // vector it builds by hand from a `Grid`, and never calls
    // `Shell::write_transcript`, never reaches `Transcript::prepare_push` (the
    // wrap this row is actually about) and never reaches `commit_band`. What
    // follows drives `commit_frame` -- the function a real session calls --
    // with a `Vec`-backed counting [`Sink`] rather than a real descriptor, so
    // what is measured is this crate's own wrap/append/preflight/diff work and
    // not a `write(2)`'s latency.
    //
    // No line below compares a duration to the threshold. The row says "run
    // that benchmark", not "gate the build on it": a hard ceiling here would
    // fail on whatever machine happens to run it slower (`editor::tests`'
    // `an_edit_and_a_submit_stay_inside_the_ceiling_on_this_machine` takes the
    // same `#[ignore]` stance, for the same reason), and it would quietly
    // become the RED half of a memoization fix nobody has decided to build
    // yet. This is a receipt to read against the PRD row by hand, not a gate.
    //
    // It is also not a claim that every input this session can see is bounded
    // by these numbers: five workloads, two geometries, one text shape apiece.
    // A different shape (many short finished lines, a deeply wrapped CJK
    // line, ...) is not measured here and this benchmark says nothing about
    // it.

    /// A `Vec<u8>` and a call count -- what a fake [`Sink`] needs to be, so
    /// that what a sample's clock includes is this crate's own work and
    /// nothing a real descriptor could stall on.
    struct CountingSink {
        written: Vec<u8>,
        calls: usize,
    }

    impl CountingSink {
        fn new() -> Self {
            Self {
                written: Vec::new(),
                calls: 0,
            }
        }

        /// Back to empty, so a sample's assertions and byte counts are about
        /// *this* sample's own output and nothing an earlier sample left
        /// behind. Called outside every sample's clock, never inside it: a
        /// real `Sink` never accumulates across frames the way an unreset
        /// fake one would, so leaving this call inside the timed window would
        /// measure a `Vec`'s amortized growth instead of the painter.
        fn reset(&mut self) {
            self.written.clear();
            self.calls = 0;
        }
    }

    impl Sink for CountingSink {
        fn emit(&mut self, bytes: &[u8]) -> Result<(), Emit> {
            self.calls += 1;
            self.written.extend_from_slice(bytes);
            Ok(())
        }
    }

    /// Whether `needle` appears anywhere in `haystack`, byte for byte.
    ///
    /// The falsification device every sample below is built around: the
    /// marker is unique to the sample that produced it, so a painter that got
    /// memoized into producing nothing -- or the wrong thing -- fails this
    /// rather than only reading fast.
    fn contains(haystack: &[u8], needle: &[u8]) -> bool {
        !needle.is_empty()
            && haystack
                .windows(needle.len())
                .any(|window| window == needle)
    }

    /// `frame.rs`'s own `ERASE_LINE` (`frame.rs:61`) is private to that
    /// module, so this is a test-only copy of the same three bytes -- the
    /// escape `place()` (`frame.rs:1615-1628`) writes once per document row,
    /// and `grid.rs`'s band-diff painter (`grid.rs:41`) writes the identical
    /// bytes for its own reasons, so counting this in a sample's output
    /// bounds "rows repainted this tick" for the document *and* the band
    /// together, not the document alone.
    const ERASE_LINE: &[u8] = b"\x1b[K";

    /// min/median/p95/max of one workload's samples. There is no acceptance
    /// bound attached to this type on purpose -- see the section comment
    /// above.
    #[derive(Debug)]
    struct Stats {
        min: Duration,
        median: Duration,
        p95: Duration,
        max: Duration,
    }

    /// Sorts `samples` and reads the three named percentiles out of them by
    /// index, in integer arithmetic: nothing here is worth a float.
    fn stats(mut samples: Vec<Duration>) -> Stats {
        samples.sort();
        let last = samples.len() - 1;
        let at = |permille: usize| samples[last * permille / 1000];
        Stats {
            min: samples[0],
            median: at(500),
            p95: at(950),
            max: samples[last],
        }
    }

    /// A handful of unmeasured warmup calls, then the samples the report is
    /// built from -- bounded on both sides, because this is a receipt read
    /// once by a human and not a suite that pays for a thousand iterations on
    /// every `cargo test`.
    const WARMUP: usize = 5;
    const SAMPLES: usize = 30;

    fn measure(mut sample: impl FnMut(usize) -> Duration) -> Stats {
        for i in 0..WARMUP {
            sample(i);
        }
        let mut samples = Vec::with_capacity(SAMPLES);
        for i in 0..SAMPLES {
            samples.push(sample(WARMUP + i));
        }
        stats(samples)
    }

    /// A shell and band solved for `(rows, cols)`, with the one frame every
    /// session's first tick owes already landed and cleared from the sink --
    /// so what a sample measures is the steady-state cost of one more tick,
    /// the shape every tick after the first really has, not the whole-band
    /// repaint every session pays exactly once.
    fn painter_at(rows: u16, cols: u16) -> (Fixture, Band, CountingSink, FrameFailures) {
        let mut shell = shell();
        let mut band = Band::new();
        if (rows, cols) != (shell.geometry.rows, shell.geometry.cols) {
            adopt_resize(&mut shell, &mut band, || (rows, cols));
        }
        let mut out = CountingSink::new();
        let mut failures = FrameFailures::default();
        commit_frame(
            &mut shell,
            &mut band,
            &mut out,
            &mut failures,
            Instant::now(),
            Reconciled,
        )
        .expect("the untimed baseline frame");
        out.reset();
        (shell, band, out, failures)
    }

    /// The control: a tick with nothing owed. Reported for scale beside the
    /// other four -- **not** used, alone or otherwise, to accept or reject the
    /// painter: an idle tick doing anything close to `commit_band`'s real work
    /// only proves this fixture is broken.
    fn run_idle(rows: u16, cols: u16) -> Stats {
        let (mut shell, mut band, mut out, mut failures) = painter_at(rows, cols);
        measure(|_i| {
            out.reset();
            let started = Instant::now();
            shell.settle_input(started);
            shell.settle_band(started);
            commit_frame(
                &mut shell,
                &mut band,
                &mut out,
                &mut failures,
                started,
                Reconciled,
            )
            .expect("an idle tick");
            let elapsed = started.elapsed();
            assert!(
                out.calls == 0 && out.written.is_empty(),
                "an idle tick wrote {} call(s), {} byte(s) -- this fixture owes \
                 something it should not: {:?}",
                out.calls,
                out.written.len(),
                String::from_utf8_lossy(&out.written)
            );
            elapsed
        })
    }

    /// One keystroke into the composer: band assembly with nothing owed on
    /// the document side. Isolates what `commit_band` alone costs from what
    /// the transcript's wrap adds on top of it.
    fn run_band_dirty(rows: u16, cols: u16) -> Stats {
        let (mut shell, mut band, mut out, mut failures) = painter_at(rows, cols);
        measure(|i| {
            // Backspace the previous sample's character and type this one, so
            // the composer's own content stays one character wide for every
            // sample rather than growing across the run.
            let ch = (b'a' + (i % 26) as u8) as char;
            let keys = format!("{}{ch}", '\u{7f}');
            out.reset();
            let started = Instant::now();
            shell.route_bytes(keys.as_bytes());
            shell.settle_input(started);
            shell.settle_band(started);
            commit_frame(
                &mut shell,
                &mut band,
                &mut out,
                &mut failures,
                started,
                Reconciled,
            )
            .expect("a band-dirty tick");
            let elapsed = started.elapsed();
            assert!(
                out.calls > 0 && !out.written.is_empty(),
                "a band-dirty tick produced no bytes at all"
            );
            assert!(
                contains(&out.written, ch.to_string().as_bytes()),
                "the keystroke {ch:?} never reached the painted bytes: {:?}",
                String::from_utf8_lossy(&out.written)
            );
            elapsed
        })
    }

    /// A normal streamed delta: a few dozen characters, the shape a
    /// token-by-token provider stream really arrives in, landing on a tail
    /// nowhere near the retention cap. The line is never ended, so this is
    /// mid-stream, not a finished paragraph.
    fn run_finite_chunk(rows: u16, cols: u16) -> Stats {
        let (mut shell, mut band, mut out, mut failures) = painter_at(rows, cols);
        measure(|i| {
            let marker = format!("chunk-{i:04}-");
            let text = format!("{marker}the quick brown fox jumps over the lazy dog ");
            out.reset();
            let started = Instant::now();
            shell.write_transcript(&text);
            shell.settle_input(started);
            shell.settle_band(started);
            commit_frame(
                &mut shell,
                &mut band,
                &mut out,
                &mut failures,
                started,
                Reconciled,
            )
            .expect("a finite-chunk tick");
            let elapsed = started.elapsed();
            assert!(
                out.calls > 0 && !out.written.is_empty(),
                "a finite-chunk tick produced no bytes at all"
            );
            assert!(
                contains(&out.written, marker.as_bytes()),
                "the chunk {marker:?} never reached the painted bytes: {:?}",
                String::from_utf8_lossy(&out.written)
            );
            elapsed
        })
    }

    /// A finished line: text followed by `end_transcript_line`, the shape a
    /// completed sentence or paragraph lands in. Two queued operations
    /// (`Push` then `EndLine`), drained by the one `commit_document` call
    /// inside the timed `commit_frame`.
    fn run_completed_line(rows: u16, cols: u16) -> Stats {
        let (mut shell, mut band, mut out, mut failures) = painter_at(rows, cols);
        measure(|i| {
            let marker = format!("line-{i:04}-done");
            let text = format!("{marker} a finished sentence lands as one row. ");
            out.reset();
            let started = Instant::now();
            shell.write_transcript(&text);
            shell.end_transcript_line();
            shell.settle_input(started);
            shell.settle_band(started);
            commit_frame(
                &mut shell,
                &mut band,
                &mut out,
                &mut failures,
                started,
                Reconciled,
            )
            .expect("a completed-line tick");
            let elapsed = started.elapsed();
            assert!(
                out.calls > 0 && !out.written.is_empty(),
                "a completed-line tick produced no bytes at all"
            );
            assert!(
                contains(&out.written, marker.as_bytes()),
                "the completed line {marker:?} never reached the painted bytes: {:?}",
                String::from_utf8_lossy(&out.written)
            );
            elapsed
        })
    }

    /// How many rows one seed push holds the streamed line at: safely under
    /// `transcript.rs:136`'s `MAX_TAIL_ROWS` (256, private to that module and
    /// pinned here as a literal for the reason every needle in this crate's
    /// tests is), and safely over either geometry's own document area, so the
    /// push a sample times always has more rows behind it than the screen can
    /// show in one frame.
    const NEAR_CAP_ROWS: usize = 250;

    /// The case the retention cap exists for: an unfinished line whose
    /// currently re-wrappable portion sits just under
    /// [`super::super::transcript`]'s `MAX_TAIL_ROWS` (256).
    ///
    /// **A single seed push does not keep it there.** Read
    /// `Transcript::prepare_push` and the `freeze` it calls before trusting
    /// otherwise: once the pushed line's row count crosses the cap, `freeze`
    /// does not pin the tail at 256 rows -- it drops every row already
    /// settled and keeps only the one still-open row (`kept = last.start` in
    /// `transcript.rs`'s `fn freeze`). A tail seeded past the cap once and
    /// never refilled collapses, after that one push, to a *one-row* re-wrap
    /// for every sample that follows -- the cheapest case this module has,
    /// not the near-cap one the PRD row is about. So every sample below
    /// re-seeds the line, untimed, back up to [`NEAR_CAP_ROWS`] before its
    /// own timed push, and then checks -- outside its own clock -- that the
    /// untimed refill actually landed exactly [`NEAR_CAP_ROWS`] committed
    /// tail rows and the timed push landed exactly one more
    /// (`Shell::tail_rows`, the transcript's own committed answer, not a
    /// repaint proxy): a tail that had silently collapsed back to a handful
    /// of rows would fail those exact counts and be caught here instead of
    /// only reading suspiciously fast. The `\x1b[K`-run count is still
    /// collected and printed per sample, but only as a diagnostic of how
    /// much settled-row reuse skipped -- it no longer carries the
    /// pass/fail.
    fn run_streaming_near_cap(rows: u16, cols: u16) -> Stats {
        let (mut shell, mut band, mut out, mut failures) = painter_at(rows, cols);
        let filler = "x".repeat(usize::from(cols) * NEAR_CAP_ROWS);
        // This geometry's real document area, from the two real inputs this
        // fixture already has -- the row count it was solved for, and
        // `Shell::band_rows`'s own length -- not a guessed constant. A push
        // whose tail genuinely has more rows than this can address at most
        // `area` document rows in one tick (`render_append`'s `shown =
        // min(settled, area)`), which is what makes it the upper bound on
        // `\x1b[K` runs below -- not a target a repaint should saturate at.
        // With settled-row reuse, a tick's actual repaint may write far
        // fewer: any settled row that already matches is omitted entirely.
        let band_rows = shell.band_rows().len();
        let area = usize::from(rows).saturating_sub(band_rows);
        // Upper bound unchanged from before settled-row reuse existed: the
        // document area's share of `ERASE_LINE` plus the band's own, plus a
        // small named slack for the one or two extra rows a push landing
        // exactly at the cap may add before its own `freeze` would fire.
        let upper = area + band_rows + 8;
        // Every sample's own `\x1b[K` count, timed samples only (`i >=
        // WARMUP`), so the distribution printed below matches the same
        // `SAMPLES` the duration stats above it are built from.
        let mut el_samples: Vec<usize> = Vec::with_capacity(SAMPLES);

        let stats = measure(|i| {
            // The untimed refill: end whatever the previous sample's small
            // push left open, then push a fresh near-cap-sized unbroken line,
            // and land both before the clock starts.
            shell.end_transcript_line();
            shell.write_transcript(&filler);
            let refill_now = Instant::now();
            shell.settle_input(refill_now);
            shell.settle_band(refill_now);
            commit_frame(
                &mut shell,
                &mut band,
                &mut out,
                &mut failures,
                refill_now,
                Reconciled,
            )
            .expect("landing the untimed near-cap refill");
            // Before the clock starts: `filler`'s length is an exact
            // multiple of `cols` with no whitespace in it at all, so it
            // hard-wraps into exactly `NEAR_CAP_ROWS` (250) committed rows
            // with none left over -- true for any `cols`, checked here
            // rather than assumed, at both this receipt's geometries (80 and
            // 300). What the assertion below the clock relies on is this one
            // landing exactly at 250, not somewhere near it.
            assert_eq!(
                shell.tail_rows(),
                NEAR_CAP_ROWS,
                "sample {i}: the untimed refill landed {} committed tail rows for a \
                 {cols}x{rows} screen, not the {NEAR_CAP_ROWS} an exact-multiple filler \
                 always wraps to",
                shell.tail_rows()
            );
            out.reset();

            let marker = format!("tail-{i:04}-");
            // The leading space matters: the filler above has no whitespace
            // in it at all, so without one the marker would join the
            // filler's trailing `x`s into a single word (`wrap.rs`: a word
            // moves whole), and that combined word is long enough to hard-
            // wrap mid-marker -- splitting it across two escaped rows and
            // breaking `contains`'s contiguous-byte match on the very first
            // sample.
            let text = format!(" {marker}more unbroken text keeps the retained tail near its cap ");
            let started = Instant::now();
            shell.write_transcript(&text);
            shell.settle_input(started);
            shell.settle_band(started);
            commit_frame(
                &mut shell,
                &mut band,
                &mut out,
                &mut failures,
                started,
                Reconciled,
            )
            .expect("a near-cap streaming tick");
            let elapsed = started.elapsed();
            // Outside the clock: the filler's last word ends exactly on the
            // last column of row 250 (checked above), so `text` -- which
            // opens with a space and is far shorter than either geometry's
            // width -- can only open one fresh row and fit inside it,
            // never zero rows and never two, at 80 or 300 columns alike.
            assert_eq!(
                shell.tail_rows(),
                NEAR_CAP_ROWS + 1,
                "sample {i}: the timed push landed {} committed tail rows for a \
                 {cols}x{rows} screen, not the {} exactly one more than the refill",
                shell.tail_rows(),
                NEAR_CAP_ROWS + 1
            );
            assert!(
                out.calls > 0 && !out.written.is_empty(),
                "a near-cap streaming tick produced no bytes at all"
            );
            assert!(
                contains(&out.written, marker.as_bytes()),
                "the near-cap chunk {marker:?} never reached the painted bytes: {:?}",
                String::from_utf8_lossy(&out.written)
            );
            let erase_line_runs = out
                .written
                .windows(ERASE_LINE.len())
                .filter(|window| *window == ERASE_LINE)
                .count();
            if i >= WARMUP {
                el_samples.push(erase_line_runs);
            }
            // Lower bound is now the loosest one that still says something:
            // at least one row -- the changed marker row itself, which is
            // never eligible for reuse -- is always repainted. The old
            // lower bound (`area` minus a small slack) assumed every
            // settled row is repainted every tick; that assumption is
            // exactly what settled-row reuse (`Grid::row_matches`,
            // `Band::render_append` in `frame.rs`) removes, so a sample
            // whose settled block already matches `target` may now legally
            // emit far fewer than `area` erases. Upper bound is unchanged.
            assert!(
                (1..=upper).contains(&erase_line_runs),
                "sample {i}: {erase_line_runs} `\\x1b[K` runs is outside \
                 [1, {upper}] for a {cols}x{rows} screen with a \
                 {band_rows}-row band ({area}-row document area)"
            );
            elapsed
        });

        // The reuse optimization's measurable effect: how far below the old,
        // now-removed lower bound (`area` minus a small slack) this receipt's
        // own emitted `\x1b[K` counts actually land, printed rather than
        // merely bounded, so a reduced count is something a reader can see
        // and not only something the loosened assertion above lets pass.
        let mut sorted = el_samples.clone();
        sorted.sort_unstable();
        let last = sorted.len().saturating_sub(1);
        let old_lower = area.saturating_sub(6);
        println!(
            "near-cap {cols}x{rows} emitted `\\x1b[K` runs per sample ({} samples, bound \
             [1, {upper}], old lower bound was {old_lower}): min={} median={} max={}",
            sorted.len(),
            sorted.first().copied().unwrap_or_default(),
            sorted.get(last / 2).copied().unwrap_or_default(),
            sorted.get(last).copied().unwrap_or_default(),
        );

        stats
    }

    /// Where the near-cap budget goes, split into the phases production
    /// data can't otherwise tell apart -- added after an independent
    /// controller reproduced [`run_streaming_near_cap`]'s 300x200 threshold
    /// crossing and asked for attribution before any optimization is
    /// considered. **Diagnostic, not a second acceptance gate**: it does not
    /// change what the main receipt above accepts, and its numbers are not
    /// meant to be subtracted against that receipt's own totals as exact
    /// causality -- this is a separate run with its own call overhead, not a
    /// decomposition of the same sample.
    ///
    /// The same [`NEAR_CAP_ROWS`]-row untimed reseed as
    /// [`run_streaming_near_cap`], so the push being split up here is the
    /// same shape that workload times as one number, broken into:
    ///
    /// * **enqueue** -- `Shell::write_transcript` queuing the text, plus
    ///   `settle_input`/`settle_band`, before anything is offered to the
    ///   terminal.
    /// * **prepare** -- the gap between calling `Shell::emit_document_front`
    ///   and this closure's own first instruction running inside it. That gap
    ///   is `Transcript::prepare_front`/`prepare_push`'s clone-and-re-wrap of
    ///   the retained tail -- the cost the PRD row is about -- plus
    ///   `Band::carry_document`, called first for the same reason
    ///   `commit_document` calls it first: this workload never grows or
    ///   shrinks the band, so that call costs nothing measurable here and is
    ///   folded into this bucket rather than broken out on its own.
    /// * **append** -- `Band::append_document` alone, timed from inside that
    ///   same closure: `check::preflight` plus the write to the counting
    ///   [`Sink`].
    /// * **band** -- `commit_band`, timed separately afterwards: composer,
    ///   divider and activity-row assembly, which this text never touches.
    ///
    /// Built beside `commit_document` rather than by calling it, because
    /// `commit_document` owns the one closure that tells prepare and append
    /// apart and there is no hook to time around it from outside. So this
    /// calls `Shell::emit_document_front` directly, with the same closure
    /// body `commit_document` uses -- `Ok(()) => Landed::All`, a `Partial`
    /// error kept as `Landed::Prefix`, anything else `Landed::None` -- which
    /// keeps the real refusal semantics rather than assuming success.
    #[test]
    #[ignore = "P3-WRAP diagnostic: release only, see xfx-painter-cost.log"]
    fn the_near_cap_tick_breaks_down_into_enqueue_prepare_append_and_band() {
        for (rows, cols) in [(24u16, 80u16), (200u16, 300u16)] {
            let (mut shell, mut band, mut out, mut failures) = painter_at(rows, cols);
            let filler = "x".repeat(usize::from(cols) * NEAR_CAP_ROWS);
            let mut enqueue = Vec::with_capacity(SAMPLES);
            let mut prepare = Vec::with_capacity(SAMPLES);
            let mut append = Vec::with_capacity(SAMPLES);
            let mut band_only = Vec::with_capacity(SAMPLES);

            for i in 0..WARMUP + SAMPLES {
                // The untimed reseed: identical in shape to
                // `run_streaming_near_cap`'s, so this closure's push lands
                // against the same near-cap tail that workload measures.
                shell.end_transcript_line();
                shell.write_transcript(&filler);
                let refill_now = Instant::now();
                shell.settle_input(refill_now);
                shell.settle_band(refill_now);
                commit_frame(
                    &mut shell,
                    &mut band,
                    &mut out,
                    &mut failures,
                    refill_now,
                    Reconciled,
                )
                .expect("landing the untimed near-cap refill");
                out.reset();

                let marker = format!("tail-{i:04}-");
                let text =
                    format!(" {marker}more unbroken text keeps the retained tail near its cap ");

                let enqueue_started = Instant::now();
                shell.write_transcript(&text);
                shell.settle_input(enqueue_started);
                shell.settle_band(enqueue_started);
                let enqueue_elapsed = enqueue_started.elapsed();

                band.carry_document(&mut out, &shell.geometry)
                    .expect("no carry is owed: the band never grew or shrank");

                let geometry = shell.geometry;
                let doc_started = Instant::now();
                let mut prepare_elapsed = Duration::ZERO;
                let mut append_elapsed = Duration::ZERO;
                let landed = shell.emit_document_front(|doc_append| {
                    prepare_elapsed = doc_started.elapsed();
                    let append_started = Instant::now();
                    let result = band.append_document(
                        &mut out,
                        doc_append.scroll,
                        &doc_append.rows,
                        &geometry,
                    );
                    append_elapsed = append_started.elapsed();
                    match result {
                        Ok(()) => Landed::All,
                        Err(emit) => {
                            if matches!(emit, Emit::Partial { .. }) {
                                Landed::Prefix(emit)
                            } else {
                                Landed::None(emit)
                            }
                        }
                    }
                });
                assert!(
                    matches!(landed, Some(Ok(()))),
                    "the near-cap push was not landed cleanly: {landed:?}"
                );

                let band_started = Instant::now();
                commit_band(&mut shell, &mut band, &mut out, &mut failures, band_started)
                    .expect("a near-cap band tick");
                let band_elapsed = band_started.elapsed();

                assert!(
                    contains(&out.written, marker.as_bytes()),
                    "the near-cap chunk {marker:?} never reached the painted bytes: {:?}",
                    String::from_utf8_lossy(&out.written)
                );

                if i >= WARMUP {
                    enqueue.push(enqueue_elapsed);
                    prepare.push(prepare_elapsed);
                    append.push(append_elapsed);
                    band_only.push(band_elapsed);
                }
            }

            for (label, samples) in [
                ("enqueue (write_transcript + settle)", enqueue),
                (
                    "prepare (Transcript::prepare_push, before the append callback runs)",
                    prepare,
                ),
                (
                    "append (Band::append_document: preflight + write-to-Vec)",
                    append,
                ),
                ("band (commit_band)", band_only),
            ] {
                let breakdown = stats(samples);
                println!(
                    "painter-cost-breakdown {cols}x{rows} {label}: {SAMPLES} samples \
                     min={:?} median={:?} p95={:?} max={:?} -- diagnostic, not summed \
                     against run_streaming_near_cap's own number (separate run, separate \
                     call overhead)",
                    breakdown.min, breakdown.median, breakdown.p95, breakdown.max
                );
            }
        }
    }

    /// The receipt itself. Ignored by default for the same reason
    /// `editor::tests`' timing case is: a debug build measures the compiler,
    /// not the code, and this is read by a human against
    /// `.prd/tui-phase2/loop.md:127`, not by CI.
    ///
    /// Run it with:
    /// `cargo test --release --lib --exact \
    ///   tui::event_loop::tests::the_shipped_painter_stays_measured_against_the_re_deferred_wrap_thresholds \
    ///   -- --ignored --nocapture`
    #[test]
    #[ignore = "P3-WRAP timing receipt: release only, see xfx-painter-cost.log"]
    fn the_shipped_painter_stays_measured_against_the_re_deferred_wrap_thresholds() {
        /// One workload's label and the function that runs it -- named so
        /// the array below is a list of these rather than a bare
        /// `fn(u16, u16) -> Stats` clippy's `type_complexity` lint reads as
        /// two nested types.
        type Workload = (&'static str, fn(u16, u16) -> Stats);
        let workloads: [Workload; 5] = [
            ("idle (control -- not an acceptance input)", run_idle),
            (
                "band-dirty (composer keystroke, no document work)",
                run_band_dirty,
            ),
            (
                "finite chunk (mid-stream delta, tail well under the cap)",
                run_finite_chunk,
            ),
            (
                "completed line (a finished paragraph lands as a row)",
                run_completed_line,
            ),
            (
                "streaming unfinished tail near its 256-row cap, reseeded to 250 rows every sample",
                run_streaming_near_cap,
            ),
        ];

        for (rows, cols, budget_ms) in [(24u16, 80u16, 8u64), (200u16, 300u16, 32u64)] {
            for (name, run) in workloads {
                let stats = run(rows, cols);
                println!(
                    "painter-cost {cols}x{rows} {name}: {SAMPLES} samples \
                     min={:?} median={:?} p95={:?} max={:?} (PRD budget {budget_ms} ms/frame, \
                     .prd/tui-phase2/loop.md:127)",
                    stats.min, stats.median, stats.p95, stats.max
                );
            }
        }
    }

    // -----------------------------------------------------------------------
    // P3-LAYOUT: one settled Shell/Band transition sequence, replayed idle
    // -----------------------------------------------------------------------
    //
    // Bounded acceptance slice (scratchpad `xfx-layout-acceptance.md`): seed
    // distinct document rows, grow a multirow draft, start a turn, ask a real
    // inline approval question, deny it, end the turn, then shrink the draft
    // back down. After each transition lands, `commit_frame` is replayed once
    // more **at the same instant**, with nothing new queued. This is a
    // no-op-on-a-settled-state check, nothing more: it says an ordinary
    // reconciliation of state that has not changed writes zero bytes and
    // leaves the geometry alone. It is **not** coverage of the `ZeroProgress`
    // write-refusal/retry path (`FlakyScreen`'s cases, elsewhere in this
    // module, own that) -- no write is ever refused here, so nothing here
    // says anything about what happens when one is.
    //
    // `frame.rs`'s own `Screen` decoder (`frame.rs`'s test module) parses only
    // `CUP` column 1 / `EL` / `LF` and no whole-band frame, is private to that
    // module, and is out of this file's ownership for this slice, so it is
    // neither extended nor exposed here. What follows checks the wire
    // directly and only as far as that lets it: each marker is asserted
    // present, in document order, in the one frame that lands it, and then
    // **not re-emitted** by any later frame that only changes the band. That
    // is weaker than preservation -- an `ED`/erase could still remove a row
    // without ever re-emitting its marker bytes, and this file cannot tell
    // the two apart from the wire alone. Proving the marker's *final* content
    // and position (as opposed to "was never written a second time") needs a
    // real screen/scrollback decode and is this acceptance's tracked next
    // step (a PTY scenario), not this one. Literal coordinates are checked
    // only where the shell exposes them directly (`Shell::cursor`,
    // `Geometry::band_top`/`divider`/`input_rows`), not by decoding painted
    // cells.

    /// The shell, band and sink one transition sequence plays out on, all at
    /// one fixed instant -- so no transition below is ever mistaken for
    /// instability caused by the clock rather than by an event.
    struct Rig {
        shell: Fixture,
        band: Band,
        out: CountingScreen,
        failures: FrameFailures,
        now: Instant,
    }

    impl std::ops::Deref for Rig {
        type Target = Fixture;
        fn deref(&self) -> &Fixture {
            &self.shell
        }
    }

    impl std::ops::DerefMut for Rig {
        fn deref_mut(&mut self) -> &mut Fixture {
            &mut self.shell
        }
    }

    impl Rig {
        fn new() -> Self {
            Self {
                shell: shell(),
                band: Band::new(),
                out: CountingScreen::default(),
                failures: FrameFailures::default(),
                now: Instant::now(),
            }
        }

        /// Settles the shell and lands one frame, and hands back what it
        /// wrote -- the shape every transition below lands with.
        fn land(&mut self) -> String {
            self.out.written.clear();
            self.out.calls = 0;
            self.shell.settle_input(self.now);
            self.shell.settle_band(self.now);
            commit_frame(
                &mut self.shell,
                &mut self.band,
                &mut self.out,
                &mut self.failures,
                self.now,
                Reconciled,
            )
            .expect("a settled frame lands");
            String::from_utf8_lossy(&self.out.written).into_owned()
        }

        /// The no-progress check every transition below runs once landed:
        /// `commit_frame` again, at the same instant and with nothing new
        /// queued, must write nothing and leave the geometry unchanged.
        fn assert_idle(&mut self, label: &str) {
            let before = self.shell.geometry;
            self.out.written.clear();
            self.out.calls = 0;
            commit_frame(
                &mut self.shell,
                &mut self.band,
                &mut self.out,
                &mut self.failures,
                self.now,
                Reconciled,
            )
            .expect("an idle replay at the same instant lands");
            assert!(
                self.out.calls == 0 && self.out.written.is_empty(),
                "{label}: an idle replay wrote {} call(s), {} byte(s): {:?}",
                self.out.calls,
                self.out.written.len(),
                String::from_utf8_lossy(&self.out.written)
            );
            assert_eq!(
                self.shell.geometry, before,
                "{label}: an idle replay changed the geometry"
            );
        }
    }

    /// None of `markers` appear a second time in `written` -- the check
    /// every band-only transition below runs against the rows seeded
    /// earlier. **Proves no re-emission, not preservation**: an `ED` this
    /// frame issues could still erase a marked row without ever writing the
    /// marker's bytes again, and this check cannot see that either way. A
    /// VT/PTY decode of the real screen is what a preservation claim needs.
    fn assert_document_not_reemitted(label: &str, written: &str, markers: [&str; 3]) {
        for marker in markers {
            assert!(
                !written.contains(marker),
                "{label}: re-emitted a document row's marker ({marker:?}): {written:?}"
            );
        }
    }

    #[test]
    fn document_and_band_transitions_settle_without_reemitting_document_rows() {
        let mut rig = Rig::new();
        rig.land();
        rig.assert_idle("the first frame");

        // --- seed distinct document rows ------------------------------------
        let markers = ["DOC-ROW-ALPHA", "DOC-ROW-BRAVO", "DOC-ROW-CHARLIE"];
        for marker in markers {
            rig.write_transcript(&format!("{marker} a committed document row"));
            rig.end_transcript_line();
        }
        let written = rig.land();
        let positions: Vec<usize> = markers
            .iter()
            .map(|marker| {
                written.find(marker).unwrap_or_else(|| {
                    panic!("{marker:?} never reached the painted bytes: {written:?}")
                })
            })
            .collect();
        assert!(
            positions.windows(2).all(|pair| pair[0] < pair[1]),
            "the seeded rows did not land in document order: {positions:?} in {written:?}"
        );
        rig.assert_idle("the seeded document rows");

        // --- grow a multirow draft -------------------------------------------
        let draft = "first draft line\nsecond draft line\nthird draft line";
        rig.route_bytes(draft.as_bytes());
        let written = rig.land();
        assert!(!written.is_empty(), "a multirow draft painted nothing");
        assert_document_not_reemitted("the multirow draft", &written, markers);
        let (caret_row, _caret_col) = rig.cursor();
        assert!(
            caret_row >= rig.geometry.band_top(),
            "the draft's caret (row {caret_row}) overlaps the document (band starts at row {})",
            rig.geometry.band_top()
        );
        // Two `\n` (`InsertNewline`, `shell.rs:2706`) over three lines: this
        // is really a *multirow* draft and not just a wide one-row composer.
        assert_eq!(
            rig.geometry.input_rows(),
            3,
            "the two newlines did not grow the composer to three rows"
        );
        rig.assert_idle("the multirow draft");

        // --- TurnStarted: the activity row grows the band ---------------------
        rig.apply(UiEvent::TurnStarted);
        let written = rig.land();
        assert!(!written.is_empty(), "the turn's own row painted nothing");
        assert_document_not_reemitted("the started turn", &written, markers);
        rig.assert_idle("the started turn");

        // --- a real inline approval panel --------------------------------------
        rig.apply(asked_inline());
        // **Not `ScreenOwner::Approval`.** That owner is only for a question
        // too big for the band (`Asked::Alternate`, `shell.rs`'s `ask`); an
        // inline one keeps the primary plane and grows the band around its
        // own panel instead (`Asked::Inline` sets `owner = Primary`).
        assert_eq!(
            rig.screen_owner(),
            ScreenOwner::Primary,
            "an inline question is not the one that takes the plane"
        );
        assert!(
            !rig.band.on_alternate(),
            "a diff-less question took the alternate screen instead of the band's own panel"
        );
        assert!(
            rig.geometry.panel > 0,
            "asking installed no panel: the geometry never grew to hold one"
        );
        let written = rig.land();
        assert!(!written.is_empty(), "the inline panel painted nothing");
        assert_document_not_reemitted("the inline approval panel", &written, markers);
        let (caret_row, _) = rig.cursor();
        assert!(
            (rig.geometry.band_top()..=rig.geometry.divider).contains(&caret_row),
            "the panel's caret (row {caret_row}) is not inside the band ({}..={})",
            rig.geometry.band_top(),
            rig.geometry.divider
        );
        rig.assert_idle("the inline approval panel");

        // --- deny/close it -------------------------------------------------------
        // Digit `3` is `CHOICES[2]` (`approval.rs`'s `keyed`) -- `Deny`,
        // answerable without the readiness gate that guards `Once`/`Always`
        // (`shell.rs`'s `decide`), so this is deterministic with no need to
        // first earn or wait out a disclosure receipt.
        rig.route_bytes(b"3");
        assert_eq!(
            rig.geometry.panel, 0,
            "the panel is closed but the geometry still reserves rows for it"
        );
        let written = rig.land();
        assert!(!written.is_empty(), "closing the panel painted nothing");
        assert_document_not_reemitted("the denied approval panel", &written, markers);
        rig.assert_idle("the denied approval panel");

        // --- TurnEnded -------------------------------------------------------
        rig.apply(UiEvent::TurnEnded { failure: None });
        let written = rig.land();
        assert!(!written.is_empty(), "ending the turn painted nothing");
        assert_document_not_reemitted("the ended turn", &written, markers);
        rig.assert_idle("the ended turn");

        // --- shrink the draft back down ----------------------------------------
        for _ in 0..draft.len() {
            rig.route_bytes(b"\x7f");
        }
        let written = rig.land();
        assert!(!written.is_empty(), "shrinking the draft painted nothing");
        assert_document_not_reemitted("the shrunk draft", &written, markers);
        assert_eq!(
            rig.geometry.input_rows(),
            1,
            "backspacing the whole draft did not shrink the composer back to one row"
        );
        rig.assert_idle("the shrunk draft");
    }

    // -----------------------------------------------------------------------
    // P3-LAYOUT: the retained body is the committed source, and only that
    // -----------------------------------------------------------------------
    //
    // The sibling of the acceptance slice above, for the question it says it
    // cannot answer: that comment pins *no re-emission* and states outright
    // that preservation needs a decode of the real screen. This case makes the
    // narrow version of that decode -- of the bytes the sink **took**, in
    // order, onto an in-memory screen -- and so can say what is on the rows
    // rather than only what was written to them a second time.
    //
    // What it is about is one join the controller already makes and nothing
    // here may loosen: the document's newest row is a function of the source
    // the transcript has **committed**, and a refused append commits nothing
    // (`transcript.rs:505-531` adopts only on `Landed::All`). So an append the
    // terminal refused must leave the retained row reading exactly what the
    // last accepted vector put there -- and the frame that would have followed
    // it must not run at all (`event_loop.rs:959-962`). Nothing here caches a
    // source anywhere: the freshness is read off the screen the accepted bytes
    // make.
    //
    // It is **not** a claim about a real terminal. No pty, no process, no
    // clock: one fixed instant and a modelled screen, which is the same
    // instrument every other case in this module uses and carries the same
    // limit -- what a terminal does with these bytes is `scripts/smoke-tui.sh`
    // and the tracked pty scenario's question, not this one's.
    //
    // And the screen below is narrower than a terminal in three further ways,
    // all of which the fixture stays inside rather than papering over: the
    // text is fixed ASCII, so no width beyond one column per `char` is
    // modelled; the rows arrive **already wrapped** by the transcript, so
    // there is no autowrap at the right margin and none is implemented; and
    // `CR` (and every other control this loop does not write) is unmodelled
    // and refused loudly rather than ignored. A vector that needed any of the
    // three would panic here instead of being scored wrong.

    /// A [`Sink`] scripted like [`ScriptedSink`] that also keeps **what the
    /// terminal took**.
    ///
    /// Neither seam above answers this on its own, and the difference is the
    /// whole point of the case: [`ScriptedSink`] records every vector it was
    /// *offered*, refused ones included, and [`CountingScreen`] keeps bytes it
    /// has no way to refuse. What a screen is holding is neither of those --
    /// it is the concatenation of the vectors that were **accepted**, in the
    /// order they were accepted, and a refused vector is not in it. A case
    /// that read `calls` would score a refused append as if it had landed.
    #[derive(Default)]
    struct TakenScreen {
        answers: std::collections::VecDeque<Result<(), Emit>>,
        /// Every vector offered, taken or not. Read only for *how many* and
        /// what shape: whether a band frame followed a refused append is a
        /// question about what was offered, not about what the screen holds.
        offered: Vec<Vec<u8>>,
        /// The vectors this sink answered `Ok` to, concatenated. The screen.
        taken: Vec<u8>,
    }

    impl TakenScreen {
        /// The bytes accepted since `mark` -- one tick's worth, when `mark` was
        /// taken before it.
        fn taken_since(&self, mark: usize) -> &[u8] {
            &self.taken[mark..]
        }
    }

    impl Sink for TakenScreen {
        fn emit(&mut self, bytes: &[u8]) -> Result<(), Emit> {
            self.offered.push(bytes.to_vec());
            match self.answers.pop_front().unwrap_or(Ok(())) {
                Ok(()) => {
                    self.taken.extend_from_slice(bytes);
                    Ok(())
                }
                Err(emit) => Err(emit),
            }
        }
    }

    /// An accepted byte stream, replayed onto an in-memory screen.
    ///
    /// `frame.rs`'s own `Screen` models four rules for *append* vectors alone,
    /// is private to that module, and `frame.rs` is not this slice's to touch
    /// -- so the loop's whole accepted stream, band frames and appends
    /// together, has no decoder in this file. This is that decoder and no more
    /// than that: exactly the sequences this loop really writes, each one
    /// either modelled or refused loudly, so that a vector which grows a new
    /// shape cannot be silently mis-scored into a passing assertion.
    ///
    /// * `CUP(row, column)` places the cursor, clamped to the screen.
    /// * a linefeed on the bottom row scrolls, and the top row leaves into
    ///   native scrollback; anywhere else it walks the cursor down.
    /// * `ED` (no parameter) erases from the cursor to the end of the screen,
    ///   `EL` (no parameter) to the end of its row.
    /// * `SGR` and **the two private modes `?25` and `?2026`** -- those two by
    ///   name, and no other private mode -- paint no cell and are ignored:
    ///   this models **content**, not attributes.
    /// * printable text overwrites from the cursor rightwards, and a control
    ///   character inside it is refused rather than stored as a cell.
    struct Wire {
        rows: u16,
        lines: Vec<Vec<char>>,
        row: u16,
        column: usize,
        scrolled_off: Vec<String>,
    }

    impl Wire {
        /// The screen a session opens on: blank, with the caret at its origin.
        fn blank(geometry: &crate::tui::layout::Geometry) -> Self {
            Self {
                rows: geometry.rows,
                lines: vec![Vec::new(); usize::from(geometry.rows)],
                row: 1,
                column: 0,
                scrolled_off: Vec::new(),
            }
        }

        fn feed(&mut self, bytes: &[u8]) {
            let mut rest = std::str::from_utf8(bytes).expect("the loop writes text and escapes");
            while !rest.is_empty() {
                if let Some(tail) = rest.strip_prefix('\n') {
                    self.linefeed();
                    rest = tail;
                } else if let Some(tail) = rest.strip_prefix('\u{1b}') {
                    let tail = tail.strip_prefix('[').unwrap_or_else(|| {
                        panic!("an escape this screen does not model: {tail:?}")
                    });
                    let end = tail
                        .find(|glyph: char| !matches!(glyph, '0'..='9' | ';' | '?'))
                        .unwrap_or_else(|| panic!("an unterminated control sequence: {tail:?}"));
                    let (params, tail) = tail.split_at(end);
                    let (ending, tail) = tail.split_at(1);
                    self.control(params, ending);
                    rest = tail;
                } else {
                    let end = rest.find(['\u{1b}', '\n']).unwrap_or(rest.len());
                    let (text, tail) = rest.split_at(end);
                    self.print(text);
                    rest = tail;
                }
            }
        }

        fn control(&mut self, params: &str, ending: &str) {
            match ending {
                "H" => {
                    let mut coordinates = params.split(';');
                    let row = coordinates
                        .next()
                        .and_then(|at| at.parse().ok())
                        .unwrap_or(1);
                    let column: usize = coordinates
                        .next()
                        .and_then(|at| at.parse().ok())
                        .unwrap_or(1);
                    self.row = row.clamp(1, self.rows);
                    self.column = column.max(1) - 1;
                }
                "J" => {
                    assert!(
                        params.is_empty() || params == "0",
                        "an erase this screen does not model: CSI {params}J"
                    );
                    self.erase_below();
                }
                "K" => {
                    assert!(
                        params.is_empty() || params == "0",
                        "an erase this screen does not model: CSI {params}K"
                    );
                    self.erase_to_end_of_row();
                }
                // Attributes paint no cell of their own, and neither does the
                // caret (`?25`) or synchronized output (`?2026`): all three
                // are read by the cases that are about them, and this one is
                // about content.
                //
                // **Exactly those two modes, by name.** A mode is not
                // ignorable for being private: `?1049` hands the terminal a
                // different buffer, and a stream that switched planes and went
                // on writing would have its cells scored onto the primary
                // screen this models
                // (`the_screen_takes_the_two_private_modes_it_models_and_refuses_every_other`).
                "m" => {}
                "h" | "l" => assert!(
                    matches!(params, "?25" | "?2026"),
                    "a mode this screen does not model: CSI {params}{ending}"
                ),
                _ => {
                    panic!("the loop wrote CSI {params}{ending}, which this screen does not model")
                }
            }
        }

        fn print(&mut self, text: &str) {
            // Before a cell is touched: `feed` splits on `LF` and `ESC` and
            // gives everything else to this function, so every *other* control
            // -- `CR`, `TAB`, `BEL`, `NUL` -- would arrive here and be stored
            // as an ordinary glyph. A terminal moves the caret (or rings, or
            // does nothing) for those, so a row holding one is a row this
            // screen has scored wrong rather than one it has refused
            // (`the_screen_refuses_a_control_character_inside_ordinary_text`).
            assert!(
                !text.chars().any(char::is_control),
                "the loop wrote a control this screen does not model, inside \
                 text it would otherwise store as cells: {text:?}"
            );
            let line = &mut self.lines[usize::from(self.row) - 1];
            for (offset, glyph) in text.chars().enumerate() {
                let at = self.column + offset;
                while line.len() <= at {
                    line.push(' ');
                }
                line[at] = glyph;
            }
            self.column += text.chars().count();
        }

        fn erase_to_end_of_row(&mut self) {
            let column = self.column;
            self.lines[usize::from(self.row) - 1].truncate(column);
        }

        fn erase_below(&mut self) {
            self.erase_to_end_of_row();
            for line in &mut self.lines[usize::from(self.row)..] {
                line.clear();
            }
        }

        fn linefeed(&mut self) {
            if self.row < self.rows {
                self.row += 1;
                return;
            }
            let leaving: String = self.lines.remove(0).into_iter().collect();
            self.scrolled_off.push(leaving);
            self.lines.push(Vec::new());
        }

        /// What row `line` -- one-based, as the terminal counts -- says.
        fn row(&self, line: u16) -> String {
            self.lines[usize::from(line) - 1].iter().collect()
        }

        /// How many rows the session owns anywhere -- native scrollback
        /// included -- mention `needle`.
        ///
        /// Scrollback is counted because an append is a scroll: a row written
        /// twice could have had its first copy carried off the top of the
        /// screen, where it is the user's for good and where a check that read
        /// only the visible rows would not see it.
        fn rows_mentioning(&self, needle: &str) -> usize {
            self.scrolled_off
                .iter()
                .filter(|line| line.contains(needle))
                .count()
                + (1..=self.rows)
                    .filter(|line| self.row(*line).contains(needle))
                    .count()
        }
    }

    /// The head of a band frame, and therefore the needle that says one
    /// followed an append in the same accepted stream.
    ///
    /// Pinned as a literal here for the reason every needle in this module is:
    /// a case that imported the constant it checks would pass for whatever the
    /// emitter happened to declare.
    const OPENS_A_FRAME: &str = "\u{1b}[?2026h";

    #[test]
    fn the_screen_refuses_a_control_character_inside_ordinary_text() {
        // The clause above says every control this loop does not write is
        // refused rather than ignored, and that has to be true of the *text*
        // path too, not only of the parser's: `feed` splits on `LF` and `ESC`
        // and hands everything else to `print`, so a `CR`, a `TAB`, a `BEL` or
        // a `NUL` would otherwise be stored as an ordinary cell -- a row that
        // reads `"a\rb"` where the terminal would read `"b"`, which is a wrong
        // score rather than a loud one.
        let geometry = crate::tui::layout::solve(24, 80, 1).expect("a band");
        for text in ["a\rb", "a\tb", "a\u{7}b", "a\0b"] {
            let mut wire = Wire::blank(&geometry);
            let refused = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                wire.feed(text.as_bytes());
            }));
            assert!(
                refused.is_err(),
                "{text:?} was stored as ordinary cells by a screen that does \
                 not model its control"
            );
        }
        // And the ordinary ASCII this loop really writes still lands.
        let mut wire = Wire::blank(&geometry);
        wire.feed(b"ab");
        assert_eq!(wire.row(1), "ab", "plain text stopped landing");
    }

    #[test]
    fn the_screen_takes_the_two_private_modes_it_models_and_refuses_every_other() {
        // **`?1049` is why this is an exact list rather than "anything
        // private".** It hands the terminal a *different buffer*, so a stream
        // that entered the alternate plane and went on writing would have
        // those cells scored onto the primary screen [`Wire`] models -- and
        // that is silence in the one direction this file's cases read as
        // preservation: a row nobody touched and a row touched on a buffer
        // nobody can see look identical. `?25` and `?2026` move no cell and no
        // plane, which is the whole reason they are ignorable at all.
        let geometry = crate::tui::layout::solve(24, 80, 1).expect("a band");
        for vector in [
            "\u{1b}[?25l",
            "\u{1b}[?25h",
            "\u{1b}[?2026h",
            "\u{1b}[?2026l",
        ] {
            let mut wire = Wire::blank(&geometry);
            wire.feed(vector.as_bytes());
            assert_eq!(wire.row(1), "", "{vector:?} painted a cell");
        }
        for vector in ["\u{1b}[?1049h", "\u{1b}[?1049l", "\u{1b}[?7h", "\u{1b}[4h"] {
            let mut wire = Wire::blank(&geometry);
            let refused = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                wire.feed(vector.as_bytes());
            }));
            assert!(
                refused.is_err(),
                "{vector:?} was taken quietly by a screen that does not model it"
            );
        }
    }

    /// One refusal, made fresh for the run that uses it.
    ///
    /// A function rather than a value because an [`Emit`] carries an
    /// `io::Error`, which is not `Clone`: the same refusal cannot be handed to
    /// two runs of the sequence below, and a list of them would be consumed by
    /// the first.
    type Refusal = fn() -> Emit;

    #[test]
    fn retained_primary_body_tracks_only_successfully_committed_document_source() {
        // Parameterised over the two refusals that provably moved **no bytes**
        // -- the checker's own (`Rejected`) and the descriptor's
        // (`ZeroProgress`). They reach `commit_document`'s batch loop by
        // different routes and are required to leave the same screen behind,
        // which is a statement neither one of them makes alone. `Partial` is
        // deliberately not in this list: it put a prefix on the terminal, so
        // the retained row is *not* what the last accepted vector wrote, and
        // its own case above owns that.
        let refusals: [(&str, Refusal); 2] = [
            ("a rejected append", || {
                Emit::Rejected(io::Error::other("the output check refused this vector"))
            }),
            ("a zero-progress append", || {
                Emit::ZeroProgress(io::Error::new(io::ErrorKind::BrokenPipe, "not now"))
            }),
        ];

        for (label, refusal) in refusals {
            let mut shell = shell();
            let mut band = Band::new();
            let mut failures = FrameFailures::default();
            // One instant for the whole sequence, as the acceptance rig above
            // takes one: nothing below may be mistaken for the clock moving,
            // and the retry has to be inside the frame budget rather than
            // outside it.
            let now = Instant::now();
            let mut out = TakenScreen::default();
            let mut land = |shell: &mut Fixture, band: &mut Band, out: &mut TakenScreen| {
                shell.settle_input(now);
                shell.settle_band(now);
                commit_frame(shell, band, out, &mut failures, now, Reconciled)
                    .unwrap_or_else(|err| panic!("{label}: a frame ended the session: {err}"))
            };

            // --- the band's own first frame, and the screen it leaves --------
            land(&mut shell, &mut band, &mut out);
            // Fixed for the whole case: nothing below resizes, so every row
            // number read out of it means the same thing at every step.
            let geometry = shell.geometry;
            let mut wire = Wire::blank(&geometry);
            wire.feed(&out.taken);
            let mut mark = out.taken.len();

            // --- land an unfinished answer -----------------------------------
            // No `end_transcript_line`: the line stays open, which is what
            // makes the continuation below a second delta of the same row
            // rather than a new one.
            shell.write_transcript("SOURCE-A");
            land(&mut shell, &mut band, &mut out);
            wire.feed(out.taken_since(mark));
            mark = out.taken.len();
            assert_eq!(
                wire.row(geometry.content_bottom),
                "SOURCE-A",
                "{label}: the unfinished answer is not on the document's last row",
            );

            // --- queue the continuation and an edit the band has to repaint ---
            shell.write_transcript("-B");
            shell.route_bytes(b"z");
            out.answers.push_back(Err(refusal()));
            let offered = out.offered.len();
            land(&mut shell, &mut band, &mut out);

            // Nothing reached the screen, so nothing about it may have moved.
            assert_eq!(
                out.taken.len(),
                mark,
                "{label}: a refused append still put bytes on the screen: {:?}",
                String::from_utf8_lossy(out.taken_since(mark)),
            );
            let refused: Vec<String> = out.offered[offered..]
                .iter()
                .map(|vector| String::from_utf8_lossy(vector).into_owned())
                .collect();
            assert_eq!(
                refused.len(),
                1,
                "{label}: the tick did not stop at the refused append: {refused:?}",
            );
            assert!(
                refused[0].contains("SOURCE-A-B") && !refused[0].contains(OPENS_A_FRAME),
                "{label}: the one refused vector was not the append: {refused:?}",
            );
            // The continuation of an open line rewrites the row it is already
            // on: no linefeed, so no scroll, so the row number below is the
            // same one the first half landed on.
            assert!(
                !refused[0].contains('\n'),
                "{label}: the continuation scrolled the screen instead of \
                 rewriting the row it was already on: {refused:?}",
            );
            wire.feed(out.taken_since(mark));
            assert_eq!(
                wire.row(geometry.content_bottom),
                "SOURCE-A",
                "{label}: a refused append changed the retained row",
            );

            // --- the retry, inside the budget --------------------------------
            land(&mut shell, &mut band, &mut out);
            let retried = String::from_utf8_lossy(out.taken_since(mark)).into_owned();
            wire.feed(out.taken_since(mark));
            mark = out.taken.len();
            let landed_at = retried
                .find("SOURCE-A-B")
                .unwrap_or_else(|| panic!("{label}: the refused append was never retried"));
            let frame_at = retried
                .find(OPENS_A_FRAME)
                .unwrap_or_else(|| panic!("{label}: the scroll's own frame never followed"));
            assert!(
                landed_at < frame_at,
                "{label}: the band was painted before the append it follows: {retried:?}",
            );
            assert_eq!(
                wire.row(geometry.content_bottom),
                "SOURCE-A-B",
                "{label}: the retained row is not what the accepted append wrote",
            );
            assert_eq!(
                wire.rows_mentioning("SOURCE"),
                1,
                "{label}: the answer is on more than one row of the session",
            );

            // --- a frame that changes only the band ---------------------------
            shell.route_bytes(b"y");
            land(&mut shell, &mut band, &mut out);
            let footer = String::from_utf8_lossy(out.taken_since(mark)).into_owned();
            wire.feed(out.taken_since(mark));
            mark = out.taken.len();
            assert!(
                !footer.is_empty() && !footer.contains("SOURCE"),
                "{label}: a band-only frame rewrote the document: {footer:?}",
            );
            assert_eq!(
                wire.row(geometry.content_bottom),
                "SOURCE-A-B",
                "{label}: a band-only frame did not leave the document row alone",
            );

            // --- an idle replay of the very same state -------------------------
            let offered = out.offered.len();
            land(&mut shell, &mut band, &mut out);
            assert_eq!(
                (out.offered.len() - offered, out.taken.len() - mark),
                (0, 0),
                "{label}: an idle replay at the same instant wrote to the screen",
            );
        }
    }
}
