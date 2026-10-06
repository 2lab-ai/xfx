//! The runtime thread: one turn at a time, and the protocol for ending it.
//!
//! This is where the tokio runtime lives now. `main` builds none on the TUI
//! path (`src/main.rs:22-25`), because the UI thread has to be free to sit in
//! `pselect(2)` holding the terminal; the runtime is therefore built *here*, on
//! a thread of its own, and a submitted prompt runs through exactly the call
//! `xfx ask` runs -- [`crate::agent::run_turn_saved`], over a bundle from
//! [`crate::provider::Bundle::select`], against a session from
//! [`crate::session::SessionStore`]. Nothing about a turn is re-implemented for
//! the TUI. What is different is only where its events go: into
//! [`super::bridge`]'s channels instead of onto a file descriptor.
//!
//! Three properties are the whole of this module, and each is a rule the rest
//! of the plan depends on.
//!
//! **The runtime thread never touches the terminal.** Not to print, not to
//! read, not to restore -- not even when it panics. A panic inside a turn is
//! caught at the turn boundary and becomes a [`UiEvent::Fatal`], which the UI
//! thread paints, restores behind, and exits nonzero on. The panic hook's
//! ownership test (`super::panic`) is what enforces the other half of that: a
//! hook that restored for any thread would cook the terminal while the thread
//! actually holding it still believed it was raw.
//!
//! **The runtime thread takes no signal from the UI thread.** It is spawned
//! while [`super::signals::block_owned`]'s mask is still whole -- before
//! `install` lifts the death signals -- so it inherits a mask with `SIGINT`,
//! `SIGTERM`, `SIGHUP` and `SIGTSTP` blocked and can never be the thread the
//! kernel picks to deliver one to. That was Task 3's standing constraint on
//! whichever task added the process's second thread, and this is the task.
//!
//! **Exactly one terminal event per turn.** `agent::machine` emits its
//! conclusion *and* returns it (`machine.rs:334-345`); the sink drops the
//! emitted half (`bridge::translate`), and the returned half becomes the single
//! [`UiEvent::TurnEnded`] sent here, after the session log has been published.
//! Publication before the terminal event is the ordering the UI's drain
//! depends on: the drain stops at the first terminal event, so anything the
//! worker still owed after one would never be waited for.
//!
//! # The permission session is built here, and not by `app::permission_session`
//!
//! That helper attaches `TtyPrompter`, which reads a line from standard input
//! -- the descriptor the UI thread owns and is polling. Two readers on one
//! terminal is precisely the bug this topology exists to prevent. So the
//! worker builds its own [`PermissionSession`] and attaches
//! [`super::approval::TuiPrompter`], which asks by sending the UI a
//! [`UiEvent::Approval`] and hears the answer back on the `TurnControl`
//! channel. Nothing on this thread reads a descriptor, and `ask` -- the
//! default-safe mode -- runs under the TUI rather than denying every mutation
//! for want of somewhere to ask.
//!
//! The prompter is built **once, with the session**, and not per turn: an
//! "always" answer is recorded in the `PermissionSession` the conversation
//! holds, so a session rebuilt per turn would make "for the rest of this
//! session" mean "for the rest of this turn". `/new` drops the conversation and
//! with it the grants, which is what that command means on the other front end
//! too.
//!
//! # The control channel has two readers, and only one at a time
//!
//! `ApprovalPrompter::request` is synchronous, so it parks this thread while it
//! waits -- which means [`raced_against_control`] is not running while it does,
//! and the prompter has to read the control channel itself. That sharing, the
//! waker it costs and the rule that makes it sound all live in
//! [`super::approval::ControlChannel`], which is the only thing on either side
//! that touches the receiver.

use std::future::Future;
use std::io;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use futures_util::FutureExt;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::mpsc::{self, Receiver, Sender, UnboundedReceiver, UnboundedSender};

use super::approval::{ControlChannel, TuiPrompter};
use super::bridge::{
    self, Cancellation, ModelAnswer, ModelChange, TurnCancel, TurnControl, TurnWork, UiEvent,
    UiEventSink,
};
use super::permissions::{self, Change, Grants};
use super::question::TuiQuestioner;
use crate::agent::{run_turn_saved, TurnRequest};
use crate::clipboard::last_reply;
use crate::config::PermissionMode;
use crate::config::{Environment, RuntimeConfig};
use crate::gateway::{CancelToken, DEFAULT_MAX_ATTEMPTS};
use crate::interactive::{
    bind_conversation, open_conversation, rename_conversation, status_snapshot, Conversation,
};
use crate::output::StatusSnapshot;
use crate::permission::PermissionSession;
use crate::provider::model::{
    CatalogEntry, CatalogState, ModelOutcome, ModelRequest, ModelSelector, MAX_RENDERED_MODELS,
};
use crate::provider::setup::{setup_transaction, SetupProblem};
use crate::provider::{Bundle, ProviderId};
use crate::session::{SessionEvent, SessionId, SessionStore, WritableSession};
use crate::workspace::{AccessScope, ProjectContext};

/// How long the UI keeps painting what the worker has already produced before
/// it stops waiting for the rest.
///
/// Long enough for a turn that was cancelled mid-stream to unwind, publish its
/// session log, and say so; short enough that a runtime wedged on something
/// nobody can name does not hold a terminal the user asked for back.
pub(crate) const DRAIN_DEADLINE: Duration = Duration::from_secs(2);

/// How long the join is given once the drain is over.
///
/// The thread has been cancelled, told to shut down, and had its channel closed
/// under it by then, so this is the cost of a thread finishing rather than of
/// anything it might still do. Past it the handle is dropped and the thread is
/// left detached -- which is safe for exactly one reason, and it is the reason
/// this module exists: it cannot write to the terminal.
const JOIN_GRACE: Duration = Duration::from_millis(250);

/// How long the drain sleeps between looks at a channel that has nothing yet.
const DRAIN_POLL: Duration = Duration::from_millis(2);

/// What the runtime thread is called in a debugger and a crash report.
const THREAD_NAME: &str = "xfx-turn";

/// What the UI is told when there is no runtime to run a turn on.
///
/// A `&'static str` because of where it is sent from: see
/// [`fatal_before_the_runtime`].
const NO_RUNTIME: &str = "xfx cannot start the async runtime, so no turn can run";

/// What a panic that carried something other than text is reported as.
const UNKNOWN_PANIC: &str = "a turn panicked with a payload that is not text";

/// Why a submission was not accepted.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Rejected {
    /// The runtime already holds everything it will hold: one piece of work in
    /// flight, and one queued behind it waiting to be picked up ([`WORK_LIMIT`]).
    ///
    /// "Busy" is about *both* of those together and not about a turn alone --
    /// a session with one prompt running and none waiting takes the next one.
    Busy,
    /// The runtime thread is gone. A [`UiEvent::Fatal`] is already on its way,
    /// or has already arrived; this is not a refusal the user can act on, and
    /// it is told apart from [`Self::Busy`] so that the UI never says "a turn
    /// is already running" about a runtime that is not running anything.
    Gone,
}

/// How much work the session holds at once: the piece in flight, and one more
/// waiting behind it.
///
/// Two rather than one, because "one turn at a time" is a statement about what
/// *runs* and not about what may be typed. A user who knows what they want next
/// can say it while the answer to the last one is still arriving, and the band
/// says `queued 1` for the whole time it waits -- which is the difference
/// between a queue and the "queued into a surprise" a hidden one would be.
///
/// Two rather than more, because the thing that makes the queue safe is that its
/// depth fits in a sentence on one row. A third submission is refused where the
/// user can read it, with the draft left in the composer.
///
/// **This is also the work channel's capacity, and the two must not differ.** A
/// channel one slot shallower than the count would make the queue's real depth
/// depend on *when the runtime happened to pick an item up*: two submissions
/// before the first pickup would meet a full channel and the second would be
/// refused, while the same two a millisecond later would both be taken. A
/// refusal that a scheduler decides is not a contract, and it is not one a user
/// can learn.
pub(crate) const WORK_LIMIT: usize = 2;

/// The two channels the UI writes to, without the thread it writes them for.
///
/// Cloneable and cheap, because the shell needs to submit and the loop needs to
/// shut down, and those are different owners with different lifetimes: the
/// [`Worker`] is the thread's handle and lives in the event loop, while this is
/// what the shell holds to answer a `Return`.
#[derive(Clone)]
pub(crate) struct WorkHandle {
    work: Sender<TurnWork>,
    control: UnboundedSender<TurnControl>,
    /// How much work the runtime has in hand: what it is running, plus what is
    /// waiting to be picked up.
    ///
    /// **The channel cannot answer this**, and that is the whole reason the
    /// count exists. A `mpsc` permit is freed the moment the receiver *takes*
    /// the item, which is the beginning of a turn and not its end -- so a
    /// one-slot channel is empty for the entire minute a turn is running, and a
    /// session that asked the channel would keep saying yes and run each
    /// answer as a surprise when the last one finished. The count is claimed by
    /// the sender and released by the loop when the item is **done**, so the
    /// window the channel leaves open is closed from both ends, and the number
    /// the band shows is the same number the refusal is decided from.
    outstanding: Arc<AtomicUsize>,
    /// How many submissions this session has ever accepted.
    ///
    /// Monotonic, and never given back -- it is an *index*, not a depth. It is
    /// what a cancellation quotes so that the runtime can tell the work the
    /// user was interrupting from the work they typed after deciding to
    /// interrupt it ([`TurnControl::Cancel`]).
    accepted: Arc<AtomicUsize>,
    /// What `/copy` copies: the conversation's last reply, as the runtime
    /// last published it ([`LastReply`]).
    reply: LastReply,
    /// What `/status` reads, as the runtime last published it.
    status: StatusBoard,
    /// Where a change to the session's authority goes: `/permissions` and
    /// Shift-Tab ([`Change`]).
    ///
    /// **A channel of its own, not a [`TurnWork`]**, and each of the three
    /// differences is a bug the work channel would have: a change would hold a
    /// place in [`WORK_LIMIT`] and be refused as busy behind one waiting
    /// prompt, it would show on the band as `queued 1`, and a Ctrl-C would drop
    /// it with the queue ([`Queue::abandon`]) after the footer had already said
    /// the new mode. Unbounded, like the control channel, because a keystroke
    /// that changes the mode must never be refused for want of room. The
    /// runtime reads it only between pieces of work (`turn_loop`), which is
    /// what keeps a change from landing under a running turn.
    permissions: UnboundedSender<Change>,
    /// What a bare `/permissions` lists, as the runtime last published it.
    grants: Grants,
}

/// What `/status` reads of the runtime thread, as that thread last left it.
///
/// The configuration, the model in force and the conversation all live on the
/// runtime thread, and `/status` is typed on the UI thread -- which may not
/// wait for that one: mid-turn it is busy for as long as the turn is, and at an
/// approval it is parked. Asking it through the work queue would also put the
/// question behind the turn and on the band as `queued 1`. So the runtime
/// **publishes** a snapshot wherever one of those facts changes
/// ([`Runtime::publish`]) -- when a turn, a `/new`, a provider switch or a
/// model change is over -- and the UI reads the latest one. Mid-turn that is
/// the snapshot the last of those left, so the counts are the conversation's
/// as of the turn before.
///
/// **The lock guards a store and a clone and nothing else.** Neither side holds
/// it across I/O, a channel or an `.await`, so the UI thread's wait on it is
/// the length of copying one struct.
#[derive(Clone)]
pub(crate) struct StatusBoard(Arc<Mutex<StatusSnapshot>>);

impl StatusBoard {
    fn new(snapshot: StatusSnapshot) -> Self {
        Self(Arc::new(Mutex::new(snapshot)))
    }

    /// Replaces what `/status` will read next.
    fn publish(&self, snapshot: StatusSnapshot) {
        *self.lock() = snapshot;
    }

    /// The snapshot the runtime last published.
    pub(crate) fn read(&self) -> StatusSnapshot {
        self.lock().clone()
    }

    /// A board no runtime publishes to, seeded the way `spawn` seeds one:
    /// from a configuration the real loader resolved, over an empty home and
    /// an empty workspace. A case that is about what `/status` shows
    /// publishes what it needs ([`WorkHandle::publish_status`]).
    #[cfg(test)]
    fn unpublished() -> Self {
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let config = RuntimeConfig::load_with(
            &Environment::new(Some(home.path().to_path_buf()), Default::default()),
            workspace.path(),
        )
        .expect("load a configuration");
        Self::new(status_snapshot(&config, &config.model, None))
    }

    /// Never poisoned in practice -- nothing that holds it can panic -- and a
    /// snapshot is whole either way: it is replaced by one assignment.
    fn lock(&self) -> MutexGuard<'_, StatusSnapshot> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl WorkHandle {
    /// Offers the runtime one piece of work, refusing rather than waiting.
    ///
    /// `try_send`, never `send`: the UI thread owns the terminal, and a UI that
    /// waited here would be a UI that stops painting because the runtime is
    /// busy -- which is exactly when it must not.
    pub(crate) fn submit(&self, work: TurnWork) -> Result<(), Rejected> {
        // Asked before the flag, so a runtime that has died mid-turn -- leaving
        // the flag set behind it -- is reported as gone rather than as forever
        // busy.
        if self.work.is_closed() {
            return Err(Rejected::Gone);
        }
        // The claim and the test are one operation: two keystrokes cannot both
        // find the last place free.
        // fetch_update is renamed try_update in Rust 1.99; kept for the 1.96 MSRV.
        #[allow(deprecated)]
        if self
            .outstanding
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |held| {
                (held < WORK_LIMIT).then_some(held + 1)
            })
            .is_err()
        {
            return Err(Rejected::Busy);
        }
        match self.work.try_send(work) {
            Ok(()) => {
                self.accepted.fetch_add(1, Ordering::Release);
                Ok(())
            }
            // Nothing was taken, so nothing is owed: the claim goes back before
            // the refusal does, or the session holds a place for ever over a
            // submission that never happened.
            //
            // `Full` is **unreachable** while the channel is [`WORK_LIMIT`]
            // deep -- the count above refuses the item the channel would have
            // to -- and it is mapped rather than trusted away because the two
            // agreeing is an invariant of this file and not of the language. A
            // session that ever reached it would refuse visibly instead of
            // dropping a prompt.
            Err(err) => {
                self.outstanding.fetch_sub(1, Ordering::Release);
                Err(match err {
                    TrySendError::Full(_) => Rejected::Busy,
                    TrySendError::Closed(_) => Rejected::Gone,
                })
            }
        }
    }

    /// How many submissions this session has accepted, ever.
    ///
    /// Quoted by a cancellation so the runtime can tell what the user was
    /// interrupting from what they typed afterwards.
    pub(crate) fn accepted(&self) -> usize {
        self.accepted.load(Ordering::Acquire)
    }

    /// How much work the runtime has in hand, running and waiting together.
    ///
    /// What decides whether a Ctrl-C is a cancellation or a cleared draft
    /// (`super::shell`): there is something to stop exactly when this is not
    /// zero.
    pub(crate) fn outstanding(&self) -> usize {
        self.outstanding.load(Ordering::Acquire)
    }

    /// How many submissions are waiting behind the one in flight.
    ///
    /// What the band's hint row says. Derived from [`Self::outstanding`] rather
    /// than counted separately, so the number on the screen and the number the
    /// refusal is decided from cannot drift apart.
    pub(crate) fn queued(&self) -> usize {
        self.outstanding().saturating_sub(1)
    }

    /// Tells a turn that is already running something. Never blocks, never
    /// full: a UI that could not say "stop" is a UI that cannot stop it.
    pub(crate) fn control(&self, message: TurnControl) {
        let _ = self.control.send(message);
    }

    /// The conversation's last reply, exactly as the model wrote it, or `None`
    /// when it has none.
    ///
    /// Answered at once, whatever the runtime is doing: it is a copy the
    /// runtime keeps current ([`LastReply`]), not a question put to a thread
    /// that may be in the middle of a turn.
    pub(crate) fn last_reply(&self) -> Option<Arc<str>> {
        self.reply.get()
    }

    /// Gives one place back, as the runtime does once a piece of work has said
    /// its last word -- or has been dropped by an interrupt.
    #[cfg(test)]
    pub(crate) fn finished_one(&self) {
        self.outstanding.fetch_sub(1, Ordering::Release);
    }

    /// Makes `reply` the conversation's last reply, as the runtime would after
    /// a turn that said it.
    #[cfg(test)]
    pub(crate) fn replied(&self, reply: &str) {
        self.reply.publish(Some(reply));
    }

    /// What `/status` reports: the runtime's last published snapshot
    /// ([`StatusBoard`]). Never waits for the runtime.
    pub(crate) fn status(&self) -> StatusSnapshot {
        self.status.read()
    }

    /// Hands the runtime a change to the session's authority, to apply before
    /// the next piece of work it starts. Never blocks, never full.
    ///
    /// `Err` means the runtime thread is gone, which is the one thing a caller
    /// must not paint a new mode over.
    pub(crate) fn change_permissions(&self, change: Change) -> Result<(), Rejected> {
        self.permissions.send(change).map_err(|_| Rejected::Gone)
    }

    /// The conversation's grants, as a bare `/permissions` lists them, from
    /// the runtime's last publication. Never waits for the runtime.
    pub(crate) fn grants(&self) -> Vec<String> {
        self.grants.read()
    }

    /// A handle onto channels with no runtime behind them, and both receiving
    /// ends handed back.
    ///
    /// For the tests that ask what the UI **sent** rather than what a turn did
    /// with it. The receivers are returned rather than dropped for a reason
    /// that would otherwise be a silent trap: a dropped receiver closes its
    /// channel, and a `submit` against a closed channel is a
    /// [`Rejected::Gone`], so a fixture that discarded them would exercise a
    /// path no real session takes.
    #[cfg(test)]
    pub(crate) fn detached() -> (
        Self,
        Receiver<TurnWork>,
        mpsc::UnboundedReceiver<TurnControl>,
    ) {
        let (handle, work_rx, control_rx, changes) = Self::detached_with_changes();
        // Kept open for the handle's whole life rather than dropped: a closed
        // channel would make every `/permissions` in a fixture read as a
        // runtime that is gone. Leaked deliberately -- a test process that
        // ends takes it with it.
        std::mem::forget(changes);
        (handle, work_rx, control_rx)
    }

    /// [`Self::detached`], with the receiving end of the authority channel
    /// handed back too, for the cases that ask what `/permissions` and
    /// Shift-Tab sent.
    #[cfg(test)]
    pub(crate) fn detached_with_changes() -> (
        Self,
        Receiver<TurnWork>,
        mpsc::UnboundedReceiver<TurnControl>,
        UnboundedReceiver<Change>,
    ) {
        let (work, work_rx) = mpsc::channel(WORK_LIMIT);
        let (control, control_rx) = mpsc::unbounded_channel();
        let (permissions, changes) = mpsc::unbounded_channel();
        (
            Self {
                work,
                control,
                outstanding: Arc::new(AtomicUsize::new(0)),
                accepted: Arc::new(AtomicUsize::new(0)),
                reply: LastReply::default(),
                status: StatusBoard::unpublished(),
                permissions,
                grants: Grants::default(),
            },
            work_rx,
            control_rx,
            changes,
        )
    }

    /// Stands in for the runtime's publication of the grants list.
    #[cfg(test)]
    pub(crate) fn publish_grants(&self, lines: Vec<String>) {
        self.grants.publish(lines);
    }

    /// Stands in for the runtime's own publication, for the cases that are
    /// about what the UI does with a snapshot.
    #[cfg(test)]
    pub(crate) fn publish_status(&self, snapshot: StatusSnapshot) {
        self.status.publish(snapshot);
    }

    /// Stands in for the runtime giving one place back -- a piece of work
    /// that is over (`turn_loop`), or one a cancellation dropped -- for the
    /// cases that are about what the UI does once it has.
    #[cfg(test)]
    pub(crate) fn give_back(&self) {
        self.outstanding.fetch_sub(1, Ordering::Release);
    }
}

/// The conversation's last reply, kept where the UI can read it without
/// asking the runtime.
///
/// Upstream's `/copy` reads the session's history on its UI thread
/// (`commandCopyLast`, `app_commands.zig:1974-2001`). Here the history is the
/// runtime thread's, and asking that thread would mean waiting behind whatever
/// turn it is running -- or spending a place in the queue ([`WORK_LIMIT`]) and
/// being dropped with the queue by the next Ctrl-C. So the runtime **publishes**
/// it instead: once just before a turn's conclusion is sent, and once after
/// every piece of work (`turn_loop`). The UI reads the newest one.
///
/// An `Arc<str>` behind the lock, so a read is a reference count rather than a
/// copy of the reply -- the copy that matters is the one the clipboard gets.
#[derive(Debug, Clone, Default)]
struct LastReply(Arc<Mutex<Option<Arc<str>>>>);

impl LastReply {
    fn publish(&self, reply: Option<&str>) {
        let mut held = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        // The same reply published again -- every `/model`, every catalog --
        // is not worth a second allocation of it.
        if held.as_deref() != reply {
            *held = reply.map(Arc::from);
        }
    }

    fn get(&self) -> Option<Arc<str>> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

/// The runtime thread and everything needed to end it.
pub(crate) struct Worker {
    /// `None` once the thread has been joined, or once it has been left
    /// detached because the join grace ran out.
    ///
    /// What the thread hands back is the session its conversation was in when
    /// it stopped, if that one is worth coming back to
    /// ([`Runtime::resumable`]).
    thread: Option<JoinHandle<Option<SessionId>>>,
    /// Set by the thread as the last thing it does, which is what makes the
    /// join *bounded*: `JoinHandle::join` has no timeout, so the wait is on
    /// this and the join is what follows a wait that succeeded.
    finished: Arc<AtomicBool>,
    /// The session's cancellation. Held here rather than passed to
    /// [`Worker::shutdown`] because cancelling is the first step of the drain
    /// protocol and a caller that could forget it would deadlock instead.
    cancel: Cancellation,
    handle: WorkHandle,
    /// The session to name on the way out, once the thread has said which
    /// ([`Worker::resumable`]).
    resumable: Option<SessionId>,
}

impl Worker {
    /// The session the runtime ended in, if it had a conversation -- read
    /// **after** [`Worker::shutdown`], which is what joins the thread that
    /// knows.
    ///
    /// `None` for a session that asked nothing, which upstream does not keep
    /// either (`shouldCreateResumeHandoff`, `app_session_runtime.zig:350-354`):
    /// xfx creates a session at its first prompt, so a session with no turn was
    /// never written. `None` too for a thread the join grace gave up on, which
    /// is the conservative side: no line is better than a line about a session
    /// whose last state nobody saw.
    pub(crate) fn resumable(&self) -> Option<&SessionId> {
        self.resumable.as_ref()
    }

    /// See [`WorkHandle::control`].
    ///
    /// There is deliberately no `Worker::submit` beside it: submitting is the
    /// shell's, through the [`WorkHandle`] below, and a second door onto the
    /// same channel would be a method with no caller.
    pub(crate) fn control(&self, message: TurnControl) {
        self.handle.control(message);
    }

    /// What the shell holds so that a submitted line reaches the runtime.
    pub(crate) fn handle(&self) -> WorkHandle {
        self.handle.clone()
    }

    /// Ends the session's work and gives the UI everything already produced.
    ///
    /// The order is the protocol, and every step is here because of the step
    /// before it:
    ///
    /// 1. **Cancel** -- the mirror, then the token ([`Cancellation::cancel`]).
    ///    The transport polls the first and a task parked on a full channel is
    ///    woken by the second, so this is what makes a turn *stop* rather than
    ///    merely be asked to.
    /// 2. **Say so on the control channel.** Cancelling ends the turn that is
    ///    running; this is what ends the loop that would otherwise wait for the
    ///    next piece of work.
    /// 3. **Keep receiving, and keep painting.** This is the step the naive
    ///    "signal, then join" order gets wrong: a producer parked in
    ///    `send().await` on a full `UiEvent` channel cannot reach its own
    ///    terminal event until somebody drains the channel, so a UI that
    ///    stopped draining and joined would wait for a thread that is waiting
    ///    for it. It ends on the turn's terminal event, on a closed channel, or
    ///    on `deadline`.
    /// 4. **Close the channel on the deadline.** A send still parked then
    ///    resolves `Err` rather than staying parked forever, which is what lets
    ///    the loop reach its own exit.
    /// 5. **Join, bounded.** And return either way: the caller restores the
    ///    terminal on every path, and a thread that cannot write to a terminal
    ///    is not a thread worth holding one for.
    pub(crate) fn shutdown(
        &mut self,
        events: &mut Receiver<UiEvent>,
        deadline: Instant,
        mut drain: impl FnMut(UiEvent),
    ) {
        self.cancel.cancel();
        self.control(TurnControl::Shutdown);

        loop {
            match events.try_recv() {
                Ok(event) => {
                    let last = event.is_terminal();
                    drain(event);
                    if last {
                        break;
                    }
                }
                // The runtime thread is gone and dropped its sender with it.
                // There is nothing more to wait for.
                Err(mpsc::error::TryRecvError::Disconnected) => break,
                Err(mpsc::error::TryRecvError::Empty) => {
                    if Instant::now() >= deadline {
                        break;
                    }
                    std::thread::sleep(DRAIN_POLL);
                }
            }
        }

        events.close();

        let grace = Instant::now() + JOIN_GRACE;
        while !self.finished.load(Ordering::Acquire) && Instant::now() < grace {
            std::thread::sleep(DRAIN_POLL);
        }
        if self.finished.load(Ordering::Acquire) {
            if let Some(thread) = self.thread.take() {
                // A thread that panicked past its catcher names nothing.
                self.resumable = thread.join().ok().flatten();
            }
        }
    }
}

/// Sets the flag [`Worker::shutdown`] waits on, however the thread ends.
///
/// A guard rather than a statement at the bottom of the closure, so that a path
/// which returns early -- a runtime that could not be built -- is not a path on
/// which the UI waits out the whole grace for a thread that has already gone.
struct Finished(Arc<AtomicBool>);

impl Drop for Finished {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

/// Starts the runtime thread, and hands back the channel the UI reads.
///
/// **Called while the owned signals are still blocked**, so the thread inherits
/// a mask in which none of them can be delivered to it. `hold` does that by
/// calling this before [`super::signals::install`] consumes the block.
///
/// The session store is opened *here*, on the calling thread, rather than
/// lazily inside the first turn: "xfx cannot write here" is a fact about the
/// machine, and the moment to say it is before the user has typed a paragraph
/// they are about to lose. It is the same reason `interactive::run` opens it
/// before its first prompt.
///
/// A `resumed` session is the conversation from the first turn on, rather than
/// one opened at the first prompt ([`Runtime::resume`]).
pub(crate) fn spawn(
    config: RuntimeConfig,
    cancel: Cancellation,
    resumed: Option<Resumption>,
) -> io::Result<(Worker, Receiver<UiEvent>)> {
    let store = open_store(&config)?;
    let (events_tx, events_rx) = mpsc::channel(bridge::UI_EVENTS);
    let (work_tx, mut work_rx) = mpsc::channel::<TurnWork>(WORK_LIMIT);
    let (control_tx, control_rx) = mpsc::unbounded_channel::<TurnControl>();
    let (permissions_tx, mut permissions_rx) = mpsc::unbounded_channel::<Change>();
    // One receiver, read by the turn loop and -- while it is parked in an
    // approval -- by the prompter. See `super::approval::ControlChannel` for
    // why that is sound and what it costs.
    let control = ControlChannel::new(control_rx);
    let prompter = TuiPrompter::new(events_tx.clone(), Arc::clone(&control), cancel.clone());
    // Beside the prompter, over **the same** channel, the same UI sender and the
    // same session cancellation: one receiver, one runtime, one park protocol.
    // The two never wait at once -- a turn asks one thing at a time, which is
    // the shell's own rule (`super::shell::Shell::ask`) -- and each consumes and
    // ignores the other's stale traffic rather than answering it.
    let questioner = Arc::new(TuiQuestioner::new(
        events_tx.clone(),
        Arc::clone(&control),
        cancel.clone(),
    ));
    let finished = Arc::new(AtomicBool::new(false));
    let done = Arc::clone(&finished);
    // Seeded here, on the calling thread and from the configuration the
    // runtime is about to be built from, so a `/status` typed before that
    // thread has constructed anything still reads the session it will be:
    // the configured model and no conversation yet.
    let status = StatusBoard::new(status_snapshot(&config, &config.model, None));
    let publishes = status.clone();
    let outstanding = Arc::new(AtomicUsize::new(0));
    let loops_outstanding = Arc::clone(&outstanding);
    let reply = LastReply::default();
    let loops_reply = reply.clone();
    let grants = Grants::default();
    let publishes_grants = grants.clone();
    let session_cancel = cancel.clone();
    let thread = std::thread::Builder::new()
        .name(THREAD_NAME.to_string())
        .spawn(move || {
            // Declared first so it is dropped **last**: the flag must be set
            // after the sender below has gone, or the UI could join a thread
            // whose channel has not closed yet and wait out its own deadline
            // for a message that will never come.
            let _done = Finished(done);
            let events_tx = events_tx;
            let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                // Nothing can run. Say so through the one channel the UI reads.
                fatal_before_the_runtime(&events_tx, NO_RUNTIME);
                return None;
            };
            let mut state = Runtime::new(config, store, prompter, questioner);
            state.reply = loops_reply;
            state.grants = publishes_grants;
            if let Some(resumed) = resumed {
                state.resume(resumed, &session_cancel.mirror());
                // A resumed conversation's last reply is `/copy`'s from the
                // start, as fx's hydrated history is (`session.zig:2067-2079`).
                state.publish_reply();
            }
            // `/status` reads the resumed conversation's counts from the start
            // too: `publishing_to` publishes what the runtime is now.
            let state = state.publishing_to(publishes);
            runtime.block_on(turn_loop(
                state,
                &events_tx,
                &mut work_rx,
                &mut permissions_rx,
                control,
                session_cancel,
                &loops_outstanding,
            ))
        })?;
    Ok((
        Worker {
            thread: Some(thread),
            finished,
            cancel,
            handle: WorkHandle {
                work: work_tx,
                control: control_tx,
                outstanding,
                accepted: Arc::new(AtomicUsize::new(0)),
                reply,
                status,
                permissions: permissions_tx,
                grants,
            },
            resumable: None,
        },
        events_rx,
    ))
}

/// A saved session the TUI continues, taken before the terminal is.
///
/// Both halves are the ones that can fail, which is why they are in hand
/// before the session starts: an id that names nothing, a session another xfx
/// holds, a workspace that is not a directory. Each is reported on a terminal
/// xfx never touched, and what reaches the runtime thread is bound without a
/// failure path ([`bind_conversation`]).
pub(crate) struct Resumption {
    session: WritableSession,
    scope: AccessScope,
}

impl Resumption {
    /// Opens the session `selector` names for writing -- its lock held from
    /// here until the runtime lets the conversation go -- under the scope a
    /// fresh conversation would get.
    pub(crate) fn open(
        config: &RuntimeConfig,
        selector: &crate::session::Selector,
    ) -> io::Result<Self> {
        let scope = AccessScope::primary_only(&config.workspace_root).map_err(io::Error::other)?;
        let store = open_store(config)?;
        let resumed = store
            .resume(selector, &config.workspace_root)
            .map_err(io::Error::other)?;
        Ok(Self {
            session: resumed.session,
            scope,
        })
    }

    /// The model the session last ran under.
    pub(crate) fn model(&self) -> &str {
        &self.session.state().model
    }

    /// What the session replays as on the screen it continues on
    /// ([`super::hydration`]): read off the state this value already holds, so
    /// the launch builds it with no read of its own.
    pub(crate) fn hydration(&self) -> super::hydration::Hydration {
        super::hydration::Hydration::of(self.session.state(), None)
    }
}

/// Opens the session store this session records into.
fn open_store(config: &RuntimeConfig) -> io::Result<SessionStore> {
    let Some(profile_dir) = config.profile_dir.as_ref() else {
        return Err(io::Error::other(crate::interactive::NO_STORE));
    };
    SessionStore::open(profile_dir).map_err(io::Error::other)
}

/// Hands the UI a fatal notice from **outside** the runtime.
///
/// The one send in this module that is not [`bridge::send_ui`] or
/// [`bridge::send_terminal`], and it is here because there is no runtime to
/// await in yet -- `blocking_send` is legal on this thread for exactly as long
/// as that is true, and would panic anywhere else in this file.
///
/// Two things keep it inside the rule that nothing foreign reaches the terminal
/// still able to command it. The argument is `&'static str`, so only text this
/// crate wrote can be spelled here; and [`bridge::inert`] is applied anyway, so
/// the guarantee does not rest on that argument's provenance.
fn fatal_before_the_runtime(events: &Sender<UiEvent>, message: &'static str) {
    let _ = events.blocking_send(UiEvent::Fatal(bridge::inert(message).into_owned()));
}

/// Everything a turn needs that outlives one turn.
///
/// Both of the built-on-first-use halves are the shell's own policy, for the
/// shell's own reasons: a session that is opened and closed without asking
/// anything leaves no empty session behind, and xfx must not reach for a
/// network endpoint until there is something to send -- which is what makes a
/// TUI usable on a machine with no credential.
struct Runtime {
    config: RuntimeConfig,
    /// The variables and home a reload is allowed to observe.
    ///
    /// Captured once, on this thread, rather than read again per reload: a
    /// provider switch re-reads the *file*, and re-reading the environment
    /// under a running session would let a variable exported after launch
    /// change what the session is, which is not a thing a settings write did.
    env: Environment,
    store: SessionStore,
    conversation: Option<Conversation>,
    provider: Option<Bundle>,
    /// The way to ask the person at the terminal, cloned into each
    /// conversation's authority. Held here rather than made per turn -- see the
    /// module header on what "always" is worth.
    prompter: TuiPrompter,
    /// The way the **model** asks the person at the terminal a question, cloned
    /// into each conversation's `ToolContext`.
    ///
    /// Held here for the reason the prompter is, and for one of its own: the
    /// request ids it mints are a session-wide sequence, so a requester rebuilt
    /// per conversation would start again at one and make a keystroke left over
    /// from the previous conversation's last batch indistinguishable from an
    /// answer to the new one's first.
    ///
    /// **The concrete type rather than `Arc<dyn QuestionRequester>`**: what a
    /// real session asks through is this surface's requester, and a field typed
    /// at the trait would let a fixture put something else there and still be
    /// asserting about "the runtime".
    questioner: Arc<TuiQuestioner>,
    /// The model in force, and the catalog of the **currently configured**
    /// provider.
    ///
    /// **One field, not two**, and that is the point of it being here at all:
    /// the model a turn is held in and the catalog a `/model` is judged against
    /// are the same object's two halves, and a `String` beside this one would
    /// be a second answer to "which model is this session in" that only one
    /// caller updates. `Runtime::model` reads it.
    ///
    /// Held across turns because loading the catalog costs a socket and its
    /// whole contract is "once": `ModelSelector::ensure_catalog` attempts a
    /// load only from `NotLoaded`, so a failed load stays failed and a second
    /// `/model` reports the same reason without asking a daemon that is not
    /// there again. Rebuilt -- not merely cleared -- when the provider changes,
    /// because its fetcher is that provider's endpoint and its model is that
    /// provider's model.
    selector: ModelSelector,
    /// The session's reasoning effort: one value for every model, carried
    /// across a model change and a provider switch alike, as upstream's
    /// `app.effort` is (`crate::provider::effort`). Whether a turn *sends* it
    /// is decided per turn, against the catalog's row for the model in force
    /// ([`Runtime::request_options`]).
    effort: crate::provider::effort::Effort,
    /// Where the conversation's last reply is published for `/copy`
    /// ([`Runtime::publish_reply`]).
    reply: LastReply,
    /// Where `/status` reads this runtime from ([`StatusBoard`]), written by
    /// [`Runtime::publish`] and by nothing else.
    board: StatusBoard,
    /// Where a bare `/permissions` reads the conversation's grants from,
    /// written by [`Runtime::publish`] beside `/status`'s snapshot so the two
    /// cannot describe different moments.
    grants: Grants,
}

impl Runtime {
    fn new(
        config: RuntimeConfig,
        store: SessionStore,
        prompter: TuiPrompter,
        questioner: Arc<TuiQuestioner>,
    ) -> Self {
        Self::with_environment(
            config,
            Environment::from_process(),
            store,
            prompter,
            questioner,
        )
    }

    /// [`Runtime::new`], with the environment named rather than read.
    ///
    /// The seam a unit test holds: a reload is a pure function of a settings
    /// file and an environment, and a test that had to export a variable into
    /// the process would race every other test in the binary.
    fn with_environment(
        config: RuntimeConfig,
        env: Environment,
        store: SessionStore,
        prompter: TuiPrompter,
        questioner: Arc<TuiQuestioner>,
    ) -> Self {
        let selector = ModelSelector::new(&config);
        let board = StatusBoard::new(status_snapshot(&config, selector.model(), None));
        let effort = config.effort.clone();
        Self {
            config,
            env,
            store,
            conversation: None,
            provider: None,
            prompter,
            questioner,
            selector,
            effort,
            reply: LastReply::default(),
            board,
            grants: Grants::default(),
        }
    }

    /// Publishes to `board` from here on -- the one the UI reads -- starting
    /// with what this runtime is now.
    fn publishing_to(mut self, board: StatusBoard) -> Self {
        self.board = board;
        self.publish();
        self
    }

    /// Tells `/status` what this runtime is now: its configuration, the model
    /// in force, and the conversation a turn would continue.
    ///
    /// Called where one of those has just changed and **before** the event
    /// that tells the UI so, so a `/status` typed the moment a turn is seen to
    /// end already reads that turn in its count.
    fn publish(&self) {
        self.board.publish(status_snapshot(
            &self.config,
            self.model(),
            self.conversation.as_ref(),
        ));
        self.grants.publish(self.grant_lines());
    }

    /// The conversation's grants as `/permissions` lists them, made inert:
    /// a command a model wrote is text this crate does not trust on a terminal.
    fn grant_lines(&self) -> Vec<String> {
        let Some(conversation) = self.conversation.as_ref() else {
            return Vec::new();
        };
        conversation
            .grant_lines(&self.config.workspace_root)
            .iter()
            .map(|line| bridge::inert(line).into_owned())
            .collect()
    }

    /// Applies one change to the session's authority, **between turns**, and
    /// records it; answers what the session has to be told, if anything.
    ///
    /// Three places hold the mode and all three move together. The
    /// configuration, because every conversation this runtime opens later is
    /// built from it ([`Runtime::authority`]). The live conversation's
    /// authority, because that is built **once per conversation** and not per
    /// turn -- an "always" answer has to outlive the turn it was given in
    /// (module header) -- so changing only the configuration would leave the
    /// conversation the user is talking in at the mode it started with. And
    /// the session log, as [`Runtime::resume`] records a mode that changed, so
    /// a later resume reads what this session really ran under.
    ///
    /// The profile write is here, on this thread, rather than on the UI's: it
    /// stages a file, syncs it, renames it and syncs the directory, which is
    /// disk latency the thread holding the terminal must not wait on -- the
    /// reason `/setup` writes from here too. A write that fails leaves the
    /// change applied and says so, in upstream's words
    /// (`reportUserSettingsFailure`, `session_commands.zig:110-133`).
    fn change_permissions(&mut self, change: Change) -> Option<super::notice::Notice> {
        let mode = match change {
            Change::Mode(mode) => mode,
            Change::Reset => PermissionMode::Ask,
            Change::Acknowledge => {
                self.config.yolo_acknowledged = true;
                return self
                    .persist("yolo_acknowledged", serde_json::Value::Bool(true))
                    .err()
                    .map(|reason| permissions::not_saved("full-access-acknowledgment", &reason));
            }
        };
        self.config.permission_mode = mode;
        if let Some(conversation) = self.conversation.as_mut() {
            conversation.set_permission_mode(mode, change == Change::Reset);
        }
        self.publish();
        self.persist(
            "permission_mode",
            serde_json::Value::String(mode.label().to_string()),
        )
        .err()
        .map(|reason| permissions::not_saved("permission-mode", &reason))
    }

    /// Writes one key of the profile, or says why it could not.
    fn persist(&self, key: &str, value: serde_json::Value) -> Result<(), String> {
        let Some(path) = self.config.user_settings_path.as_ref() else {
            return Err("no home directory".to_string());
        };
        crate::provider::profile::write_preference(path, &[key], value)
            .map_err(|err| err.to_string())
    }

    /// Publishes what `/copy` would copy from the conversation as it stands.
    ///
    /// Read out of the session log's own state ([`last_reply`]) rather than
    /// out of the stream the UI was shown, because that stream has been made
    /// inert on its way over (`bridge::inert`): a tab in a code block reached
    /// the screen as a space, and the clipboard is owed the tab.
    fn publish_reply(&self) {
        self.reply.publish(
            self.conversation
                .as_ref()
                .and_then(|conversation| last_reply(conversation.recorder.state())),
        );
    }

    /// Builds whatever this turn needs and does not have yet.
    fn ready(&mut self, mirror: &CancelToken) -> Result<(), String> {
        if self.provider.is_none() {
            self.provider = Some(Bundle::select(&self.config, mirror)?);
        }
        if self.conversation.is_none() {
            let model = self.model().to_string();
            self.conversation = Some(open_conversation(
                &self.store,
                &self.config,
                &model,
                self.authority(),
                // The TUI's own requester, over the control channel this
                // session's prompter already reads
                // ([`super::question::TuiQuestioner`]). This is the one front
                // end that has a UI to ask in, so it is the one that hands
                // `ask_user_question` a requester at all: the line shell and a
                // non-interactive run pass `None` and get the availability
                // sentinel.
                //
                // **The session's requester, not a new one**, so the ids a
                // rebuilt conversation asks under go on where the last one left
                // off -- see the field.
                Some(Arc::clone(&self.questioner) as Arc<dyn crate::tools::QuestionRequester>),
                mirror,
            )?);
        }
        Ok(())
    }

    /// The conversation, opened now when no prompt has opened it: what a
    /// `/rename` before the first prompt writes into.
    ///
    /// [`Self::ready`]'s second half without its first, because naming a
    /// session reaches no provider: a machine with no credential can still
    /// give its session a title.
    fn conversation(&mut self, mirror: &CancelToken) -> Result<&mut Conversation, String> {
        if self.conversation.is_none() {
            let model = self.model().to_string();
            self.conversation = Some(open_conversation(
                &self.store,
                &self.config,
                &model,
                self.authority(),
                Some(Arc::clone(&self.questioner) as Arc<dyn crate::tools::QuestionRequester>),
                mirror,
            )?);
        }
        self.conversation
            .as_mut()
            .ok_or_else(|| "the conversation could not be opened".to_string())
    }

    /// `/rename`: the title written into the conversation's session, which is
    /// opened for it when no prompt has opened one.
    ///
    /// Opened rather than held back for the first prompt, because a title is
    /// what upstream keeps a session for: its session exists from launch, and
    /// one renamed before anything was asked in it survives the exit
    /// (`discardableOnClose`, `app_session_runtime.zig:5702-5720`). Opening it
    /// here is that, with the store's own failure reported at the rename --
    /// which is when upstream reports it -- rather than at a prompt that came
    /// later.
    fn rename(&mut self, title: &str, mirror: &CancelToken) -> Result<(), String> {
        let conversation = self.conversation(mirror)?;
        rename_conversation(conversation, title)
    }

    /// What a `/resume` picker lists, read now: both scopes, newest first,
    /// without the session this runtime is in and without sessions that hold
    /// no turn (`hasResumableContent`, `session_store_types.zig:71-73`).
    ///
    /// `None` when either listing could not be read, which the picker says as
    /// upstream's does (`Unable to load sessions.`).
    fn sessions(&self) -> Option<super::session_picker::Listing> {
        use crate::session::{ListFilter, ListScope, MAX_LIST_LIMIT};
        let active = self
            .conversation
            .as_ref()
            .map(|conversation| conversation.recorder.id().as_str().to_string());
        let read = |scope: ListScope| {
            self.store
                .list(&ListFilter::new(scope).with_limit(MAX_LIST_LIMIT))
                .ok()
        };
        let rows =
            |list: crate::session::SessionList| -> Vec<super::session_picker::ListedSession> {
                list.sessions
                    .into_iter()
                    .filter(|summary| summary.history_turns > 0)
                    .filter(|summary| active.as_deref() != Some(summary.id.as_str()))
                    .map(|summary| super::session_picker::ListedSession {
                        id: summary.id,
                        title: summary.title,
                        workspace: summary.workspace_root,
                        updated_at_ms: summary.updated_at_ms,
                        turns: summary.history_turns,
                    })
                    .collect()
            };
        let current = read(ListScope::CurrentWorkspace(
            self.config.workspace_root.clone(),
        ))?;
        let all = read(ListScope::AllWorkspaces)?;
        Some(super::session_picker::Listing {
            current: rows(current),
            all: rows(all),
            now_ms: crate::session::event::system_now_ms(),
        })
    }

    /// `/resume`'s Enter: the session `id` names becomes the conversation, and
    /// the answer is what it replays as.
    ///
    /// **Opened before anything is let go of**: a session that cannot be
    /// opened -- another xfx holds it, it is gone, it cannot be trusted --
    /// leaves the conversation this runtime is in exactly as it was. Opened
    /// without waiting on a held lock, as upstream's picker is
    /// (`session_lock_deadline_ms = 0`, `app_session_runtime.zig:2263-2265`).
    /// Then the conversation it replaces is dropped, which closes its log and
    /// releases its lock; xfx writes no session before its first prompt or a
    /// `/rename`, so the one dropped here is always one worth keeping.
    ///
    /// The model follows the launch's rule ([`super::session`]): a session
    /// continues in the model it last ran under when no setting chose one.
    fn resume_by_id(
        &mut self,
        id: &str,
        mirror: &CancelToken,
    ) -> Result<super::hydration::Hydration, super::session_picker::ResumeFailure> {
        use super::session_picker::ResumeFailure;
        let id = crate::session::SessionId::parse(id).map_err(|_| ResumeFailure::Unavailable)?;
        let scope = AccessScope::primary_only(&self.config.workspace_root)
            .map_err(|_| ResumeFailure::Unavailable)?;
        let resumed = self
            .store
            .resume_without_waiting(
                &crate::session::Selector::Id(id),
                &self.config.workspace_root,
            )
            .map_err(|err| match err {
                crate::session::SessionError::Busy { .. } => ResumeFailure::OpenElsewhere,
                _ => ResumeFailure::Unavailable,
            })?;
        let resumption = Resumption {
            session: resumed.session,
            scope,
        };
        if self.config.sources.model == crate::config::SettingSource::CompiledDefault
            && self.model() != resumption.model()
        {
            self.config.model = resumption.model().to_string();
            self.selector = ModelSelector::new(&self.config);
        }
        let replay = super::hydration::Hydration::of(
            resumption.session.state(),
            Some(self.model().to_string()),
        );
        self.conversation = None;
        self.resume(resumption, mirror);
        self.publish_reply();
        self.publish();
        Ok(replay)
    }

    /// The model this session's next turn is held in.
    ///
    /// Read from the selector rather than kept beside it: what `/model`
    /// changes and what a turn is sent under have to be one fact, or a refusal
    /// the selector made would leave the two disagreeing about which model the
    /// session is in.
    fn model(&self) -> &str {
        self.selector.model()
    }

    /// The permission authority a turn on this thread runs under.
    ///
    /// **Not `app::permission_session`**, and this is the one line of the
    /// module header that is a line of code: that helper attaches a prompter
    /// which reads standard input, and standard input is the descriptor the UI
    /// thread owns and is polling. This one asks through the band instead
    /// ([`super::approval::TuiPrompter`]), so `ask` mode is answerable here
    /// without a second reader on the terminal.
    ///
    /// A named step rather than an argument written inline, so that what a turn
    /// may say yes with is something a test can hold.
    fn authority(&self) -> PermissionSession {
        PermissionSession::new(self.config.permission_mode)
            .with_prompter(Box::new(self.prompter.clone()))
    }

    /// Makes a saved session this runtime's conversation, before any work.
    ///
    /// Bound the way [`Runtime::ready`] binds a new one -- this thread's
    /// authority, the session's requester, the session's cancellation -- so a
    /// resumed conversation differs from a fresh one only in what its log
    /// already holds: its history, which every turn replays from the log
    /// (`one_turn`), and its grants ([`bind_conversation`]).
    ///
    /// The model and the permission mode this session runs under are written
    /// down when they differ from the ones the log last recorded, which is
    /// `xfx ask --resume`'s step (`app::run_ask`): the mode is never restored
    /// from a log, and the model is the configuration's, so a later resume
    /// reads what this one really used.
    fn resume(&mut self, resumed: Resumption, mirror: &CancelToken) {
        let mut conversation = bind_conversation(
            &self.store,
            resumed.session,
            resumed.scope,
            self.authority(),
            Some(Arc::clone(&self.questioner) as Arc<dyn crate::tools::QuestionRequester>),
            mirror,
        );
        let model = self.model().to_string();
        let mode = self.config.permission_mode;
        let recorded = conversation.recorder.state();
        let model_changed = recorded.model != model;
        let mode_changed = recorded.permission_mode != mode;
        if model_changed || mode_changed {
            conversation
                .recorder
                .commit(SessionEvent::PreferencesChanged {
                    model: model_changed.then_some(model),
                    permission_mode: mode_changed.then(|| mode.label().to_string()),
                });
        }
        self.conversation = Some(conversation);
    }

    /// The session this runtime's conversation is recorded in, when it holds
    /// one with at least one turn in it.
    ///
    /// Upstream's test for a resume handoff is "not pristine"
    /// (`shouldCreateResumeHandoff`, `app_session_runtime.zig:350-354`;
    /// pristine is no history, `session_store.zig:4498-4509`), and a turn is
    /// the history a log holds. A conversation `/new` dropped is not this
    /// runtime's any more, so its session is not named either: what is named
    /// is the one the next prompt would have gone to.
    fn resumable(&self) -> Option<SessionId> {
        self.conversation
            .as_ref()
            .filter(|conversation| !conversation.recorder.state().turns.is_empty())
            .map(|conversation| conversation.recorder.id().clone())
    }

    /// Switches the provider a turn talks to, and records the choice.
    ///
    /// Runs [`setup_transaction`] and, only on a reload that succeeded, performs
    /// its step (g). The reload closure is built here because this is where the
    /// workspace and the environment are: a reload is `RuntimeConfig::load_with`
    /// and nothing else, which is what keeps this from becoming a second place
    /// that decides what a settings file means.
    async fn select_provider(
        &mut self,
        provider: ProviderId,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Selected, SetupProblem> {
        let env = self.env.clone();
        let workspace = self.config.workspace_root.clone();
        let reload =
            move || RuntimeConfig::load_with(&env, &workspace).map_err(|err| err.to_string());
        let reloaded =
            setup_transaction(&self.config, &self.env, provider, cancelled, &reload).await?;
        Ok(self.adopt(reloaded))
    }

    /// Step (g): the swap, all at once.
    ///
    /// **Every field from the reloaded configuration**, never from the report:
    /// the provider, the model, the URL, the sources and a rejected provider
    /// value all come from what `load_with` made of the file, so a layer that
    /// outranks the profile still outranks it and the session says what it will
    /// really do rather than what the write intended.
    ///
    /// The bundle and the conversation are dropped **here and not earlier**, so
    /// a transaction that failed anywhere above leaves a working session alone.
    /// They are dropped rather than kept because a conversation carried across a
    /// switch would replay one provider's history into another's wire format,
    /// and a bundle is a connection to an endpoint that is no longer configured.
    fn adopt(&mut self, reloaded: RuntimeConfig) -> Selected {
        // **Except the session's permission mode**, which is not what the file
        // says but what the person last chose: `/permissions` and Shift-Tab
        // write the profile too, but an `XFX_PERMISSION_MODE` that outranks it
        // would otherwise take a provider switch back to the launch's mode
        // while the hint row goes on saying the chosen one. The same for a
        // warning already shown: it stays shown.
        let mode = self.config.permission_mode;
        let acknowledged = self.config.yolo_acknowledged || reloaded.yolo_acknowledged;
        self.config = reloaded;
        self.config.permission_mode = mode;
        self.config.yolo_acknowledged = acknowledged;
        // The model comes back with it, because the selector is where the model
        // is: rebuilding it from the reloaded configuration is what makes the
        // switch's model the one the file now says rather than the one the
        // session was in.
        self.selector = ModelSelector::new(&self.config);
        self.provider = None;
        self.conversation = None;
        self.publish();
        // Asked of the **new** provider, here, where the reloaded configuration
        // is: the UI holds a bool rather than a configuration, so this is the
        // only place the question can be answered after a switch.
        let missing_credential =
            !crate::provider::credential_ready(self.config.provider, &self.config);
        Selected {
            provider: self.config.provider,
            model: self.model().to_string(),
            missing_credential,
            // Computed from the **reloaded** configuration rather than taken
            // from the report, and the difference is the whole value of saying
            // it: the report was decided before the write, against the
            // configuration that was; this is what outranks the file now that it
            // has been written. A setup that recorded a model an `XFX_MODEL` in
            // the shell overrides, and reported plain success, would be a
            // receipt for a change with no effect.
            overridden_by: crate::llmux::setup::overriding_layers(&self.config),
        }
    }

    /// The configured provider's catalog, bounded to what the UI will render.
    ///
    /// **Once per provider.** `ensure_catalog` attempts a load only from
    /// `NotLoaded`, so a failure is remembered: a second `/model` on a daemon
    /// that is not there reports the same reason without opening a second
    /// socket. The bound is applied here rather than at the painter because it
    /// is the *event* that should not carry ten thousand rows across a channel
    /// the UI drains one frame at a time.
    async fn catalog(&mut self) -> CatalogAnswer {
        from_state(self.selector.ensure_catalog().await)
    }

    /// Loads the catalog if nothing has asked for it yet, and says whether this
    /// call is the one that did -- which is when the UI has to be told
    /// ([`Runtime::catalog_event`]), because its rows are what the band
    /// resolves a query and fills the effort column from.
    async fn load_catalog(&mut self) -> bool {
        let unasked = matches!(self.selector.catalog(), CatalogState::NotLoaded);
        self.selector.ensure_catalog().await;
        unasked && !matches!(self.selector.catalog(), CatalogState::NotLoaded)
    }

    /// What the UI is told about the catalog as it stands: its rows, bounded
    /// and in the menu's order; no rows, for a provider with none; or that it
    /// could not be read, and whether asking again could help.
    fn catalog_event(&self) -> UiEvent {
        let provider = self.config.provider;
        match from_state(self.selector.catalog()) {
            CatalogAnswer::Rows(entries) => UiEvent::CatalogLoaded { provider, entries },
            // The provider has no catalog; nothing went wrong, and the menu
            // says there is nothing in it.
            CatalogAnswer::NoneAdvertised => UiEvent::CatalogLoaded {
                provider,
                entries: Vec::new(),
            },
            CatalogAnswer::Unread(_) => UiEvent::CatalogUnread {
                provider,
                retry: self.selector.failure_retry(),
            },
        }
    }

    /// What every request of the next turn asks for beside its prompt: the
    /// session's effort, **if** the catalog lists it for the model in force
    /// (`Effort::for_request`; `resolveProviderOptionsForCapabilities`,
    /// `model_capabilities.zig:194-206`). A model the catalog does not
    /// describe -- not listed, or no catalog read -- is sent none, which is
    /// upstream's fallback (`capabilitiesForModel` knows no levels).
    fn request_options(&self) -> crate::gateway::protocol::RequestOptions {
        crate::gateway::protocol::RequestOptions {
            reasoning: self
                .effort
                .for_request(self.selector.entry(self.selector.model())),
        }
    }

    /// What `/model <query>` means, with the line shell's own meaning.
    ///
    /// **The decision is [`ModelSelector::apply`]'s and is not remade here.**
    /// That is where the whole catalog is, and the catalog is what a query is
    /// resolved against -- an id or an alias spelled exactly, else upstream's
    /// fuzzy score over the ids, else the query as typed, which is applied
    /// as upstream applies it. A front end that resolved for itself would be a
    /// second reading of a list only one of the two holds.
    ///
    /// What is this function's own is the **durable** half: a selection is
    /// recorded so that a resumed session continues in the model the
    /// conversation was actually held in, from the same outcome
    /// `interactive::apply_model` records it from. Nothing else: which
    /// *provider* a prompt goes to is decided by the configuration, and a front
    /// end that could change it would be a second place that decides.
    fn use_model(&mut self, name: &str) -> ModelOutcome {
        self.apply_model(ModelRequest::Select(name))
    }

    /// [`Runtime::use_model`] for an id the band already settled -- a row, a
    /// literal id, a query it resolved -- which is applied as written
    /// ([`ModelRequest::Exact`]).
    fn use_exact_model(&mut self, id: &str) -> ModelOutcome {
        self.apply_model(ModelRequest::Exact(id))
    }

    fn apply_model(&mut self, request: ModelRequest<'_>) -> ModelOutcome {
        let outcome = self.selector.apply(request);
        // **Only a selection is written down.** A refusal changed nothing and
        // an unchanged model is not a change; a log entry for either would be
        // read back by every later resume as a preference this session never
        // expressed.
        if let ModelOutcome::Selected { model, .. } = &outcome {
            if let Some(conversation) = self.conversation.as_mut() {
                conversation
                    .recorder
                    .commit(SessionEvent::PreferencesChanged {
                        model: Some(model.clone()),
                        permission_mode: None,
                    });
            }
        }
        outcome
    }

    /// Records `model` as the configured provider's in the profile
    /// (`models.<provider>`), and the `effort` chosen with it when there was
    /// one, and says what is worth saying about it.
    ///
    /// Through [`crate::provider::profile::write_preferences`], the one writer
    /// every command that changes a preference shares -- **one** transaction
    /// for the pair, under the settings lock -- and **on this thread**: a
    /// settings write is a read, a staged file and a rename, none of which the
    /// thread holding the terminal may wait on. Read back at the next launch by
    /// [`RuntimeConfig::load_with`], which ranks the model as upstream does:
    /// the environment over the profile over the compiled default. The effort
    /// is the profile's alone ([`crate::config::EFFORT_KEY`]).
    ///
    /// `None` -- nothing to say -- is the ordinary case, as upstream's benign
    /// `saved to user settings` line is dropped when the caller has announced
    /// the change (`reportUserSettingsCommit`, `session_commands.zig:27-85`).
    /// Said, under upstream's label for how the change was made (`model
    /// picker` or `model`, `:914`, `:1187`): that a layer above the profile
    /// will still decide what a fresh session uses (upstream's words), or that
    /// the write failed and the change holds for this process only. A session
    /// with no home has no profile to write and says nothing about one.
    fn save_change(
        &self,
        model: &str,
        effort: Option<&crate::provider::effort::Effort>,
        picker: bool,
    ) -> Option<super::notice::Notice> {
        let path = self.config.user_settings_path.as_ref()?;
        let topic = if picker { "model picker" } else { "model" };
        let model_key = [crate::config::MODELS_KEY, self.config.provider.label()];
        let effort_key = [crate::config::EFFORT_KEY];
        let mut preferences: Vec<(&[&str], serde_json::Value)> =
            vec![(&model_key, serde_json::Value::String(model.to_string()))];
        if let Some(effort) = effort {
            preferences.push((
                &effort_key,
                serde_json::Value::String(effort.label().to_string()),
            ));
        }
        match crate::provider::profile::write_preferences(path, &preferences) {
            Ok(()) => {
                let source = self.config.sources.model;
                (source > crate::config::SettingSource::UserGlobal).then(|| {
                    super::notice::Notice::neutral(
                        topic,
                        format!(
                            "saved to user settings (scope=user); fresh sessions here use \
                             higher-precedence model={}",
                            source.label()
                        ),
                    )
                })
            }
            Err(err) => Some(super::notice::Notice::error(
                topic,
                format!("active for this process but not saved to user settings ({err})"),
            )),
        }
    }
}

/// What a provider switch produced, once the reload has said so.
#[derive(Debug, PartialEq, Eq)]
struct Selected {
    provider: ProviderId,
    model: String,
    missing_credential: bool,
    /// The layer that still outranks the profile the switch just wrote, if
    /// there is one. `None` is the ordinary case and says nothing.
    overridden_by: Option<String>,
}

/// What a `/model` browse produced.
///
/// **Three answers, not two**, and the third is the whole point: "there is
/// nothing to browse" and "xfx could not ask" are different claims about
/// different things. The first is a property of the session -- a Gateway with
/// no credential to ask its catalog with, a llmux with no daemon URL
/// (`crate::provider::model::catalog_for`) -- and the menu says the list is
/// empty. The second really is a failure, and the menu says so.
#[derive(Debug, PartialEq, Eq)]
enum CatalogAnswer {
    /// The provider's rows, bounded to what the UI will render.
    Rows(Vec<CatalogEntry>),
    /// This provider has no catalog at all. Informational.
    NoneAdvertised,
    /// There is a catalog and xfx could not read it. Carries the reason.
    Unread(String),
}

/// What a UI is given for a catalog in this state.
///
/// The whole of `Runtime::catalog` that is not the load itself, split out so
/// that the mapping is a claim a test can make over all four states without a
/// socket. `Unavailable` and `NotLoaded` answer the same way because they are
/// the same fact from the UI's side -- there is nothing to browse -- and
/// answering rather than panicking keeps a report from taking the session down.
fn from_state(state: &CatalogState) -> CatalogAnswer {
    match state {
        CatalogState::Loaded(entries) => CatalogAnswer::Rows(bounded(entries)),
        CatalogState::Failed(reason) => CatalogAnswer::Unread(reason.clone()),
        CatalogState::Unavailable | CatalogState::NotLoaded => CatalogAnswer::NoneAdvertised,
    }
}

/// The rows of `entries` a UI will actually be given.
///
/// Bounded at the **event** rather than at the painter, because the cost being
/// avoided is a ten-thousand-row list crossing a channel the UI drains one
/// frame at a time. A named function rather than a `take` inline so that "the
/// browser is bounded" is a claim a test can make without a daemon.
fn bounded(entries: &[CatalogEntry]) -> Vec<CatalogEntry> {
    // Ordered first, so the rows the bound keeps are the ones the menu would
    // have put on top.
    crate::provider::model::picker_order(entries)
        .into_iter()
        .take(MAX_RENDERED_MODELS)
        .collect()
}

/// Runs work until the UI says to stop, or until it is gone, and says which
/// session it ended in ([`Runtime::resumable`]).
///
/// **Control first, then at most one piece of work**, which is what `biased`
/// buys: a shutdown that arrived while this was waiting for a prompt is
/// answered rather than made to wait behind one.
async fn turn_loop(
    mut state: Runtime,
    events: &Sender<UiEvent>,
    work: &mut Receiver<TurnWork>,
    changes: &mut UnboundedReceiver<Change>,
    control: Arc<ControlChannel>,
    cancel: Cancellation,
    outstanding: &AtomicUsize,
) -> Option<SessionId> {
    let mut queue = Queue {
        work,
        outstanding,
        taken: 0,
    };
    // Whether the UI can still send a change. A closed channel answers `recv`
    // at once, every time, so an arm left on it would spin this loop.
    let mut changes_open = true;
    loop {
        // The select borrows `work` for its own branch, so the cancellation arm
        // cannot drain the queue where it is decided; it says so and the drain
        // happens below, outside that borrow.
        let next = tokio::select! {
            biased;
            message = control.recv() => match message {
                // The session is over, or the UI is gone with its sender.
                // Answered below rather than here: the changes still waiting
                // are applied on the way out, and the select holds that
                // channel for its own branch.
                Some(TurnControl::Shutdown) | None => Taken::Shutdown,
                // Between turns there is nothing *running* to cancel -- but
                // there may be work waiting that nobody has started, and after
                // an interrupt nothing may start.
                Some(TurnControl::Cancel { through }) => Taken::Abandon(through),
                // The same, and the conversation goes too -- at once, because
                // nothing is running to wait for.
                Some(TurnControl::Reset { through }) => Taken::Reset(through),
                // Nothing is running, so the turn after this moment is the next
                // one: the change is made now.
                Some(TurnControl::Model(change)) => Taken::Model(change),
                // No question is outstanding between turns, so an answer is
                // consumed and dropped rather than left to be misread by the
                // next one: the prompter that could have asked one is only
                // reachable from inside a turn. The model's own questions are
                // the same fact under a different name -- and their id makes it
                // provable rather than argued: no turn is running, so no
                // requester holds the id this message names.
                Some(
                    TurnControl::Answer { .. }
                    | TurnControl::QuestionAnswer { .. }
                    | TurnControl::QuestionCancelled { .. },
                ) => Taken::Nothing,
            },
            // **After control and before work**, and that order is the whole
            // of "between turns": this loop is the only place a turn starts,
            // so a change typed while one was running is applied the moment it
            // ends and before whatever was queued behind it begins -- upstream
            // syncs its queued prompts to a new mode the same way
            // (`syncQueuedPermissionSnapshot`, `app_permission_runtime.zig`).
            change = changes.recv(), if changes_open => Taken::Change(change),
            item = queue.work.recv() => Taken::Work(item),
        };
        let item = match next {
            Taken::Abandon(through) => {
                queue.abandon(through);
                continue;
            }
            Taken::Reset(through) => {
                queue.abandon(through);
                drop_conversation(&mut state, events, through).await;
                continue;
            }
            Taken::Model(change) => {
                let ended =
                    change_model(&mut state, change, events, &cancel, &control, &mut queue).await;
                // A change waiting on the catalog can be overtaken by anything
                // the channel carries, and a model change read meanwhile is put
                // aside like one read during a turn.
                match apply_deferred(&mut state, ended, events, &cancel, &control, &mut queue).await
                {
                    Ended::Session => return settled(state, changes),
                    Ended::Reset(through) => drop_conversation(&mut state, events, through).await,
                    Ended::Turn => {}
                }
                continue;
            }
            Taken::Nothing => continue,
            Taken::Shutdown => return settled(state, changes),
            Taken::Change(Some(change)) => {
                // Not a terminal event -- nothing is waiting on one for a
                // change -- but `send_terminal` is the send that makes the
                // event inert and does not depend on a turn's token.
                if let Some(notice) = state.change_permissions(change) {
                    bridge::send_terminal(events, UiEvent::Notice(notice)).await;
                }
                continue;
            }
            Taken::Change(None) => {
                changes_open = false;
                continue;
            }
            // The UI dropped its side of the work channel without saying
            // anything, which is the same fact as a shutdown and is treated as
            // one.
            Taken::Work(None) => return settled(state, changes),
            Taken::Work(Some(item)) => queue.took(item),
        };
        let ended = match item {
            TurnWork::Submit(prompt) => {
                run_turn(&mut state, prompt, events, &cancel, &control, &mut queue).await
            }
            // Both of these open a socket, so both are work rather than control
            // messages, and both owe **exactly one** terminal event -- which is
            // what lets the UI's drain and the queue's accounting treat them
            // like any other item.
            TurnWork::Setup(provider) => {
                run_setup(&mut state, provider, events, &cancel, &control, &mut queue).await
            }
            TurnWork::Catalog => {
                run_catalog(&mut state, events, &cancel, &control, &mut queue).await
            }
            // Three answers that read or write the store and nothing else, so
            // like `/model` they need no race against the control channel:
            // there is no socket to stop and nothing a Ctrl-C would shorten.
            TurnWork::Sessions => {
                let listing = state.sessions();
                bridge::send_terminal(events, UiEvent::Sessions(listing)).await;
                Ended::Turn
            }
            TurnWork::Resume(id) => {
                let answer = match state.resume_by_id(&id, &cancel.mirror()) {
                    Ok(replay) => UiEvent::Resumed(replay),
                    Err(failure) => UiEvent::ResumeFailed { id, failure },
                };
                bridge::send_terminal(events, answer).await;
                Ended::Turn
            }
            TurnWork::Rename(title) => {
                run_rename(&mut state, &title, events, &cancel.mirror()).await;
                Ended::Turn
            }
        };
        // After **every** piece of work rather than in the arms that change
        // the conversation: a provider switch replaces it, and a piece of work
        // added later that does either is covered without remembering to be.
        // A reset drops it outside these arms and republishes there
        // ([`drop_conversation`]).
        state.publish_reply();
        // **After the terminal event, not before it.** The place `submit`
        // claimed covers the whole of one piece of work, so the moment it is
        // given back is the moment the queue really has room -- and by then the
        // UI has been told this one is over. Given back on the way out of a
        // fatal turn too: nothing more will run, and a refusal that talked
        // about a queue on a dead runtime would be a lie the `Gone` arm exists
        // to avoid.
        queue.outstanding.fetch_sub(1, Ordering::Release);
        // The model changes asked for while it ran, **now** -- after its last
        // word and before anything queued behind it is taken, so the prompt
        // that waited is the first turn to run in the model the band said it
        // would ([`TurnControl::Model`]).
        let ended = apply_deferred(&mut state, ended, events, &cancel, &control, &mut queue).await;
        match ended {
            Ended::Session => return settled(state, changes),
            // A reset reached this work while it ran: the work has said its
            // last word, and the conversation goes before anything else is
            // taken.
            Ended::Reset(through) => drop_conversation(&mut state, events, through).await,
            Ended::Turn => {}
        }
    }
}

/// `/new`, `/clear` and `/reset`, with the line shell's own meaning: the
/// recorder is dropped, which closes its log and releases the session's writer
/// lock, so the next prompt opens a genuinely new identity
/// (`interactive.rs:457-464`). The tool context goes with it, because a session
/// grant and the read proofs that let a file be edited are sold as being about
/// *this* conversation. Upstream opens its fresh session at once
/// (`beginFreshPersistedSession`, `app_session_runtime.zig:1619-1648`) and
/// deletes it again on exit if nothing was said in it; xfx opens it with the
/// first prompt, which leaves the same sessions on disk.
///
/// And the UI is told, after the drop and not before: everything the work
/// ahead of it sent is already on the channel, so this is the line between the
/// conversation the reset handed to scrollback and the one it started
/// ([`UiEvent::Fresh`]). Through `send_terminal` because nothing but the UI
/// going away may stop it: a fence that a cancellation could drop would leave
/// the UI discarding the next conversation's events too.
///
/// `/copy`'s slot and `/status`'s board are republished with it, before the
/// fence: this drop happens outside the work arms that `turn_loop` republishes
/// after, and a `/copy` or a `/status` typed once the fresh screen is up must
/// find no reply to copy and no turns in the conversation, as upstream's do
/// after a reset (`history_turns=0`, `tui-slash-extra.test.ts:314`).
async fn drop_conversation(state: &mut Runtime, events: &Sender<UiEvent>, through: usize) {
    state.conversation = None;
    state.publish_reply();
    state.publish();
    bridge::send_terminal(events, UiEvent::Fresh { through }).await;
}

/// Applies every authority change still waiting, then names the session to
/// resume -- the way out of [`turn_loop`], on every path.
///
/// A change typed while the last turn ran and followed at once by `/quit` was
/// shown to the user and has not been applied yet: the loop takes changes
/// only between pieces of work. Upstream writes the preference the moment the
/// key is pressed (`selectMode`), so a session that left without this would be
/// the one place the two disagree about what the next launch starts in. What
/// the UI is owed for a failed write here is not sent: it is leaving.
fn settled(mut state: Runtime, changes: &mut UnboundedReceiver<Change>) -> Option<SessionId> {
    while let Ok(change) = changes.try_recv() {
        let _ = state.change_permissions(change);
    }
    state.resumable()
}

/// What one turn of [`turn_loop`]'s wait produced.
///
/// A value rather than an early `continue` inside the `select!` because the
/// cancellation arm needs the work receiver the `select!` has already borrowed
/// for its other branch: the decision is made in there and acted on out here.
#[derive(Debug)]
enum Taken {
    /// Work to do, or a closed channel.
    Work(Option<TurnWork>),
    /// The user interrupted, having submitted this many things by then.
    /// Anything waiting from among them is dropped.
    Abandon(usize),
    /// The user reset the session: [`Self::Abandon`], and the conversation
    /// dropped with it.
    Reset(usize),
    /// `/model`, between turns: applied now.
    Model(ModelChange),
    /// A message that means nothing here.
    Nothing,
    /// A change to the session's authority, or the channel that carries them
    /// closing.
    Change(Option<Change>),
    /// The session is over.
    Shutdown,
}

/// The work waiting on the runtime thread, and what an interrupt does to it.
///
/// **This is the whole of "after the interrupt, nothing else starts", and it is
/// sound for one reason worth stating: this thread takes work only at the top
/// of [`turn_loop`].** So while a turn is running -- including while
/// [`Queue::abandon`] is called from inside one, off the cancellation that
/// stopped it -- nothing behind it can have been picked up, and everything
/// still in the channel is something that has not begun. A cancellation that
/// stopped only the running turn would leave the next prompt to start by itself
/// a moment later, which is the surprise the queue was made visible to prevent.
///
/// The count is what the band reads, not an event, so the session's idea of how
/// much is outstanding is honest again the moment a drop returns rather than
/// whenever a message is delivered.
///
/// [`Queue::abandon`] is the whole of it; this is where the reasoning lives.
struct Queue<'a> {
    work: &'a mut Receiver<TurnWork>,
    /// The session's own count of what is in hand, running and waiting. Given
    /// back per item, because `WorkHandle::submit` claimed a place per item.
    outstanding: &'a AtomicUsize,
    /// How many items this thread has ever removed from the channel.
    ///
    /// An index rather than a depth, and the runtime's half of a cancellation's
    /// arithmetic: paired with the count the UI quotes, it says exactly which
    /// waiting items an interrupt was about.
    taken: usize,
}

impl Queue<'_> {
    /// Records that one item has been taken, and hands it back.
    fn took(&mut self, item: TurnWork) -> TurnWork {
        self.taken += 1;
        item
    }

    /// Drops every piece of work submitted through `through` that nobody has
    /// started yet, giving each place back. Returns how many.
    ///
    /// `through` is how many submissions the UI had made when the key was
    /// pressed, and **only the work before that line is dropped**. Work that
    /// arrived after the interrupt is a new intention, and eating it would make
    /// Ctrl-C swallow the question the user asked next -- reachable in the
    /// plainest way there is, because the UI paints the interrupt notice on its
    /// own thread and the user can read it and type again long before this
    /// thread has looked at the message.
    fn abandon(&mut self, through: usize) -> usize {
        let mut dropped = 0usize;
        while self.taken < through {
            if self.work.try_recv().is_err() {
                break;
            }
            self.taken += 1;
            self.outstanding.fetch_sub(1, Ordering::Release);
            dropped += 1;
        }
        dropped
    }
}

/// What ended when a turn ended.
#[derive(Debug, PartialEq, Eq)]
enum Ended {
    /// The turn. The session goes on to the next piece of work.
    Turn,
    /// The turn, and the conversation with it: a reset reached it while it
    /// ran ([`TurnControl::Reset`]), quoting this count -- the latest one, if
    /// more than one did. The session goes on, in a fresh one.
    Reset(usize),
    /// The session. Nothing more will run on this thread.
    Session,
}

/// Runs one turn and sends **exactly one** terminal event for it.
///
/// The body is caught rather than allowed to unwind, because a thread that came
/// apart cannot be asked to say so and the UI would find out only by the
/// channel closing -- which it cannot tell from an orderly finish. Caught, the
/// panic is data: a [`UiEvent::Fatal`] the UI paints after it has put the
/// terminal back.
///
/// # The control channel is read *here*, and that is what makes a Ctrl-C work
///
/// [`turn_loop`] reads control **between** pieces of work, which is the wrong
/// place for every message the channel actually carries: a cancellation the
/// user typed while an answer was streaming would sit in the queue until the
/// turn it was meant to stop had ended by itself. So the turn is raced against
/// the channel rather than awaited past it, and `biased` puts the channel
/// first: a `select!` polls both, so the message is seen even when the turn's
/// own future is parked in a `send().await` on a full `UiEvent` channel -- the
/// exact state a user reaches for Ctrl-C in.
///
/// **The turn is stopped here rather than by the UI thread**, and the reason is
/// a one-word difference: the UI holds a [`Cancellation`], whose `cancel` ends
/// the **session** -- every later [`Cancellation::turn`] takes a child of a
/// cancelled root and is born cancelled (`super::bridge`). One Ctrl-C would
/// therefore poison every turn after it. Only this thread holds the turn's own
/// [`TurnCancel`], so only this thread can stop one turn and leave the session
/// alive.
async fn run_turn(
    state: &mut Runtime,
    prompt: String,
    events: &Sender<UiEvent>,
    cancel: &Cancellation,
    control: &ControlChannel,
    queue: &mut Queue<'_>,
) -> Ended {
    let turn = cancel.turn();
    // A turn has begun, and this is the only place that is true: the prompt
    // was accepted when it was typed, and it may have waited behind another
    // turn since. The UI's activity row starts here and ends with the
    // conclusion below, so the row is exactly this turn's lifetime rather than
    // a guess assembled from a queue depth.
    //
    // Through `send_ui`, so a turn cancelled before it could start paints no
    // row -- its conclusion is still sent below, and a conclusion that ends a
    // row nobody drew ends nothing. The result is discarded because there is
    // nothing this function could do about it: the turn still owes its terminal
    // event either way.
    let _ = bridge::send_ui(events, &turn.token, UiEvent::TurnStarted).await;
    let body = AssertUnwindSafe(ReportedByTheCatcher::around(one_turn(
        state, prompt, events, &turn,
    )))
    .catch_unwind();
    // Both halves of one keystroke, and they have to happen **together, at the
    // moment the cancellation arrives**: the turn stops, and everything queued
    // behind it is dropped. Doing the second half after the body finished would
    // be too late for the case that matters most -- a provider that has gone
    // quiet without hanging up never lets the body finish at all, and the queue
    // would sit there claiming a place the band is still announcing.
    let (caught, ended) = raced_against_control(body, control, |through| {
        turn.cancel();
        queue.abandon(through);
    })
    .await;
    // **Before the conclusion goes**, so that a `/copy` typed once the UI has
    // shown this turn ending copies this turn's reply. Published again after
    // the work is over ([`turn_loop`]), which is the copy that covers every
    // other kind of work; this is the one that is ordered against the event.
    state.publish_reply();
    match caught {
        Ok(failure) => {
            // The turn is in the conversation's count now, and so is any
            // "always" it was answered -- both are written down by the time the
            // body returns ([`record_grants`]).
            state.publish();
            bridge::send_terminal(events, UiEvent::TurnEnded { failure }).await;
            ended
        }
        Err(payload) => {
            // Through `send_terminal` like every other conclusion, so the text
            // -- which is a panic message and therefore not xfx's to trust --
            // is made inert at the channel rather than by this call site
            // remembering to.
            bridge::send_terminal(events, UiEvent::Fatal(panic_message(&*payload))).await;
            Ended::Session
        }
    }
}

/// Runs one provider switch and sends **exactly one** terminal event for it.
///
/// Raced against the control channel so the UI keeps being heard while this
/// runs -- but **what the race buys here is much narrower than in a turn, and
/// the narrowness is the contract.** The cancellation is read at step (c) and
/// nowhere else: it can abandon a switch that has not been written, and it can
/// never half-undo one that has.
///
/// In particular it does **not** shorten a slow probe. Step (b) is
/// `setup::prepare`, whose HTTP calls do not take the turn's token, so a
/// Ctrl-C pressed while a daemon is not answering is recorded immediately and
/// acted on when `prepare` returns -- after its own connect and read timeouts
/// (`provider::model`'s `CATALOG_CONNECT_TIMEOUT` and `CATALOG_READ_TIMEOUT`,
/// three and five seconds on loopback). The keystroke is not lost and the
/// switch really is abandoned; what it is not is instant. Saying otherwise here
/// would promise a responsiveness the code does not have.
async fn run_setup(
    state: &mut Runtime,
    provider: ProviderId,
    events: &Sender<UiEvent>,
    cancel: &Cancellation,
    control: &ControlChannel,
    queue: &mut Queue<'_>,
) -> Ended {
    let turn = cancel.turn();
    let token = turn.token.clone();
    let outcome = {
        let cancelled = move || token.is_cancelled();
        let body = state.select_provider(provider, &cancelled);
        let (outcome, ended) = raced_against_control(body, control, |through| {
            turn.cancel();
            queue.abandon(through);
        })
        .await;
        (outcome, ended)
    };
    let (outcome, ended) = outcome;
    match outcome {
        Ok(selected) => {
            // Before the selection, because it is a caveat *on* it: a receipt
            // that arrived after the good news would read as being about the
            // next thing rather than about this one.
            if let Some(source) = selected.overridden_by {
                bridge::send_terminal(
                    events,
                    // A warning under `provider`: the switch happened, and
                    // this is the caveat on it.
                    UiEvent::Notice(super::notice::Notice::warning(
                        "provider",
                        format!(
                            "{source} outranks the profile, so this configuration is not what \
                             the next turn in this shell will use"
                        ),
                    )),
                )
                .await;
            }
            // **Not** through `send_ui`. By the time this is sent the file is
            // written and the runtime has already swapped, so a cancellation
            // that arrived in the meantime must not be allowed to drop the one
            // event that tells the UI what the session now is.
            bridge::send_terminal(
                events,
                UiEvent::ProviderSelected {
                    provider: selected.provider,
                    model: selected.model,
                    missing_credential: selected.missing_credential,
                },
            )
            .await;
            bridge::send_terminal(events, UiEvent::TurnEnded { failure: None }).await;
            ended
        }
        // Nothing was written and nothing changed. The turn is over and there is
        // no failure to report, because refusing to do something the user
        // cancelled is not a failure.
        Err(SetupProblem::Cancelled) => {
            bridge::send_terminal(events, UiEvent::TurnEnded { failure: None }).await;
            ended
        }
        Err(SetupProblem::Failed(message)) => {
            bridge::send_terminal(
                events,
                UiEvent::TurnEnded {
                    failure: Some(message),
                },
            )
            .await;
            ended
        }
        // Another writer owns the settings file. The runtime is untouched, this
        // process cannot say what its configuration is, and it stops rather than
        // guessing.
        Err(SetupProblem::Conflict(message)) => {
            bridge::send_terminal(events, UiEvent::Fatal(message)).await;
            Ended::Session
        }
    }
}

/// Applies one `/model` change ([`TurnControl::Model`]) and tells the UI what
/// became of it.
///
/// **A query the band could not resolve goes to the catalog first**, when none
/// is in hand: upstream resolves a query against the catalog it holds and
/// fetches it when it holds none (`resolveModelQuery`,
/// `session_commands.zig:1147-1166`), so `/model opus` typed before any list
/// was loaded still finds `anthropic/claude-opus-5`. This is `/model`, so a
/// load that failed is asked again ([`ModelSelector::retry_after_failure`]),
/// and the rows that arrive go to the UI as well, which is what lets the next
/// change be resolved -- and announced -- on the band. The load is raced
/// against the control channel, because it is the one piece of this that
/// waits on a network: a Ctrl-C while it waits stops it with everything queued
/// behind it, and nothing is applied -- nothing was said about it either.
///
/// **A change the band resolved is applied as it stands**, with no socket: the
/// band named the id and has already said `Switched to` it, and an id spelled
/// exactly resolves to itself.
///
/// Then the selection, the effort chosen with it, and **the profile write,
/// here on the runtime thread** ([`Runtime::save_change`]): `/setup`'s file
/// discipline, off the thread that holds the terminal. The answer goes through
/// [`bridge::send_terminal`] rather than `send_ui`, for the reason a provider
/// switch's does: by the time it is sent the change has been made and written
/// into the session log, and a cancellation that dropped the one event saying
/// so would leave the band showing a model the session is no longer in.
async fn change_model(
    state: &mut Runtime,
    change: ModelChange,
    events: &Sender<UiEvent>,
    cancel: &Cancellation,
    control: &ControlChannel,
    queue: &mut Queue<'_>,
) -> Ended {
    let mut ended = Ended::Turn;
    if !change.announced {
        state.selector.retry_after_failure();
        let turn = cancel.turn();
        let (fresh, raced) = raced_against_control(state.load_catalog(), control, |through| {
            turn.cancel();
            queue.abandon(through);
        })
        .await;
        if turn.token.is_cancelled() {
            return raced;
        }
        ended = raced;
        if fresh {
            let _ = bridge::send_ui(events, &turn.token, state.catalog_event()).await;
        }
    }
    let outcome = if change.announced {
        state.use_exact_model(&change.name)
    } else {
        state.use_model(&change.name)
    };
    // The effort goes with the model it was chosen for, and only when the
    // model was taken: a refusal changed nothing.
    let effort = match &outcome {
        ModelOutcome::Selected { .. } | ModelOutcome::Unchanged { .. } => change.effort.clone(),
        ModelOutcome::Refused { .. } | ModelOutcome::Reported { .. } => None,
    };
    if let Some(effort) = &effort {
        state.effort = effort.clone();
    }
    state.publish();
    let (answered, chosen) = match outcome {
        ModelOutcome::Selected {
            unverified, model, ..
        } => (ModelAnswer::Applied { unverified }, Some(model)),
        ModelOutcome::Unchanged { model } => (ModelAnswer::Unchanged, Some(model)),
        ModelOutcome::Refused { reason } => (ModelAnswer::Refused { reason }, None),
        // A select request cannot produce a report. Answered rather than
        // asserted away: a `/model` that took the session down because an enum
        // grew an arm would be a worse failure than one that says nothing.
        ModelOutcome::Reported { .. } => return ended,
    };
    // Written for an unchanged model too, as upstream writes it
    // (`setResolvedModel` persists whatever it resolved): the operator named
    // this model, and the profile is where a later launch reads that.
    let saved = chosen.and_then(|model| state.save_change(&model, effort.as_ref(), change.picker));
    bridge::send_terminal(
        events,
        UiEvent::ModelAnswered {
            // **The model in force afterwards**, read back from the selector
            // rather than echoed from the request: on a refusal that is the
            // standing model, and it is the one the band must show.
            model: state.model().to_string(),
            effort: state.effort.clone(),
            outcome: answered,
            next_turn: change.next_turn,
            announced: change.announced,
        },
    )
    .await;
    // After the switch it is about, as upstream's commit line follows the
    // announcement.
    if let Some(notice) = saved {
        bridge::send_terminal(events, UiEvent::Notice(notice)).await;
    }
    ended
}

/// Applies the model changes put aside while a piece of work ran
/// ([`ControlChannel::defer`]), oldest first, and says what ended.
///
/// `ended` is what the work itself ended with. A session that is over still
/// applies a change the band already announced -- it needs no socket, and the
/// profile is where the operator's choice outlives the process -- and drops
/// one that would have had to load a catalog first.
async fn apply_deferred(
    state: &mut Runtime,
    mut ended: Ended,
    events: &Sender<UiEvent>,
    cancel: &Cancellation,
    control: &ControlChannel,
    queue: &mut Queue<'_>,
) -> Ended {
    loop {
        let changes = control.take_deferred();
        if changes.is_empty() {
            return ended;
        }
        for change in changes {
            if ended == Ended::Session && !change.announced {
                continue;
            }
            let after = change_model(state, change, events, cancel, control, queue).await;
            ended = match (ended, after) {
                (Ended::Session, _) | (_, Ended::Session) => Ended::Session,
                (Ended::Reset(earlier), Ended::Reset(later)) => Ended::Reset(earlier.max(later)),
                (Ended::Reset(through), Ended::Turn) | (Ended::Turn, Ended::Reset(through)) => {
                    Ended::Reset(through)
                }
                (Ended::Turn, Ended::Turn) => Ended::Turn,
            };
        }
    }
}

/// `/rename`, and whether it was written down (`handleRenameCommand`,
/// `app_commands.zig:3815-3858`). The session is called `title` either way --
/// upstream sets its cached title before it writes the manifest
/// (`renameActiveSession`, `app_session_runtime.zig:3802-3822`) -- and the
/// shell says which ([`UiEvent::Renamed`]).
async fn run_rename(
    state: &mut Runtime,
    title: &str,
    events: &Sender<UiEvent>,
    mirror: &CancelToken,
) {
    let unsaved = state.rename(title, mirror).err();
    state.publish();
    bridge::send_terminal(
        events,
        UiEvent::Renamed {
            title: title.to_string(),
            unsaved,
        },
    )
    .await;
}

/// Loads the catalog once and sends **exactly one** terminal event for it.
///
/// What the UI is told is what `/model`'s menu shows: the rows, in the order
/// the menu lists them ([`crate::provider::model::picker_order`]); no rows at
/// all when the provider has no catalog (`No models available.`); or that the
/// load failed (`Unable to load models.`). None of the three is written into
/// the document -- the menu is where the answer is, as upstream's is.
async fn run_catalog(
    state: &mut Runtime,
    events: &Sender<UiEvent>,
    cancel: &Cancellation,
    control: &ControlChannel,
    queue: &mut Queue<'_>,
) -> Ended {
    let turn = cancel.turn();
    // `/model`'s menu or column is what asked, which is the gesture upstream's
    // failure text says to repeat (`retry /model.`): a load that failed is
    // asked again here, and only here and in `change_model`.
    state.selector.retry_after_failure();
    let ((), ended) = raced_against_control(
        async {
            state.catalog().await;
        },
        control,
        |through| {
            turn.cancel();
            queue.abandon(through);
        },
    )
    .await;
    // Through `send_ui`: a catalog whose turn was cancelled is a list nobody
    // is waiting for any more, and the conclusion below still closes the item
    // either way.
    let _ = bridge::send_ui(events, &turn.token, state.catalog_event()).await;
    bridge::send_terminal(events, UiEvent::TurnEnded { failure: None }).await;
    ended
}

/// Runs `body` while still listening to the UI, and says what ended.
///
/// `stop` is what a cancellation *does*: in production it stops the turn and
/// drops everything queued behind it ([`abandon_pending`]). It is an argument
/// rather than a statement in the arm below so that "a cancellation reaches
/// work that is still running" is a claim a unit test can make without a
/// provider, a session store and a socket.
///
/// Extracted, named, and given its `stop` as an argument so that "a
/// cancellation reaches a turn that is *running*" is a claim a unit test can
/// make. Inlined into [`run_turn`] it would be reachable only through a real
/// provider, a real session store and a real socket, and the one interleaving
/// that matters -- a message arriving while the body is parked -- is not
/// something a pseudoterminal can be asked to arrange.
///
/// `biased`, so the channel is looked at first: a `select!` polls both sides, so
/// the message is seen even while the body is parked in a `send().await` on a
/// full `UiEvent` channel, which is the exact state a user reaches for Ctrl-C
/// in. The body is always awaited to completion afterwards -- whatever ended it,
/// a turn still owes its one terminal event.
///
/// **One interval is the exception, and it is answered rather than lost**: while
/// an approval is up the body has parked this whole thread, so nothing here is
/// polled at all. The prompter reads the channel itself for exactly that
/// interval, refuses the question on a `Cancel` or a `Shutdown`, and hands the
/// message back for this loop to act on ([`super::approval::ControlChannel`]).
async fn raced_against_control<F: Future>(
    body: F,
    control: &ControlChannel,
    mut stop: impl FnMut(usize),
) -> (F::Output, Ended) {
    tokio::pin!(body);
    let mut ended = Ended::Turn;
    // A closed channel answers `recv` immediately and for ever, so the branch is
    // disabled once it has: an enabled one would spin this loop instead of
    // polling the body.
    let mut listening = true;
    let output = loop {
        tokio::select! {
            biased;
            message = control.recv(), if listening => match message {
                // The user's Ctrl-C. This piece of work stops, and so does
                // whatever they had queued behind it when they pressed the key;
                // the session does not.
                Some(TurnControl::Cancel { through }) => stop(through),
                // A reset: the same stop, and the conversation goes once the
                // work has said its last word -- unless the session is ending
                // anyway, which outranks it.
                Some(TurnControl::Reset { through }) => {
                    stop(through);
                    if ended != Ended::Session {
                        ended = Ended::Reset(through);
                    }
                }
                // The UI is leaving. `Worker::shutdown` has already cancelled
                // the session's root, so the stop below is not what ends this
                // turn -- what this arm carries is that the loop must not go
                // round for another prompt afterwards.
                Some(TurnControl::Shutdown) => {
                    ended = Ended::Session;
                    // Everything, however lately it was submitted: the session
                    // is over, so there is no "after the interrupt" left for a
                    // later prompt to belong to.
                    stop(usize::MAX);
                }
                // `/model` while this work runs: for the turn after it, so put
                // aside until it is over ([`ControlChannel::defer`]). Nothing
                // here stops.
                Some(TurnControl::Model(change)) => control.defer(change),
                // An answer to a question that is no longer being asked: the
                // prompter takes its own answer off this channel while it is
                // parked (`super::approval`), so anything reaching here is a
                // second keystroke on a panel that has already gone. Consumed
                // rather than left, so the next question does not inherit it.
                //
                // The model's own question batches reach this arm for the same
                // reason and are discarded for a stricter one: nothing on this
                // side is waiting on the id they name --
                // [`super::question::TuiQuestioner`] takes its own answer off
                // this channel while it is parked, and while it is parked this
                // loop is not running -- so an answer that arrives here is by
                // construction stale.
                Some(
                    TurnControl::Answer { .. }
                    | TurnControl::QuestionAnswer { .. }
                    | TurnControl::QuestionCancelled { .. },
                ) => {}
                // The UI dropped its sender, which is the same fact as a
                // shutdown and is treated as one.
                None => {
                    listening = false;
                    ended = Ended::Session;
                    stop(usize::MAX);
                }
            },
            output = &mut body => break output,
        }
    };
    (output, ended)
}

/// One turn, exactly as `xfx ask` runs one. `Some` is why it did not finish.
async fn one_turn(
    state: &mut Runtime,
    prompt: String,
    events: &Sender<UiEvent>,
    turn: &TurnCancel,
) -> Option<String> {
    // The matrix row for a panic on the thread that owns nothing. It is taken
    // here, inside the caught body, because what is being measured is that a
    // turn coming apart reaches the user as data -- not that a thread can die.
    #[cfg(feature = "fault-injection")]
    if super::fault::injected(super::fault::Fault::WorkerTurn) {
        panic!("a turn panicked");
    }

    if let Err(message) = state.ready(&turn.mirror) {
        return Some(message);
    }
    // **The effort, before anything is sent.** Whether this model takes it is
    // the catalog's answer, so a session in a level with no catalog read yet --
    // a launch whose profile carries one -- reads it now: once, raced against
    // the turn's own cancellation so a Ctrl-C does not wait out a slow
    // Gateway, and told to the UI, whose footer and columns read the same rows.
    // A session in the default asks for nothing and costs no socket.
    if !state.effort.is_default() {
        let fresh = tokio::select! {
            biased;
            () = turn.token.cancelled() => false,
            fresh = state.load_catalog() => fresh,
        };
        if fresh {
            let _ = bridge::send_ui(events, &turn.token, state.catalog_event()).await;
        }
    }
    let options = state.request_options();
    state
        .provider
        .as_ref()
        .expect("the bundle was just built")
        .stream
        .set_options(options);
    // Read before the conversation is borrowed, and read from the selector that
    // owns it: the model this turn is sent under is the model `/model` last
    // applied, whatever the configuration said at launch.
    let model = state.model().to_string();
    // Two fields of one struct rather than one method returning both: the
    // provider is read and the conversation is written, and the borrow checker
    // is what keeps that honest.
    let provider = state
        .provider
        .as_ref()
        .expect("the bundle was just built")
        .stream
        .as_ref();
    let conversation = state
        .conversation
        .as_mut()
        .expect("the conversation was just opened");

    // Project instructions are read now rather than remembered, so editing
    // `AGENTS.md` in another window takes effect on the next prompt.
    let context = ProjectContext::discover(conversation.tools.scope());
    conversation
        .recorder
        .commit(SessionEvent::ProjectContextRecorded {
            sources: context
                .sources()
                .iter()
                .map(|path| path.display().to_string())
                .collect(),
            bytes: context.total_bytes() as u64,
        });

    let replay = conversation
        .recorder
        .state()
        .history_messages(state.config.provider.wire());
    for notice in &replay.notices {
        // Best effort by construction: a notice about the history is worth
        // less than the turn it precedes, and the only ways this fails are a
        // turn that was cancelled before it began and a UI that is gone --
        // both of which the turn below reports for itself.
        // A warning under `session`: the turn goes ahead, without what the
        // history could not carry. The store's sentence is the line shell's
        // too, so its `xfx: ` comes off here where the glyph says it.
        let notice = super::notice::Notice::warning("session", super::notice::unattributed(notice));
        let _ = bridge::send_ui(events, &turn.token, UiEvent::Notice(notice)).await;
    }

    let request = TurnRequest {
        model,
        prompt,
        history: replay.messages,
        max_steps: state.config.max_agent_steps,
        max_attempts: DEFAULT_MAX_ATTEMPTS,
        // The same token the tools were given when the conversation was
        // opened, reset by `Cancellation::turn` for this turn.
        cancel: turn.mirror.clone(),
        tools: conversation.tools.clone(),
    };
    let mut sink = UiEventSink::new(events.clone(), turn.token.clone());
    // The recorder, with what the full transcript shows of each call -- its
    // arguments, its whole result -- handed to the UI as it is written down
    // (`super::full_transcript::Teed`). The log is written first and either
    // way: the view is best effort and the record is not.
    let mut journal = super::full_transcript::Teed::new(
        &mut conversation.recorder,
        events.clone(),
        turn.token.clone(),
    );
    // And each request the turn settles, as the full transcript's `* network:`
    // record (`super::full_transcript::Networked`).
    let provider = super::full_transcript::Networked::new(
        provider,
        state.config.provider.label(),
        events.clone(),
        turn.token.clone(),
    );
    let outcome = run_turn_saved(request, context, &provider, &mut sink, &mut journal).await;

    // Approvals the user gave in the band become durable after the turn, once,
    // so an "always" answered here survives to the next resume -- the same step
    // `interactive::one_turn` and `app::ask` take, from the same recorder
    // method. **Whatever the turn did**: a turn that failed after the user
    // approved something still collected the approval, and asking again next
    // time would be asking a question the user has already answered.
    record_grants(conversation);

    // What the turn spent, from the **completed** turn's own outcome
    // (`agent::TurnOutcome::usage`, the same value the session log records at
    // `agent::machine`'s `UsageRecorded`). Only a turn that finished has one: a
    // turn that failed mid-stream has no total to report, and inventing one
    // from the deltas that did arrive would put a number on the hint row that
    // no provider ever said.
    //
    // Best effort, like the history notices above: the conclusion below is what
    // the turn owes, and a meter is worth less than the answer it measures.
    if let Ok(finished) = &outcome {
        let _ = bridge::send_ui(
            events,
            &turn.token,
            UiEvent::Usage {
                input: finished.usage.input_tokens,
                output: finished.usage.output_tokens,
            },
        )
        .await;
    }

    // Read after the turn, so the answer that did arrive is not withheld
    // because the log of it could not be written -- the same order
    // `interactive::one_turn` reports in. A turn that failed reports its own
    // failure first: that is the one that says why there is no answer.
    match outcome {
        Ok(_) => conversation
            .recorder
            .failure()
            .map(|problem| format!("xfx: {problem}")),
        Err(err) => Some(err.to_string()),
    }
}

/// Writes down the approvals a turn collected, after it.
///
/// A named step rather than two lines inline, for the reason the module's other
/// named steps are named: it is a thing a test can hold. The whole of it is
/// **which** grants are new, and that question belongs to the log rather than
/// to this thread ([`SessionRecorder::record_new_grants`]) -- the TUI keeps one
/// conversation open across every turn of a session, so a tally kept here would
/// have to stay in step with the log turn after turn.
///
/// The ledger's lock is released before the commit: a commit is a disk write,
/// and the lock is shared with the tools of any turn that comes next.
fn record_grants(conversation: &mut Conversation) {
    let granted = conversation.tools.permissions().grants().to_vec();
    conversation.recorder.record_new_grants(&granted);
}

/// `future`, with this thread's panics marked as this call's to report -- for
/// the length of each poll, and only for that.
///
/// **Per poll rather than per await, and the difference is the whole reason
/// this is a type rather than a guard.** A token held across the `await` would
/// still be set while the runtime polls somebody *else's* work between polls of
/// this one -- the transport's connection tasks, which this `catch_unwind` will
/// never see and whose panics must be reported the ordinary way. Marking inside
/// `poll` makes the silence exactly as wide as the catch.
struct ReportedByTheCatcher<F> {
    /// Boxed so this future is `Unpin`, which is what keeps the projection
    /// below safe code rather than an `unsafe` pin projection for the sake of
    /// one allocation per turn.
    inner: std::pin::Pin<Box<F>>,
}

impl<F: Future> ReportedByTheCatcher<F> {
    fn around(future: F) -> Self {
        Self {
            inner: Box::pin(future),
        }
    }
}

impl<F: Future> Future for ReportedByTheCatcher<F> {
    type Output = F::Output;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        context: &mut std::task::Context<'_>,
    ) -> std::task::Poll<F::Output> {
        let _marked = super::panic::caught_on_this_thread();
        self.inner.as_mut().poll(context)
    }
}

/// What a caught panic said, as text.
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&'static str>() {
        return (*text).to_string();
    }
    if let Some(text) = payload.downcast_ref::<String>() {
        return text.clone();
    }
    UNKNOWN_PANIC.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::BTreeMap;

    use crate::config::{Environment, PermissionMode};
    use crate::gateway::protocol::{Completion, FinishReason, ToolCall, Usage};
    use crate::gateway::CancelToken;
    use crate::permission::{
        AllowSource, ApprovalAnswer, Grant, MutationKind, MutationPlan, PolicyDecision, Preimage,
        ProposedAction, TargetScope,
    };
    use crate::provider::Wire;

    /// A configuration from a home and a workspace that exist and hold no
    /// settings.
    fn config(home: &std::path::Path, workspace: &std::path::Path) -> RuntimeConfig {
        RuntimeConfig::load_with(
            &Environment::new(Some(home.to_path_buf()), BTreeMap::new()),
            workspace,
        )
        .expect("load a configuration")
    }

    /// How long a drain in a unit test is given before it is a hang.
    ///
    /// Far longer than the work below takes, because it bounds a **failure**:
    /// nothing here should ever spend it, and a case that did would otherwise
    /// be a test that never returns.
    const TEST_DRAIN: Duration = Duration::from_secs(10);

    #[test]
    fn the_drain_keeps_receiving_until_the_turn_says_it_is_over() {
        // Step 3 of the protocol, and the step a bounded join would otherwise
        // hide: a producer parked in `send().await` on a full channel cannot
        // reach its own terminal event until somebody drains the channel, so a
        // UI that stopped receiving and went straight to the join would be
        // waiting for a thread that is waiting for it. The channel here is two
        // deep and nine events go through it, so seven of them cannot land at
        // all unless the drain is really receiving.
        //
        // The pty cannot prove this: interleaving two threads on demand is not
        // something a terminal can be asked for. Here it is arranged.
        let (events_tx, mut events_rx) = mpsc::channel::<UiEvent>(2);
        let (work_tx, work_rx) = mpsc::channel::<TurnWork>(1);
        let (control_tx, mut control_rx) = mpsc::unbounded_channel::<TurnControl>();
        let finished = Arc::new(AtomicBool::new(false));
        let done = Arc::clone(&finished);
        let cancel = Cancellation::new(CancelToken::new());
        let watched = cancel.clone();

        let thread = std::thread::spawn(move || {
            let _done = Finished(done);
            // Held for the same reason the real loop holds it: dropping it
            // would close the work channel and change what the UI can see.
            let _work = work_rx;
            assert_eq!(
                control_rx.blocking_recv(),
                Some(TurnControl::Shutdown),
                "the drain never told the loop to stop"
            );
            // A guard on step 1 preceding step 2, and **not** a proof of it:
            // this runs a wake-up after the control message lands, by which
            // time a cancellation issued a statement later has usually landed
            // too. Reordering the two in `shutdown` was run as a mutant and
            // survived here. It is kept because it can only ever fire when the
            // order really is wrong, and it costs nothing.
            assert!(
                watched.is_cancelled(),
                "the shutdown arrived before the cancellation"
            );
            for n in 0..8 {
                events_tx
                    .blocking_send(UiEvent::Delta(format!("d{n}")))
                    .expect("the ui is still receiving");
            }
            events_tx
                .blocking_send(UiEvent::TurnEnded { failure: None })
                .expect("the ui is still receiving");
            // A runtime with no conversation names nothing on its way out.
            None
        });
        let mut worker = Worker {
            thread: Some(thread),
            finished,
            cancel,
            handle: WorkHandle {
                work: work_tx,
                control: control_tx,
                outstanding: Arc::new(AtomicUsize::new(0)),
                accepted: Arc::new(AtomicUsize::new(0)),
                reply: LastReply::default(),
                status: StatusBoard::unpublished(),
                permissions: mpsc::unbounded_channel().0,
                grants: Grants::default(),
            },
            resumable: None,
        };

        let mut seen = Vec::new();
        worker.shutdown(&mut events_rx, Instant::now() + TEST_DRAIN, |event| {
            seen.push(event)
        });

        assert_eq!(
            seen.len(),
            9,
            "the drain stopped before the turn was over: {seen:?}"
        );
        assert_eq!(
            seen.last(),
            Some(&UiEvent::TurnEnded { failure: None }),
            "the drain ended on something other than the turn's conclusion"
        );
        assert!(
            worker.thread.is_none(),
            "a thread that finished inside the grace was left detached"
        );
    }

    #[test]
    fn the_drain_stops_at_the_first_terminal_event_and_not_at_the_last() {
        // The UI leaves on the turn's conclusion, so an event sent after one is
        // not something the drain waits for -- which is why the worker
        // publishes its session log *before* it sends one.
        let (events_tx, mut events_rx) = mpsc::channel::<UiEvent>(8);
        let (work_tx, _work_rx) = mpsc::channel::<TurnWork>(1);
        let (control_tx, _control_rx) = mpsc::unbounded_channel::<TurnControl>();
        events_tx
            .blocking_send(UiEvent::Delta("before".into()))
            .expect("room");
        events_tx
            .blocking_send(UiEvent::TurnEnded { failure: None })
            .expect("room");
        events_tx
            .blocking_send(UiEvent::Delta("after".into()))
            .expect("room");

        let mut worker = Worker {
            // No thread at all, which the join step has to survive: a `Worker`
            // whose runtime never started is the `spawn` failure path.
            thread: None,
            finished: Arc::new(AtomicBool::new(true)),
            cancel: Cancellation::new(CancelToken::new()),
            handle: WorkHandle {
                work: work_tx,
                control: control_tx,
                outstanding: Arc::new(AtomicUsize::new(0)),
                accepted: Arc::new(AtomicUsize::new(0)),
                reply: LastReply::default(),
                status: StatusBoard::unpublished(),
                permissions: mpsc::unbounded_channel().0,
                grants: Grants::default(),
            },
            resumable: None,
        };
        let mut seen = Vec::new();
        worker.shutdown(&mut events_rx, Instant::now() + TEST_DRAIN, |event| {
            seen.push(event)
        });

        assert_eq!(
            seen,
            vec![
                UiEvent::Delta("before".into()),
                UiEvent::TurnEnded { failure: None }
            ]
        );
    }

    #[test]
    fn a_drain_whose_runtime_is_already_gone_returns_at_once() {
        let (events_tx, mut events_rx) = mpsc::channel::<UiEvent>(1);
        let (work_tx, _work_rx) = mpsc::channel::<TurnWork>(1);
        let (control_tx, _control_rx) = mpsc::unbounded_channel::<TurnControl>();
        drop(events_tx);

        let mut worker = Worker {
            thread: None,
            finished: Arc::new(AtomicBool::new(true)),
            cancel: Cancellation::new(CancelToken::new()),
            handle: WorkHandle {
                work: work_tx,
                control: control_tx,
                outstanding: Arc::new(AtomicUsize::new(0)),
                accepted: Arc::new(AtomicUsize::new(0)),
                reply: LastReply::default(),
                status: StatusBoard::unpublished(),
                permissions: mpsc::unbounded_channel().0,
                grants: Grants::default(),
            },
            resumable: None,
        };
        let started = Instant::now();
        let mut seen = 0usize;
        // A deadline in the past would pass this by doing nothing at all, so it
        // is a live one: what is being measured is that a closed channel is
        // answered by the channel rather than by the clock.
        worker.shutdown(&mut events_rx, Instant::now() + TEST_DRAIN, |_| seen += 1);

        assert_eq!(seen, 0);
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "a closed channel was waited out on the deadline: {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn a_third_submission_is_refused_though_the_channel_is_empty() {
        // The defect this exists for: a `mpsc` permit is freed when the
        // receiver **takes** the item, which is where a turn begins. So for the
        // whole length of a turn the one-slot channel is empty, and a session
        // that asked the channel would keep saying yes -- accepting a third
        // prompt, a fourth, a tenth, saying nothing about any of them, and
        // running each one as a surprise when the last finished.
        //
        // The count is what says no. `WORK_LIMIT` of it is spent here: one turn
        // running, one prompt waiting where the band can say so.
        let (handle, mut work_rx, _control_rx) = WorkHandle::detached();
        handle
            .submit(TurnWork::Submit("first".into()))
            .expect("an idle runtime takes work");

        // The loop takes it. The turn has only just begun; the channel is
        // already empty.
        assert_eq!(
            work_rx.try_recv(),
            Ok(TurnWork::Submit("first".into())),
            "the loop could not take the work"
        );
        assert!(
            work_rx.try_recv().is_err(),
            "this case is only a case while the channel is empty"
        );

        handle
            .submit(TurnWork::Submit("second".into()))
            .expect("one prompt may wait behind the turn that is running");
        assert_eq!(
            handle.queued(),
            1,
            "the band would not have said `queued 1`"
        );

        assert_eq!(
            handle.submit(TurnWork::Submit("third".into())),
            Err(Rejected::Busy),
            "a third prompt was accepted with the queue already full"
        );
        assert_eq!(
            work_rx.try_recv(),
            Ok(TurnWork::Submit("second".into())),
            "the queued prompt is the one that was queued"
        );
        assert!(
            work_rx.try_recv().is_err(),
            "the refused prompt was queued anyway, and would run as a surprise"
        );
    }

    #[test]
    fn the_runtime_takes_work_again_once_the_work_it_was_given_is_over() {
        // The other side of the claim: a refusal that never lifted would be a
        // session that answers two prompts and then nothing.
        let (handle, mut work_rx, _control_rx) = WorkHandle::detached();
        handle
            .submit(TurnWork::Submit("first".into()))
            .expect("idle");
        let _taken = work_rx.try_recv().expect("the loop took it");
        handle
            .submit(TurnWork::Submit("second".into()))
            .expect("one may wait");
        let _queued = work_rx.try_recv().expect("the loop took the queued one");
        assert_eq!(
            handle.submit(TurnWork::Submit("third".into())),
            Err(Rejected::Busy)
        );

        // What `turn_loop` does after each terminal event.
        handle.outstanding.fetch_sub(1, Ordering::Release);

        handle
            .submit(TurnWork::Submit("third".into()))
            .expect("the queue has room again");
        assert_eq!(work_rx.try_recv(), Ok(TurnWork::Submit("third".into())));
    }

    #[test]
    fn what_the_band_says_is_the_number_the_refusal_was_decided_from() {
        // One count, read two ways, so the row on the screen and the rule that
        // refused cannot drift apart. A separate tally kept by the UI would be
        // right until the first turn that ended between a submission and a
        // frame.
        let (handle, mut work_rx, _control_rx) = WorkHandle::detached();
        assert_eq!(handle.outstanding(), 0);
        assert_eq!(handle.queued(), 0, "an idle session was holding a queue");

        handle
            .submit(TurnWork::Submit("first".into()))
            .expect("idle");
        assert_eq!(handle.outstanding(), 1);
        assert_eq!(
            handle.queued(),
            0,
            "the prompt being run was counted as one waiting"
        );

        let _taken = work_rx.try_recv().expect("the loop took it");
        handle
            .submit(TurnWork::Submit("second".into()))
            .expect("one may wait");
        assert_eq!(handle.outstanding(), WORK_LIMIT);
        assert_eq!(handle.queued(), 1);
    }

    #[test]
    fn a_refused_submission_does_not_leave_the_queue_looking_full_for_ever() {
        // The place is claimed before the send, so a send that fails has to
        // give it back -- or one refusal makes every later prompt a refusal too.
        let (handle, work_rx, _control_rx) = WorkHandle::detached();
        drop(work_rx);
        assert_eq!(
            handle.submit(TurnWork::Submit("first".into())),
            Err(Rejected::Gone)
        );
        assert_eq!(
            handle.outstanding(),
            0,
            "a submission that was never taken left a place claimed"
        );
    }

    #[test]
    fn both_of_two_submissions_are_taken_before_the_runtime_has_picked_up_either() {
        // The capacity claim, made in the window where a shallower channel gets
        // it wrong. Nothing reads this handle at all -- which is a runtime
        // blocked for ever, the strongest form of "before the pickup" there is
        // -- so if the channel held one slot the second submission would meet a
        // full one and be refused, while the same two keystrokes a millisecond
        // after a pickup would both be taken. A refusal a scheduler decides is
        // not a contract.
        let (handle, mut work_rx, _control_rx) = WorkHandle::detached();

        handle
            .submit(TurnWork::Submit("first".into()))
            .expect("an idle runtime takes work");
        handle
            .submit(TurnWork::Submit("second".into()))
            .expect("one prompt may wait, picked up or not");

        assert_eq!(handle.outstanding(), WORK_LIMIT);
        assert_eq!(handle.queued(), 1);
        assert_eq!(
            handle.submit(TurnWork::Submit("third".into())),
            Err(Rejected::Busy),
            "a third submission was taken with the queue already full"
        );

        // And both of the accepted ones are really there, in order: a count
        // that said yes to something the channel then dropped would be worse
        // than a refusal.
        assert_eq!(work_rx.try_recv(), Ok(TurnWork::Submit("first".into())));
        assert_eq!(work_rx.try_recv(), Ok(TurnWork::Submit("second".into())));
        assert!(work_rx.try_recv().is_err(), "the refused one was queued");
    }

    #[test]
    fn an_interrupt_drops_the_work_nobody_has_started_and_gives_its_places_back() {
        // The other half of a cancellation, and the half a "stop the running
        // turn" reading misses: an item still in the channel has not begun, so
        // after the interrupt it must not begin. A count left claimed for it
        // would also leave the band announcing a queue that no longer exists
        // and refusing the prompt the user types next.
        let (handle, mut work_rx, _control_rx) = WorkHandle::detached();
        handle
            .submit(TurnWork::Submit("first".into()))
            .expect("idle");
        handle
            .submit(TurnWork::Submit("second".into()))
            .expect("one may wait");

        let mut queue = Queue {
            work: &mut work_rx,
            outstanding: &handle.outstanding,
            taken: 0,
        };
        let dropped = queue.abandon(handle.accepted());

        assert_eq!(dropped, 2);
        assert_eq!(queue.taken, 2, "the drops were not counted as taken");
        assert!(
            queue.work.try_recv().is_err(),
            "a prompt survived the interrupt and would start by itself"
        );
        assert_eq!(
            handle.outstanding(),
            0,
            "a place claimed for work that will never run was never given back"
        );
        handle
            .submit(TurnWork::Submit("after".into()))
            .expect("the session takes work again once the queue is really empty");
    }

    #[test]
    fn an_interrupt_does_not_eat_the_prompt_the_user_typed_after_it() {
        // Reachable in the plainest way there is, and it cost a red run to
        // find: the UI paints the interrupt notice on its **own** thread, so a
        // user can read "stopping the turn" and type their next question long
        // before the runtime has looked at the control channel. A cancellation
        // that simply emptied the queue would swallow that question -- Ctrl-C
        // silently eating the prompt typed after it, which is the one the user
        // most certainly meant.
        let (handle, mut work_rx, _control_rx) = WorkHandle::detached();
        handle
            .submit(TurnWork::Submit("running".into()))
            .expect("idle");
        let _picked_up = work_rx.try_recv().expect("the turn began");

        // The keystroke. One submission had been made by then, and it is the
        // one already running.
        let through = handle.accepted();

        // ... and the user types again while the message is still in flight.
        handle
            .submit(TurnWork::Submit("asked after the interrupt".into()))
            .expect("the queue had room");

        let mut queue = Queue {
            work: &mut work_rx,
            outstanding: &handle.outstanding,
            taken: 1,
        };
        let dropped = queue.abandon(through);

        assert_eq!(dropped, 0, "the interrupt reached past the keystroke");
        assert_eq!(
            queue.work.try_recv(),
            Ok(TurnWork::Submit("asked after the interrupt".into())),
            "the prompt typed after the interrupt was eaten by it"
        );
    }

    #[test]
    fn the_work_a_turn_is_running_is_already_counted_and_is_not_dropped_twice() {
        // What `Queue::took` buys. The running turn's item is inside the
        // watermark the interrupt quotes, so a queue that had not counted its
        // own pickup would spend that allowance on the **next** item instead --
        // dropping a prompt the interrupt was never about.
        let (handle, mut work_rx, _control_rx) = WorkHandle::detached();
        handle
            .submit(TurnWork::Submit("running".into()))
            .expect("idle");
        let through = handle.accepted();
        handle
            .submit(TurnWork::Submit("queued after".into()))
            .expect("one may wait");

        let mut queue = Queue {
            work: &mut work_rx,
            outstanding: &handle.outstanding,
            taken: 0,
        };
        let running = queue.work.try_recv().expect("the turn began");
        let _running = queue.took(running);

        assert_eq!(queue.abandon(through), 0);
        assert_eq!(
            queue.work.try_recv(),
            Ok(TurnWork::Submit("queued after".into())),
            "the interrupt spent its allowance on a prompt it was not about"
        );
    }

    #[test]
    fn a_cancellation_mid_turn_drops_the_queue_at_the_moment_it_arrives() {
        // **At the moment it arrives**, not when the cancelled turn finishes
        // unwinding -- which for the turn most worth interrupting, the one whose
        // provider has gone quiet without hanging up, is never. The body below
        // is exactly that turn: it does not end when it is stopped, and the
        // queue still has to be empty by the time this returns.
        let (handle, mut work_rx, control_rx) = WorkHandle::detached();
        let control = ControlChannel::new(control_rx);
        handle
            .submit(TurnWork::Submit("running".into()))
            .expect("idle");
        let _picked_up = work_rx.try_recv().expect("the turn began");
        handle
            .submit(TurnWork::Submit("queued".into()))
            .expect("one may wait");
        let through = handle.accepted();
        handle.control(TurnControl::Cancel { through });

        let stopped = Arc::new(AtomicBool::new(false));
        let watched = Arc::clone(&stopped);
        // A turn that ignores its cancellation entirely, so nothing below can
        // pass because the body happened to finish.
        let body = async move {
            for _ in 0..4 {
                tokio::task::yield_now().await;
            }
            watched.load(Ordering::Acquire)
        };
        let counted = Arc::clone(&handle.outstanding);
        // One item has already been picked up, which is what the running turn
        // is; the drop must reach the one behind it and no further.
        let mut queue = Queue {
            work: &mut work_rx,
            outstanding: &counted,
            taken: 1,
        };
        let (was_stopped, ended) = on_a_runtime(raced_against_control(body, &control, |through| {
            stopped.store(true, Ordering::Release);
            queue.abandon(through);
        }));

        assert!(
            was_stopped,
            "the cancellation never reached the running turn"
        );
        assert_eq!(ended, Ended::Turn);
        assert_eq!(
            handle.outstanding(),
            1,
            "the queued prompt kept its place, so the band still says `queued 1`"
        );
    }

    #[test]
    fn a_submission_to_a_runtime_that_is_gone_is_not_reported_as_a_busy_one() {
        // The distinction is the whole of this test: telling the user "a turn
        // is already running" about a thread that is not running anything is a
        // refusal they cannot act on and cannot see through.
        let (work, work_rx) = mpsc::channel::<TurnWork>(1);
        let (control, _control_rx) = mpsc::unbounded_channel::<TurnControl>();
        let handle = WorkHandle {
            work,
            control,
            outstanding: Arc::new(AtomicUsize::new(0)),
            accepted: Arc::new(AtomicUsize::new(0)),
            reply: LastReply::default(),
            status: StatusBoard::unpublished(),
            permissions: mpsc::unbounded_channel().0,
            grants: Grants::default(),
        };
        drop(work_rx);
        assert_eq!(
            handle.submit(TurnWork::Submit("anything".into())),
            Err(Rejected::Gone)
        );
    }

    /// What a body that was really stopped answers.
    const STOPPED: &str = "the cancellation reached the body";

    /// What one that was not answers, having given up waiting for it.
    const NEVER_STOPPED: &str = "the body was never stopped";

    /// How many times the body below is polled before it gives up.
    ///
    /// **Bounded, and that is the point.** A body that waited forever would
    /// make a cancellation that never arrives a test that *hangs* rather than
    /// one that fails, and a hang is the one outcome a reader cannot tell from
    /// a slow machine. Far more polls than the one it takes for a message
    /// already in the channel to be seen.
    const GIVE_UP_AFTER: usize = 10_000;

    /// A body that finishes when it is stopped, and says so when it was not.
    ///
    /// The shape a real turn has: it is parked on something -- a socket, a full
    /// channel -- and the thing that ends it is the cancellation. A body that
    /// finished by itself would pass these cases whether or not the message
    /// ever arrived.
    fn stopped_or_not() -> (impl Future<Output = &'static str>, Arc<AtomicBool>) {
        let stopped = Arc::new(AtomicBool::new(false));
        let watched = Arc::clone(&stopped);
        let body = async move {
            for _ in 0..GIVE_UP_AFTER {
                if watched.load(Ordering::Acquire) {
                    return STOPPED;
                }
                tokio::task::yield_now().await;
            }
            NEVER_STOPPED
        };
        (body, stopped)
    }

    /// Runs `work` on a runtime of its own, as the worker thread does.
    fn on_a_runtime<T>(work: impl Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime")
            .block_on(work)
    }

    #[test]
    fn a_cancellation_reaches_a_turn_that_is_still_running() {
        // The claim the whole mid-turn race exists for. `turn_loop` reads
        // control *between* pieces of work, so a Ctrl-C answered only there
        // would sit in the queue until the turn it was meant to stop had ended
        // by itself -- which, for a provider that has gone quiet without
        // hanging up, may be never.
        let (body, stopped) = stopped_or_not();
        let (control_tx, control_rx) = mpsc::unbounded_channel::<TurnControl>();
        let control = ControlChannel::new(control_rx);
        control_tx
            .send(TurnControl::Cancel { through: 0 })
            .expect("the receiver is alive");

        let (output, ended) = on_a_runtime(raced_against_control(body, &control, |_through| {
            stopped.store(true, Ordering::Release)
        }));

        assert_eq!(
            output, STOPPED,
            "the turn ran on with a cancellation sitting in the channel"
        );
        assert_eq!(
            ended,
            Ended::Turn,
            "a cancelled turn ended the session as well"
        );
    }

    #[test]
    fn a_shutdown_mid_turn_stops_the_turn_and_the_loop_behind_it() {
        let (body, stopped) = stopped_or_not();
        let (control_tx, control_rx) = mpsc::unbounded_channel::<TurnControl>();
        let control = ControlChannel::new(control_rx);
        control_tx.send(TurnControl::Shutdown).expect("alive");

        let (output, ended) = on_a_runtime(raced_against_control(body, &control, |_through| {
            stopped.store(true, Ordering::Release)
        }));

        assert_eq!(output, STOPPED, "a shutdown left the turn running");
        assert_eq!(ended, Ended::Session);
    }

    // -----------------------------------------------------------------------
    // `/permissions` and Shift-Tab
    // -----------------------------------------------------------------------

    /// The profile `state` writes, as text, or nothing when it has none yet.
    fn profile_text(state: &Runtime) -> String {
        std::fs::read_to_string(state.config.user_settings_path.as_ref().expect("a profile"))
            .unwrap_or_default()
    }

    /// The session log of the conversation `state` holds, as text.
    fn log_text(state: &Runtime) -> String {
        let conversation = state.conversation.as_ref().expect("a conversation");
        std::fs::read_to_string(
            state
                .store
                .sessions_dir()
                .join(conversation.recorder.id().as_str())
                .join(crate::session::store::EVENTS_FILE),
        )
        .expect("the log")
    }

    #[test]
    fn a_mode_change_moves_the_conversation_the_configuration_the_log_and_the_profile() {
        let (_home, _workspace, mut state) = recording();
        assert_eq!(state.config.permission_mode, PermissionMode::Auto);
        assert_eq!(
            state.change_permissions(Change::Mode(PermissionMode::Yolo)),
            None
        );

        assert_eq!(state.config.permission_mode, PermissionMode::Yolo);
        let conversation = state.conversation.as_ref().expect("still open");
        assert_eq!(
            conversation.tools.permissions().mode(),
            PermissionMode::Yolo,
            "the conversation the user is talking in kept the mode it started with"
        );
        assert_eq!(
            conversation.recorder.state().permission_mode,
            PermissionMode::Yolo
        );
        assert!(log_text(&state).contains(r#""permission_mode":"yolo""#));
        assert_eq!(
            profile_text(&state),
            "{\n  \"permission_mode\": \"yolo\"\n}\n"
        );
        // A conversation opened after it is opened in it.
        assert_eq!(state.authority().mode(), PermissionMode::Yolo);

        // The same mode again changes nothing the log has to record.
        let before = log_text(&state);
        assert_eq!(
            state.change_permissions(Change::Mode(PermissionMode::Yolo)),
            None
        );
        assert_eq!(log_text(&state), before);
    }

    #[test]
    fn a_reset_is_ask_and_no_grants_and_says_so_to_the_board() {
        let (_home, _workspace, mut state) = recording();
        let grants = Grants::default();
        state.grants = grants.clone();
        state
            .conversation
            .as_ref()
            .expect("open")
            .tools
            .permissions()
            .grant(Grant::new(
                "write_file",
                state.config.workspace_root.join("a.txt").to_string_lossy(),
            ));
        state.publish();
        assert_eq!(grants.read(), vec![" - write_file -> a.txt".to_string()]);

        assert_eq!(state.change_permissions(Change::Reset), None);
        let conversation = state.conversation.as_ref().expect("still open");
        assert_eq!(conversation.tools.permissions().mode(), PermissionMode::Ask);
        assert!(conversation.tools.permissions().grants().is_empty());
        assert!(grants.read().is_empty(), "the board still lists the grant");
        assert_eq!(state.config.permission_mode, PermissionMode::Ask);
        assert!(profile_text(&state).contains(r#""permission_mode": "ask""#));
    }

    #[test]
    fn a_grant_is_listed_inert() {
        // A command a model wrote is not text the terminal may be handed.
        let (_home, _workspace, mut state) = recording();
        let grants = Grants::default();
        state.grants = grants.clone();
        state
            .conversation
            .as_ref()
            .expect("open")
            .tools
            .permissions()
            .grant(Grant::new("terminal", "printf '\u{1b}[2J' in /w"));
        state.publish();
        let listed = grants.read();
        assert_eq!(listed.len(), 1);
        assert!(!listed[0].contains('\u{1b}'), "{listed:?}");
        assert!(listed[0].starts_with(" - terminal -> printf"), "{listed:?}");
    }

    #[test]
    fn the_acknowledgement_is_recorded_beside_whatever_else_the_profile_says() {
        let (_home, _workspace, mut state) = recording();
        let path = state.config.user_settings_path.clone().expect("a profile");
        std::fs::write(
            &path,
            "{\"provider\":\"gateway\",\"permission_mode\":\"yolo\"}\n",
        )
        .expect("seed");
        assert_eq!(state.change_permissions(Change::Acknowledge), None);
        assert!(state.config.yolo_acknowledged);
        let written: serde_json::Value = serde_json::from_str(&profile_text(&state)).expect("json");
        assert_eq!(
            written,
            serde_json::json!({
                "provider": "gateway",
                "permission_mode": "yolo",
                "yolo_acknowledged": true,
            })
        );
        // And the next launch reads it back.
        let reloaded = RuntimeConfig::load_with(
            &Environment::new(Some(_home.path().to_path_buf()), BTreeMap::new()),
            _workspace.path(),
        )
        .expect("reload");
        assert!(reloaded.yolo_acknowledged);
        assert_eq!(reloaded.permission_mode, PermissionMode::Yolo);
    }

    #[test]
    fn a_write_that_fails_leaves_the_change_applied_and_says_so() {
        let (_home, _workspace, mut state) = recording();
        // A directory where the settings file belongs: unreadable as a file,
        // so the write is refused rather than made over it.
        let path = state.config.user_settings_path.clone().expect("a profile");
        std::fs::create_dir_all(&path).expect("a directory in the way");
        let said = state
            .change_permissions(Change::Mode(PermissionMode::Ask))
            .expect("a failure is said");
        assert_eq!(said.topic, "permission-mode");
        assert!(
            said.body
                .starts_with("active for this process but not saved to user settings ("),
            "{said:?}"
        );
        assert_eq!(state.config.permission_mode, PermissionMode::Ask);
        let said = state
            .change_permissions(Change::Acknowledge)
            .expect("a failure is said");
        assert_eq!(said.topic, "full-access-acknowledgment");
    }

    #[test]
    fn a_change_sent_after_work_was_queued_is_applied_before_that_work_starts() {
        // Upstream syncs a new mode into the prompts already queued
        // (`syncQueuedPermissionSnapshot`); here the loop takes changes before
        // work, so whatever was waiting starts under the mode the user chose
        // after it was typed. Observed through a queued `/rename`, which writes
        // its own line into the same log: the change's line has to come first.
        // (A queued `/model` used to be the witness; a model change is a
        // control message now, and takes no place in the queue.)
        let (_home, _workspace, state) = recording();
        let log = state
            .store
            .sessions_dir()
            .join(
                state
                    .conversation
                    .as_ref()
                    .expect("open")
                    .recorder
                    .id()
                    .as_str(),
            )
            .join(crate::session::store::EVENTS_FILE);
        let (work_tx, mut work_rx) = mpsc::channel::<TurnWork>(WORK_LIMIT);
        work_tx
            .try_send(TurnWork::Rename("queued-witness-title".into()))
            .expect("room");
        let (changes_tx, mut changes) = mpsc::unbounded_channel::<Change>();
        changes_tx
            .send(Change::Mode(PermissionMode::Ask))
            .expect("open");
        drop(work_tx);
        let (events, _seen) = mpsc::channel::<UiEvent>(bridge::UI_EVENTS);
        let (_control_tx, control_rx) = mpsc::unbounded_channel::<TurnControl>();
        let outstanding = AtomicUsize::new(1);
        on_a_runtime(turn_loop(
            state,
            &events,
            &mut work_rx,
            &mut changes,
            ControlChannel::new(control_rx),
            Cancellation::new(CancelToken::new()),
            &outstanding,
        ));
        let log = std::fs::read_to_string(log).expect("the log");
        let mode = log
            .find(r#""permission_mode":"ask""#)
            .unwrap_or_else(|| panic!("the change never reached the log: {log}"));
        let work = log
            .find("queued-witness-title")
            .unwrap_or_else(|| panic!("the work never ran: {log}"));
        assert!(mode < work, "the queued work ran before the change: {log}");
    }

    #[test]
    fn a_change_lands_while_the_loop_is_idle_and_one_typed_before_leaving_is_not_lost() {
        let (home, _workspace, state) = recording();
        let path = home.path().join(".xfx").join("settings.json");
        let (work_tx, mut work_rx) = mpsc::channel::<TurnWork>(WORK_LIMIT);
        let (changes_tx, mut changes) = mpsc::unbounded_channel::<Change>();
        let (events, _seen) = mpsc::channel::<UiEvent>(bridge::UI_EVENTS);
        let (control_tx, control_rx) = mpsc::unbounded_channel::<TurnControl>();
        let outstanding = AtomicUsize::new(0);
        let driver = async {
            // Nothing else is sent: only the change itself can wake the loop.
            changes_tx
                .send(Change::Mode(PermissionMode::Yolo))
                .expect("open");
            while !std::fs::read_to_string(&path)
                .unwrap_or_default()
                .contains("yolo")
            {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            // Typed, then straight out: applied on the way.
            changes_tx.send(Change::Acknowledge).expect("open");
            control_tx.send(TurnControl::Shutdown).expect("open");
        };
        let looped = turn_loop(
            state,
            &events,
            &mut work_rx,
            &mut changes,
            ControlChannel::new(control_rx),
            Cancellation::new(CancelToken::new()),
            &outstanding,
        );
        on_a_runtime(async {
            tokio::time::timeout(TEST_DRAIN, async { tokio::join!(looped, driver) })
                .await
                .expect("the loop never applied the change")
        });
        drop(work_tx);
        let written = std::fs::read_to_string(&path).expect("the profile");
        assert!(
            written.contains(r#""yolo_acknowledged": true"#),
            "{written}"
        );
    }

    #[test]
    fn a_provider_switch_keeps_the_mode_the_person_chose() {
        let (_home, _workspace, mut state) = recording();
        state.change_permissions(Change::Mode(PermissionMode::Ask));
        state.change_permissions(Change::Acknowledge);
        // A reload that says otherwise -- an environment override of the
        // mode, say -- does not take the session back to it.
        let mut reloaded = state.config.clone();
        reloaded.permission_mode = PermissionMode::Yolo;
        reloaded.yolo_acknowledged = false;
        state.adopt(reloaded);
        assert_eq!(state.config.permission_mode, PermissionMode::Ask);
        assert!(state.config.yolo_acknowledged);
    }

    // -----------------------------------------------------------------------
    // what `/status` reads
    // -----------------------------------------------------------------------

    /// A runtime holding an open conversation, publishing to a board the case
    /// can read -- the arrangement `spawn` makes between this thread and the
    /// UI's `WorkHandle`.
    fn published_runtime() -> (tempfile::TempDir, tempfile::TempDir, Runtime, StatusBoard) {
        let (home, workspace, state) = recording();
        let board = StatusBoard::unpublished();
        let state = state.publishing_to(board.clone());
        (home, workspace, state, board)
    }

    #[test]
    fn the_runtime_publishes_what_it_is_the_moment_it_starts_publishing() {
        let (_home, _workspace, state, board) = published_runtime();
        let seen = board.read();
        assert_eq!(seen.model, state.model());
        assert_eq!(seen.provider, state.config.provider.label());
        assert_eq!(
            seen.workspace,
            state.config.workspace_root.display().to_string()
        );
        assert_eq!(seen.history_turns, 0);
        assert_eq!(seen.session_permission_grants, 0);
        // And in the shape the notice body is, which is the shape `xfx status`
        // prints less its `[status] ` prefix.
        assert_eq!(
            seen.notice_body(),
            seen.render_text().replace("[status] ", "")
        );
    }

    #[test]
    fn a_turn_is_in_the_published_count_before_the_ui_is_told_it_ended() {
        // `history_turns` is the live conversation's (`session.historyLen()`
        // upstream), so a `/status` typed the moment the band sees the turn end
        // must already count it: the publication is ahead of `TurnEnded` on
        // this thread, and the UI reads the board only after it has the event.
        let (_home, _workspace, mut state, board) = published_runtime();
        state.provider = Some(Bundle {
            id: ProviderId::Gateway,
            stream: Box::new(ScriptedProvider::new(vec![answers("hello back")])),
        });
        let (events_tx, mut events_rx) = mpsc::channel(bridge::UI_EVENTS);
        let (_control_tx, control_rx) = mpsc::unbounded_channel();
        let control = ControlChannel::new(control_rx);
        let cancel = Cancellation::new(CancelToken::new());
        let (_work_tx, mut work_rx) = mpsc::channel::<TurnWork>(WORK_LIMIT);
        let outstanding = AtomicUsize::new(1);
        let mut queue = Queue {
            work: &mut work_rx,
            outstanding: &outstanding,
            taken: 0,
        };

        let ended = on_a_runtime(run_turn(
            &mut state,
            "hello".to_string(),
            &events_tx,
            &cancel,
            &control,
            &mut queue,
        ));

        assert_eq!(ended, Ended::Turn);
        let mut last = None;
        while let Ok(event) = events_rx.try_recv() {
            last = Some(event);
        }
        assert_eq!(last, Some(UiEvent::TurnEnded { failure: None }));
        assert_eq!(board.read().history_turns, 1);
    }

    #[test]
    fn a_grant_a_model_and_a_new_conversation_each_reach_what_status_reads() {
        let (_home, _workspace, mut state, board) = published_runtime();

        // An "always" is held by the authority from the moment it is given.
        state
            .conversation
            .as_ref()
            .expect("open")
            .tools
            .permissions()
            .grant(Grant::new("edit_file", "/w/notes.txt"));
        state.publish();
        assert_eq!(board.read().session_permission_grants, 1);

        // `/model`: the selector's model, not the configuration's.
        state
            .selector
            .set_catalog_for_test(CatalogState::Loaded(vec![published("second-model")]));
        let _said = ran_model(&mut state, "second-model", false);
        assert_eq!(board.read().model, "second-model");
        let (events_tx, _events_rx) = mpsc::channel(bridge::UI_EVENTS);
        assert_ne!(
            state.config.model, "second-model",
            "the case proves nothing"
        );

        // A reset (`/new`, `/clear`, `/reset`), through the loop that answers
        // it: the conversation goes, and with it both counts.
        let (work_tx, mut work_rx) = mpsc::channel::<TurnWork>(WORK_LIMIT);
        let (control_tx, control_rx) = mpsc::unbounded_channel();
        let outstanding = AtomicUsize::new(0);
        control_tx
            .send(TurnControl::Reset { through: 0 })
            .expect("alive");
        drop(work_tx);
        let (_changes_tx, mut changes) = mpsc::unbounded_channel::<Change>();
        on_a_runtime(turn_loop(
            state,
            &events_tx,
            &mut work_rx,
            &mut changes,
            ControlChannel::new(control_rx),
            Cancellation::new(CancelToken::new()),
            &outstanding,
        ));
        drop(control_tx);
        let after = board.read();
        assert_eq!(after.history_turns, 0);
        assert_eq!(after.session_permission_grants, 0);
        assert_eq!(after.model, "second-model", "/new changed the model");
    }

    #[test]
    fn a_ui_that_dropped_its_sender_is_a_shutdown_and_not_a_spin() {
        // A closed channel answers `recv` immediately, for ever. The branch has
        // to be disabled once it has, or this loop takes the channel's answer
        // every time it goes round and the body is never polled at all -- which
        // is a hang rather than a failure, so the bounded body below is the
        // proof.
        let (body, stopped) = stopped_or_not();
        let (control_tx, control_rx) = mpsc::unbounded_channel::<TurnControl>();
        let control = ControlChannel::new(control_rx);
        drop(control_tx);

        let (output, ended) = on_a_runtime(raced_against_control(body, &control, |_through| {
            stopped.store(true, Ordering::Release)
        }));

        // Deleting the flag **hangs** this case rather than failing it, and
        // that is worth being exact about: a `recv` on a closed channel is
        // ready immediately, so the biased first branch wins every time round
        // and the body is never polled -- a livelock with no yield point a
        // timeout could fire in. The hang is the signal; there is no assertion
        // that can be made from outside a loop that never comes back.
        assert_eq!(output, STOPPED);
        assert_eq!(ended, Ended::Session);
    }

    #[test]
    fn an_answer_to_a_question_nobody_asked_neither_stops_a_turn_nor_ends_it() {
        // The prompter takes its own answer off this channel while it is
        // parked (`super::approval`), so an `Answer` reaching the loop is a
        // second keystroke on a panel that has already gone. It is consumed
        // rather than left in the channel, and it does not end anything -- a
        // turn stopped by a message it did not understand would be worse than
        // one that ignored it.
        let stopped = Arc::new(AtomicBool::new(false));
        let watched = Arc::clone(&stopped);
        let (control_tx, control_rx) = mpsc::unbounded_channel::<TurnControl>();
        let control = ControlChannel::new(control_rx);
        control_tx
            .send(TurnControl::Answer {
                id: super::super::approval_readiness::ApprovalId(1),
                answer: crate::permission::ApprovalAnswer::Deny,
                feedback: None,
            })
            .expect("alive");
        // The body outlives the stray and then finishes by itself, which is what
        // makes "the stray changed nothing" observable.
        let body = async move { !watched.load(Ordering::Acquire) };

        let (untouched, ended) = on_a_runtime(raced_against_control(body, &control, |_through| {
            stopped.store(true, Ordering::Release)
        }));

        assert!(untouched, "a stray answer cancelled the turn");
        assert_eq!(ended, Ended::Turn);
        assert!(
            control.recv().now_or_never().is_none(),
            "the stray was left in the channel for the next turn to misread"
        );
    }

    /// How many times a parked turn is re-polled before the case gives up on it
    /// being parked.
    ///
    /// It bounds a **premise**, not the property: the turn below must be parked
    /// in its terminal send, and one poll that happened to return `Pending` for
    /// some other reason would satisfy a single check. Nothing else can move it
    /// -- this test owns the only receiver -- so any of these polls returning
    /// `Ready` is the defect.
    const PARKED_POLLS: usize = 8;

    #[test]
    fn a_concluded_turn_reaches_the_ui_through_a_channel_that_had_no_room_for_it() {
        // The third arm of the drain protocol, at the real `run_turn` rather
        // than at a second copy of `bridge::send_terminal`: the turn has
        // **already concluded** -- nothing is cancelled, nothing is streaming --
        // and its one terminal event cannot enter the `UiEvent` channel because
        // the event that opened the turn is still sitting in it. `send_terminal`
        // is deliberately not selected against cancellation, so what has to be
        // true is that the *drain* frees the permit and the conclusion arrives
        // on the very next poll, inside `DRAIN_DEADLINE`. A turn whose
        // conclusion needed the token, or a send that gave up on a full channel,
        // would leave the UI draining until its deadline and then reporting a
        // runtime that never said it was done.
        //
        // Deterministic by construction rather than by timing: this test owns
        // the only receiver, so the permit is freed exactly where it says, and
        // the future is polled by hand so "parked" is observed rather than
        // waited for. A pty cannot arrange it (`tests/tui.rs`'s slow-ui case
        // proves the *other* half, the deadline).
        let (_home, _workspace, mut state) = recording();
        // The premise, proven rather than assumed: this turn concludes without
        // opening a socket, because the sandbox has no credential for the
        // configured provider. `one_turn` therefore returns the refusal and
        // sends nothing of its own, so the only events in flight are the two
        // `run_turn` owes.
        assert!(
            Bundle::select(&state.config, &CancelToken::new()).is_err(),
            "the fixture would reach a real endpoint, so the conclusion below \
             would not be deterministic"
        );

        // One permit, which `TurnStarted` takes. The channel is then full for
        // exactly as long as nobody receives.
        let (events, mut seen) = mpsc::channel::<UiEvent>(1);
        let (_work_tx, mut work_rx) = mpsc::channel::<TurnWork>(1);
        let (_control_tx, control_rx) = mpsc::unbounded_channel::<TurnControl>();
        let control = ControlChannel::new(control_rx);
        let outstanding = AtomicUsize::new(1);
        let cancel = Cancellation::new(CancelToken::new());
        let mut queue = Queue {
            work: &mut work_rx,
            outstanding: &outstanding,
            taken: 1,
        };

        let started = Instant::now();
        let (ended, concluded) = on_a_runtime(async {
            let mut turn = std::pin::pin!(run_turn(
                &mut state,
                "anything".to_string(),
                &events,
                &cancel,
                &control,
                &mut queue,
            ));
            for poll in 0..PARKED_POLLS {
                let pending = std::future::poll_fn(|context| {
                    std::task::Poll::Ready(turn.as_mut().poll(context).is_pending())
                })
                .await;
                assert!(
                    pending,
                    "poll {poll}: the turn returned without its conclusion \
                     having anywhere to go"
                );
            }
            assert_eq!(
                events.capacity(),
                0,
                "the channel had room, so nothing was proven about a full one"
            );
            assert_eq!(
                seen.recv().await,
                Some(UiEvent::TurnStarted),
                "the event the drain frees a permit by taking"
            );

            // The drain has freed a permit. The conclusion is owed and nothing
            // else can produce it.
            let ended = turn.await;
            (ended, seen.try_recv())
        });

        assert_eq!(ended, Ended::Turn, "the session ended with the turn");
        let Ok(UiEvent::TurnEnded { failure: Some(why) }) = concluded else {
            panic!("the concluded turn's terminal event never arrived: {concluded:?}");
        };
        assert!(
            !why.is_empty(),
            "the conclusion arrived without saying why the turn failed"
        );
        assert!(
            started.elapsed() < DRAIN_DEADLINE,
            "the conclusion outlived the deadline the UI drains under: {:?}",
            started.elapsed()
        );
    }

    /// A turn that answered `text`, written into `state`'s conversation the
    /// way a real one is: the user's message, then the assistant step.
    fn answered(state: &mut Runtime, text: &str) {
        let recorder = &mut state
            .conversation
            .as_mut()
            .expect("the fixture opened a conversation")
            .recorder;
        recorder.commit(SessionEvent::UserMessage {
            text: "asked".to_string(),
        });
        recorder.commit(SessionEvent::AssistantMessage {
            text: text.to_string(),
            tool_calls: Vec::new(),
            raw_content: Vec::new(),
            responses_state: Vec::new(),
            wire: None,
        });
        assert_eq!(recorder.failure(), None, "the log refused the turn");
    }

    #[test]
    fn a_turn_publishes_the_reply_before_its_conclusion_is_sent() {
        // A `/copy` typed the moment the UI shows a turn ending must copy that
        // turn's reply. So the reply is published before `TurnEnded` can
        // enter the channel -- observed here with the conclusion parked on a
        // full channel, which is a moment the UI has not yet been told.
        let (_home, _workspace, mut state) = recording();
        assert!(
            Bundle::select(&state.config, &CancelToken::new()).is_err(),
            "the fixture would reach a real endpoint"
        );
        let reply = LastReply::default();
        state.reply = reply.clone();
        answered(&mut state, "the reply,\twith its tab");
        assert_eq!(reply.get(), None, "published before any work ran");

        let (events, mut seen) = mpsc::channel::<UiEvent>(1);
        let (_work_tx, mut work_rx) = mpsc::channel::<TurnWork>(1);
        let (_control_tx, control_rx) = mpsc::unbounded_channel::<TurnControl>();
        let control = ControlChannel::new(control_rx);
        let outstanding = AtomicUsize::new(1);
        let cancel = Cancellation::new(CancelToken::new());
        let mut queue = Queue {
            work: &mut work_rx,
            outstanding: &outstanding,
            taken: 1,
        };
        let (published, concluded) = on_a_runtime(async {
            let mut turn = std::pin::pin!(run_turn(
                &mut state,
                "anything".to_string(),
                &events,
                &cancel,
                &control,
                &mut queue,
            ));
            for poll in 0..PARKED_POLLS {
                let pending = std::future::poll_fn(|context| {
                    std::task::Poll::Ready(turn.as_mut().poll(context).is_pending())
                })
                .await;
                assert!(pending, "poll {poll}: the conclusion was not parked");
            }
            // `TurnStarted` holds the one permit: the conclusion is not sent.
            let published = reply.get();
            assert_eq!(seen.recv().await, Some(UiEvent::TurnStarted));
            turn.await;
            (published, seen.try_recv())
        });
        assert_eq!(published.as_deref(), Some("the reply,\twith its tab"));
        assert!(
            matches!(concluded, Ok(UiEvent::TurnEnded { .. })),
            "{concluded:?}"
        );
    }

    #[test]
    fn a_reset_that_drops_the_conversation_takes_its_reply_with_it() {
        // A reset drops the conversation outside the work arms the loop
        // republishes after, so the drop republishes for itself -- and before
        // the fence, so a `/copy` typed on the fresh screen finds nothing.
        // Both ways a reset reaches the drop: between turns, and as the way a
        // piece of work ended.
        for between_turns in [true, false] {
            let (_home, _workspace, mut state) = recording();
            let reply = LastReply::default();
            state.reply = reply.clone();
            answered(&mut state, "an answer");
            state.publish_reply();
            assert_eq!(reply.get().as_deref(), Some("an answer"));

            let (work_tx, mut work_rx) = mpsc::channel::<TurnWork>(WORK_LIMIT);
            let (events, mut seen) = mpsc::channel::<UiEvent>(bridge::UI_EVENTS);
            let (control_tx, control_rx) = mpsc::unbounded_channel::<TurnControl>();
            let outstanding = AtomicUsize::new(0);
            if between_turns {
                control_tx
                    .send(TurnControl::Reset { through: 0 })
                    .expect("alive");
                control_tx.send(TurnControl::Shutdown).expect("alive");
                let (_changes_tx, mut changes) = mpsc::unbounded_channel::<Change>();
                on_a_runtime(turn_loop(
                    state,
                    &events,
                    &mut work_rx,
                    &mut changes,
                    ControlChannel::new(control_rx),
                    Cancellation::new(CancelToken::new()),
                    &outstanding,
                ));
            } else {
                on_a_runtime(async {
                    drop_conversation(&mut state, &events, 4).await;
                });
            }
            drop(work_tx);
            assert_eq!(
                reply.get(),
                None,
                "a reset left the old conversation's reply"
            );
            let through = if between_turns { 0 } else { 4 };
            assert_eq!(seen.try_recv(), Ok(UiEvent::Fresh { through }));
        }
    }

    #[test]
    fn the_handle_reads_the_reply_the_runtime_published() {
        let (handle, _work, _control) = WorkHandle::detached();
        assert_eq!(handle.last_reply(), None);
        handle.reply.publish(Some("first"));
        handle.reply.publish(Some("second"));
        assert_eq!(handle.last_reply().as_deref(), Some("second"));
        handle.reply.publish(None);
        assert_eq!(handle.last_reply(), None);
    }

    #[test]
    fn the_fatal_sent_before_the_runtime_exists_cannot_command_the_terminal() {
        // The one send that does not go through `bridge::send_ui` or
        // `send_terminal`, and therefore the one that has to strip for itself.
        let (events, mut rx) = mpsc::channel::<UiEvent>(1);
        fatal_before_the_runtime(&events, "a runtime \u{1b}[2J that clears the screen");
        assert_eq!(
            rx.blocking_recv(),
            Some(UiEvent::Fatal(
                "a runtime  [2J that clears the screen".to_string()
            ))
        );
    }

    #[test]
    fn the_no_runtime_notice_is_the_crate_s_own_text() {
        // `fatal_before_the_runtime` takes a `&'static str` so that only text
        // this crate wrote can be spelled at that call site. This is the other
        // half of that claim: the one literal it is called with says something,
        // and says it without asking the terminal for anything.
        assert!(!NO_RUNTIME.is_empty());
        assert!(!NO_RUNTIME.chars().any(char::is_control));
    }

    #[test]
    fn a_panic_that_carried_text_is_reported_in_the_words_it_panicked_with() {
        let payload = std::panic::catch_unwind(|| panic!("a turn panicked"))
            .expect_err("the closure panicked");
        assert_eq!(panic_message(&*payload), "a turn panicked");

        let owned = std::panic::catch_unwind(|| panic!("{}", String::from("owned words")))
            .expect_err("the closure panicked");
        assert_eq!(panic_message(&*owned), "owned words");
    }

    #[test]
    fn a_panic_that_carried_something_else_is_still_reported() {
        let payload = std::panic::catch_unwind(|| std::panic::panic_any(7u32))
            .expect_err("the closure panicked");
        assert_eq!(panic_message(&*payload), UNKNOWN_PANIC);
    }

    #[test]
    fn a_session_with_nowhere_to_record_is_refused_before_a_thread_is_started() {
        let workspace = tempfile::tempdir().expect("a workspace");
        let mut config = config(workspace.path(), workspace.path());
        config.profile_dir = None;
        let err = open_store(&config).expect_err("a store with no home was opened anyway");
        assert!(
            err.to_string().contains("cannot record"),
            "the refusal does not say why: {err}"
        );
    }

    // -----------------------------------------------------------------------
    // the session `/quit` names, and the one `--resume` continues
    // -----------------------------------------------------------------------

    /// The sessions a store holds, by directory.
    fn sessions_in(home: &std::path::Path) -> Vec<String> {
        let dir = home.join(".xfx").join("sessions");
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        entries
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn a_runtime_that_was_asked_nothing_names_no_session_and_wrote_none() {
        // Upstream keeps no pristine session and hands off none
        // (`app_session_runtime.zig:350-354`, `:5702-5720`). xfx creates its
        // session at the first prompt (`Runtime::ready`), so the rule holds
        // by there being nothing to delete -- which is what this pins.
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let config = config(home.path(), workspace.path());
        let store = open_store(&config).expect("a store");
        let state = Runtime::new(config, store, a_prompter(), a_questioner());
        assert_eq!(state.resumable(), None);
        assert_eq!(sessions_in(home.path()), Vec::<String>::new());
    }

    #[test]
    fn the_session_named_is_the_conversations_once_it_holds_a_turn() {
        let (home, _workspace, mut state) = recording();
        // Open, but asked nothing yet: no history, nothing to come back to.
        assert_eq!(state.resumable(), None);
        let conversation = state.conversation.as_mut().expect("open");
        conversation.recorder.commit(SessionEvent::UserMessage {
            text: "first question".to_string(),
        });
        let id = conversation.recorder.id().clone();
        assert_eq!(state.resumable(), Some(id.clone()));
        assert_eq!(sessions_in(home.path()), vec![id.to_string()]);
        // A conversation a reset dropped (`turn_loop`'s `drop_conversation`)
        // is not the one the next prompt goes to, so it is not named.
        state.conversation = None;
        assert_eq!(state.resumable(), None);
    }

    /// A session with one turn and one grant in its log, closed again, in the
    /// store `config` records into.
    fn a_saved_session(config: &RuntimeConfig, grant: &Grant) -> SessionId {
        let store = open_store(config).expect("a store");
        let mut conversation = open_conversation(
            &store,
            config,
            &config.model,
            PermissionSession::new(config.permission_mode),
            None,
            &CancelToken::new(),
        )
        .expect("open a conversation");
        conversation.recorder.commit(SessionEvent::UserMessage {
            text: "first question alpha".to_string(),
        });
        conversation
            .recorder
            .record_new_grants(std::slice::from_ref(grant));
        conversation.recorder.id().clone()
        // Dropped here, which closes the log and releases its lock.
    }

    #[test]
    fn a_resumed_session_is_the_conversation_from_the_start_with_its_grants() {
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let config = config(home.path(), workspace.path());
        let grant = Grant::new(
            "edit_file",
            workspace
                .path()
                .join("notes.txt")
                .to_string_lossy()
                .into_owned(),
        );
        let id = a_saved_session(&config, &grant);

        let resumed = Resumption::open(&config, &crate::session::Selector::Id(id.clone()))
            .expect("the saved session opens");
        assert_eq!(
            resumed.hydration().title.as_deref(),
            Some("first question alpha")
        );
        let store = open_store(&config).expect("a store");
        let mut state = Runtime::new(config, store, a_prompter(), a_questioner());
        state.resume(resumed, &CancelToken::new());

        // The conversation before any prompt, and it is the saved one: the
        // next turn replays its history from this log (`one_turn`).
        let conversation = state.conversation.as_ref().expect("bound at once");
        assert_eq!(conversation.recorder.id(), &id);
        assert_eq!(
            conversation.recorder.state().turns.len(),
            1,
            "the saved turn is not in the conversation the next prompt continues"
        );
        // An "always" answered last time is not asked again
        // (`app::run_ask` restores the same list).
        assert!(
            conversation.tools.permissions().grants().contains(&grant),
            "the saved grant was not restored"
        );
        // A quit before any prompt still names it: it is not pristine.
        assert_eq!(state.resumable(), Some(id.clone()));
        // And no second session was made for it.
        assert_eq!(sessions_in(home.path()), vec![id.to_string()]);
    }

    #[test]
    fn a_rename_before_any_prompt_opens_the_session_and_keeps_it_titled() {
        // Upstream's session exists from launch, and one renamed and never
        // asked anything survives the exit (`discardableOnClose`,
        // `app_session_runtime.zig:5702-5720`) -- with no resume line, since a
        // titled session with no turn is still pristine (`:350-354`).
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let config = config(home.path(), workspace.path());
        let store = open_store(&config).expect("a store");
        let mut state = Runtime::new(config.clone(), store, a_prompter(), a_questioner());
        state
            .rename("My Title", &CancelToken::new())
            .expect("the rename is written");
        let id = state
            .conversation
            .as_ref()
            .expect("opened for the rename")
            .recorder
            .id()
            .clone();
        assert_eq!(state.resumable(), None, "a session with no turn was named");
        state
            .rename("Second name", &CancelToken::new())
            .expect("the second rename is written");
        drop(state);
        assert_eq!(sessions_in(home.path()), vec![id.to_string()]);
        // On disk, published, with the last title.
        let store = open_store(&config).expect("a store");
        let listed = store
            .list(&crate::session::ListFilter::new(
                crate::session::ListScope::AllWorkspaces,
            ))
            .expect("a listing");
        assert_eq!(listed.sessions[0].title.as_deref(), Some("Second name"));
        assert_eq!(listed.sessions[0].history_turns, 0);
    }

    #[cfg(unix)]
    #[test]
    fn a_rename_the_store_refuses_says_why_and_the_session_keeps_its_old_title() {
        use std::os::unix::fs::PermissionsExt;
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let config = config(home.path(), workspace.path());
        let store = open_store(&config).expect("a store");
        let mut state = Runtime::new(config.clone(), store, a_prompter(), a_questioner());
        state
            .rename("My Title", &CancelToken::new())
            .expect("the first rename is written");
        // The manifest is replaced by a rename in the session's directory, so
        // a directory nobody may write in refuses the next one.
        let dir = state
            .conversation
            .as_ref()
            .expect("open")
            .recorder
            .state()
            .id
            .clone();
        let dir = home.path().join(".xfx").join("sessions").join(dir);
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o500)).expect("chmod");
        let refused = state.rename("Second name", &CancelToken::new());
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).expect("chmod");
        let problem = refused.expect_err("a rename the store refused was reported as written");
        assert!(problem.contains("session_renamed"), "{problem}");
        // And it stays refused: the recorder writes nothing after a failure.
        assert!(state.rename("Third", &CancelToken::new()).is_err());
        drop(state);
        let listed = open_store(&config)
            .expect("a store")
            .list(&crate::session::ListFilter::new(
                crate::session::ListScope::AllWorkspaces,
            ))
            .expect("a listing");
        assert_eq!(listed.sessions[0].title.as_deref(), Some("My Title"));
    }

    #[test]
    fn a_listing_leaves_out_the_session_this_one_is_in_and_the_ones_with_no_turn() {
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let config = config(home.path(), workspace.path());
        let grant = Grant::new("edit_file", "notes.txt".to_string());
        let saved = a_saved_session(&config, &grant);
        let store = open_store(&config).expect("a store");
        let mut state = Runtime::new(config.clone(), store, a_prompter(), a_questioner());
        // This runtime's own session: renamed, so on disk, but empty.
        state
            .rename("Only a title", &CancelToken::new())
            .expect("renamed");
        let listing = state.sessions().expect("the store reads");
        let ids: Vec<&str> = listing.current.iter().map(|row| row.id.as_str()).collect();
        assert_eq!(ids, vec![saved.as_str()]);
        assert_eq!(listing.all, listing.current);
        assert_eq!(
            listing.current[0].title.as_deref(),
            Some("first question alpha")
        );
        assert_eq!(listing.current[0].turns, 1);
        // Once it holds a turn it is listed -- by another runtime, not this one.
        let conversation = state.conversation.as_mut().expect("open");
        conversation.recorder.commit(SessionEvent::UserMessage {
            text: "now asked".to_string(),
        });
        let mine = conversation.recorder.id().clone();
        assert!(state
            .sessions()
            .expect("reads")
            .current
            .iter()
            .all(|row| row.id != mine.as_str()));
    }

    #[test]
    fn resuming_from_the_picker_swaps_the_conversation_and_refuses_a_held_one_at_once() {
        use super::super::session_picker::ResumeFailure;
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let config = config(home.path(), workspace.path());
        let grant = Grant::new("edit_file", "notes.txt".to_string());
        let saved = a_saved_session(&config, &grant);
        let store = open_store(&config).expect("a store");
        let mut state = Runtime::new(config.clone(), store, a_prompter(), a_questioner());
        // The conversation it is in now has a turn, so it is kept.
        state
            .conversation(&CancelToken::new())
            .expect("opened")
            .recorder
            .commit(SessionEvent::UserMessage {
                text: "the one before".to_string(),
            });
        let before = state.resumable().expect("a turn");

        // Held by another writer: refused at once, and nothing changed.
        let held = open_store(&config)
            .expect("a store")
            .resume(
                &crate::session::Selector::Id(saved.clone()),
                workspace.path(),
            )
            .expect("the other writer opens it");
        let started = Instant::now();
        assert_eq!(
            state.resume_by_id(saved.as_str(), &CancelToken::new()),
            Err(ResumeFailure::OpenElsewhere)
        );
        assert!(
            started.elapsed() < Duration::from_millis(200),
            "the picker waited on the lock"
        );
        assert_eq!(state.resumable(), Some(before.clone()));
        drop(held);

        // A name that is not a session is the other failure.
        assert_eq!(
            state.resume_by_id("not a session id!", &CancelToken::new()),
            Err(ResumeFailure::Unavailable)
        );

        let replay = state
            .resume_by_id(saved.as_str(), &CancelToken::new())
            .expect("resumed");
        assert_eq!(replay.title.as_deref(), Some("first question alpha"));
        assert_eq!(replay.turns.len(), 1);
        assert_eq!(replay.model.as_deref(), Some(config.model.as_str()));
        assert_eq!(state.resumable(), Some(saved.clone()));
        // Both sessions are on disk; the one before was closed, not deleted.
        let mut on_disk = sessions_in(home.path());
        on_disk.sort();
        let mut expected = vec![before.to_string(), saved.to_string()];
        expected.sort();
        assert_eq!(on_disk, expected);
        // And the one this runtime left is the picker's to list now.
        let listing = state.sessions().expect("reads");
        assert_eq!(
            listing
                .current
                .iter()
                .map(|row| row.id.clone())
                .collect::<Vec<_>>(),
            vec![before.to_string()]
        );
    }

    #[test]
    fn a_resume_under_another_model_writes_down_the_one_it_runs_under() {
        // `app::run_ask`'s step: a later resume reads what this one used.
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let saved_under = config(home.path(), workspace.path());
        let grant = Grant::new("edit_file", "notes.txt".to_string());
        let id = a_saved_session(&saved_under, &grant);

        let mut config = saved_under.clone();
        config.model = "another-model".to_string();
        let resumed = Resumption::open(&config, &crate::session::Selector::Id(id))
            .expect("the saved session opens");
        assert_eq!(resumed.model(), saved_under.model);
        let store = open_store(&config).expect("a store");
        let mut state = Runtime::new(config, store, a_prompter(), a_questioner());
        state.resume(resumed, &CancelToken::new());
        assert_eq!(
            state
                .conversation
                .as_ref()
                .expect("bound")
                .recorder
                .state()
                .model,
            "another-model"
        );
    }

    #[test]
    fn a_resume_that_names_nothing_is_refused_before_anything_is_taken() {
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let config = config(home.path(), workspace.path());
        for selector in [
            crate::session::Selector::Last,
            crate::session::Selector::Id(SessionId::parse("no-such-session").expect("an id")),
        ] {
            let Err(err) = Resumption::open(&config, &selector) else {
                panic!("{selector:?} opened a session that does not exist");
            };
            assert!(!err.to_string().is_empty(), "{selector:?}");
        }
        assert_eq!(sessions_in(home.path()), Vec::<String>::new());
    }

    #[test]
    fn the_join_hands_back_the_session_the_runtime_ended_in() {
        // The thread is the only thing that knows which conversation it held
        // when it stopped, so the answer travels back through the join.
        let (events_tx, mut events_rx) = mpsc::channel::<UiEvent>(1);
        let (work_tx, _work_rx) = mpsc::channel::<TurnWork>(1);
        let (control_tx, _control_rx) = mpsc::unbounded_channel::<TurnControl>();
        let finished = Arc::new(AtomicBool::new(false));
        let done = Arc::clone(&finished);
        let id = SessionId::parse("1759800000000-0123456789abcdef").expect("an id");
        let named = id.clone();
        let thread = std::thread::spawn(move || {
            let _done = Finished(done);
            drop(events_tx);
            Some(named)
        });
        let mut worker = Worker {
            thread: Some(thread),
            finished,
            cancel: Cancellation::new(CancelToken::new()),
            handle: WorkHandle {
                work: work_tx,
                control: control_tx,
                outstanding: Arc::new(AtomicUsize::new(0)),
                accepted: Arc::new(AtomicUsize::new(0)),
                reply: LastReply::default(),
                status: StatusBoard::unpublished(),
                permissions: mpsc::unbounded_channel().0,
                grants: Grants::default(),
            },
            resumable: None,
        };
        assert_eq!(
            worker.resumable(),
            None,
            "named before the thread was joined"
        );
        worker.shutdown(&mut events_rx, Instant::now() + TEST_DRAIN, |_| {});
        assert!(worker.thread.is_none(), "the thread was not joined");
        assert_eq!(worker.resumable(), Some(&id));
    }

    /// A `Runtime` with a real store and a real conversation open in it, which
    /// is what the durable half of `/model` needs to be observable at all.
    /// A prompter with nothing on either end of it.
    ///
    /// The fixtures below never run a turn, so nothing here ever asks anybody
    /// anything. What matters is that the authority a turn *would* run under is
    /// built the way production builds it, rather than the way a fixture found
    /// convenient.
    fn a_prompter() -> TuiPrompter {
        let (events, _seen) = mpsc::channel(1);
        let (_control_tx, control_rx) = mpsc::unbounded_channel();
        TuiPrompter::new(
            events,
            ControlChannel::new(control_rx),
            Cancellation::new(CancelToken::new()),
        )
    }

    /// The same, for the requester the model's questions go through. Nothing on
    /// either end of it either: the fixtures that use it never run a turn, and
    /// what they are about is that a conversation is *given* one.
    fn a_questioner() -> Arc<TuiQuestioner> {
        let (events, _seen) = mpsc::channel(1);
        let (_control_tx, control_rx) = mpsc::unbounded_channel();
        Arc::new(TuiQuestioner::new(
            events,
            ControlChannel::new(control_rx),
            Cancellation::new(CancelToken::new()),
        ))
    }

    /// A `Runtime` with an open conversation, held in the model the
    /// configuration resolves to.
    ///
    /// The model is **not** a parameter: the selector owns it, and a fixture
    /// that named a different one would be building a session production
    /// cannot produce -- `spawn` constructs the selector from the same
    /// configuration the conversation is opened against.
    fn recording() -> (tempfile::TempDir, tempfile::TempDir, Runtime) {
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let config = config(home.path(), workspace.path());
        let store = open_store(&config).expect("a store");
        let conversation = open_conversation(
            &store,
            &config,
            &config.model.clone(),
            PermissionSession::new(config.permission_mode),
            None, // this fixture does not exercise the question path
            &CancelToken::new(),
        )
        .expect("open a conversation");
        let mut state = Runtime::new(config, store, a_prompter(), a_questioner());
        state.conversation = Some(conversation);
        (home, workspace, state)
    }

    /// One catalog row, published under `id`.
    fn published(id: &str) -> CatalogEntry {
        CatalogEntry {
            id: id.to_string(),
            aliases: Vec::new(),
            name: None,
            efforts: Vec::new(),
            max_context: None,
            facts: Default::default(),
        }
    }

    #[test]
    fn a_reset_between_turns_drops_what_waits_and_then_says_the_conversation_is_gone() {
        // The fence a reset drains to (`UiEvent::Fresh`): sent once the drop
        // has happened, quoting the reset's count. What was submitted through
        // that count and not yet started is dropped with its place given back;
        // what was submitted after it is a new intention and stays.
        let (handle, mut work_rx, _unused) = WorkHandle::detached();
        handle
            .submit(TurnWork::Submit("dropped".into()))
            .expect("idle");
        let through = handle.accepted();
        handle
            .submit(TurnWork::Submit("kept".into()))
            .expect("one may wait");
        let mut queue = Queue {
            work: &mut work_rx,
            outstanding: &handle.outstanding,
            taken: 0,
        };
        assert_eq!(queue.abandon(through), 1);
        assert_eq!(handle.outstanding(), 1);
        assert_eq!(
            queue.work.try_recv(),
            Ok(TurnWork::Submit("kept".into())),
            "a submission past the reset's count was dropped with it"
        );

        let (_home, _workspace, state) = recording();
        assert!(state.conversation.is_some(), "nothing to drop");
        let (events_tx, mut events_rx) = mpsc::channel(bridge::UI_EVENTS);
        let (work_tx, mut work_rx) = mpsc::channel::<TurnWork>(WORK_LIMIT);
        let (control_tx, control_rx) = mpsc::unbounded_channel();
        control_tx
            .send(TurnControl::Reset { through: 0 })
            .expect("alive");
        control_tx.send(TurnControl::Shutdown).expect("alive");
        let outstanding = AtomicUsize::new(0);
        let (_changes_tx, mut changes) = mpsc::unbounded_channel::<Change>();
        on_a_runtime(turn_loop(
            state,
            &events_tx,
            &mut work_rx,
            &mut changes,
            ControlChannel::new(control_rx),
            Cancellation::new(CancelToken::new()),
            &outstanding,
        ));
        drop(work_tx);
        assert_eq!(events_rx.try_recv(), Ok(UiEvent::Fresh { through: 0 }));
        assert!(
            events_rx.try_recv().is_err(),
            "the reset sent more than its fence"
        );
    }

    #[test]
    fn a_reset_reaches_work_that_is_still_running_and_names_the_last_count() {
        // The reset's stop is a Ctrl-C's -- the turn and what waits behind it
        // -- and what ended carries the count of the latest reset that reached
        // it, so one fence answers both when two arrive in one turn. A
        // shutdown outranks it.
        let (control_tx, control_rx) = mpsc::unbounded_channel();
        let control = ControlChannel::new(control_rx);
        control_tx
            .send(TurnControl::Reset { through: 1 })
            .expect("alive");
        control_tx
            .send(TurnControl::Reset { through: 3 })
            .expect("alive");
        let mut stops = Vec::new();
        let (_, ended) = on_a_runtime(raced_against_control(
            async {
                tokio::time::sleep(Duration::from_millis(50)).await;
            },
            &control,
            |through| stops.push(through),
        ));
        assert_eq!(stops, vec![1, 3]);
        assert_eq!(ended, Ended::Reset(3));

        let (control_tx, control_rx) = mpsc::unbounded_channel();
        let control = ControlChannel::new(control_rx);
        control_tx
            .send(TurnControl::Reset { through: 1 })
            .expect("alive");
        control_tx.send(TurnControl::Shutdown).expect("alive");
        let (_, ended) = on_a_runtime(raced_against_control(
            async {
                tokio::time::sleep(Duration::from_millis(50)).await;
            },
            &control,
            |_| {},
        ));
        assert_eq!(ended, Ended::Session);
    }

    #[test]
    fn a_model_change_is_recorded_where_a_resumed_session_will_read_it() {
        let (_home, _workspace, mut state) = recording();
        let first = state.model().to_string();
        assert_eq!(
            state
                .conversation
                .as_ref()
                .expect("open")
                .recorder
                .state()
                .model,
            first
        );
        // Published, so the catalog is not the thing being measured here.
        state
            .selector
            .set_catalog_for_test(CatalogState::Loaded(vec![published("second-model")]));

        let outcome = state.use_model("second-model");

        assert!(
            matches!(&outcome, ModelOutcome::Selected { model, previous, .. }
                if model == "second-model" && previous == &first),
            "{outcome:?}"
        );
        // The next turn talks to it ...
        assert_eq!(state.model(), "second-model");
        // ... and so does the next `xfx ask --resume-id <id>`, which is the
        // half that a field alone would not give.
        assert_eq!(
            state
                .conversation
                .as_ref()
                .expect("open")
                .recorder
                .state()
                .model,
            "second-model"
        );
    }

    #[test]
    fn the_same_model_again_records_nothing() {
        let (_home, _workspace, mut state) = recording();
        let same = state.model().to_string();
        let before = state
            .conversation
            .as_ref()
            .expect("open")
            .recorder
            .state()
            .last_event_seq;

        let outcome = state.use_model(&same);

        assert!(matches!(&outcome, ModelOutcome::Unchanged { model } if model == &same));
        assert_eq!(state.model(), same);
        assert_eq!(
            state
                .conversation
                .as_ref()
                .expect("open")
                .recorder
                .state()
                .last_event_seq,
            before,
            "a model that did not change was written to the log anyway"
        );
    }

    #[test]
    fn a_model_a_loaded_catalog_does_not_list_is_applied_and_recorded_as_typed() {
        // Used to be refused here. Upstream applies it
        // (`fx-model-fuzzy-40x120-unknown`): a catalog is what a provider
        // lists, and the provider is the one place that can say no. So the id
        // becomes the model the next turn talks to and is written into the log
        // a resume reads, as any selection is.
        let (_home, _workspace, mut state) = recording();
        state
            .selector
            .set_catalog_for_test(CatalogState::Loaded(vec![published("published-model")]));

        // Nothing of it scores against the one id listed, so it is the id.
        let applied = state.use_model("unlisted-xyz");

        assert!(
            matches!(&applied, ModelOutcome::Selected { model, unverified: None, .. }
                if model == "unlisted-xyz"),
            "{applied:?}"
        );
        assert_eq!(state.model(), "unlisted-xyz");
        assert_eq!(
            state
                .conversation
                .as_ref()
                .expect("open")
                .recorder
                .state()
                .model,
            "unlisted-xyz"
        );
        // And a query the catalog does answer is resolved to its id.
        assert!(matches!(
            state.use_model("published"),
            ModelOutcome::Selected { model, .. } if model == "published-model"
        ));
    }

    /// Runs one `/model` the way the loop runs it -- a query typed with nothing
    /// resolved on the band -- and hands back what it told the UI.
    fn ran_model(state: &mut Runtime, name: &str, next_turn: bool) -> Vec<UiEvent> {
        ran_change(
            state,
            ModelChange {
                name: name.to_string(),
                effort: None,
                picker: false,
                announced: false,
                next_turn,
            },
        )
    }

    /// Runs one model change the way the loop runs it, and hands back what it
    /// told the UI.
    fn ran_change(state: &mut Runtime, change: ModelChange) -> Vec<UiEvent> {
        let (events_tx, mut events_rx) = mpsc::channel(bridge::UI_EVENTS);
        let (_control_tx, control_rx) = mpsc::unbounded_channel();
        let control = ControlChannel::new(control_rx);
        let cancel = Cancellation::new(CancelToken::new());
        let (_work_tx, mut work_rx) = mpsc::channel::<TurnWork>(WORK_LIMIT);
        let outstanding = AtomicUsize::new(1);
        let mut queue = Queue {
            work: &mut work_rx,
            outstanding: &outstanding,
            taken: 0,
        };
        let ended = on_a_runtime(change_model(
            state, change, &events_tx, &cancel, &control, &mut queue,
        ));
        assert_eq!(ended, Ended::Turn);
        let mut said = Vec::new();
        while let Ok(event) = events_rx.try_recv() {
            said.push(event);
        }
        said
    }

    /// A catalog that never answers: upstream's `transport` failure, which it
    /// asks again about.
    struct Unreachable;

    #[async_trait::async_trait(?Send)]
    impl crate::provider::model::ModelCatalog for Unreachable {
        async fn fetch(&self) -> Result<Vec<CatalogEntry>, crate::provider::model::CatalogError> {
            Err(crate::provider::model::CatalogError::Gateway {
                detail: "connection refused".to_string(),
                retry: Some(crate::provider::model::CatalogRetry::Unreachable),
            })
        }
    }

    /// A catalog that answers with `entries`, and counts how often it was asked.
    struct Answering {
        entries: Vec<CatalogEntry>,
        asked: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait(?Send)]
    impl crate::provider::model::ModelCatalog for Answering {
        async fn fetch(&self) -> Result<Vec<CatalogEntry>, crate::provider::model::CatalogError> {
            self.asked.fetch_add(1, Ordering::SeqCst);
            Ok(self.entries.clone())
        }
    }

    /// One catalog row offering `efforts`.
    fn leveled(id: &str, efforts: &[&str]) -> CatalogEntry {
        CatalogEntry {
            efforts: efforts.iter().map(|effort| effort.to_string()).collect(),
            ..published(id)
        }
    }

    #[test]
    fn a_change_the_band_announced_is_applied_with_its_effort_and_saved_as_one_write() {
        // `selectModelFromPicker`: the model and the level go together, in one
        // write, under upstream's `model picker` -- and the answer says
        // nothing the band has not already said.
        let (home, workspace, mut state) = recording();
        std::fs::create_dir_all(home.path().join(".xfx")).expect("a profile home");
        std::fs::write(
            home.path().join(".xfx").join("settings.json"),
            r#"{"theme":"dark"}"#,
        )
        .expect("seed");
        let low = crate::provider::effort::Effort::Named("low".to_string());
        let said = ran_change(
            &mut state,
            ModelChange {
                name: "openai/gpt-6-astra".to_string(),
                effort: Some(low.clone()),
                picker: true,
                announced: true,
                next_turn: false,
            },
        );
        assert_eq!(
            said,
            [UiEvent::ModelAnswered {
                model: "openai/gpt-6-astra".to_string(),
                effort: low.clone(),
                // The fixture's Gateway has no credential, so no catalog to
                // check against: applied, and no caveat.
                outcome: ModelAnswer::Applied { unverified: None },
                next_turn: false,
                announced: true,
            }]
        );
        assert_eq!(state.effort, low);
        assert_eq!(
            settings(home.path()),
            serde_json::json!({"theme": "dark", "effort": "low",
                               "models": {"gateway": "openai/gpt-6-astra"}})
        );
        // The next launch starts in it.
        let reloaded = config(home.path(), workspace.path());
        assert_eq!(reloaded.effort, low);
        assert_eq!(reloaded.model, "openai/gpt-6-astra");

        // A router-form change leaves the effort as it is, and does not write
        // it: it changed only the model (`setResolvedModel`).
        let said = ran_model(&mut state, "acme/one", false);
        let UiEvent::ModelAnswered { effort, .. } = &said[0] else {
            panic!("{said:?}");
        };
        assert_eq!(effort, &low, "a model change dropped the session's effort");
        assert_eq!(settings(home.path())["effort"], "low");
        assert_eq!(settings(home.path())["models"]["gateway"], "acme/one");
    }

    #[test]
    fn a_query_with_no_catalog_in_hand_loads_it_retrying_a_failure_and_tells_the_ui() {
        // `/model opus` before any list was read: the catalog first, so the
        // query resolves -- and its rows go to the band too, which is what
        // lets the next change be resolved and announced there. A load that
        // failed is asked again, because this is `/model`.
        let (_home, _workspace, mut state) = recording();
        let asked = Arc::new(AtomicUsize::new(0));
        state.selector.set_fetcher_for_test(Box::new(Answering {
            entries: vec![published("anthropic/claude-opus-5")],
            asked: Arc::clone(&asked),
        }));
        state
            .selector
            .set_catalog_for_test(CatalogState::Failed("nothing answered".to_string()));
        let said = ran_model(&mut state, "opus", false);
        assert_eq!(
            asked.load(Ordering::SeqCst),
            1,
            "the failure was not asked again"
        );
        assert!(
            matches!(&said[0], UiEvent::CatalogLoaded { entries, .. } if entries.len() == 1),
            "{said:?}"
        );
        assert!(
            matches!(&said[1], UiEvent::ModelAnswered { model, announced: false, .. }
                if model == "anthropic/claude-opus-5"),
            "{said:?}"
        );
        // Loaded now: the next query asks no socket and sends no rows again.
        let said = ran_model(&mut state, "opus", false);
        assert_eq!(asked.load(Ordering::SeqCst), 1);
        assert!(
            matches!(&said[0], UiEvent::ModelAnswered { .. }),
            "{said:?}"
        );
    }

    #[test]
    fn a_turn_sends_the_effort_only_to_a_model_the_catalog_says_takes_it() {
        // `resolveProviderOptionsForCapabilities`: nothing for the default,
        // the level for a model that lists it, and nothing for a model that
        // does not -- or that no catalog read describes.
        let (_home, _workspace, mut state) = recording();
        let options = |state: &Runtime| state.request_options().reasoning;
        assert_eq!(options(&state), None, "the default asks for nothing");
        state.effort = crate::provider::effort::Effort::Named("high".to_string());
        assert_eq!(options(&state), None, "no catalog describes the model");
        let model = state.model().to_string();
        state
            .selector
            .set_catalog_for_test(CatalogState::Loaded(vec![leveled(
                &model,
                &["low", "high"],
            )]));
        assert_eq!(options(&state), Some("high".to_string()));
        state
            .selector
            .set_catalog_for_test(CatalogState::Loaded(vec![leveled(&model, &["low"])]));
        assert_eq!(options(&state), None, "a level the model does not list");
        state
            .selector
            .set_catalog_for_test(CatalogState::Loaded(vec![leveled(
                "other/model",
                &["high"],
            )]));
        assert_eq!(options(&state), None, "another model's levels");
    }

    #[test]
    fn a_model_change_while_work_runs_is_put_aside_and_stops_nothing() {
        // `TurnControl::Model` read during a turn: the turn keeps the model it
        // began with, nothing it was doing is stopped, and the change waits
        // for the loop to apply between this and the next piece of work.
        let (control_tx, control_rx) = mpsc::unbounded_channel();
        let control = ControlChannel::new(control_rx);
        let asked = ModelChange {
            name: "next/model".to_string(),
            effort: None,
            picker: false,
            announced: true,
            next_turn: true,
        };
        control_tx
            .send(TurnControl::Model(asked.clone()))
            .expect("alive");
        let mut stops = Vec::new();
        let (_, ended) = on_a_runtime(raced_against_control(
            async {
                tokio::time::sleep(Duration::from_millis(50)).await;
            },
            &control,
            |through| stops.push(through),
        ));
        assert!(stops.is_empty(), "a model change stopped the work");
        assert_eq!(ended, Ended::Turn);
        assert_eq!(control.take_deferred(), vec![asked]);
        assert!(control.take_deferred().is_empty(), "taken twice");
    }

    #[test]
    fn a_change_put_aside_is_applied_before_the_next_piece_of_work_and_survives_the_end() {
        // Applied by the loop after the work ends: with the session going on,
        // and -- for one the band already announced -- with the session over
        // too, because the profile is where the choice outlives the process.
        // One the band could not resolve needs a catalog, and a session that
        // is over does not open a socket for it.
        let (home, _workspace, mut state) = recording();
        let (events_tx, mut events_rx) = mpsc::channel(bridge::UI_EVENTS);
        let (_control_tx, control_rx) = mpsc::unbounded_channel();
        let control = ControlChannel::new(control_rx);
        let cancel = Cancellation::new(CancelToken::new());
        let (_work_tx, mut work_rx) = mpsc::channel::<TurnWork>(WORK_LIMIT);
        let outstanding = AtomicUsize::new(0);
        let mut queue = Queue {
            work: &mut work_rx,
            outstanding: &outstanding,
            taken: 0,
        };
        control.defer(ModelChange {
            name: "kept/model".to_string(),
            effort: None,
            picker: true,
            announced: true,
            next_turn: true,
        });
        control.defer(ModelChange {
            name: "unresolved".to_string(),
            effort: None,
            picker: false,
            announced: false,
            next_turn: true,
        });
        let ended = on_a_runtime(apply_deferred(
            &mut state,
            Ended::Session,
            &events_tx,
            &cancel,
            &control,
            &mut queue,
        ));
        assert_eq!(ended, Ended::Session);
        assert_eq!(state.model(), "kept/model");
        assert_eq!(settings(home.path())["models"]["gateway"], "kept/model");
        assert!(matches!(
            events_rx.try_recv(),
            Ok(UiEvent::ModelAnswered { model, .. }) if model == "kept/model"
        ));
        assert!(
            events_rx.try_recv().is_err(),
            "the unresolved one was applied"
        );
    }

    fn settings(home: &std::path::Path) -> serde_json::Value {
        let path = home.join(".xfx").join("settings.json");
        serde_json::from_str(&std::fs::read_to_string(path).expect("a profile")).expect("json")
    }

    #[test]
    fn a_model_is_saved_for_its_provider_and_the_next_launch_reads_it_back() {
        let (home, workspace, mut state) = recording();
        std::fs::create_dir_all(home.path().join(".xfx")).expect("a profile home");
        std::fs::write(
            home.path().join(".xfx").join("settings.json"),
            r#"{"permission_mode":"ask","models":{"llmux":"fable"}}"#,
        )
        .expect("seed");

        let said = ran_model(&mut state, "acme/one", false);

        assert_eq!(
            said,
            [UiEvent::ModelAnswered {
                model: "acme/one".to_string(),
                effort: crate::provider::effort::Effort::Auto,
                outcome: ModelAnswer::Applied { unverified: None },
                next_turn: false,
                announced: false,
            }],
            "a write nothing outranks says nothing of its own"
        );
        assert_eq!(
            settings(home.path()),
            serde_json::json!({"permission_mode": "ask",
                               "models": {"llmux": "fable", "gateway": "acme/one"}})
        );
        // Round trip: the next launch reads it, from the profile.
        let reloaded = config(home.path(), workspace.path());
        assert_eq!(reloaded.model, "acme/one");
        assert_eq!(
            reloaded.sources.model,
            crate::config::SettingSource::UserGlobal
        );

        // The model already in force is announced and saved again, as
        // upstream's is, and the answer carries the wording's fact through.
        std::fs::write(home.path().join(".xfx").join("settings.json"), "{}").expect("reset");
        let said = ran_model(&mut state, "acme/one", true);
        assert_eq!(
            said,
            [UiEvent::ModelAnswered {
                model: "acme/one".to_string(),
                effort: crate::provider::effort::Effort::Auto,
                outcome: ModelAnswer::Unchanged,
                next_turn: true,
                announced: false,
            }]
        );
        assert_eq!(
            settings(home.path()),
            serde_json::json!({"models": {"gateway": "acme/one"}})
        );
    }

    #[test]
    fn a_save_a_higher_layer_outranks_or_that_fails_says_so_after_the_switch() {
        // `XFX_MODEL` decides a fresh session's model over the profile, so the
        // write is upstream's `saved ... fresh sessions here use
        // higher-precedence model=process_override`.
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let env = Environment::new(
            Some(home.path().to_path_buf()),
            BTreeMap::from([("XFX_MODEL".to_string(), "pinned/model".to_string())]),
        );
        let config = RuntimeConfig::load_with(&env, workspace.path()).expect("load");
        let store = open_store(&config).expect("a store");
        let mut state = Runtime::with_environment(config, env, store, a_prompter(), a_questioner());
        let said = ran_model(&mut state, "acme/two", false);
        assert_eq!(said.len(), 2, "{said:?}");
        assert_eq!(
            said[1],
            UiEvent::Notice(super::super::notice::Notice::neutral(
                "model",
                "saved to user settings (scope=user); fresh sessions here use \
                 higher-precedence model=process_override"
            ))
        );
        assert_eq!(settings(home.path())["models"]["gateway"], "acme/two");

        // A profile xfx could not read is never written over: the model holds
        // for this process, and the session says the save did not happen.
        let (home, _workspace, mut state) = recording();
        std::fs::create_dir_all(home.path().join(".xfx")).expect("a profile home");
        std::fs::write(home.path().join(".xfx").join("settings.json"), "{ not json").expect("seed");
        let said = ran_model(&mut state, "acme/three", false);
        assert_eq!(state.model(), "acme/three");
        let UiEvent::Notice(notice) = &said[1] else {
            panic!("no notice for a failed save: {said:?}");
        };
        assert_eq!(notice.tone, super::super::notice::Tone::Error);
        assert!(
            notice
                .body
                .starts_with("active for this process but not saved to user settings ("),
            "{notice:?}"
        );
        assert_eq!(
            std::fs::read_to_string(home.path().join(".xfx").join("settings.json")).expect("read"),
            "{ not json"
        );
    }

    #[test]
    fn a_catalog_load_answers_the_menu_and_never_the_document() {
        // Each of the three answers the menu has, from the one item that
        // asked; none of them is a notice or a failed turn.
        let (_home, _workspace, mut state) = recording();
        let catalog = |state: &mut Runtime| {
            let (events_tx, mut events_rx) = mpsc::channel(bridge::UI_EVENTS);
            let (_control_tx, control_rx) = mpsc::unbounded_channel();
            let control = ControlChannel::new(control_rx);
            let cancel = Cancellation::new(CancelToken::new());
            let (_work_tx, mut work_rx) = mpsc::channel::<TurnWork>(WORK_LIMIT);
            let outstanding = AtomicUsize::new(1);
            let mut queue = Queue {
                work: &mut work_rx,
                outstanding: &outstanding,
                taken: 0,
            };
            on_a_runtime(run_catalog(
                state, &events_tx, &cancel, &control, &mut queue,
            ));
            let mut said = Vec::new();
            while let Ok(event) = events_rx.try_recv() {
                said.push(event);
            }
            said
        };
        let provider = state.config.provider;
        // No catalog at all: an empty list.
        assert_eq!(
            catalog(&mut state),
            [
                UiEvent::CatalogLoaded {
                    provider,
                    entries: Vec::new()
                },
                UiEvent::TurnEnded { failure: None }
            ]
        );
        // Unread: the menu's failure -- and whether asking again could help
        // -- and no failed turn.
        state.selector.set_fetcher_for_test(Box::new(Unreachable));
        assert_eq!(
            catalog(&mut state),
            [
                UiEvent::CatalogUnread {
                    provider,
                    retry: Some(crate::provider::model::CatalogRetry::Unreachable),
                },
                UiEvent::TurnEnded { failure: None }
            ]
        );
        // Rows: in the order the menu lists them.
        state
            .selector
            .set_catalog_for_test(CatalogState::Loaded(vec![
                published("zeta/one"),
                published("alpha/two"),
            ]));
        let said = catalog(&mut state);
        let UiEvent::CatalogLoaded { entries, .. } = &said[0] else {
            panic!("{said:?}");
        };
        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>(),
            ["alpha/two", "zeta/one"]
        );
    }

    #[test]
    fn a_catalog_that_could_not_be_read_admits_the_id_and_says_it_was_not_checked() {
        // The third answer, and it is not a refusal: a provider that publishes
        // no catalog, or one xfx could not read, must not stop an operator
        // changing a preference. What the caller owes then is the caveat, and
        // the caveat is the reason the check did not happen.
        let (_home, _workspace, mut state) = recording();
        state
            .selector
            .set_catalog_for_test(CatalogState::Failed("nothing answered".to_string()));

        let outcome = state.use_model("unverifiable-model");

        assert!(
            matches!(&outcome, ModelOutcome::Selected { unverified: Some(reason), .. }
                if reason == "nothing answered"),
            "{outcome:?}"
        );
        assert_eq!(state.model(), "unverifiable-model");
    }

    /// A `Runtime` whose band has already answered "always", with a
    /// conversation opened under the authority a real turn runs under.
    ///
    /// The channel ends are handed back rather than dropped: a prompter whose
    /// UI channel is closed reports that there is nobody to ask, and one whose
    /// control channel is closed hears no answer -- so a fixture that let them
    /// go would be measuring a session that had already gone away.
    #[allow(clippy::type_complexity)]
    fn answering_always() -> (
        tempfile::TempDir,
        tempfile::TempDir,
        Runtime,
        mpsc::Receiver<UiEvent>,
        mpsc::UnboundedSender<TurnControl>,
    ) {
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let mut config = config(home.path(), workspace.path());
        // Named rather than inherited: the whole case is a question the user is
        // asked, and only `ask` asks one.
        config.permission_mode = PermissionMode::Ask;
        let store = open_store(&config).expect("a store");

        let (events, seen) = mpsc::channel(4);
        let (control_tx, control_rx) = mpsc::unbounded_channel();
        control_tx
            .send(TurnControl::Answer {
                id: super::super::approval_readiness::ApprovalId(1),
                answer: ApprovalAnswer::Always,
                feedback: None,
            })
            .expect("the channel is open");
        let prompter = TuiPrompter::new(
            events,
            ControlChannel::new(control_rx),
            Cancellation::new(CancelToken::new()),
        );

        let mut state = Runtime::new(config, store, prompter, a_questioner());
        // Through `authority()`, which is what `ready` opens a real
        // conversation with: a fixture that built its own permission session
        // would be asserting against its own argument.
        let model = state.model().to_string();
        let conversation = open_conversation(
            &state.store,
            &state.config,
            &model,
            state.authority(),
            None, // this fixture does not exercise the question path
            &CancelToken::new(),
        )
        .expect("open a conversation");
        state.conversation = Some(conversation);
        (home, workspace, state, seen, control_tx)
    }

    #[test]
    fn an_always_answered_in_the_band_is_recorded_where_a_resumed_session_will_read_it() {
        // The band really produces `ApprovalAnswer::Always`
        // (`super::super::approval`), and the worker really keeps one
        // conversation for the life of the session -- so the grant is in the
        // permission ledger and nowhere else until this step writes it down.
        // Without it "for the rest of this session" quietly means "until xfx
        // exits", and the next `xfx ask --resume-id <id>` asks a question the
        // user has already answered.
        let (_home, workspace, mut state, _seen, _control) = answering_always();
        let target = workspace.path().join("notes.txt");
        let plan = MutationPlan::new(
            MutationKind::Edit,
            target.clone(),
            "notes.txt".to_string(),
            TargetScope::PrimaryWorkspace,
            Preimage::Absent,
            b"beta\n".to_vec(),
        );
        let conversation = state.conversation.as_mut().expect("open");

        // The answer, through the real policy path: `ask` mode has no verdict
        // of its own, so this is the prompter's, and the grant it leaves is the
        // panel's.
        //
        // The guard is released before the recording below: a commit is a disk
        // write, and the ledger's lock is not reentrant.
        let decision = {
            let mut permissions = conversation.tools.permissions();
            permissions
                .decide_with_feedback(ProposedAction::Mutation(&plan))
                .decision
        };
        assert_eq!(
            decision,
            PolicyDecision::Allow {
                source: AllowSource::InteractiveAlways
            },
            "the panel's `always` was not the verdict, so this case is not \
             about a grant at all"
        );

        record_grants(conversation);

        assert_eq!(
            conversation.recorder.state().grants,
            vec![Grant::new(
                "edit_file",
                target.to_string_lossy().into_owned()
            )],
            "an `always` answered in the band is not in the log a resume reads"
        );
    }

    #[test]
    fn the_same_grant_is_not_written_down_twice() {
        // One conversation lives for the whole session here, so this step runs
        // after every turn with the same ledger under it. A tally kept beside
        // the log would have to stay in step turn after turn; the log answers
        // for itself instead.
        let (_home, workspace, mut state, _seen, _control) = answering_always();
        let plan = MutationPlan::new(
            MutationKind::Edit,
            workspace.path().join("notes.txt"),
            "notes.txt".to_string(),
            TargetScope::PrimaryWorkspace,
            Preimage::Absent,
            b"beta\n".to_vec(),
        );
        let conversation = state.conversation.as_mut().expect("open");
        {
            let mut permissions = conversation.tools.permissions();
            permissions.decide_with_feedback(ProposedAction::Mutation(&plan));
        }

        record_grants(conversation);
        let after_the_first = conversation.recorder.state().last_event_seq;
        record_grants(conversation);

        assert_eq!(
            conversation.recorder.state().last_event_seq,
            after_the_first,
            "a grant already in the log was appended to it again"
        );
        assert_eq!(conversation.recorder.state().grants.len(), 1);
    }

    #[test]
    fn the_permission_authority_a_turn_runs_under_can_ask_the_band() {
        // Asked of `Runtime::authority`, which is what `ready` hands
        // `open_conversation`, rather than of a session the fixture built for
        // itself -- a fixture asserting against its own argument would pass
        // whatever the worker did.
        //
        // **What this cannot see**, and it is worth saying: off a terminal
        // `app::permission_session` attaches no prompter either, so this
        // assertion would pass for the wrong spelling of the same line as long
        // as *something* were attached. The two are told apart only by an
        // `ask`-mode tool call on a real pty, which is what
        // `tests/tui.rs::ask_mode_asks_in_the_band_and_a_yes_lets_the_edit_through`
        // is: it types `1` into the band, and only a prompter that hears the
        // band can turn that into an edit.
        let (_home, _workspace, state) = recording();
        assert!(
            state.authority().has_prompter(),
            "`ask` mode has nowhere to ask, so every mutation is refused"
        );
        assert_eq!(state.authority().mode(), state.config.permission_mode);
    }
    #[test]
    fn the_catalog_a_ui_is_given_is_bounded_at_the_event() {
        // A daemon may advertise any number of models; the browser renders at
        // most `MAX_RENDERED_MODELS` of them, and the cut is made before the
        // list crosses the channel rather than at the painter.
        let many: Vec<CatalogEntry> = (0..MAX_RENDERED_MODELS + 50)
            .map(|index| CatalogEntry {
                id: format!("model-{index}"),
                aliases: Vec::new(),
                name: None,
                efforts: Vec::new(),
                max_context: None,
                facts: Default::default(),
            })
            .collect();
        let shown = bounded(&many);
        assert_eq!(shown.len(), MAX_RENDERED_MODELS);
        assert_eq!(shown[0].id, "model-0", "the provider's own order is kept");
        // A catalog inside the bound is not cut at all.
        assert_eq!(bounded(&many[..3]).len(), 3);
        assert_eq!(bounded(&[]).len(), 0);
    }

    #[test]
    fn a_provider_with_no_catalog_is_informational_rather_than_a_failed_turn() {
        // A provider with no catalog to ask is a **fact about the session**,
        // not something that went wrong. The line shell says so in
        // as many words (`interactive`'s `print_catalog`); a bare `/model` in
        // the band must not turn the same fact into a turn failure, or every
        // Gateway user's first `/model` reads as a broken session.
        assert_eq!(
            from_state(&CatalogState::Unavailable),
            CatalogAnswer::NoneAdvertised
        );
        assert_eq!(
            from_state(&CatalogState::NotLoaded),
            CatalogAnswer::NoneAdvertised
        );
        // A genuine failure stays a failure: "xfx could not ask" and "there is
        // nothing to ask for" are different claims about different things, and
        // a browser that blurred them would report a daemon that is down as a
        // provider with no models.
        assert_eq!(
            from_state(&CatalogState::Failed("nothing answered".to_string())),
            CatalogAnswer::Unread("nothing answered".to_string())
        );
    }

    #[test]
    fn a_catalog_that_failed_reports_the_reason_rather_than_an_empty_list() {
        // The four states, and the one that matters most is `Failed`: a browser
        // that answered an empty list for a daemon that is down would say the
        // provider has no models, which is a different and much worse claim than
        // "xfx could not ask". The load-once half of this contract is
        // `ModelSelector`'s and is pinned there
        // (`ensure_catalog_does_not_retry_after_failure`); what is pinned here is
        // that the reason survives the trip to the UI.
        assert_eq!(
            from_state(&CatalogState::Failed("nothing answered".to_string())),
            CatalogAnswer::Unread("nothing answered".to_string())
        );
        let entries = vec![CatalogEntry {
            id: "m-1".to_string(),
            aliases: vec!["fable".to_string()],
            name: None,
            efforts: Vec::new(),
            max_context: Some(1),
            facts: Default::default(),
        }];
        assert_eq!(
            from_state(&CatalogState::Loaded(entries.clone())),
            CatalogAnswer::Rows(entries)
        );
        // An empty catalog is a readable answer and is not a failure here: what
        // refuses one is the setup probe, which has a reason to.
        assert_eq!(
            from_state(&CatalogState::Loaded(Vec::new())),
            CatalogAnswer::Rows(Vec::new())
        );
    }

    #[test]
    fn a_provider_switch_drops_the_old_bundle_and_the_conversation() {
        // Step (g), and the two things it must not keep. A conversation carried
        // across a switch would replay one provider's history into another's
        // wire format; a bundle is a connection to an endpoint that is no longer
        // configured. `ready()` builds whatever it does not have, so dropping
        // them here is exactly what makes the next prompt select the new one.
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        // A credential, because a bundle cannot be selected without one and the
        // thing being measured is that the bundle is *dropped*.
        let env = Environment::new(
            Some(home.path().to_path_buf()),
            BTreeMap::from([(
                "AI_GATEWAY_API_KEY".to_string(),
                "xfx-test-key-not-a-real-credential".to_string(),
            )]),
        );
        let before = RuntimeConfig::load_with(&env, workspace.path()).expect("load");

        let (events, _rx) = mpsc::channel(bridge::UI_EVENTS);
        let control = ControlChannel::new(mpsc::unbounded_channel().1);
        let cancel = Cancellation::new(CancelToken::new());
        let prompter = TuiPrompter::new(events, control, cancel);
        let store = SessionStore::open(before.profile_dir.as_ref().expect("a profile dir"))
            .expect("open the store");
        let mut runtime =
            Runtime::with_environment(before.clone(), env, store, prompter, a_questioner());
        // Stand something in for what a session accumulates. The bundle is what
        // a turn would reuse; `provider` being `Some` is the whole of "there is
        // a connection to the old endpoint".
        runtime.provider = Bundle::select(&runtime.config, &CancelToken::new()).ok();
        assert!(runtime.provider.is_some(), "the fixture needs a bundle");
        // A real conversation, because "the conversation is dropped" is only a
        // claim about a session that had one. A `None` left standing would make
        // the assertion below true of a runtime that never opened one.
        let model = runtime.model().to_string();
        runtime.conversation = Some(
            open_conversation(
                &runtime.store,
                &runtime.config,
                &model,
                runtime.authority(),
                None, // this fixture does not exercise the question path
                &CancelToken::new(),
            )
            .expect("open a conversation"),
        );

        let mut after = before.clone();
        after.provider = ProviderId::Llmux;
        after.model = "fable".to_string();
        after.llmux_url = Some("http://127.0.0.1:3456".to_string());
        let selected = runtime.adopt(after);

        assert_eq!(selected.provider, ProviderId::Llmux);
        assert_eq!(selected.model, "fable");
        assert_eq!(runtime.model(), "fable");
        assert!(
            runtime.provider.is_none(),
            "the old bundle survived the swap"
        );
        assert!(
            runtime.conversation.is_none(),
            "the old conversation survived the swap"
        );
        assert_eq!(
            runtime.config.llmux_url.as_deref(),
            Some("http://127.0.0.1:3456"),
            "the swap took the url from anything other than the reloaded configuration"
        );
    }

    // -----------------------------------------------------------------------
    // the requester a real session's tools are given
    // -----------------------------------------------------------------------

    /// A runtime whose `ready()` can really run: a credential, so a bundle can
    /// be selected without a socket, and a store to record into.
    ///
    /// The same shape `a_provider_switch_drops_the_old_bundle_and_the_conversation`
    /// builds, because the fact under test is the same one: what `ready()`
    /// constructs, rather than what a fixture assembled by hand.
    fn a_runtime_that_can_open_one() -> (tempfile::TempDir, tempfile::TempDir, Runtime) {
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let env = Environment::new(
            Some(home.path().to_path_buf()),
            BTreeMap::from([(
                "AI_GATEWAY_API_KEY".to_string(),
                "xfx-test-key-not-a-real-credential".to_string(),
            )]),
        );
        let config = RuntimeConfig::load_with(&env, workspace.path()).expect("load");
        let store = SessionStore::open(config.profile_dir.as_ref().expect("a profile dir"))
            .expect("open the store");
        let runtime = Runtime::with_environment(config, env, store, a_prompter(), a_questioner());
        (home, workspace, runtime)
    }

    #[test]
    fn a_real_tui_conversation_can_ask_the_user_a_question() {
        // The wiring this task exists for, asked of `ready()` -- the one place
        // a real session's conversation is opened -- rather than of a
        // `ToolContext` a fixture built. Without it `ask_user_question` answers
        // every call with the availability sentinel on the **one** front end
        // that has a UI to ask in.
        let (_home, _workspace, mut state) = a_runtime_that_can_open_one();
        state.ready(&CancelToken::new()).expect("a conversation");

        assert!(
            state
                .conversation
                .as_ref()
                .expect("open")
                .tools
                .questioner()
                .is_some(),
            "the real TUI session's tools have nobody to ask, so the model's \
             question is answered with the unavailable sentinel"
        );
    }

    #[test]
    fn a_conversation_rebuilt_after_a_provider_switch_can_still_ask() {
        // `adopt` drops the conversation on purpose, and `ready()` builds
        // whatever it does not have. A requester attached only at the first
        // `ready` would leave every session that switched provider -- or typed
        // `/new`, which drops the same field -- unable to ask, and the failure
        // would look like a model that stopped using the tool.
        let (_home, _workspace, mut state) = a_runtime_that_can_open_one();
        state.ready(&CancelToken::new()).expect("a conversation");

        let mut after = state.config.clone();
        after.provider = ProviderId::Llmux;
        after.model = "fable".to_string();
        after.llmux_url = Some("http://127.0.0.1:3456".to_string());
        state.adopt(after);
        assert!(
            state.conversation.is_none(),
            "the switch kept the old conversation, so the rebuild is not what is measured"
        );

        state.ready(&CancelToken::new()).expect("a conversation");
        assert!(
            state
                .conversation
                .as_ref()
                .expect("open")
                .tools
                .questioner()
                .is_some(),
            "the rebuilt conversation has nobody to ask"
        );
    }

    // -----------------------------------------------------------------------
    // a Ctrl-C typed at a question, through the real machine
    // -----------------------------------------------------------------------

    /// A provider that answers from a script and counts what it was asked.
    ///
    /// The same shape `tests/tool_loop.rs` uses. It is here rather than there
    /// because the case below needs the **worker's** control channel and the
    /// TUI's requester in the same turn, neither of which a test outside the
    /// crate can reach.
    struct ScriptedProvider {
        steps: std::cell::RefCell<std::collections::VecDeque<Completion>>,
        seen: std::cell::RefCell<usize>,
    }

    impl ScriptedProvider {
        fn new(steps: Vec<Completion>) -> Self {
            Self {
                steps: std::cell::RefCell::new(steps.into()),
                seen: std::cell::RefCell::new(0),
            }
        }

        /// How many model requests this turn really spent.
        fn requests(&self) -> usize {
            *self.seen.borrow()
        }
    }

    #[async_trait::async_trait(?Send)]
    impl crate::gateway::Provider for ScriptedProvider {
        async fn stream(
            &self,
            _request: &crate::gateway::protocol::CompletionRequest,
            _deltas: &mut dyn crate::gateway::DeltaSink,
        ) -> Result<Completion, crate::gateway::ProviderError> {
            // **Yields before answering**, because a real step awaits a socket
            // and the whole question below is what the runtime does at its
            // yields: a provider that answered without one would never let
            // `raced_against_control` poll the control channel at all, and the
            // case would be measuring the fixture.
            tokio::task::yield_now().await;
            *self.seen.borrow_mut() += 1;
            Ok(self
                .steps
                .borrow_mut()
                .pop_front()
                .expect("the provider was asked more times than the script allows"))
        }
    }

    /// One scripted step that calls tools, in the order given.
    fn calls(tool_calls: Vec<ToolCall>) -> Completion {
        Completion {
            text: String::new(),
            tool_calls,
            finish_reason: FinishReason::ToolCalls,
            usage: Usage::default(),
            provider_detail: None,
            raw_content: Vec::new(),
            wire: Wire::VercelGateway,
        }
    }

    /// A step with no tool calls, which is where a turn ends.
    fn answers(text: &str) -> Completion {
        Completion {
            text: text.to_string(),
            tool_calls: Vec::new(),
            finish_reason: FinishReason::Stop,
            usage: Usage::default(),
            provider_detail: None,
            raw_content: Vec::new(),
            wire: Wire::VercelGateway,
        }
    }

    fn call(id: &str, name: &str, input: serde_json::Value) -> ToolCall {
        ToolCall {
            id: id.to_string(),
            name: name.to_string(),
            input,
        }
    }

    /// One session that can run turns the way the worker runs them, kept whole
    /// so that "the session is still usable" is a claim a **second** turn can
    /// make rather than an inference from the first one's exit.
    struct AskingSession {
        workspace: tempfile::TempDir,
        events_tx: Sender<UiEvent>,
        events_rx: Receiver<UiEvent>,
        control_tx: UnboundedSender<TurnControl>,
        control: Arc<ControlChannel>,
        cancel: Cancellation,
        tools: crate::tools::ToolContext,
    }

    /// What one turn of such a session did.
    struct Ran {
        outcome: Result<crate::agent::TurnOutcome, crate::agent::TurnError>,
        ended: Ended,
        /// Whether the loop's `stop` really ran, so a case about a cancelled
        /// turn cannot pass on a turn nobody interrupted.
        stopped: bool,
    }

    /// The file the second call of the interrupted step would create.
    const AFTER_THE_INTERRUPT: &str = "after-the-interrupt.txt";

    impl AskingSession {
        /// `asks` is what the tools may do without being asked: `Yolo` where the
        /// case is about the cancellation, `Ask` where it is about the approval
        /// panel.
        fn new(mode: PermissionMode) -> Self {
            let workspace = tempfile::tempdir().expect("a workspace");
            let (events_tx, events_rx) = mpsc::channel(bridge::UI_EVENTS);
            let (control_tx, control_rx) = mpsc::unbounded_channel();
            let control = ControlChannel::new(control_rx);
            let cancel = Cancellation::new(CancelToken::new());
            let questioner = Arc::new(TuiQuestioner::new(
                events_tx.clone(),
                Arc::clone(&control),
                cancel.clone(),
            ));
            let scope =
                crate::workspace::AccessScope::primary_only(workspace.path()).expect("a scope");
            let permissions = PermissionSession::new(mode).with_prompter(Box::new(
                TuiPrompter::new(events_tx.clone(), Arc::clone(&control), cancel.clone()),
            ));
            let tools = crate::tools::ToolContext::new(scope)
                .with_permissions(permissions)
                // The **session's** mirror, exactly as `open_conversation` is
                // given it once and every turn then resets it
                // (`bridge::Cancellation::turn`).
                .with_cancel(cancel.turn().mirror)
                .with_questioner(
                    Arc::clone(&questioner) as Arc<dyn crate::tools::QuestionRequester>
                );
            Self {
                workspace,
                events_tx,
                events_rx,
                control_tx,
                control,
                cancel,
                tools,
            }
        }

        fn written(&self) -> std::path::PathBuf {
            self.workspace.path().join(AFTER_THE_INTERRUPT)
        }

        /// Runs one turn, with the UI typing `typed` the moment `watch_for`
        /// names an event as the panel this case is about.
        ///
        /// The runtime thread parks inside the panel, so the typing has to
        /// happen on another thread -- which is the production topology, not a
        /// test convenience.
        fn turn_while_typing(
            &mut self,
            provider: &ScriptedProvider,
            watch_for: fn(UiEvent) -> Option<Option<super::super::question::QuestionId>>,
            typed: impl FnOnce(Option<super::super::question::QuestionId>) -> Vec<TurnControl> + Send,
        ) -> Ran {
            let turn = self.cancel.turn();
            let request = TurnRequest {
                model: "vendor/model".to_string(),
                prompt: "anything".to_string(),
                history: Vec::new(),
                max_steps: 4,
                max_attempts: 1,
                cancel: turn.mirror.clone(),
                tools: self.tools.clone(),
            };
            let mut sink = UiEventSink::new(self.events_tx.clone(), turn.token.clone());
            let stopped = Arc::new(AtomicBool::new(false));
            let watched = Arc::clone(&stopped);
            let interrupted = turn.clone();
            let AskingSession {
                events_rx,
                control_tx,
                control,
                cancel,
                ..
            } = self;
            let finished = Arc::new(AtomicBool::new(false));
            let done = Arc::clone(&finished);
            let (outcome, ended) = std::thread::scope(|scope| {
                scope.spawn(|| {
                    let seen = panel_shown(events_rx, watch_for, cancel);
                    for message in typed(seen) {
                        control_tx.send(message).expect("the runtime is listening");
                    }
                });
                // **The bound on the whole turn.** A turn parked in a panel
                // nobody answers is exactly what a missing cancellation boundary
                // produces -- the call *behind* the interrupted one asks its own
                // question, and there is no second keystroke coming -- and it
                // would hang the suite rather than fail it. Ending the session
                // is the one exit every panel has.
                let watchdog = cancel.clone();
                scope.spawn(move || {
                    let deadline = Instant::now() + TEST_DRAIN;
                    while Instant::now() < deadline {
                        if done.load(Ordering::Acquire) {
                            return;
                        }
                        std::thread::sleep(Duration::from_millis(2));
                    }
                    watchdog.cancel();
                });
                let ran = on_a_runtime(raced_against_control(
                    crate::agent::run_turn(request, provider, &mut sink),
                    control,
                    move |_through| {
                        // What `turn_loop` does with a `Cancel`: this turn stops
                        // and the session does not.
                        interrupted.cancel();
                        watched.store(true, Ordering::Release);
                    },
                ));
                finished.store(true, Ordering::Release);
                ran
            });
            Ran {
                outcome,
                ended,
                stopped: stopped.load(Ordering::Acquire),
            }
        }

        /// A turn nobody interrupts, for the claim that the session survived one
        /// that was.
        fn plain_turn(&mut self, provider: &ScriptedProvider) -> Ran {
            let turn = self.cancel.turn();
            let request = TurnRequest {
                model: "vendor/model".to_string(),
                prompt: "and now".to_string(),
                history: Vec::new(),
                max_steps: 4,
                max_attempts: 1,
                cancel: turn.mirror.clone(),
                tools: self.tools.clone(),
            };
            let mut sink = UiEventSink::new(self.events_tx.clone(), turn.token.clone());
            let (outcome, ended) = on_a_runtime(raced_against_control(
                crate::agent::run_turn(request, provider, &mut sink),
                &self.control,
                |_through| panic!("nothing interrupted this turn"),
            ));
            Ran {
                outcome,
                ended,
                stopped: false,
            }
        }

        /// Whether the model's question came back as the user's cancellation
        /// rather than as a failure or an unanswered call.
        fn question_was_cancelled(&mut self) -> bool {
            self.question_came_back(true, "the question was cancelled")
        }

        /// Whether the model's question came back as a call the user stopped
        /// the turn at: upstream's `aborted by user`, never the cancellation
        /// sentinel (`crate::tools::question::ABORTED_OUTPUT`).
        fn question_was_aborted(&mut self) -> bool {
            self.question_came_back(false, crate::tools::question::ABORTED_OUTPUT)
        }

        fn question_came_back(&mut self, expect_ok: bool, expect_detail: &str) -> bool {
            let mut seen = false;
            while let Ok(event) = self.events_rx.try_recv() {
                if let UiEvent::ToolResult {
                    call_id,
                    tool,
                    ok,
                    detail,
                    ..
                } = event
                {
                    if call_id == "call-1" && tool == "ask_user_question" {
                        assert_eq!(ok, expect_ok, "the question came back as {detail:?}");
                        assert_eq!(detail, expect_detail);
                        seen = true;
                    }
                }
            }
            seen
        }
    }

    /// Waits until the UI is shown the panel this case is about.
    ///
    /// Bounded, and the bound **ends the session** rather than only failing: the
    /// runtime thread is parked inside the panel, so a bare panic here would
    /// leave the turn waiting for an answer that is never coming and the test
    /// would hang instead of failing.
    fn panel_shown(
        events_rx: &mut Receiver<UiEvent>,
        watch_for: fn(UiEvent) -> Option<Option<super::super::question::QuestionId>>,
        cancel: &Cancellation,
    ) -> Option<super::super::question::QuestionId> {
        let deadline = Instant::now() + TEST_DRAIN;
        while Instant::now() < deadline {
            match events_rx.try_recv() {
                Ok(event) => {
                    if let Some(seen) = watch_for(event) {
                        return seen;
                    }
                }
                Err(_) => std::thread::sleep(Duration::from_millis(2)),
            }
        }
        cancel.cancel();
        panic!("the panel this case is about never reached the UI");
    }

    /// The model's question, and the id it was asked under.
    fn a_question(event: UiEvent) -> Option<Option<super::super::question::QuestionId>> {
        match event {
            UiEvent::Question(request) => Some(Some(request.id)),
            _ => None,
        }
    }

    /// A permission question, which carries no id of its own.
    fn an_approval(event: UiEvent) -> Option<Option<super::super::question::QuestionId>> {
        match event {
            UiEvent::Approval(_) => Some(None),
            _ => None,
        }
    }

    /// A step that asks the user something and then writes a file, in that
    /// order: the second call is what an interrupted turn must not run.
    fn ask_then_write() -> Completion {
        calls(vec![
            call(
                "call-1",
                "ask_user_question",
                serde_json::json!({"questions": [{"question": "Which depth?",
                    "options": [{"label": "Thorough"}, {"label": "Quick"}]}]}),
            ),
            call(
                "call-2",
                "write_file",
                serde_json::json!({"path": AFTER_THE_INTERRUPT,
                    "content": "written on a turn the user had stopped\n"}),
            ),
        ])
    }

    #[test]
    fn a_ctrl_c_at_a_question_stops_the_turn_before_any_later_call_or_request() {
        // **The real machine, not a queue.** A Ctrl-C typed at the model's
        // question is an *interrupt*, and the shell sends exactly one message
        // for it (`super::super::shell`'s `answer_question`): the turn's
        // `Cancel`. What has to be true afterwards is a fact about the turn --
        // the question comes back aborted (never as the cancellation sentinel:
        // upstream's e2e `tui-decision-prompts.test.ts:2454`), no later call
        // of that same step runs, and no second model request is spent.
        //
        // The two hops that make it true, and both are load-bearing: the
        // requester hands the `Cancel` back to the loop **before** it returns
        // (`super::super::question`'s `request`), so the message is already in
        // `ControlChannel::put_back` when the machine reaches its next
        // boundary; and the machine yields at that boundary
        // (`crate::agent::machine`'s `execute_tool_calls`), which is the only
        // moment `raced_against_control` can consume it and set the mirror the
        // machine then reads.
        let mut session = AskingSession::new(PermissionMode::Yolo);
        let provider = ScriptedProvider::new(vec![
            ask_then_write(),
            answers("a second request the stopped turn must not have made"),
        ]);

        let ran = session.turn_while_typing(&provider, a_question, |_id| {
            vec![TurnControl::Cancel { through: 0 }]
        });

        assert!(
            session.question_was_aborted(),
            "the question was never answered at all, so nothing here is about a cancelled one"
        );
        assert!(
            ran.stopped,
            "the interrupt never reached the loop, so nothing here is about a cancelled turn"
        );
        assert!(
            matches!(ran.outcome, Err(crate::agent::TurnError::Cancelled)),
            "the interrupted turn did not end as a cancelled one: {:?}",
            ran.outcome
        );
        assert!(
            !session.written().exists(),
            "a mutating call from the interrupted step ran after the user stopped the turn"
        );
        assert_eq!(
            provider.requests(),
            1,
            "the stopped turn spent another model request after the interrupt"
        );
        assert_eq!(ran.ended, Ended::Turn, "one Ctrl-C ended the whole session");

        // And the session is still usable, which is the other half of "the turn
        // stopped, not the session": the next turn runs on the same
        // conversation's tools, with the mirror reset by `Cancellation::turn`.
        let next = ScriptedProvider::new(vec![answers("the session is still usable")]);
        let after = session.plain_turn(&next);
        assert!(
            after.outcome.is_ok(),
            "the turn after the interrupt could not run: {:?}",
            after.outcome
        );
        assert_eq!(next.requests(), 1);
    }

    #[test]
    fn a_shutdown_at_a_question_stops_the_turn_and_the_loop_behind_it() {
        // The same boundary, reached by the other stop. A shutdown must not be
        // a slower interrupt: the calls behind the question are as unwanted when
        // the session is going down as when one turn is.
        let mut session = AskingSession::new(PermissionMode::Yolo);
        let provider = ScriptedProvider::new(vec![
            ask_then_write(),
            answers("a second request the shut-down turn must not have made"),
        ]);

        let ran =
            session.turn_while_typing(&provider, a_question, |_id| vec![TurnControl::Shutdown]);

        assert!(ran.stopped);
        assert!(
            matches!(ran.outcome, Err(crate::agent::TurnError::Cancelled)),
            "{:?}",
            ran.outcome
        );
        assert!(!session.written().exists());
        assert_eq!(provider.requests(), 1);
        assert_eq!(
            ran.ended,
            Ended::Session,
            "a shutdown left the loop going round for another prompt"
        );
    }

    #[test]
    fn a_declined_question_is_the_sentinel_and_the_turn_runs_on() {
        // **The distinction the two messages carry.** A declined question --
        // which the band sends only for a batch the screen cannot show
        // (`super::super::shell`'s `ask_question`), since Escape and Ctrl-C
        // both stop the turn now (divergence 7) -- is told to the model with
        // its own sentinel and the turn goes on, later calls and next request
        // included. A stop ends the turn. A machine that treated a declined
        // question as an interrupt would collapse the two into one meaning and
        // lose the ability to say "not that question, but carry on".
        let mut session = AskingSession::new(PermissionMode::Yolo);
        let provider = ScriptedProvider::new(vec![
            ask_then_write(),
            answers("the turn carried on after the question was declined"),
        ]);

        let ran = session.turn_while_typing(&provider, a_question, |id| {
            vec![TurnControl::QuestionCancelled {
                id: id.expect("the question carried an id"),
            }]
        });

        assert!(session.question_was_cancelled());
        assert!(
            !ran.stopped,
            "a declined question stopped the turn as though it were an interrupt"
        );
        assert!(ran.outcome.is_ok(), "{:?}", ran.outcome);
        assert!(
            session.written().exists(),
            "the call behind the declined question was dropped, so declining a question \
             now stops the turn"
        );
        assert_eq!(
            provider.requests(),
            2,
            "the model was never told what it asked for"
        );
    }

    #[test]
    fn a_ctrl_c_at_an_approval_stops_the_turn_before_the_next_call_too() {
        // The approval panel parks the same thread at the same kind of boundary
        // (`super::super::approval::TuiPrompter`), so the same interrupt has to
        // reach the same place. Without the machine's boundary this refuses one
        // mutation and then performs the next one.
        let mut session = AskingSession::new(PermissionMode::Ask);
        let provider = ScriptedProvider::new(vec![
            calls(vec![
                call(
                    "call-1",
                    "write_file",
                    serde_json::json!({"path": "asked-about.txt", "content": "first\n"}),
                ),
                call(
                    "call-2",
                    "write_file",
                    serde_json::json!({"path": AFTER_THE_INTERRUPT, "content": "second\n"}),
                ),
            ]),
            answers("a second request the stopped turn must not have made"),
        ]);

        let ran = session.turn_while_typing(&provider, an_approval, |_none| {
            vec![TurnControl::Cancel { through: 0 }]
        });

        assert!(ran.stopped, "the interrupt never reached the loop");
        assert!(
            matches!(ran.outcome, Err(crate::agent::TurnError::Cancelled)),
            "{:?}",
            ran.outcome
        );
        assert!(
            !session.workspace.path().join("asked-about.txt").exists(),
            "the refused mutation was performed anyway"
        );
        assert!(
            !session.written().exists(),
            "the call behind the refused one ran after the user stopped the turn"
        );
        assert_eq!(provider.requests(), 1);
    }

    #[test]
    fn a_reset_at_an_approval_stops_the_turn_and_ends_it_as_a_reset() {
        // `/clear`, `/new` or `/reset` reaching a turn parked at an approval:
        // the prompter refuses the question and hands the message back to the
        // loop (`super::super::approval`'s catch-all), which stops the turn
        // exactly as a Ctrl-C does -- and says the conversation goes with it.
        let mut session = AskingSession::new(PermissionMode::Ask);
        let provider = ScriptedProvider::new(vec![
            calls(vec![
                call(
                    "call-1",
                    "write_file",
                    serde_json::json!({"path": "asked-about.txt", "content": "first\n"}),
                ),
                call(
                    "call-2",
                    "write_file",
                    serde_json::json!({"path": AFTER_THE_INTERRUPT, "content": "second\n"}),
                ),
            ]),
            answers("a second request the reset turn must not have made"),
        ]);

        let ran = session.turn_while_typing(&provider, an_approval, |_none| {
            vec![TurnControl::Reset { through: 0 }]
        });

        assert!(ran.stopped, "the reset never reached the loop");
        assert_eq!(ran.ended, Ended::Reset(0));
        assert!(
            matches!(ran.outcome, Err(crate::agent::TurnError::Cancelled)),
            "{:?}",
            ran.outcome
        );
        assert!(!session.workspace.path().join("asked-about.txt").exists());
        assert!(!session.written().exists());
        assert_eq!(provider.requests(), 1);
    }
}
