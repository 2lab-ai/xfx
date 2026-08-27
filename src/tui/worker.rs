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
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use futures_util::FutureExt;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::mpsc::{self, Receiver, Sender, UnboundedSender};

use super::approval::{ControlChannel, TuiPrompter};
use super::bridge::{self, Cancellation, TurnCancel, TurnControl, TurnWork, UiEvent, UiEventSink};
use crate::agent::{run_turn_saved, TurnRequest};
use crate::config::{Environment, RuntimeConfig};
use crate::gateway::{CancelToken, DEFAULT_MAX_ATTEMPTS};
use crate::interactive::{open_conversation, Conversation};
use crate::permission::PermissionSession;
use crate::provider::model::{CatalogEntry, CatalogState, ModelSelector, MAX_RENDERED_MODELS};
use crate::provider::setup::{setup_transaction, SetupProblem};
use crate::provider::{Bundle, ProviderId};
use crate::session::{SessionEvent, SessionStore};
use crate::workspace::ProjectContext;

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
        let (work, work_rx) = mpsc::channel(WORK_LIMIT);
        let (control, control_rx) = mpsc::unbounded_channel();
        (
            Self {
                work,
                control,
                outstanding: Arc::new(AtomicUsize::new(0)),
                accepted: Arc::new(AtomicUsize::new(0)),
            },
            work_rx,
            control_rx,
        )
    }
}

/// The runtime thread and everything needed to end it.
pub(crate) struct Worker {
    /// `None` once the thread has been joined, or once it has been left
    /// detached because the join grace ran out.
    thread: Option<JoinHandle<()>>,
    /// Set by the thread as the last thing it does, which is what makes the
    /// join *bounded*: `JoinHandle::join` has no timeout, so the wait is on
    /// this and the join is what follows a wait that succeeded.
    finished: Arc<AtomicBool>,
    /// The session's cancellation. Held here rather than passed to
    /// [`Worker::shutdown`] because cancelling is the first step of the drain
    /// protocol and a caller that could forget it would deadlock instead.
    cancel: Cancellation,
    handle: WorkHandle,
}

impl Worker {
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
                let _ = thread.join();
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
pub(crate) fn spawn(
    config: RuntimeConfig,
    model: String,
    cancel: Cancellation,
) -> io::Result<(Worker, Receiver<UiEvent>)> {
    let store = open_store(&config)?;
    let (events_tx, events_rx) = mpsc::channel(bridge::UI_EVENTS);
    let (work_tx, mut work_rx) = mpsc::channel::<TurnWork>(WORK_LIMIT);
    let (control_tx, control_rx) = mpsc::unbounded_channel::<TurnControl>();
    // One receiver, read by the turn loop and -- while it is parked in an
    // approval -- by the prompter. See `super::approval::ControlChannel` for
    // why that is sound and what it costs.
    let control = ControlChannel::new(control_rx);
    let prompter = TuiPrompter::new(events_tx.clone(), Arc::clone(&control), cancel.clone());
    let finished = Arc::new(AtomicBool::new(false));
    let done = Arc::clone(&finished);
    let outstanding = Arc::new(AtomicUsize::new(0));
    let loops_outstanding = Arc::clone(&outstanding);
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
                return;
            };
            runtime.block_on(turn_loop(
                Runtime::new(config, model, store, prompter),
                &events_tx,
                &mut work_rx,
                control,
                session_cancel,
                &loops_outstanding,
            ));
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
            },
        },
        events_rx,
    ))
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
    model: String,
    store: SessionStore,
    conversation: Option<Conversation>,
    provider: Option<Bundle>,
    /// The way to ask the person at the terminal, cloned into each
    /// conversation's authority. Held here rather than made per turn -- see the
    /// module header on what "always" is worth.
    prompter: TuiPrompter,
    /// The catalog of the **currently configured** provider.
    ///
    /// Held across turns because loading it costs a socket and its whole
    /// contract is "once": `ModelSelector::ensure_catalog` attempts a load only
    /// from `NotLoaded`, so a failed load stays failed and a second `/model`
    /// reports the same reason without asking a daemon that is not there again.
    /// Rebuilt -- not merely cleared -- when the provider changes, because its
    /// fetcher is that provider's endpoint.
    selector: ModelSelector,
}

impl Runtime {
    fn new(
        config: RuntimeConfig,
        model: String,
        store: SessionStore,
        prompter: TuiPrompter,
    ) -> Self {
        Self::with_environment(config, Environment::from_process(), model, store, prompter)
    }

    /// [`Runtime::new`], with the environment named rather than read.
    ///
    /// The seam a unit test holds: a reload is a pure function of a settings
    /// file and an environment, and a test that had to export a variable into
    /// the process would race every other test in the binary.
    fn with_environment(
        config: RuntimeConfig,
        env: Environment,
        model: String,
        store: SessionStore,
        prompter: TuiPrompter,
    ) -> Self {
        let selector = ModelSelector::new(&config);
        Self {
            config,
            env,
            model,
            store,
            conversation: None,
            provider: None,
            prompter,
            selector,
        }
    }

    /// Builds whatever this turn needs and does not have yet.
    fn ready(&mut self, mirror: &CancelToken) -> Result<(), String> {
        if self.provider.is_none() {
            self.provider = Some(Bundle::select(&self.config, mirror)?);
        }
        if self.conversation.is_none() {
            self.conversation = Some(open_conversation(
                &self.store,
                &self.config,
                &self.model,
                self.authority(),
                mirror,
            )?);
        }
        Ok(())
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
        self.config = reloaded;
        self.model = self.config.model.clone();
        self.selector = ModelSelector::new(&self.config);
        self.provider = None;
        self.conversation = None;
        // Asked of the **new** provider, here, where the reloaded configuration
        // is: the UI holds a bool rather than a configuration, so this is the
        // only place the question can be answered after a switch.
        let missing_credential =
            crate::provider::resolve_credential_for(self.config.provider, &self.config).is_none();
        Selected {
            provider: self.config.provider,
            model: self.model.clone(),
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

    /// What `/model <id>` means, with the line shell's own meaning.
    ///
    /// The model a turn talks to, from the next turn on, and a durable record
    /// of it so that a resumed session continues in the model the conversation
    /// was actually held in (`interactive::apply_model`). Nothing else: which
    /// *provider* a prompt goes to is decided by the configuration, and a
    /// front end that could change it would be a second place that decides.
    fn use_model(&mut self, name: String) {
        if name == self.model {
            return;
        }
        self.model = name;
        if let Some(conversation) = self.conversation.as_mut() {
            conversation
                .recorder
                .commit(SessionEvent::PreferencesChanged {
                    model: Some(self.model.clone()),
                    permission_mode: None,
                });
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
/// different things. The first is a property of the provider -- the Gateway
/// publishes no catalog endpoint this port has evidence for
/// (`crate::provider::model::catalog_for`) -- and reporting it as a turn
/// failure made every Gateway user's first `/model` read as a broken session.
/// The second really is a failure and stays one.
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
    entries.iter().take(MAX_RENDERED_MODELS).cloned().collect()
}

/// Runs work until the UI says to stop, or until it is gone.
///
/// **Control first, then at most one piece of work**, which is what `biased`
/// buys: a shutdown that arrived while this was waiting for a prompt is
/// answered rather than made to wait behind one.
async fn turn_loop(
    mut state: Runtime,
    events: &Sender<UiEvent>,
    work: &mut Receiver<TurnWork>,
    control: Arc<ControlChannel>,
    cancel: Cancellation,
    outstanding: &AtomicUsize,
) {
    let mut queue = Queue {
        work,
        outstanding,
        taken: 0,
    };
    loop {
        // The select borrows `work` for its own branch, so the cancellation arm
        // cannot drain the queue where it is decided; it says so and the drain
        // happens below, outside that borrow.
        let next = tokio::select! {
            biased;
            message = control.recv() => match message {
                // The session is over, or the UI is gone with its sender.
                Some(TurnControl::Shutdown) | None => return,
                // Between turns there is nothing *running* to cancel -- but
                // there may be work waiting that nobody has started, and after
                // an interrupt nothing may start.
                Some(TurnControl::Cancel { through }) => Taken::Abandon(through),
                // No question is outstanding between turns, so an answer is
                // consumed and dropped rather than left to be misread by the
                // next one: the prompter that could have asked one is only
                // reachable from inside a turn.
                Some(TurnControl::Answer(_)) => Taken::Nothing,
            },
            item = queue.work.recv() => Taken::Work(item),
        };
        let item = match next {
            Taken::Abandon(through) => {
                queue.abandon(through);
                continue;
            }
            Taken::Nothing => continue,
            // The UI dropped its side of the work channel without saying
            // anything, which is the same fact as a shutdown and is treated as
            // one.
            Taken::Work(None) => return,
            Taken::Work(Some(item)) => queue.took(item),
        };
        let ended = match item {
            TurnWork::Model(name) => {
                state.use_model(name);
                Ended::Turn
            }
            // `/new`, with the line shell's own meaning: the recorder is
            // dropped, which closes its log and releases the session's writer
            // lock, so the next prompt opens a genuinely new identity
            // (`interactive.rs:457-464`). The tool context goes with it,
            // because a session grant and the read proofs that let a file be
            // edited are sold as being about *this* conversation.
            TurnWork::New => {
                state.conversation = None;
                Ended::Turn
            }
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
        };
        // **After the terminal event, not before it.** The place `submit`
        // claimed covers the whole of one piece of work, so the moment it is
        // given back is the moment the queue really has room -- and by then the
        // UI has been told this one is over. Given back on the way out of a
        // fatal turn too: nothing more will run, and a refusal that talked
        // about a queue on a dead runtime would be a lie the `Gone` arm exists
        // to avoid.
        queue.outstanding.fetch_sub(1, Ordering::Release);
        if ended == Ended::Session {
            return;
        }
    }
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
    /// A message that means nothing here.
    Nothing,
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
    match caught {
        Ok(failure) => {
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
                    UiEvent::Notice(format!(
                        "xfx: {source} outranks the profile, so this configuration is not what \
                         the next turn in this shell will use"
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

/// Loads the catalog once and sends **exactly one** terminal event for it.
async fn run_catalog(
    state: &mut Runtime,
    events: &Sender<UiEvent>,
    cancel: &Cancellation,
    control: &ControlChannel,
    queue: &mut Queue<'_>,
) -> Ended {
    let turn = cancel.turn();
    let provider = state.config.provider;
    let (loaded, ended) = raced_against_control(state.catalog(), control, |through| {
        turn.cancel();
        queue.abandon(through);
    })
    .await;
    match loaded {
        CatalogAnswer::Rows(entries) => {
            // Through `send_ui`: a catalog whose turn was cancelled is a list
            // nobody is waiting for any more, and the conclusion below still
            // closes the item either way.
            let _ = bridge::send_ui(
                events,
                &turn.token,
                UiEvent::CatalogLoaded { provider, entries },
            )
            .await;
            bridge::send_terminal(events, UiEvent::TurnEnded { failure: None }).await;
        }
        // A **notice**, and a turn that ended without a failure. The provider
        // has no catalog; nothing went wrong. The words are the line shell's
        // own (`crate::provider::model::NO_CATALOG_NOTICE`), so `/model` says
        // the same thing on both surfaces rather than reading as a broken
        // session on one of them.
        CatalogAnswer::NoneAdvertised => {
            let _ = bridge::send_ui(
                events,
                &turn.token,
                UiEvent::Notice(crate::provider::model::NO_CATALOG_NOTICE.to_string()),
            )
            .await;
            bridge::send_terminal(events, UiEvent::TurnEnded { failure: None }).await;
        }
        // This one really is a failure: there is a catalog and xfx could not
        // read it, which is a fact about the daemon the operator has to act on.
        CatalogAnswer::Unread(reason) => {
            bridge::send_terminal(
                events,
                UiEvent::TurnEnded {
                    failure: Some(format!("xfx: {reason}")),
                },
            )
            .await;
        }
    }
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
                // An answer to a question that is no longer being asked: the
                // prompter takes its own answer off this channel while it is
                // parked (`super::approval`), so anything reaching here is a
                // second keystroke on a panel that has already gone. Consumed
                // rather than left, so the next question does not inherit it.
                Some(TurnControl::Answer(_)) => {}
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
        let _ = bridge::send_ui(events, &turn.token, UiEvent::Notice(notice.clone())).await;
    }

    let request = TurnRequest {
        model: state.model.clone(),
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
    let outcome = run_turn_saved(
        request,
        context,
        provider,
        &mut sink,
        &mut conversation.recorder,
    )
    .await;

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
    use crate::gateway::CancelToken;
    use crate::permission::{
        AllowSource, ApprovalAnswer, Grant, MutationKind, MutationPlan, PolicyDecision, Preimage,
        ProposedAction, TargetScope,
    };

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
            },
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
            },
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
            },
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
            .send(TurnControl::Answer(crate::permission::ApprovalAnswer::Deny))
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

    fn recording(model: &str) -> (tempfile::TempDir, tempfile::TempDir, Runtime) {
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let config = config(home.path(), workspace.path());
        let store = open_store(&config).expect("a store");
        let conversation = open_conversation(
            &store,
            &config,
            model,
            PermissionSession::new(config.permission_mode),
            &CancelToken::new(),
        )
        .expect("open a conversation");
        let mut state = Runtime::new(config, model.to_string(), store, a_prompter());
        state.conversation = Some(conversation);
        (home, workspace, state)
    }

    #[test]
    fn a_model_change_is_recorded_where_a_resumed_session_will_read_it() {
        let (_home, _workspace, mut state) = recording("first-model");
        assert_eq!(
            state
                .conversation
                .as_ref()
                .expect("open")
                .recorder
                .state()
                .model,
            "first-model"
        );

        state.use_model("second-model".to_string());

        // The next turn talks to it ...
        assert_eq!(state.model, "second-model");
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
        let (_home, _workspace, mut state) = recording("same");
        let before = state
            .conversation
            .as_ref()
            .expect("open")
            .recorder
            .state()
            .last_event_seq;

        state.use_model("same".to_string());

        assert_eq!(state.model, "same");
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
            .send(TurnControl::Answer(ApprovalAnswer::Always))
            .expect("the channel is open");
        let prompter = TuiPrompter::new(
            events,
            ControlChannel::new(control_rx),
            Cancellation::new(CancelToken::new()),
        );

        let mut state = Runtime::new(config, "any-model".to_string(), store, prompter);
        // Through `authority()`, which is what `ready` opens a real
        // conversation with: a fixture that built its own permission session
        // would be asserting against its own argument.
        let conversation = open_conversation(
            &state.store,
            &state.config,
            &state.model,
            state.authority(),
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
            permissions.decide(ProposedAction::Mutation(&plan))
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
            permissions.decide(ProposedAction::Mutation(&plan));
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
        let (_home, _workspace, state) = recording("any-model");
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
        // The Gateway advertises no catalog, and that is a **fact about the
        // provider**, not something that went wrong. The line shell says so in
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
            Runtime::with_environment(before.clone(), env, before.model.clone(), store, prompter);
        // Stand something in for what a session accumulates. The bundle is what
        // a turn would reuse; `provider` being `Some` is the whole of "there is
        // a connection to the old endpoint".
        runtime.provider = Bundle::select(&runtime.config, &CancelToken::new()).ok();
        assert!(runtime.provider.is_some(), "the fixture needs a bundle");
        // A real conversation, because "the conversation is dropped" is only a
        // claim about a session that had one. A `None` left standing would make
        // the assertion below true of a runtime that never opened one.
        runtime.conversation = Some(
            open_conversation(
                &runtime.store,
                &runtime.config,
                &runtime.model,
                runtime.authority(),
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
        assert_eq!(runtime.model, "fable");
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
}
