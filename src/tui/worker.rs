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
//! terminal is precisely the bug this topology exists to prevent, so until
//! Task 17 lands a prompter that asks through the `TurnControl` channel, the
//! worker builds a [`PermissionSession`] with **no approval channel at all**.
//! `ask` mode therefore denies a mutation with the same `no approval channel`
//! refusal a pipe gets: fail-closed, visible as a tool result rather than
//! silent, and stated in `docs/parity.md`.

use std::future::Future;
use std::io;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use futures_util::FutureExt;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::mpsc::{self, Receiver, Sender, UnboundedReceiver, UnboundedSender};

use super::bridge::{self, Cancellation, TurnCancel, TurnControl, TurnWork, UiEvent, UiEventSink};
use crate::agent::{run_turn_saved, TurnRequest};
use crate::config::RuntimeConfig;
use crate::gateway::{CancelToken, DEFAULT_MAX_ATTEMPTS};
use crate::interactive::{open_conversation, Conversation};
use crate::permission::PermissionSession;
use crate::provider::Bundle;
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
    /// A turn is already running and one more is already waiting. The work
    /// channel holds one item, because one turn runs at a time.
    Busy,
    /// The runtime thread is gone. A [`UiEvent::Fatal`] is already on its way,
    /// or has already arrived; this is not a refusal the user can act on, and
    /// it is told apart from [`Self::Busy`] so that the UI never says "a turn
    /// is already running" about a runtime that is not running anything.
    Gone,
}

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
    /// Whether the runtime already has work in flight.
    ///
    /// **The channel cannot answer this**, and that is the whole reason the
    /// flag exists. A `mpsc` permit is freed the moment the receiver *takes*
    /// the item, which is the beginning of a turn and not its end -- so a
    /// one-slot channel is empty for the entire minute a turn is running, a
    /// second `try_send` succeeds, and the prompt the user was never told about
    /// runs by itself when the first one finishes. That is the "queued into a
    /// surprise" this capacity was chosen to prevent, and the topology's rule
    /// for this channel is a **visible refusal** rather than a queue.
    ///
    /// Claimed by the sender and released by the loop when the item is done,
    /// so the window the channel leaves open is closed from both ends.
    busy: Arc<AtomicBool>,
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
        // find the runtime idle.
        if self
            .busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(Rejected::Busy);
        }
        match self.work.try_send(work) {
            Ok(()) => Ok(()),
            // Nothing was taken, so nothing is owed: the claim goes back before
            // the refusal does, or the session is busy for ever over a
            // submission that never happened.
            Err(err) => {
                self.busy.store(false, Ordering::Release);
                Err(match err {
                    TrySendError::Full(_) => Rejected::Busy,
                    TrySendError::Closed(_) => Rejected::Gone,
                })
            }
        }
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
    pub(crate) fn detached() -> (Self, Receiver<TurnWork>, UnboundedReceiver<TurnControl>) {
        let (work, work_rx) = mpsc::channel(1);
        let (control, control_rx) = mpsc::unbounded_channel();
        (
            Self {
                work,
                control,
                busy: Arc::new(AtomicBool::new(false)),
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
    let (work_tx, mut work_rx) = mpsc::channel::<TurnWork>(1);
    let (control_tx, control_rx) = mpsc::unbounded_channel::<TurnControl>();
    let finished = Arc::new(AtomicBool::new(false));
    let done = Arc::clone(&finished);
    let busy = Arc::new(AtomicBool::new(false));
    let loops_busy = Arc::clone(&busy);
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
                Runtime::new(config, model, store),
                &events_tx,
                &mut work_rx,
                control_rx,
                session_cancel,
                &loops_busy,
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
                busy,
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
    model: String,
    store: SessionStore,
    conversation: Option<Conversation>,
    provider: Option<Bundle>,
}

impl Runtime {
    fn new(config: RuntimeConfig, model: String, store: SessionStore) -> Self {
        Self {
            config,
            model,
            store,
            conversation: None,
            provider: None,
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
    /// thread owns and is polling. With no prompter at all, `ask` mode denies a
    /// mutation with `NoApprovalChannel` -- fail-closed and visible -- until
    /// Task 17 lands one that asks through the `TurnControl` channel instead.
    ///
    /// A named step rather than an argument written inline, so that what a turn
    /// may say yes with is something a test can hold.
    fn authority(&self) -> PermissionSession {
        PermissionSession::new(self.config.permission_mode)
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

/// Runs work until the UI says to stop, or until it is gone.
///
/// **Control first, then at most one piece of work**, which is what `biased`
/// buys: a shutdown that arrived while this was waiting for a prompt is
/// answered rather than made to wait behind one.
async fn turn_loop(
    mut state: Runtime,
    events: &Sender<UiEvent>,
    work: &mut Receiver<TurnWork>,
    mut control: UnboundedReceiver<TurnControl>,
    cancel: Cancellation,
    busy: &AtomicBool,
) {
    loop {
        let item = tokio::select! {
            biased;
            message = control.recv() => match message {
                // The session is over, or the UI is gone with its sender.
                Some(TurnControl::Shutdown) | None => return,
                // Between turns there is nothing running to cancel and no
                // question outstanding to answer, so both are consumed and
                // dropped rather than left to be misread by the next turn.
                Some(TurnControl::Cancel | TurnControl::Answer(_)) => continue,
            },
            item = work.recv() => item,
        };
        // The UI dropped its side of the work channel without saying anything,
        // which is the same fact as a shutdown and is treated as one.
        let Some(item) = item else {
            return;
        };
        let ended = match item {
            TurnWork::Model(name) => {
                state.use_model(name);
                Ended::Turn
            }
            TurnWork::Submit(prompt) => run_turn(&mut state, prompt, events, &cancel).await,
        };
        // **After the terminal event, not before it.** The claim `submit` made
        // covers the whole of one piece of work, so the moment it is released
        // is the moment a second prompt stops being a surprise -- and by then
        // the UI has been told this one is over. Released on the way out of a
        // fatal turn too: nothing more will run, and a refusal that said "a
        // turn is already running" about a dead runtime would be a lie the
        // `Gone` arm exists to avoid.
        busy.store(false, Ordering::Release);
        if ended == Ended::Session {
            return;
        }
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
async fn run_turn(
    state: &mut Runtime,
    prompt: String,
    events: &Sender<UiEvent>,
    cancel: &Cancellation,
) -> Ended {
    let turn = cancel.turn();
    match AssertUnwindSafe(ReportedByTheCatcher::around(one_turn(
        state, prompt, events, &turn,
    )))
    .catch_unwind()
    .await
    {
        Ok(failure) => {
            bridge::send_terminal(events, UiEvent::TurnEnded { failure }).await;
            Ended::Turn
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

    use crate::config::Environment;
    use crate::gateway::CancelToken;

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
                busy: Arc::new(AtomicBool::new(false)),
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
                busy: Arc::new(AtomicBool::new(false)),
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
                busy: Arc::new(AtomicBool::new(false)),
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
    fn a_submission_while_a_turn_is_running_is_refused_though_the_channel_is_empty() {
        // The defect this exists for: a `mpsc` permit is freed when the
        // receiver **takes** the item, which is where a turn begins. So for the
        // whole length of a turn the one-slot channel is empty, and a refusal
        // that asked the channel would accept a second prompt, say nothing, and
        // run it by itself when the first finished.
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

        assert_eq!(
            handle.submit(TurnWork::Submit("second".into())),
            Err(Rejected::Busy),
            "a prompt was accepted while a turn was running"
        );
        assert!(
            work_rx.try_recv().is_err(),
            "the refused prompt was queued anyway, and would run as a surprise"
        );
    }

    #[test]
    fn the_runtime_takes_work_again_once_the_turn_it_was_given_is_over() {
        // The other side of the claim: a refusal that never lifted would be a
        // session that answers one prompt and then nothing.
        let (handle, mut work_rx, _control_rx) = WorkHandle::detached();
        handle
            .submit(TurnWork::Submit("first".into()))
            .expect("idle");
        let _taken = work_rx.try_recv().expect("the loop took it");
        assert_eq!(
            handle.submit(TurnWork::Submit("second".into())),
            Err(Rejected::Busy)
        );

        // What `turn_loop` does after the terminal event.
        handle.busy.store(false, Ordering::Release);

        handle
            .submit(TurnWork::Submit("second".into()))
            .expect("the runtime is idle again");
        assert_eq!(work_rx.try_recv(), Ok(TurnWork::Submit("second".into())));
    }

    #[test]
    fn a_refused_submission_does_not_leave_the_runtime_looking_busy_for_ever() {
        // The claim is made before the send, so a send that fails has to give
        // it back -- or one refusal makes every later prompt a refusal too.
        let (handle, work_rx, _control_rx) = WorkHandle::detached();
        drop(work_rx);
        assert_eq!(
            handle.submit(TurnWork::Submit("first".into())),
            Err(Rejected::Gone)
        );
        assert!(
            !handle.busy.load(Ordering::Acquire),
            "a submission that was never taken left the runtime claimed"
        );
    }

    #[test]
    fn a_second_submission_is_refused_as_busy_rather_than_queued_into_a_surprise() {
        // The channel's own refusal, with nothing reading it. It is the weaker
        // of the two halves -- the case above is the one a running turn
        // reaches -- and it is kept because the flag must not be the *only*
        // thing standing between two prompts and one slot.
        let (work, _work_rx) = mpsc::channel::<TurnWork>(1);
        let (control, _control_rx) = mpsc::unbounded_channel::<TurnControl>();
        let handle = WorkHandle {
            work,
            control,
            busy: Arc::new(AtomicBool::new(false)),
        };
        assert_eq!(handle.submit(TurnWork::Submit("first".into())), Ok(()));
        assert_eq!(
            handle.submit(TurnWork::Submit("second".into())),
            Err(Rejected::Busy)
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
            busy: Arc::new(AtomicBool::new(false)),
        };
        drop(work_rx);
        assert_eq!(
            handle.submit(TurnWork::Submit("anything".into())),
            Err(Rejected::Gone)
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
        let mut state = Runtime::new(config, model.to_string(), store);
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

    #[test]
    fn the_permission_authority_a_turn_runs_under_has_no_way_to_ask_a_question() {
        // The fail-closed half of this phase. Asked of `Runtime::authority`,
        // which is what `ready` hands `open_conversation`, rather than of a
        // session the fixture built for itself -- a fixture asserting against
        // its own argument would pass whatever the worker did.
        //
        // **What this cannot see**, and it is worth saying: off a terminal
        // `app::permission_session` attaches no prompter either, so swapping
        // one spelling for the other here changes nothing a unit test can read.
        // The two are told apart only by an `ask`-mode tool call on a real pty,
        // which is Task 17's acceptance to write.
        let (_home, _workspace, state) = recording("any-model");
        assert!(
            !state.authority().has_prompter(),
            "the runtime thread built an approval channel onto the UI's terminal"
        );
        assert_eq!(state.authority().mode(), state.config.permission_mode);
    }
}
