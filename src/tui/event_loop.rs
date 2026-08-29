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
use super::shell::{Resize, Shell};
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
        &mut io::stdout().lock(),
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
    out: &mut impl Write,
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
    out: &mut impl Write,
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
    out: &mut impl Write,
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
        adopt_resume(band, signals::take_plane_restored());
        shell.render.request(Reason::ExternalDamage);
    }

    // Taken rather than left set, so that the *launch* measurement -- which
    // acts on the same flag -- cannot be answered by a resize this loop already
    // saw. The band is not re-solved here: a drag is a burst of these, and what
    // it starts is a deadline ([`super::render_request::RESIZE_DEBOUNCE`]).
    if signals::take_winch() {
        shell.render.mark_resize(now);
    }
    resolve_resize(shell, band, now, super::term::reported_window_size);

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
fn adopt_resume(band: &mut Band, plane_restored: bool) {
    if plane_restored {
        band.plane_given_back();
    }
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
    out: &mut impl Write,
    failures: &mut FrameFailures,
    now: Instant,
) -> io::Result<bool> {
    let Some(attempt) = shell.render.begin() else {
        return Ok(true);
    };
    if attempt.damaged() {
        // Something that is not this band wrote on the screen -- a resume that
        // handed the terminal to the shell and took it back, a `/clear` that
        // erased it, a Ctrl-L asking for exactly this. The shadow is a claim
        // about what is on those rows and that claim is now false about all of
        // them, so the frame below is a whole one rather than a difference from
        // a screen that no longer exists.
        band.invalidate(shell.geometry.rows, shell.geometry.cols);
    }
    match band.commit(out, &shell.band_rows(), &shell.geometry, shell.cursor()) {
        // A frame that wrote nothing is a frame the screen already had, so the
        // budget is whole for the same reason a delivered one leaves it whole.
        Ok(_) => {
            failures.succeeded();
            Ok(true)
        }
        Err(err) => {
            shell.render.restore(attempt);
            match failures.failed(err, now) {
                Some(fatal) => Err(fatal),
                None => Ok(false),
            }
        }
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
/// would be the one that forgot the carry, or the order, or that a refused
/// append is not owed again.
fn commit_document(
    shell: &mut Shell,
    band: &mut Band,
    out: &mut impl Write,
    failures: &mut FrameFailures,
    now: Instant,
) -> io::Result<bool> {
    if let Err(err) = band.carry_document(out, &shell.geometry) {
        return match failures.failed(err, now) {
            Some(fatal) => Err(fatal),
            None => Ok(false),
        };
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
    //
    // **The ones behind it are owed again, and only that one is not.** The take
    // above drains everything the session owed, and a refusal ends the tick --
    // so what is left in the batch is rows that were never offered to the
    // terminal and cannot have moved it. The reason above does not reach them:
    // they are not a write that may have half-happened, they are a write that
    // did not happen. Dropped here they would be gone for good, because Phase 1
    // never repaints a document row.
    let mut owed = shell.take_pending();
    let mut index = 0;
    while index < owed.len() {
        let append = &owed[index];
        if let Err(err) = band.append_document(out, append.scroll, &append.rows, &shell.geometry) {
            // Everything after the refused one, oldest first, back in front of
            // whatever has been owed since.
            let untried = owed.split_off(index + 1);
            shell.restore_pending(untried);
            return match failures.failed(err, now) {
                Some(fatal) => Err(fatal),
                None => Ok(false),
            };
        }
        index += 1;
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
    out: &mut impl Write,
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
            let frame = band.restore_primary(&shell.band_rows(), &shell.geometry, cursor);
            match out.write_all(frame.bytes()).and_then(|()| out.flush()) {
                Ok(()) => {
                    band.frame_landed(&frame, &shell.geometry, cursor);
                    debug_assert!(!band.on_alternate());
                    // The band on the primary plane is now exactly what this
                    // frame painted, so whatever asked for a frame has had one.
                    let _ = shell.render.begin();
                    failures.succeeded();
                    Ok(())
                }
                Err(err) => match failures.failed(err, now) {
                    Some(fatal) => Err(fatal),
                    None => Ok(()),
                },
            }
        }
        // A change the band cannot show: take the plane and paint the whole
        // surface onto it in the same frame.
        (false, ScreenOwner::Approval) => {
            let frame =
                band.enter_alternate(&shell.screen_rows(), &shell.geometry, shell.screen_cursor());
            match out.write_all(frame.bytes()).and_then(|()| out.flush()) {
                Ok(()) => {
                    band.frame_landed(&frame, &shell.geometry, shell.screen_cursor());
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
                Err(err) => match failures.failed(err, now) {
                    Some(fatal) => Err(fatal),
                    None => Ok(()),
                },
            }
        }
        // Already there: a marker that moved, or a screen that changed size.
        (true, ScreenOwner::Approval) => {
            let Some(attempt) = shell.render.begin() else {
                return Ok(());
            };
            let frame = band.repaint_alternate(
                &shell.screen_rows(),
                &shell.geometry,
                shell.screen_cursor(),
            );
            // The screen already holds this, which is the commonest frame while
            // a person is reading a change: the band's animation asks for one
            // twice a second and nothing on this plane has moved.
            if frame.bytes().is_empty() {
                failures.succeeded();
                return Ok(());
            }
            match out.write_all(frame.bytes()).and_then(|()| out.flush()) {
                Ok(()) => {
                    band.frame_landed(&frame, &shell.geometry, shell.screen_cursor());
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
        // Not this function's turn; `commit_frame` guards against reaching here.
        (false, ScreenOwner::Primary) => Ok(()),
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

    use tokio::sync::mpsc;

    // The debounce the loop's own cases advance a clock past. Imported rather
    // than pinned because these cases are not the ones that claim what it is --
    // `render_request::tests::the_resize_debounce_is_fifty_milliseconds` is --
    // and a second literal here would have to be kept correct beside it.
    use super::super::render_request::RESIZE_DEBOUNCE;

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
                crate::tui::theme::Palette {
                    mode: crate::tui::theme::Mode::Dark,
                    depth: crate::tui::theme::Depth::Ansi256,
                },
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
    fn what_a_refused_append_was_ahead_of_is_still_owed() {
        // The other half of the sibling above, and the one a `take` of the
        // whole batch makes possible to lose. A refusal ends the tick with rows
        // still in hand that the terminal was never offered -- they moved no
        // bytes, so nothing about them may have half-happened, and the reason
        // the refused one is not repeated does not reach them. Phase 1 never
        // repaints a document row, so dropping them is not a late frame; it is
        // an answer with a hole in it.
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
        assert!(
            text.contains("behind it"),
            "an append the terminal was never offered was dropped with the one \
             it was behind: {text:?}"
        );
        assert!(
            !text.contains("refused"),
            "the refused append was replayed onto a screen that may already \
             have taken it: {text:?}"
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

    /// A screen that counts the calls, not the bytes.
    ///
    /// The one-write invariant is about **write calls**: a sampled snapshot of a
    /// pty that happened to look atomic satisfies nothing, because a terminal
    /// presents whatever it has whenever it is scheduled to. So the seam counts
    /// `write_all`, which is what the loop is required to call exactly once for
    /// the restore, and the default implementation -- which loops on `write` --
    /// is overridden so that one call is one count however the vector is
    /// delivered.
    #[derive(Default)]
    struct CountingScreen {
        calls: usize,
        written: Vec<u8>,
    }

    impl Write for CountingScreen {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.calls += 1;
            self.written.extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
            self.calls += 1;
            self.written.extend_from_slice(bytes);
            Ok(())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// A question about a change no band summary could show.
    fn a_change_too_big_for_the_band() -> UiEvent {
        UiEvent::Approval(crate::permission::ApprovalRequest {
            tool: "edit_file",
            target: "notes.txt".to_string(),
            summary: "edit `notes.txt`: replace \"alpha\" with \"beta\"".to_string(),
            always_scope: "allow every future edit_file to `notes.txt`".to_string(),
            diff: Some(crate::permission::ApprovalDiff {
                before: "a".repeat(4_000),
                after: "b".repeat(4_000),
            }),
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
        // **Two** rows, because the two halves of the refusal rule are
        // different: the one the terminal was offered may have moved the screen
        // already and is not offered again (`commit_document`), and the one
        // behind it moved nothing and is still owed.
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

    impl Write for DeafToTheBand {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if !self.refused && String::from_utf8_lossy(bytes).contains(self.needle) {
                self.refused = true;
                return Err(io::Error::new(io::ErrorKind::BrokenPipe, "not now"));
            }
            self.written.extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
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
        adopt_resume(&mut band, false);
        shell.render.request(Reason::ExternalDamage);
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

        adopt_resume(&mut band, true);
        shell.render.request(Reason::ExternalDamage);
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
    fn the_event_loop_issues_exactly_one_write_all_for_the_restore_frame() {
        // The invariant this seam exists for. Two writes is two presentations
        // on a terminal that does not implement synchronized output: the plane
        // given back, and then -- a scheduler quantum later -- the band.
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
            "the restore was written in {} calls: {written:?}",
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
        // is one this loop never produces: the two are one `write_all`, so
        // there is no moment at which the plane has been given back and the
        // band has not been painted. Asserted as the byte fact that makes it
        // true -- one call, and the band inside it, after the leave.
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
}
