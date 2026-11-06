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
//! **The runtime thread takes no signal from the UI thread.** It is started
//! with every owned signal blocked ([`super::signals::spawn_unsignalled`]) --
//! `SIGINT`, `SIGTERM`, `SIGHUP` and `SIGTSTP`, and the flag-setting
//! `SIGCONT` and `SIGWINCH` with them -- so it, and every thread it starts,
//! can never be the thread the kernel picks to deliver one to. That was Task
//! 3's standing constraint on whichever task added the process's second
//! thread, and `signals::release`'s safety argument rests on it.
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
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use futures_util::FutureExt;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::mpsc::{self, Receiver, Sender, UnboundedReceiver, UnboundedSender};

use super::approval::{ControlChannel, Deferred, TuiPrompter};
use super::bridge::{
    self, Cancellation, ModelAnswer, ModelChange, SpeedChange, TurnCancel, TurnControl, TurnWork,
    UiEvent, UiEventSink,
};
use super::compaction::{Feedback, Update};
use super::notice::Notice;
use super::permissions::{self, Change, Granted, Grants, SavedRules};
use super::question::TuiQuestioner;
use crate::agent::compact::{self, Caller, Failure, Size};
use crate::agent::compaction_trace::{self, Context, Kind as TraceKind, Origin};
use crate::agent::{Steer, Steering, TurnJournal, TurnMachine, TurnRequest};
use crate::clipboard::last_reply;
use crate::config::PermissionMode;
use crate::config::{Environment, RuntimeConfig};
use crate::gateway::{CancelToken, DEFAULT_MAX_ATTEMPTS};
#[cfg(test)]
use crate::interactive::open_conversation;
use crate::interactive::{
    bind_conversation, conversation_scope, open_conversation_as, rename_conversation,
    status_snapshot, Conversation, OpenFailure,
};
use crate::output::{ShellSession, StatusSnapshot};
use crate::permission::{LiveAuthority, LiveRules, PermissionSession};
use crate::provider::model::{
    may_read_again, CatalogEntry, CatalogReader, CatalogState, LoadView, ModelOutcome,
    ModelRequest, ModelSelector, StartedLoad,
};
use crate::provider::setup::{setup_transaction, SetupProblem};
use crate::provider::{Bundle, ProviderId};
use crate::session::{SessionEvent, SessionId, SessionStore, WritableSession};
use crate::workspace::access::{self as workspace_access, WorkspaceAccess};
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

/// Why a turn in Ultra mode was not sent: upstream refuses the request
/// (`error.UltrafastUnavailable`, `model_capabilities.zig:209-224`) rather
/// than send it at a speed the user did not choose. xfx's words, with the way
/// out in them.
const ULTRAFAST_UNAVAILABLE: &str =
    "Ultra mode is unavailable for this model; run /ultrafast off or choose a model that offers it";

/// What a turn in Ultra mode says, once, when a response's finish did not
/// confirm the Ultra tier (`orchestrator.zig:8015-8022`): upstream's words,
/// as answer prose ahead of the answer ([`UiEvent::Operational`]).
const ULTRAFAST_UNCONFIRMED: &str = "Ultrafast was requested, but Gateway did not confirm it was \
     served; this response may have used a standard or lower tier.\n";

/// What a turn in Fast mode says, once, when Fast cannot be asked for because
/// the catalog that would say the model offers it could not be read
/// (`orchestrator.zig:7253-7262`): upstream's words. A catalog that was read
/// and does not offer Fast for the model says nothing, as upstream's does not.
const FAST_UNAVAILABLE: &str =
    "Fast mode is unavailable for this model right now; continuing at standard speed.\n";

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
    /// Where a change to the session's authority is **recorded**:
    /// `/permissions` and Shift-Tab ([`Change`]).
    ///
    /// The change itself is in force before this is sent -- [`Self::live`] --
    /// and what the runtime does with the message is the durable half: the
    /// configuration later conversations are opened from, the session log and
    /// the profile. **A channel of its own, not a [`TurnWork`]**, and each of
    /// the three differences is a bug the work channel would have: a change
    /// would hold a place in [`WORK_LIMIT`] and be refused as busy behind one
    /// waiting prompt, it would show on the band as `queued 1`, and a Ctrl-C
    /// would drop it with the queue ([`Queue::abandon`]). Unbounded, like the
    /// control channel, because a keystroke that changes the mode must never
    /// be refused for want of room. The runtime reads it between pieces of
    /// work (`turn_loop`), which is where the session log is not held by a
    /// running turn.
    permissions: UnboundedSender<Change>,
    /// The mode and reset count every permission session on the runtime
    /// thread reads at each decision ([`LiveAuthority`]).
    ///
    /// Written here, on the UI thread, the moment the person changes it --
    /// which is what makes a Shift-Tab to `ask` gate the *next* tool call of
    /// the turn that is running, as upstream's does
    /// (`yolo-permission-mode.test.ts:373-450`) -- and never by the runtime,
    /// whose channel messages can be older than what the person chose since.
    live: LiveAuthority,
    /// The configured permission table every conversation's authority reads
    /// at each decision ([`LiveRules`]).
    ///
    /// Written here, on the UI thread, by `/allowlist` the moment it has
    /// written the file -- so the change governs the next tool call, of a
    /// running turn too, as upstream's live engine does
    /// (`replaceEffectivePermissionRules`, `session_commands.zig:927-941`) --
    /// and read by a bare `/permissions` for its `configured rules`.
    rules: LiveRules,
    /// What a bare `/permissions` lists, as the runtime last published it.
    grants: Grants,
    /// The running turn's own flag (`gateway`'s [`CancelToken`], the one the
    /// provider and every tool call of the turn poll), raised here on the UI
    /// thread the moment the person stops the turn ([`Self::control`]).
    ///
    /// **The message alone cannot reach a call that holds the runtime's
    /// thread.** A tool executes synchronously inside the turn's future, so
    /// while a command runs nothing polls the control channel
    /// (`raced_against_control` waits on a body that is not yielding) and the
    /// stop would be read only once the command had finished by itself. The
    /// command's own supervisor polls this flag every few milliseconds
    /// (`crate::tools::terminal`'s `supervise`), so raising it here stops the
    /// command -- and its process group -- at the keystroke, as upstream's
    /// `cancelActiveOperation` does. The message still follows and is what
    /// drops the queue; a flag raised between turns is reset by the next one
    /// ([`super::bridge::Cancellation::turn`]).
    stop: CancelToken,
    /// The running turn's steers, shared with the runtime
    /// ([`crate::agent::steering`]): **in force before anything is sent**, as
    /// [`Self::stop`] is, because the person's half -- taking a steer back
    /// while a tool runs -- has to know, under one lock, that the turn has
    /// not taken it already. Never a [`TurnWork`]: a steer holds no place in
    /// [`WORK_LIMIT`].
    steering: Steering,
    /// The number the last steer was sent under.
    steers: Arc<AtomicU64>,
    /// The session's saved-session rules: held for the session the next save
    /// opens, or where the open one keeps them ([`SavedRules`]).
    saved_rules: SavedRules,
    /// What `/usage`'s session scope reads, as the runtime last published it.
    spend: SessionSpend,
    /// What `/credits` asks, as the runtime last published it ([`CreditsBoard`]).
    credits: CreditsBoard,
    /// The session's MCP servers ([`crate::mcp::McpRuntime`]): the same
    /// runtime the conversation's tools call, which `/mcp` reads and changes
    /// on the UI thread without waiting on it.
    mcp: Arc<crate::mcp::McpRuntime>,
    /// Where a fresh session's title is asked for, at submit, and the
    /// `/settings` switch that decides whether it is
    /// ([`super::titles::TitleDesk`]): shared with the runtime, which
    /// publishes where to ask and settles every answer.
    titles: super::titles::TitleDesk,
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
///
/// `/trace` reads the same board, for the same reason and at the same moment:
/// the snapshot and what only a report needs beside it ([`TraceFacts`]) are
/// published together, under one lock, so a report never pairs one moment's
/// model with another moment's session.
#[derive(Clone)]
pub(crate) struct StatusBoard(Arc<Mutex<Board>>);

/// What one publication holds.
struct Board {
    status: StatusSnapshot,
    trace: TraceFacts,
}

/// What `/trace` reads of the runtime thread beside `/status`'s snapshot.
#[derive(Debug, Clone, Default)]
pub(crate) struct TraceFacts {
    /// The profile the session store is in and the conversation's session,
    /// once a prompt has opened one: where the report reads the session's log
    /// back from (`super::trace`), on a thread of its own and never this one.
    pub(crate) session: Option<(std::path::PathBuf, SessionId)>,
    /// The settings files as the configuration in force resolved them -- the
    /// runtime's, which a `/provider` re-reads, not the one the UI launched
    /// with.
    pub(crate) settings: crate::trace::Settings,
}

impl StatusBoard {
    fn new(snapshot: StatusSnapshot) -> Self {
        Self(Arc::new(Mutex::new(Board {
            status: snapshot,
            trace: TraceFacts::default(),
        })))
    }

    /// Replaces what `/status` will read next, and nothing else: a case's
    /// stand-in for the runtime ([`WorkHandle::publish_status`]).
    #[cfg(test)]
    fn publish(&self, snapshot: StatusSnapshot) {
        self.lock().status = snapshot;
    }

    /// Replaces what `/status` and `/trace` will read next, together.
    fn publish_with(&self, snapshot: StatusSnapshot, trace: TraceFacts) {
        *self.lock() = Board {
            status: snapshot,
            trace,
        };
    }

    /// The snapshot the runtime last published.
    pub(crate) fn read(&self) -> StatusSnapshot {
        self.lock().status.clone()
    }

    /// The snapshot and the trace facts the runtime last published, as one
    /// publication left them.
    fn read_with_trace(&self) -> (StatusSnapshot, TraceFacts) {
        let board = self.lock();
        (board.status.clone(), board.trace.clone())
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
    fn lock(&self) -> MutexGuard<'_, Board> {
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

    /// Sends `text` into the running turn ([`crate::agent::steering`]), under
    /// the number this returns -- or `None` when no turn is taking steers, and
    /// the prompt is an ordinary one for the next turn.
    pub(crate) fn steer(&self, text: String) -> Option<u64> {
        let id = self.steers.fetch_add(1, Ordering::Relaxed) + 1;
        self.steering.offer(id, text).then_some(id)
    }

    /// Takes back the newest steer while it still waits for a tool's results
    /// (`popQueuedSteerForEdit`, `worker_runtime.zig:1340-1386`).
    pub(crate) fn retract_steer(&self) -> Option<Steer> {
        self.steering.retract()
    }

    /// Drops every steer still waiting, as a stop or a reset does.
    pub(crate) fn drop_steers(&self) {
        self.steering.drop_pending();
    }

    /// The steering this handle sends into, as the runtime holds it.
    #[cfg(test)]
    pub(crate) fn steering(&self) -> Steering {
        self.steering.clone()
    }

    /// Tells a turn that is already running something. Never blocks, never
    /// full: a UI that could not say "stop" is a UI that cannot stop it.
    pub(crate) fn control(&self, message: TurnControl) {
        // In force before it is sent, as a change of authority is
        // ([`Self::change_permissions`]): every message that stops the running
        // turn raises its flag first ([`Self::stop`]).
        if matches!(
            message,
            TurnControl::Cancel { .. } | TurnControl::Reset { .. } | TurnControl::Shutdown
        ) {
            self.stop.cancel();
        }
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

    /// What `/trace` reads of the runtime: the same snapshot, and the facts
    /// published beside it ([`TraceFacts`]). Never waits for the runtime.
    pub(crate) fn trace_facts(&self) -> (StatusSnapshot, TraceFacts) {
        self.status.read_with_trace()
    }

    /// Puts a change to the session's authority in force at once, and hands
    /// the runtime the record of it. Never blocks, never full.
    ///
    /// **In force before it is sent**: the mode and a reset go to
    /// [`Self::live`], which the turn that may be running reads at its next
    /// decision; what the runtime does with the message, between pieces of
    /// work, is write it down. `Err` means the runtime thread is gone, which
    /// is the one thing a caller must not paint a new mode over -- so the
    /// check comes first and nothing moves on a refusal.
    pub(crate) fn change_permissions(&self, change: Change) -> Result<(), Rejected> {
        if self.permissions.is_closed() {
            return Err(Rejected::Gone);
        }
        match change {
            Change::Mode(mode) => self.live.set_mode(mode),
            Change::Reset => {
                self.live.reset();
            }
            Change::Acknowledge => {}
        }
        self.permissions.send(change).map_err(|_| Rejected::Gone)
    }

    /// The conversation's grants, as a bare `/permissions` lists them, from
    /// the runtime's last publication -- or none, when a reset has happened
    /// since it was made. Never waits for the runtime.
    pub(crate) fn grants(&self) -> Vec<Granted> {
        let (published_at, grants) = self.grants.read();
        if published_at < self.live.resets() {
            Vec::new()
        } else {
            grants
        }
    }

    /// How many resets the live authority has taken: a bare `/permissions`
    /// panel's facts carry it, so a reset is seen the moment it is in force,
    /// before the runtime publishes again.
    pub(crate) fn resets(&self) -> u64 {
        self.live.resets()
    }

    /// The session's saved-session permission rules: held for the session
    /// the next save opens, or the open session's file ([`SavedRules`]).
    pub(crate) fn saved_rules(&self) -> &SavedRules {
        &self.saved_rules
    }

    /// Stands in for the runtime's publication of an open session's store
    /// at `path`.
    #[cfg(test)]
    pub(crate) fn publish_saved_rules(&self, path: std::path::PathBuf) {
        self.saved_rules.publish_open(path);
    }

    /// Whether a reset has happened since the runtime last published, so
    /// every grant that publication counted is out of force.
    pub(crate) fn grants_reset_since_published(&self) -> bool {
        self.grants.read().0 < self.live.resets()
    }

    /// The live conversation's recorded spend, for `/usage`'s session scope,
    /// from the runtime's last publication. Never waits for the runtime.
    pub(crate) fn spent(&self) -> Spent {
        self.spend.read()
    }

    /// Stands in for the runtime's publication of the conversation's spend.
    #[cfg(test)]
    pub(crate) fn publish_spent(&self, spent: Spent) {
        self.spend.publish(spent);
    }

    /// What `/credits` would ask now, or the answer it has without asking,
    /// from the runtime's last publication. Never waits for the runtime.
    pub(crate) fn credits_question(
        &self,
    ) -> Result<crate::credits::Question, crate::credits::Answer> {
        self.credits.read()
    }

    /// Whether a fresh session's first prompt asks for a title, as the band
    /// last set it.
    pub(crate) fn session_titles(&self) -> bool {
        self.titles.on()
    }

    /// Turns title generation on or off, in force for the next prompt that
    /// would ask, whatever the runtime is doing.
    pub(crate) fn set_session_titles(&self, on: bool) {
        self.titles.set_on(on);
    }

    /// Asks a model for the session's title from `excerpt`, at the keystroke
    /// that submitted it ([`super::titles::TitleDesk::ask`]). The band has
    /// checked its half of the gate; the desk checks the rest.
    pub(crate) fn ask_title(&self, excerpt: &str) -> bool {
        self.titles.ask(excerpt)
    }

    /// The conversation the band shows was let go of -- `/new`, `/clear`,
    /// `/reset` -- at the keystroke: one asked from here on is about the
    /// next, and a question about this one runs on to an answer that is
    /// dropped ([`super::titles::TitleDesk::next_lineage`]).
    pub(crate) fn new_title_lineage(&self) {
        self.titles.next_lineage();
    }

    /// Holds `excerpt` for the runtime when a switch the band handed it is
    /// still to be made ([`Self::submit_switch`],
    /// [`super::titles::TitleDesk::defer`]), and says whether it did: the
    /// band then asks nothing at the keystroke. `at` is the prompt's position
    /// in the queue ([`Self::accepted`] once it was taken), or `None` for a
    /// steer.
    pub(crate) fn defer_title(&self, excerpt: &str, at: Option<usize>) -> bool {
        self.titles.defer(excerpt, at)
    }

    /// [`Self::submit`] for a switch of the conversation -- a resume or a
    /// provider switch -- which a prompt submitted behind it waits for before
    /// anything is asked about its title ([`Self::defer_title`]). The
    /// position is recorded first: the runtime may make the switch before
    /// this returns.
    pub(crate) fn submit_switch(&self, work: TurnWork) -> Result<(), Rejected> {
        let before = self.titles.switching(self.accepted() + 1);
        let submitted = self.submit(work);
        if submitted.is_err() {
            self.titles.switch_refused(before);
        }
        submitted
    }

    /// Whether a switch handed over with [`Self::submit_switch`] -- a resume
    /// or a provider switch -- is still to be made: until it is, the
    /// conversation a command reaches is not the one it will run against.
    pub(crate) fn switch_pending(&self) -> bool {
        self.titles.switch_pending()
    }

    /// How title generation stands, for `/trace`.
    pub(crate) fn title_generation(&self) -> crate::trace::TitleGeneration {
        self.titles.generation()
    }

    /// The requests title questions made, for `/trace`'s `## Network Calls`
    /// ([`super::titles::TitleDesk::calls`]).
    pub(crate) fn title_calls(&self) -> Vec<crate::trace::Entry> {
        self.titles.calls()
    }

    /// The desk the band asks at, for a case that has to see it.
    #[cfg(test)]
    pub(crate) fn title_desk(&self) -> &super::titles::TitleDesk {
        &self.titles
    }

    /// Puts `desk` in the handle's place, for a case that answers its own
    /// questions.
    #[cfg(test)]
    pub(crate) fn titles_for_tests(&mut self, desk: super::titles::TitleDesk) {
        self.titles = desk;
    }

    /// Stands in for the runtime's publication of what `/credits` asks.
    #[cfg(test)]
    pub(crate) fn publish_credits(
        &self,
        question: Result<crate::credits::Question, crate::credits::Answer>,
    ) {
        self.credits.publish(question);
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
                live: LiveAuthority::new(PermissionMode::Auto),
                rules: LiveRules::default(),
                grants: Grants::default(),
                stop: CancelToken::new(),
                steering: Steering::new(),
                steers: Arc::new(AtomicU64::new(0)),
                saved_rules: SavedRules::default(),
                spend: SessionSpend::default(),
                credits: CreditsBoard::default(),
                mcp: crate::mcp::McpRuntime::inert(),
                titles: super::titles::TitleDesk::new(true).0,
            },
            work_rx,
            control_rx,
            changes,
        )
    }

    /// The configured permission table every conversation decides with, for
    /// the UI thread to read and to replace ([`LiveRules`]).
    pub(crate) fn rules(&self) -> &LiveRules {
        &self.rules
    }

    /// The session's MCP servers.
    pub(crate) fn mcp(&self) -> &Arc<crate::mcp::McpRuntime> {
        &self.mcp
    }

    /// The same handle over `mcp`, for a case that needs a runtime with files
    /// behind it rather than [`crate::mcp::McpRuntime::inert`].
    #[cfg(test)]
    pub(crate) fn with_mcp(mut self, mcp: Arc<crate::mcp::McpRuntime>) -> Self {
        self.mcp = mcp;
        self
    }

    /// Stands in for the runtime's publication of the grants list, as of the
    /// reset count in force now.
    #[cfg(test)]
    pub(crate) fn publish_grants(&self, grants: Vec<Granted>) {
        self.grants.publish(self.live.resets(), grants);
    }

    /// The live authority this handle writes, for a case that has to read
    /// what a keystroke put in force.
    #[cfg(test)]
    pub(crate) fn live(&self) -> &LiveAuthority {
        &self.live
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

/// What `/credits` asks: the endpoint and key of the provider in force, or the
/// answer it has without asking anybody -- published by the runtime, which is
/// the thread a provider switch changes them on, and read by the UI without
/// waiting for it ([`super::credits_ask`]).
///
/// The key crosses here because the question is asked off the runtime
/// thread, which may be a minute into a turn. It is the credential's own
/// type, whose `Debug` is redacted.
#[derive(Debug, Clone)]
pub(crate) struct CreditsBoard(
    Arc<Mutex<Result<crate::credits::Question, crate::credits::Answer>>>,
);

impl CreditsBoard {
    fn new(question: Result<crate::credits::Question, crate::credits::Answer>) -> Self {
        Self(Arc::new(Mutex::new(question)))
    }

    fn publish(&self, question: Result<crate::credits::Question, crate::credits::Answer>) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = question;
    }

    fn read(&self) -> Result<crate::credits::Question, crate::credits::Answer> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl Default for CreditsBoard {
    /// A board nothing has published to: a provider with nothing to ask.
    fn default() -> Self {
        Self::new(Err(crate::credits::Answer::Failed(
            crate::credits::UNAVAILABLE.to_string(),
        )))
    }
}

/// What `/usage`'s session scope reads, as the runtime last published it:
/// the live conversation's recorded spend and when its session began.
///
/// Upstream reads its session scope synchronously, from the session it holds
/// (`loadUsageSnapshot(.session)`, `app_commands.zig:1207-1212`). Here the
/// conversation is the runtime thread's, which may be a minute into a turn or
/// parked at an approval; so the runtime **publishes** it beside `/status`'s
/// snapshot ([`Runtime::publish`]) and the UI reads the newest one -- as of the
/// last turn's end, mid-turn, as `/status`'s counts are.
#[derive(Debug, Clone, Default)]
pub(crate) struct SessionSpend(Arc<Mutex<Spent>>);

/// One publication of [`SessionSpend`].
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Spent {
    pub(crate) usage: crate::session::RecordedUsage,
    /// When the session began, for the wall clock ([`Runtime`]'s
    /// `began_ms`); `None` only before the runtime has published at all.
    pub(crate) started_at_ms: Option<i64>,
}

impl SessionSpend {
    fn publish(&self, spent: Spent) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = spent;
    }

    pub(crate) fn read(&self) -> Spent {
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

/// One runtime thread, and its owned signals with whether each was blocked.
#[cfg(test)]
type ThreadMask = (std::thread::ThreadId, Vec<(libc::c_int, bool)>);

/// Each runtime thread's owned-signal mask as the thread itself read it, the
/// first thing it did: what a case looks up by the thread's id.
#[cfg(test)]
static RUNTIME_MASKS: Mutex<Vec<ThreadMask>> = Mutex::new(Vec::new());

/// Starts the runtime thread, and hands back the channel the UI reads.
///
/// **Started with every owned signal blocked** ([`super::signals::spawn_unsignalled`]),
/// so none can be delivered to it -- nor to the threads it starts, its runtime's
/// blocking pool and the bills it asks for ([`super::reconcile`]) included --
/// and every handler runs on the UI thread, as `signals::release` requires.
/// `hold` calls this before [`super::signals::install`] as well, which is the
/// order its own startup needs; the mask no longer depends on it.
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
    // The id the first session this runtime opens will have -- at the first
    // prompt, or at a `/rename` typed before one -- allocated now and only in
    // memory, so launching writes nothing and `/status` can show it from
    // launch, as upstream's opens a session at launch (`shellSession`,
    // `session_commands.zig:404-408@481104fd`).
    let next_session = SessionId::generate();
    // Seeded here, on the calling thread and from the configuration the
    // runtime is about to be built from, so a `/status` typed before that
    // thread has constructed anything still reads the session it will be:
    // the configured model, no conversation yet, and the session that the
    // launch resumed or that the first prompt will open.
    let mut seed = status_snapshot(&config, &config.model, None);
    seed.session = Some(match resumed.as_ref() {
        Some(resumed) => ShellSession::Open {
            id: resumed.session.id().to_string(),
            saved: true,
        },
        None => ShellSession::Open {
            id: next_session.to_string(),
            saved: false,
        },
    });
    let status = StatusBoard::new(seed);
    let publishes = status.clone();
    let outstanding = Arc::new(AtomicUsize::new(0));
    let loops_outstanding = Arc::clone(&outstanding);
    let reply = LastReply::default();
    let loops_reply = reply.clone();
    let grants = Grants::default();
    let publishes_grants = grants.clone();
    // Seeded here for the same reason: a `remember` typed before the runtime
    // has published anything is held for the session the first save opens,
    // or made in the resumed session's file.
    let saved_rules = match resumed.as_ref() {
        Some(resumed) => SavedRules::open(crate::permission::session_rules::path_in(
            &store.session_dir(resumed.session.id()),
        )),
        None => SavedRules::pending(crate::permission::session_rules::path_in(
            &store.session_dir(&next_session),
        )),
    };
    let publishes_saved_rules = saved_rules.clone();
    // One live authority for the session: the handle writes it, every
    // conversation this runtime opens reads it.
    let live = LiveAuthority::new(config.permission_mode);
    let loops_live = live.clone();
    // The same for the configured table: one handle, written by `/allowlist`
    // on the UI thread, read by every conversation the runtime opens.
    let rules = LiveRules::new(config.permission.effective().clone());
    let loops_rules = rules.clone();
    let spend = SessionSpend::default();
    let publishes_spend = spend.clone();
    // Published from the start, so a `/credits` typed before the runtime's
    // first publication asks what that publication would say.
    let credits = CreditsBoard::new(crate::credits::question(&config));
    let publishes_credits = credits.clone();
    // The session's MCP servers: read and connected from now on, off this
    // thread and off the runtime's ([`crate::mcp::McpRuntime::start`]).
    let mcp = crate::mcp::McpRuntime::new(crate::mcp::Paths {
        profile: config
            .profile_dir
            .as_deref()
            .map(crate::mcp::config::profile_path),
        settings: config.user_settings_path.clone(),
        workspace: config.workspace_root.clone(),
    });
    mcp.start();
    let loops_mcp = Arc::clone(&mcp);
    // One desk for the session, the switch from the profile: the band asks
    // there at submit, the runtime publishes where to and settles answers.
    let (titles, title_answers) = super::titles::TitleDesk::new(config.session_titles);
    let loops_titles = titles.clone();
    let session_cancel = cancel.clone();
    let stop = cancel.mirror();
    let steering = Steering::new();
    let loops_steering = steering.clone();
    let thread = super::signals::spawn_unsignalled(THREAD_NAME, move || {
        #[cfg(test)]
        RUNTIME_MASKS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((
                std::thread::current().id(),
                super::signals::owned_blocked_here(),
            ));
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
        state.next_session = next_session;
        state.reply = loops_reply;
        state.grants = publishes_grants;
        state.saved_rules = publishes_saved_rules;
        state.live = loops_live;
        state.steering = loops_steering;
        state.rules = loops_rules;
        state.spend = publishes_spend;
        state.credits = publishes_credits;
        state.mcp = loops_mcp;
        state.titles = loops_titles;
        state.title_answers = Some(title_answers);
        if let Some(resumed) = resumed {
            state.resume(resumed, &session_cancel.mirror());
            state.ask_for_bills();
            // A resumed conversation's last reply is `/copy`'s from the
            // start, as fx's hydrated history is (`session.zig:2067-2079`).
            state.publish_reply();
        }
        // `/status` reads the resumed conversation's counts from the start
        // too: `publishing_to` publishes what the runtime is now.
        let state = state.publishing_to(publishes);
        // A set of local tasks around the loop, for the catalog load it
        // starts at launch and runs beside its work
        // ([`ModelSelector::begin_load`]).
        tokio::task::LocalSet::new().block_on(
            &runtime,
            turn_loop(
                state,
                &events_tx,
                &mut work_rx,
                &mut permissions_rx,
                control,
                session_cancel,
                &loops_outstanding,
            ),
        )
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
                live,
                rules,
                grants,
                stop,
                steering,
                steers: Arc::new(AtomicU64::new(0)),
                saved_rules,
                spend,
                credits,
                mcp,
                titles,
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
        let scope = conversation_scope(config).map_err(io::Error::other)?;
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

    /// The provider the session last recorded (`SessionStarted` and every
    /// `PreferencesChanged` since), when it recorded one this build has.
    pub(crate) fn provider(&self) -> Option<ProviderId> {
        self.session
            .state()
            .provider
            .as_deref()
            .and_then(ProviderId::parse)
    }

    /// The provider and the model this session continues on, under `config`
    /// and with `current` the model in force now.
    ///
    /// **The provider it last recorded**, as upstream's resume puts back the
    /// selection the session recorded, provider and model together
    /// (`restoreRuntimePreferences`, `vercel-labs/fx@c1db9191
    /// src/core/app/app_session_runtime.zig:6147`, `replaceSelection`), and
    /// nothing but a process override outranks it -- `--provider` or
    /// `FX_PROVIDER` there (`:6135-6146`), neither of which xfx has. Whether
    /// that provider can authenticate does not decide it: upstream stays on
    /// it and says what is missing (`restoreSessionCredential`,
    /// `app_auth_runtime.zig:151-180`; [`unauthenticated`]).
    ///
    /// **The model with it is the session's**, on the configured provider as
    /// on any other, and only a process override outranks it (`XFX_MODEL`, as
    /// upstream's `process_model_override` does, `:6150-6151`) -- never the
    /// profile's: that is the model a fresh session starts in, and on another
    /// provider a model it was never chosen for.
    ///
    /// **A session that recorded no provider** -- written before the provider
    /// was recorded, every log of the 2026-10-06 preview -- goes on with the
    /// configured one under the launch's rule: the session's model only when
    /// no setting chose one (`app::run_ask`). Its model is one whose provider
    /// the log does not say, and upstream's answer for a session that predates
    /// its recorded preferences is the configured selection
    /// (`loadMigrationPreferences`, `session_migration.zig:343-345`, seeded
    /// from the workspace's at `app_session_runtime.zig:2297`).
    ///
    /// **Never a model xfx would not have written**
    /// ([`crate::provider::model::model_id_problem`]): the log is file input,
    /// and the model chosen here reaches the band's hint row and `/settings`
    /// at launch with no channel to make it inert on the way. Such a model is
    /// not restored, on either branch: `current` is used in its place. Opening
    /// the session writes nothing ([`Runtime::resume`]); its first turn
    /// records the model it runs under ([`record_resumed_selection`]), so the
    /// log is clean again once a turn runs, and until then every resume makes
    /// the same substitution.
    /// Upstream refuses the frame that carries such a model as invalid
    /// instead (`parsePreferences`, `session_event.zig:1972-1976`, over
    /// `validateModel`, `session_codec.zig:67,3165`; `docs/parity.md`). The
    /// band says it once ([`Self::replaces_model`]).
    pub(crate) fn selection(&self, config: &RuntimeConfig, current: &str) -> (ProviderId, String) {
        let provider = self.provider().unwrap_or(config.provider);
        let model = if self.takes_logged_model(config) && !self.replaces_model(config) {
            self.model()
        } else {
            current
        };
        (provider, model.to_string())
    }

    /// Whether the rule in [`Self::selection`] takes the model the log
    /// recorded, before that model is held to anything.
    fn takes_logged_model(&self, config: &RuntimeConfig) -> bool {
        use crate::config::SettingSource;
        match self.provider() {
            Some(_) => config.sources.model != SettingSource::ProcessOverride,
            None => config.sources.model == SettingSource::CompiledDefault,
        }
    }

    /// Whether [`Self::selection`] would have restored the log's model and
    /// put the launch's in its place, because the log's is one xfx would not
    /// have written: what the band says once under the replay, without
    /// quoting either.
    pub(crate) fn replaces_model(&self, config: &RuntimeConfig) -> bool {
        self.takes_logged_model(config)
            && crate::provider::model::model_id_problem(self.model()).is_some()
    }

    /// The effort and the speed switches the session last recorded, if it
    /// recorded any (`restoreRuntimePreferences`,
    /// `app_session_runtime.zig:6126-6175`).
    pub(crate) fn settings(&self) -> Option<&crate::provider::effort::TurnSettings> {
        self.session.state().turn_settings.as_ref()
    }

    /// Whether a Fast binding held for the selection `provider`, `model` and
    /// `fast` survives continuing this session: only when the session last ran
    /// under that very selection (`restoredFastModeModelBound`,
    /// `app_session_runtime.zig:6318-6328` -- the provider's authority, the
    /// model and `fast_mode`, all three).
    ///
    /// The provider is compared by label: upstream's `same_authority` is
    /// `eql` plus the binding identity of a configured provider
    /// (`model_provider.zig:65-71`), and xfx has none -- its providers are
    /// [`ProviderId::ALL`]. A log written before the provider was recorded
    /// says nothing about it, and the binding is then decided by the other
    /// two, as it was before; one that recorded no switches leaves `fast` out
    /// the same way.
    pub(crate) fn keeps_fast_binding(&self, provider: ProviderId, model: &str, fast: bool) -> bool {
        let state = self.session.state();
        state
            .provider
            .as_deref()
            .is_none_or(|recorded| recorded == provider.label())
            && state.model == model
            && state
                .turn_settings
                .as_ref()
                .is_none_or(|settings| settings.fast == fast)
    }

    /// What the session replays as on the screen it continues on
    /// ([`super::hydration`]): read off the state this value already holds, so
    /// the launch builds it with no read of its own.
    ///
    /// Made inert here, by the step the picker's replay takes at the channel
    /// ([`bridge::inert_hydration`]): the launch hands this to the shell
    /// directly, so no channel does it on the way.
    pub(crate) fn hydration(&self) -> super::hydration::Hydration {
        bridge::inert_hydration(super::hydration::Hydration::of(self.session.state(), None))
    }
}

/// The provider a resumed session continues on, when it has nothing to
/// authenticate with: what upstream's resume says once its replay is written
/// (`restoreSessionCredential`'s `.missing`, `vercel-labs/fx@c1db9191
/// src/core/app/app_auth_runtime.zig:151-180`), on any resume -- the provider
/// the session recorded or the one in force. Asked without I/O
/// ([`crate::provider::credential_ready`]), as the hint row asks it.
pub(crate) fn unauthenticated(config: &RuntimeConfig) -> Option<ProviderId> {
    (!crate::provider::credential_ready(config.provider, config)).then_some(config.provider)
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
    /// provider's model. Its catalog is read in the background at launch and
    /// after a switch, as upstream warms its model cache ([`turn_loop`]).
    selector: ModelSelector,
    /// The session's reasoning effort and its Fast and Ultra switches
    /// (`crate::provider::effort`): the effort carried across a model change
    /// and a provider switch alike, as upstream's `app.effort` is, and the
    /// switches off whenever the model changes. Whether a turn *sends* any of
    /// them is decided per turn, against the catalog's row for the model in
    /// force ([`Runtime::request_options`]).
    settings: crate::provider::effort::TurnSettings,
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
    /// The session's saved-session rules, shared with the UI: held there for
    /// the session the next save opens, which writes them
    /// ([`Runtime::open_next`]), or where the open one keeps them, published
    /// with the grants.
    saved_rules: SavedRules,
    /// The mode and reset count the UI writes and every conversation's
    /// authority reads at each decision ([`WorkHandle::change_permissions`]).
    /// **Read here, never written**: see that field.
    live: LiveAuthority,
    /// Titles `/rename` gave after a reset the work they waited for ended
    /// with: written into the session that reset starts, once it has dropped
    /// the one it left ([`write_renamed_after_reset`]). Each with whether the
    /// shell has said it already ([`TurnControl::Rename`]'s `announced`).
    renamed_after_reset: Vec<(String, bool)>,
    /// Where the prompts the person sends into a running turn wait for it
    /// ([`crate::agent::steering`]): opened for each turn, handed to its
    /// machine, closed when it ends. The band holds the same one
    /// ([`WorkHandle::steer`]).
    steering: Steering,
    /// The configured table the UI writes and every conversation's authority
    /// reads ([`WorkHandle::rules`]). Read here, never written.
    rules: LiveRules,
    /// Where `/usage`'s session scope reads the conversation's spend from,
    /// written by [`Runtime::publish`] beside the other two.
    spend: SessionSpend,
    /// When the session `/usage` calls the session began, for its wall clock:
    /// this runtime's start, the moment `/new` dropped the conversation, or
    /// the resumed session's own creation. A provider switch keeps the
    /// conversation and so the clock.
    ///
    /// Upstream's session exists from launch and its clock with it (`reset`,
    /// `session_usage.zig:2300-2320`), and a resumed one counts from when it
    /// was created (`:1810-1816`). xfx opens the session log at the first
    /// prompt, so the log's own creation is the first prompt's time, and the
    /// clock is kept here instead.
    began_ms: i64,
    /// Asks the Gateway for the bills the conversation's streams did not
    /// carry ([`Runtime::ask_for_bills`]), and hands the answers to
    /// [`turn_loop`] to write down ([`Runtime::settle_bill`]).
    bills: super::reconcile::Reconciler,
    /// Where `/credits` reads what to ask from ([`CreditsBoard`]), written by
    /// [`Runtime::publish`] beside the others.
    credits: CreditsBoard,
    /// The session's MCP servers, which every conversation this runtime
    /// opens calls ([`Runtime::ready`]).
    mcp: Arc<crate::mcp::McpRuntime>,
    /// Where the band asks for the session's title ([`super::titles::TitleDesk`]):
    /// this side publishes where to ask ([`Runtime::publish`]), moves the
    /// lineage on a resume, and settles every answer. A provider switch keeps
    /// the conversation and so the lineage.
    titles: super::titles::TitleDesk,
    /// Where the answers arrive: read between pieces of work by [`turn_loop`],
    /// and lent to the turn that is running ([`run_turn`]).
    title_answers: Option<super::titles::Answers>,
    /// The lineage of the conversation that is open, set when it is opened
    /// ([`Runtime::opened`]).
    conversation_lineage: Option<u64>,
    /// A generated title of the lineage in force whose conversation is not
    /// open yet: written the moment it is ([`Runtime::install_pending_title`]).
    pending_title: Option<super::titles::Titled>,
    /// The resumed session whose provider, model and mode are still to be
    /// written down -- at its next turn, not at the open
    /// ([`record_resumed_selection`]).
    unrecorded_resume: Option<crate::session::SessionId>,
    /// The id the next session this runtime opens will have: allocated at
    /// launch, in memory only, and again each time the store is asked for one,
    /// so `/status` names the session the next open -- a first prompt, the
    /// first after a `/new`, or a `/rename` before either -- will write before
    /// it writes anything; a retry after a refusal never asks the store for an
    /// id it may have half made, and one after a failure the store never saw
    /// asks for the id `/status` named.
    next_session: SessionId,
    /// The store refused the last session this runtime asked it for, so
    /// `/status` says this shell can't save sessions until one opens.
    session_refused: bool,
    /// The directories `/workspace` saved for this workspace, each as the real
    /// path it was when it was read: at launch, from the profile, and after
    /// each change, from what the change read back.
    ///
    /// The **identities** a turn's scope is made of ([`Runtime::ready`]),
    /// looked at again before every turn and never resolved again: a saved
    /// directory that has since become a link to somewhere else is out of the
    /// scope rather than the somewhere else in it
    /// (`crate::workspace::AccessScope::with_observed_roots`).
    workspace: WorkspaceAccess,
    /// What the last request measured of the part compaction cannot shrink
    /// -- its instructions and tool definitions -- for a `/compact` between
    /// turns to size its room by, as upstream's keeps the agent's last
    /// measurement (`request_fixed_tokens`, `orchestrator.zig:6191-6192`).
    /// Zero until a turn has measured one.
    fixed_tokens: Arc<AtomicUsize>,
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
        let settings = crate::provider::effort::TurnSettings {
            effort: config.effort.clone(),
            fast: config.fast_mode,
            ultrafast: config.ultrafast_mode,
        };
        let config_mode = config.permission_mode;
        let workspace =
            WorkspaceAccess::load(&config.workspace_root, &config.additional_directories);
        let config_rules = config.permission.effective().clone();
        let (titles, title_answers) = super::titles::TitleDesk::new(config.session_titles);
        let next_session = SessionId::generate();
        let saved_rules = SavedRules::pending(crate::permission::session_rules::path_in(
            &store.session_dir(&next_session),
        ));
        Self {
            config,
            env,
            store,
            conversation: None,
            provider: None,
            prompter,
            questioner,
            selector,
            settings,
            reply: LastReply::default(),
            board,
            grants: Grants::default(),
            saved_rules,
            live: LiveAuthority::new(config_mode),
            renamed_after_reset: Vec::new(),
            steering: Steering::new(),
            rules: LiveRules::new(config_rules),
            spend: SessionSpend::default(),
            began_ms: crate::session::event::system_now_ms(),
            bills: super::reconcile::Reconciler::new(),
            credits: CreditsBoard::default(),
            mcp: crate::mcp::McpRuntime::inert(),
            titles,
            title_answers: Some(title_answers),
            conversation_lineage: None,
            pending_title: None,
            unrecorded_resume: None,
            next_session,
            session_refused: false,
            workspace,
            fixed_tokens: Arc::new(AtomicUsize::new(0)),
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
        self.publish_board();
        // The reset count first: a reset landing between the two reads makes
        // this publication older than it is, never newer, and an older one is
        // read as out of force -- the safe way to be wrong.
        let resets = self.live.resets();
        self.grants.publish(resets, self.granted());
        match self.conversation.as_ref().and_then(|conversation| {
            conversation
                .tools
                .permissions()
                .session_rules_path()
                .map(std::path::Path::to_path_buf)
        }) {
            Some(open) => self.saved_rules.publish_open(open),
            None => self.saved_rules.publish_pending(self.next_rules_path()),
        }
        self.spend.publish(Spent {
            usage: self
                .conversation
                .as_ref()
                .map(|conversation| conversation.recorder.state().usage.clone())
                .unwrap_or_default(),
            started_at_ms: Some(self.began_ms),
        });
        self.credits.publish(crate::credits::question(&self.config));
        self.titles.publish_asker(self.title_asker());
    }

    /// Where the session the next save opens ([`Self::next_session`]) will
    /// keep its saved-session rules.
    fn next_rules_path(&self) -> std::path::PathBuf {
        crate::permission::session_rules::path_in(&self.store.session_dir(&self.next_session))
    }

    /// The status board's half of [`Self::publish`]: what `/status` and
    /// `/trace` read, and nothing else.
    fn publish_board(&self) {
        let mut snapshot = status_snapshot(&self.config, self.model(), self.conversation.as_ref());
        snapshot.ultrafast_requested = self.settings.ultrafast;
        snapshot.session = Some(self.shell_session());
        self.board.publish_with(
            snapshot,
            TraceFacts {
                session: self.conversation.as_ref().map(|conversation| {
                    (
                        self.store.profile_dir().to_path_buf(),
                        conversation.recorder.id().clone(),
                    )
                }),
                settings: crate::trace::Settings::of(&self.config),
            },
        );
    }

    /// The session `/status` names (`shellSession`,
    /// `session_commands.zig:404-408@481104fd`): the open conversation's,
    /// saved -- its log is on disk, so `xfx --resume <id>` opens it -- or,
    /// with none open, the id the next prompt will open, not saved yet; or
    /// none, when the store refused the last one this shell asked for.
    fn shell_session(&self) -> ShellSession {
        match &self.conversation {
            Some(conversation) => ShellSession::Open {
                id: conversation.recorder.id().to_string(),
                saved: true,
            },
            None if self.session_refused => ShellSession::Unavailable,
            None => ShellSession::Open {
                id: self.next_session.to_string(),
                saved: false,
            },
        }
    }

    /// Opens the conversation a prompt -- or a `/rename` before one -- writes
    /// into, under the id `/status` has been naming ([`Self::next_session`]),
    /// and allocates the next one once the store has been asked for this one,
    /// whether or not it took it. A failure before the store is asked -- a
    /// workspace that cannot be scoped -- keeps the id, so the next attempt
    /// opens the session `/status` names.
    ///
    /// **The log's first save says what the session is in**, as upstream's
    /// does: everything its session is set to before the first turn is held
    /// in memory and written with that turn (`hold`, `writeFirstTurn`,
    /// `src/core/session_manager/session.zig:415-429`, `:503-539@481104fd`),
    /// starting from the preferences it was created with (`appendSeed`,
    /// `session_adapter.zig:1289-1299`). Here the model, the provider and the
    /// permission mode in force are `session_started`'s, and the effort and
    /// both switches -- `/settings`' effort row, a `/fast` or a `/model <id>
    /// <effort>` used before the first prompt, which had no log to go into --
    /// are the `preferences_changed` written right behind it, whatever they
    /// are: a resume restores them from there ([`Runtime::resume_by_id`]). A
    /// frame of a kind every older xfx reads, with fields one that predates
    /// them ignores (`tests/session_compat.rs`).
    ///
    /// **And the saved-session rules confirmed before it**, held until now in
    /// [`SavedRules`]: written into the session's directory before its log
    /// is, so no one sees the session without them and its first turn's first
    /// call is decided by them, as upstream's held state is in force for its
    /// first turn and written in the one publish that makes the session
    /// visible (`writeFirstTurn`, `:503-539@481104fd`). Under the store's lock
    /// from the read of what is held to the moment the file is the store, so
    /// a confirmation the UI makes in between waits and lands in the file.
    /// Rules that cannot be written open nothing -- the prompt is refused,
    /// says why, and runs no tool, as upstream's failed first publish fails
    /// its turn before any call runs (`appendRunningToolCalls`,
    /// `src/core/agent/runtime/orchestrator.zig:3894-3917@481104fd`) -- and
    /// stay held, for the next attempt under the next id.
    fn open_next(&mut self, mirror: &CancelToken) -> Result<(), String> {
        let model = self.model().to_string();
        let target = self.next_rules_path();
        let store = self.saved_rules.clone();
        let mut slot = store.lock();
        let held = slot.first_save(&target);
        let opened = open_conversation_as(
            &self.store,
            self.next_session.clone(),
            &self.config,
            &model,
            self.authority(),
            held.as_ref(),
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
        );
        match opened {
            Ok(conversation) => {
                slot.opened(target);
                drop(slot);
                self.next_session = SessionId::generate();
                self.conversation = Some(conversation);
                self.session_refused = false;
                self.record_settings();
                self.opened();
                Ok(())
            }
            Err(OpenFailure {
                message,
                store_refused,
            }) => {
                drop(slot);
                if store_refused {
                    // Never asked for again: the store may have made half of it.
                    self.next_session = SessionId::generate();
                    if !self.session_refused {
                        self.session_refused = true;
                        self.publish_board();
                    }
                }
                Err(message)
            }
        }
    }

    /// The conversation's grants as a bare `/permissions` lists them, the
    /// target as a listing says it. Not made safe: the panel's rows are
    /// (`super::info_panel::permissions_rows`).
    fn granted(&self) -> Vec<Granted> {
        let Some(conversation) = self.conversation.as_ref() else {
            return Vec::new();
        };
        conversation
            .tools
            .permissions()
            .grants()
            .iter()
            .map(|grant| Granted {
                tool: grant.tool.clone(),
                target: crate::permission::slash::grant_target(
                    &grant.tool,
                    &grant.target,
                    &self.config.workspace_root,
                ),
            })
            .collect()
    }

    /// Records one change to the session's authority, **between pieces of
    /// work**; answers what the session has to be told, if anything.
    ///
    /// The change is already in force: the handle put it in the live
    /// authority every decision reads ([`WorkHandle::change_permissions`]), so
    /// a turn that was running when it was made judged its next call by it.
    /// What is left is the record, and it is here because the session log is
    /// held by a running turn and free only between pieces of work. The
    /// configuration, because a conversation this runtime opens later is
    /// opened from it and its log starts with that mode. The conversation's
    /// own copy of the mode, and the session log, as [`Runtime::resume`]
    /// records a mode that changed, so a later resume reads what this session
    /// really ran under; for a reset, the forgetting of every grant given
    /// before it, so a resume does not bring them back
    /// ([`crate::interactive::Conversation::set_permission_mode`]). Never the
    /// live authority: a message drained here can be older than what the
    /// person has chosen since.
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
        match self.persist(
            "permission_mode",
            serde_json::Value::String(mode.label().to_string()),
        ) {
            Ok(()) => crate::provider::profile::saved_but_shadowed(
                "permission_mode",
                self.config.sources.permission_mode,
            )
            .map(|said| super::notice::Notice::neutral("permission-mode", said)),
            Err(reason) => Some(permissions::not_saved("permission-mode", &reason)),
        }
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
    /// Starts asking the Gateway for the bills of the conversation's pending
    /// requests, with the Gateway credential that made them
    /// (`startReconciliationWithCredential`, `session_usage.zig:2008-2100`).
    ///
    /// Called after every piece of work, so a turn that recorded one and a
    /// resume that opened a session holding one are both asked about, and
    /// asked about once ([`super::reconcile::Reconciler::ask`]). Without a
    /// Gateway credential nothing is asked and the requests stay pending, as
    /// upstream withholds its asking without a credential for them
    /// (`:1924-1945`).
    fn ask_for_bills(&mut self) {
        let Some(conversation) = self.conversation.as_ref() else {
            return;
        };
        if self.config.provider != crate::provider::ProviderId::Gateway {
            return;
        }
        let Some(credential) = self.config.credential.as_ref() else {
            return;
        };
        let state = conversation.recorder.state();
        let pending: Vec<&crate::session::RequestUsage> = state.usage.pending().collect();
        if pending.is_empty() {
            return;
        }
        self.bills.ask(
            conversation.recorder.id().as_str(),
            &pending,
            credential.secret(),
        );
    }

    /// Writes what the Gateway said about a pending request into the log of
    /// the session it belongs to, and tells `/usage` (`applyGeneration`,
    /// `session_usage.zig:1080-1126`).
    ///
    /// Only into the conversation in front of the user: an answer about one
    /// that was dropped or replaced is left unwritten, and that session's
    /// request stays pending for the next time it is opened.
    fn settle_bill(&mut self, answered: &super::reconcile::Answered) {
        let Some(conversation) = self.conversation.as_mut() else {
            return;
        };
        if conversation.recorder.id().as_str() != answered.session {
            return;
        }
        conversation.recorder.commit(answered.frame());
        self.publish();
    }

    /// Where a title is asked from now: the Gateway with its key, or nobody
    /// (`sessionTitleModel`, `vercel-labs/fx@c1db9191 src/main.zig:471-475`:
    /// the Gateway names a title model, a configured provider -- llmux's
    /// place here -- names none), at the endpoint every turn goes to
    /// (`Bundle::select`).
    fn title_asker(&self) -> Option<(crate::gateway::Endpoint, crate::config::Credential)> {
        if self.config.provider != ProviderId::Gateway {
            return None;
        }
        let credential = self.config.credential.clone()?;
        let endpoint = crate::gateway::Endpoint::from_process().ok()?;
        Some((endpoint, credential))
    }

    /// Drops a title that came back for a conversation not yet opened, now
    /// that it never will be (`session_changed`,
    /// `app_session_runtime.zig:3720-3728`).
    fn drop_pending_title(&mut self) {
        if self.pending_title.take().is_some() {
            self.titles.record_dropped("session_changed");
        }
    }

    /// The runtime has got past `worked` pieces of work, and asks for the
    /// session's title from a prompt the band held for a switch it has now
    /// made ([`super::titles::TitleDesk::worked_through`]) -- **of the
    /// conversation that prompt will run in**, as upstream's finalized prompt
    /// is snapshotted against the session in force then
    /// (`finalizePendingSubmission` -> `snapshotPrompt`, `src/main.zig:1238-1266`,
    /// `:1479-1481`). The gate's half the band would have read is read from
    /// the conversation itself: no title on record, none come back for it,
    /// and no turn in it (`session_untitled`, `app_session_runtime.zig:3674-3675`).
    /// After a provider switch that is the conversation it kept, which asks
    /// only if no turn has run in it; after a resume, the resumed one, whose
    /// turns ask nothing.
    fn worked_through(&mut self, worked: usize) {
        let Some(excerpt) = self.titles.worked_through(worked) else {
            return;
        };
        let untitled = self.pending_title.is_none()
            && self.conversation.as_ref().is_none_or(|conversation| {
                let state = conversation.recorder.state();
                state.renamed.is_none() && state.turns.is_empty()
            });
        if untitled {
            self.titles.ask(&excerpt);
        }
    }

    /// Records that a conversation was just opened, in the lineage in force,
    /// and writes the title waiting for it, if one is -- one the band was
    /// shown when it came back.
    fn opened(&mut self) {
        self.conversation_lineage = Some(self.titles.lineage());
        self.publish_title_session();
        // `/status` says the session is saved from here, while the turn that
        // opened it runs, as upstream's does once its log is on disk
        // (`cap/panels/new/fx-p5-panels-status-first-40x120-open-first`).
        self.publish_board();
        if let Some(titled) = self.pending_title.take() {
            let _ = self.install_title(&titled);
        }
    }

    /// Tells the desk which session is open, for `/trace`'s `session=`
    /// ([`super::titles::TitleDesk::publish_session`]).
    fn publish_title_session(&self) {
        self.titles.publish_session(
            self.conversation
                .as_ref()
                .map(|conversation| conversation.recorder.id().as_str()),
        );
    }

    /// Takes one answer off the books and gives the session its title when it
    /// is one to give, answering the title the band is to show
    /// (`pollSessionTitleGeneration`, `app_session_runtime.zig:3706-3779`).
    ///
    /// `settled` when the turn that read it has settled it already
    /// ([`titled_while`]). An answer to a question that is not the one out is
    /// no answer.
    fn title_arrived(&mut self, titled: &super::titles::Titled, settled: bool) -> Option<String> {
        if !settled && !self.titles.settle(titled) {
            return None;
        }
        self.install_title(titled)
    }

    /// Gives the session `titled`'s title if it is still one to give.
    ///
    /// **About a conversation let go of** -- an older lineage than the one in
    /// force -- it is dropped (`session_changed`). **About the one in force,
    /// whose conversation is not open yet** -- the first prompt has not
    /// started its turn, or the band has already let the open one go -- it
    /// waits for that conversation ([`Self::pending_title`]) and the band is
    /// shown it now: the conversation it waits for is the one the band shows,
    /// and a title that has none yet is no title of the user's. Otherwise
    /// upstream's rule decides (`installGeneratedTitle`,
    /// `session_title_generation.zig:378-394`; v2's,
    /// `session_adapter.zig:1675-1697`): the title on record must be none, or
    /// the one derived from the first prompt -- which upstream writes down at
    /// the first turn's commit (`session_adapter.zig:1300-1310`) and xfx
    /// derives where it reads it ([`crate::session::DurableState::derived_title`]).
    /// Anything else is a title the user chose (`user_title_present`).
    fn install_title(&mut self, titled: &super::titles::Titled) -> Option<String> {
        let title = titled.title()?;
        let current = self.titles.lineage();
        if titled.lineage < current {
            self.titles.record_dropped("session_changed");
            return None;
        }
        if self.conversation_lineage != Some(current) || self.conversation.is_none() {
            self.pending_title = Some(titled.clone());
            return Some(title.to_string());
        }
        let conversation = self.conversation.as_mut()?;
        let state = conversation.recorder.state();
        if let Some(on_record) = &state.renamed {
            if state.derived_title().as_deref() != Some(on_record.as_str()) {
                self.titles.record_dropped("user_title_present");
                return None;
            }
        }
        if crate::session::validate_title(title) != Ok(title) {
            self.titles.record_dropped("install_failed");
            return None;
        }
        conversation.recorder.commit(SessionEvent::SessionRenamed {
            title: title.to_string(),
            source: crate::session::TitleSource::Generated,
        });
        if conversation.recorder.failure().is_some() {
            self.titles.record_dropped("install_failed");
            return None;
        }
        // `/status` and `/trace` read the session's title from its log.
        self.publish();
        Some(title.to_string())
    }

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
            self.open_next(mirror)?;
        }
        // **The scope this turn runs in**, made now from the saved directories
        // that are still themselves ([`Runtime::workspace`]): a `/workspace`
        // change between turns is in force from the next one, and a directory
        // that vanished or was swapped for a link since is not.
        let scope = self
            .workspace
            .refreshed()
            .scope(&self.config.workspace_root)
            .map_err(|err| err.to_string())?;
        if let Some(conversation) = self.conversation.as_mut() {
            conversation.tools.set_scope(scope);
            // Every conversation of this session calls its MCP servers,
            // however it was opened -- a first prompt, a `/rename`, a resume.
            if conversation.tools.mcp().is_none() {
                conversation.tools = conversation.tools.clone().with_mcp(Arc::clone(&self.mcp));
            }
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
            self.open_next(mirror)?;
        }
        self.conversation
            .as_mut()
            .ok_or_else(|| "the conversation could not be opened".to_string())
    }

    /// `/undo` against the conversation as it stands: its newest change as of
    /// `through` taken back ([`crate::tools::mutate::undo_through`]), or
    /// nothing when there is no conversation -- a session that has asked
    /// nothing has changed nothing, and one `/new` dropped took its changes
    /// with it. A provider switch keeps them, as upstream's keeps its tracker
    /// (`runProviderCommand`, `app_auth_runtime.zig:387`, never reaches the
    /// clear at `app_session_runtime.zig:1999`).
    fn undo(&self, through: crate::tools::undo::Mark) -> crate::tools::undo::Undone {
        self.conversation
            .as_ref()
            .map_or(crate::tools::undo::Undone::Nothing, |conversation| {
                crate::tools::mutate::undo_through(&conversation.tools, through)
            })
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

    /// One `/workspace` change, between turns, and what the band is told of
    /// it (`handleWorkspaceCommand`, `vercel-labs/fx@c1db9191
    /// src/core/app/app_commands.zig:245-328`).
    ///
    /// The profile is changed and read back ([`workspace_access::execute`]);
    /// what was read back is what the next turn's scope is made of
    /// ([`Runtime::ready`]) -- this runs only between turns, so no tool call
    /// sees half of it -- and what a conversation `/new` opens is made of. A
    /// change the profile did not take changes nothing here either, as
    /// upstream's durable-or-nothing rule has it.
    fn change_workspace(&mut self, action: &workspace_access::Action) -> UiEvent {
        let primary = self.config.workspace_root.clone();
        let updated = match workspace_access::execute(
            &primary,
            self.config.user_settings_path.as_deref(),
            &self.workspace,
            action,
        ) {
            Ok(updated) => updated,
            Err(failure) => {
                return UiEvent::Workspace {
                    notice: Notice::error(workspace_access::TOPIC, failure.message()),
                    access: None,
                }
            }
        };
        self.config.additional_directories = updated.access.saved();
        self.workspace = updated.access.clone();
        UiEvent::Workspace {
            notice: Notice::neutral(
                workspace_access::TOPIC,
                workspace_access::snapshot(&primary, &updated.access, Some(&updated.mutation)),
            ),
            access: Some(updated.access),
        }
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
    /// The provider and the model follow the launch's rule
    /// ([`Resumption::selection`]): the session continues on the provider it
    /// last recorded, as upstream's picker resume does (`hydrateResumedSession`
    /// → `restoreRuntimePreferences`, `app_session_runtime.zig:2404`,
    /// `:6147`), and in the model it recorded there, which only `XFX_MODEL`
    /// outranks (`:6150-6151`). Nothing is written to its log here
    /// ([`Runtime::resume`]); its first turn writes what it goes on under.
    fn resume_by_id(
        &mut self,
        id: &str,
        mirror: &CancelToken,
    ) -> Result<super::hydration::Hydration, super::session_picker::ResumeFailure> {
        use super::session_picker::ResumeFailure;
        let id = crate::session::SessionId::parse(id).map_err(|_| ResumeFailure::Unavailable)?;
        let scope = conversation_scope(&self.config).map_err(|_| ResumeFailure::Unavailable)?;
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
        // The selection this session was configured with, before the resume
        // moves it: what a restored Fast binding is judged against
        // ([`Resumption::keeps_fast_binding`]).
        let keeps_fast_binding =
            resumption.keeps_fast_binding(self.config.provider, self.model(), self.settings.fast);
        let (provider, model) = resumption.selection(&self.config, self.model());
        let moved = provider != self.config.provider;
        if moved || self.model() != model {
            self.config.provider = provider;
            self.config.model = model;
            // On the same provider the same catalog -- read, or on its way --
            // describes the session's model; on another one the selector is
            // built anew, with no catalog until that provider's is read
            // (`resetForProviderChange` only on a provider change,
            // `app_auth_runtime.zig:151-158`), which the loop starts as soon
            // as it is idle again ([`begin_catalog_read`]).
            self.selector.follow_model(&self.config);
        }
        if moved {
            // A connection to the provider the session is leaving, as a
            // switch drops it ([`Self::adopt`]).
            self.provider = None;
        }
        // The effort and the switches the session recorded, as upstream's
        // resume restores them (`restoreRuntimePreferences`,
        // `app_session_runtime.zig:6126-6175`); a session that never recorded
        // them keeps the ones in force.
        if let Some(settings) = resumption.session.state().turn_settings.clone() {
            self.settings = settings;
        }
        let mut replay = super::hydration::Hydration::of(
            resumption.session.state(),
            Some(self.model().to_string()),
        );
        replay.settings = Some(self.settings.clone());
        replay.keeps_fast_binding = keeps_fast_binding;
        // The band is on the provider it was on: told where the session
        // continues, and whether that provider can authenticate, when the
        // resume moved it -- and, on any resume, told to say what is missing.
        replay.provider = moved.then(|| {
            (
                provider,
                !crate::provider::credential_ready(provider, &self.config),
            )
        });
        replay.unauthenticated = unauthenticated(&self.config);
        // And whether the session's model was one it could not restore
        // ([`Resumption::replaces_model`]; the sources it reads are the
        // launch's, which the resume above does not move).
        replay.model_replaced = resumption.replaces_model(&self.config);
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

    /// How large the conversation may be for the model in force, and how far
    /// the estimate ran from the provider's count on its last request
    /// (`compactionSize`, `vercel-labs/fx@c1db9191
    /// src/core/agent/runtime/orchestrator.zig:6187-6200`): the catalog row's
    /// window, or on the Gateway the one upstream knows the model's id by
    /// ([`Self::context_window`]). With neither, nothing is known, and
    /// upstream's rule for that holds: a manual compaction keeps the newest
    /// four turns, and nothing compacts by itself (`window.zig:127-131`).
    fn compaction_size(&self) -> Size {
        let entry = self.selector.entry(self.selector.model());
        let mut size = Size::of(
            self.context_window(),
            entry.and_then(|entry| entry.facts.max_output),
            compact::DEFAULT_PERCENT,
        );
        size.fixed = Some(self.fixed_tokens.load(Ordering::Relaxed)).filter(|fixed| *fixed > 0);
        size.correction = self
            .calibration()
            .and_then(|calibration| calibration.correction(self.model()));
        size
    }

    /// The model's window: its catalog row's, else on the Gateway the one
    /// upstream gives the id before a catalog says otherwise
    /// (`mergeCapabilities`, `model_capabilities.zig:107`;
    /// `crate::provider::model::gateway_context_window`).
    fn context_window(&self) -> Option<u64> {
        self.selector
            .entry(self.selector.model())
            .and_then(|entry| entry.max_context)
            .or_else(|| {
                (self.config.provider == ProviderId::Gateway)
                    .then(|| crate::provider::model::gateway_context_window(self.model()))
                    .flatten()
            })
    }

    /// The provider's count of the conversation's last measured request, while
    /// the history it was counted on is the one sent: no compaction since
    /// (`agent.request_token_calibration`, `orchestrator.zig:7458`).
    fn calibration(&self) -> Option<compact::Calibration> {
        let conversation = self.conversation.as_ref()?;
        let compactions = compactions_of(conversation.recorder.state());
        conversation
            .calibration
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .filter(|calibration| calibration.compactions == compactions)
    }

    /// Whether the conversation holds a turn no compaction covers yet
    /// (`hasContextToCompact`, `session.zig:1946-1951`).
    fn has_context_to_compact(&self) -> bool {
        self.conversation
            .as_ref()
            .is_some_and(|conversation| !conversation.recorder.state().uncovered_turns().is_empty())
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
            // Read at every decision, so a change the person makes while a
            // turn runs governs that turn's next call (`LiveAuthority`).
            .with_live(self.live.clone())
            // And the configured table, for the same reason (`LiveRules`).
            .with_live_rules(self.rules.clone())
    }

    /// Makes a saved session this runtime's conversation, before any work.
    ///
    /// Bound the way [`Runtime::ready`] binds a new one -- this thread's
    /// authority, the session's requester, the session's cancellation -- so a
    /// resumed conversation differs from a fresh one only in what its log
    /// already holds: its history, which every turn replays from the log
    /// (`one_turn`), and its grants ([`bind_conversation`]).
    ///
    /// **Nothing is written to the log here.** The provider, the model and the
    /// permission mode it goes on under are written down where they differ
    /// from what the log last recorded -- but at its next turn, ahead of that
    /// turn's prompt ([`record_resumed_selection`]), as `xfx ask --resume`
    /// writes them ahead of its one (`app::run_ask`). Upstream's resume
    /// appends nothing: its preferences reach the log only through a command
    /// that changes them (`commitRuntimePreferences`, `vercel-labs/fx@c1db9191
    /// src/core/app/app_session_runtime.zig:3469-3554`), never through
    /// `hydrateResumedSession` (`:2368-2420`). A session opened to be read and
    /// left is a session its owner did not change.
    fn resume(&mut self, resumed: Resumption, mirror: &CancelToken) {
        self.bills.forget();
        // Another conversation: a question about the one it replaces runs on
        // to an answer that is dropped, and one already back and waiting for
        // that conversation to open is dropped now.
        self.titles.next_lineage();
        self.drop_pending_title();
        let conversation = bind_conversation(
            &self.store,
            resumed.session,
            resumed.scope,
            self.authority(),
            Some(Arc::clone(&self.questioner) as Arc<dyn crate::tools::QuestionRequester>),
            mirror,
        );
        self.unrecorded_resume = Some(conversation.recorder.id().clone());
        // A resumed session's clock is its own: from when it was created.
        self.began_ms = conversation.recorder.state().created_at_ms;
        self.conversation = Some(conversation);
        self.conversation_lineage = Some(self.titles.lineage());
        self.publish_title_session();
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
    /// The bundle is dropped **here and not earlier**, so a transaction that
    /// failed anywhere above leaves a working session alone; it is a connection
    /// to an endpoint that is no longer configured.
    ///
    /// **The conversation is kept**, as upstream's `/provider` keeps its
    /// session (`app.provider_selection.adoptOwned`, `vercel-labs/fx@c1db9191
    /// src/core/app/app_auth_runtime.zig:1377-1381`, touches nothing of it):
    /// its log, its grants, its read proofs and its `/undo` journal go on, and
    /// the next turn replays its history in the new provider's wire. That is
    /// safe because the history is never one wire's bytes: it is rebuilt from
    /// the log for every request ([`crate::session::DurableState::history_messages`]),
    /// a step's replay state goes only to the wire that sealed it -- upstream's
    /// `projectProviderReplay`, `types.zig:1548-1563` -- and a call id the new
    /// wire cannot take goes under a portable alias
    /// ([`crate::gateway::tool_call_ids`]). The switch itself is written into
    /// the log, provider and model both, as upstream's patch writes them
    /// (`commitRuntimePreferences`, `app_session_runtime.zig:3539-3550`).
    fn adopt(&mut self, reloaded: RuntimeConfig) -> Selected {
        // **Except the session's permission mode**, which is not what the file
        // says but what the person last chose: `/permissions` and Shift-Tab
        // write the profile too, but an `XFX_PERMISSION_MODE` that outranks it
        // would otherwise take a provider switch back to the launch's mode
        // while the hint row goes on saying the chosen one. The same for a
        // warning already shown: it stays shown.
        let mode = self.config.permission_mode;
        let acknowledged = self.config.yolo_acknowledged || reloaded.yolo_acknowledged;
        // The switches go off with the model the switch replaces, as upstream's
        // go off on any model change, and the profile is told so -- a launch
        // that read them back would otherwise be in Fast on a model that was
        // never asked for it. The effort stays: it is the session's.
        let switches_were_on = self.settings.fast || self.settings.ultrafast;
        self.settings.fast = false;
        self.settings.ultrafast = false;
        self.config = reloaded;
        self.config.permission_mode = mode;
        self.config.yolo_acknowledged = acknowledged;
        if switches_were_on {
            // Best effort: the switch has been made, and a profile that kept
            // a stale `fast_mode` is read with the binding it carries.
            let _ = self.save_speed("provider");
        }
        // The directories too: the file was read again, and what it saves
        // for this workspace is what a turn runs in.
        self.workspace = WorkspaceAccess::load(
            &self.config.workspace_root,
            &self.config.additional_directories,
        );
        // The model comes back with it, because the selector is where the model
        // is: rebuilding it from the reloaded configuration is what makes the
        // switch's model the one the file now says rather than the one the
        // session was in.
        // Its catalog is read in the background as soon as the loop is idle
        // again ([`turn_loop`]).
        self.selector = ModelSelector::new(&self.config);
        self.provider = None;
        let provider = self.config.provider.label().to_string();
        let model = self.model().to_string();
        if let Some(conversation) = self.conversation.as_mut() {
            conversation
                .recorder
                .commit(SessionEvent::PreferencesChanged {
                    model: Some(model),
                    permission_mode: None,
                    provider: Some(provider),
                    effort: None,
                    fast_mode: None,
                    ultrafast_mode: None,
                });
        }
        // And the switches that went off above, into the same log: a resume
        // restores the session's settings from it (`Runtime::resume_by_id`),
        // and one that still said Fast would turn it back on for a model
        // that was never asked for it.
        self.record_settings();
        // The bills the Gateway still owes this session are asked with the
        // Gateway's credential, and only while it is the provider: upstream
        // drops its asking credential on a switch to a provider that is not
        // the Gateway and starts again with the new one on a switch to it
        // (`reconcileGatewayCredential`, `app_auth_runtime.zig:2039-2070`).
        // Forgotten here, the requests stay pending in the log, and the asking
        // after this work ([`Runtime::ask_for_bills`]) starts again on the
        // Gateway.
        self.bills.forget();
        // The title machinery's lineage stays where it is, and so does a
        // title waiting for the conversation to open: the conversation the
        // band shows is the one it showed before the switch, as upstream's
        // session is (a question out runs on and is answered into it).
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

    /// Loads the catalog if nothing has asked for it yet, and says whether this
    /// call is the one that did -- which is when the UI has to be told
    /// ([`Runtime::catalog_event`]), because its rows are what the band
    /// resolves a query and fills the effort column from.
    async fn load_catalog(&mut self) -> bool {
        // A read already on its way tells the UI itself, when it answers
        // ([`begin_catalog_read`]).
        let unasked = matches!(self.selector.catalog(), CatalogState::NotLoaded)
            && !self.selector.is_loading();
        self.selector.ensure_catalog().await;
        unasked && !matches!(self.selector.catalog(), CatalogState::NotLoaded)
    }

    /// What the UI is told about the catalog as it stands: its rows, every one
    /// in the menu's order; no rows, for a provider with none; or that it
    /// could not be read, and whether asking again could help.
    fn catalog_event(&self) -> UiEvent {
        catalog_event_of(
            self.config.provider,
            self.selector.catalog(),
            self.selector.failure_retry(),
            self.selector.last_attempt(),
        )
    }

    /// What every request of the next turn asks for beside its prompt: the
    /// session's effort, **if** the catalog lists it for the model in force
    /// (`Effort::for_request`; `resolveProviderOptionsForCapabilities`,
    /// `model_capabilities.zig:194-206`), and Fast, if the row offers it. A
    /// model the catalog does not describe -- not listed, or no catalog read --
    /// is sent neither, which is upstream's fallback (`capabilitiesForModel`
    /// knows no levels and no Fast).
    ///
    /// **Ultra is the exception, and it is a refusal rather than a silent
    /// downgrade** (`resolveUltrafastProviderOptions`, `:209-224`: "unsupported
    /// or unknown capabilities must not silently downgrade before sending"):
    /// a session in Ultra mode whose model is not an OpenAI one the Gateway's
    /// catalog offers it for does not send the turn at all, and says why.
    fn request_options(&self) -> Result<crate::gateway::protocol::RequestOptions, String> {
        let entry = self.selector.entry(self.selector.model());
        let ultrafast = self.settings.ultrafast;
        if ultrafast
            && !(self.config.provider == ProviderId::Gateway
                && self.selector.model().starts_with("openai/")
                && entry.is_some_and(|entry| entry.facts.ultrafast))
        {
            return Err(ULTRAFAST_UNAVAILABLE.to_string());
        }
        Ok(crate::gateway::protocol::RequestOptions {
            reasoning: self.settings.effort.for_request(entry),
            fast: !ultrafast && self.settings.fast && entry.is_some_and(|entry| entry.facts.fast),
            ultrafast,
            // A turn's answer is never capped; only a title's is
            // (`crate::tui::titles::options`).
            max_output_tokens: None,
        })
    }

    /// Writes the session's effort and switches into its log, where a resume
    /// reads them back -- once per change, as a group.
    fn record_settings(&mut self) {
        let settings = self.settings.clone();
        let Some(conversation) = self.conversation.as_mut() else {
            return;
        };
        if conversation.recorder.state().turn_settings.as_ref() == Some(&settings) {
            return;
        }
        conversation
            .recorder
            .commit(SessionEvent::PreferencesChanged {
                model: None,
                permission_mode: None,
                provider: None,
                effort: Some(settings.effort.label().to_string()),
                fast_mode: Some(settings.fast),
                ultrafast_mode: Some(settings.ultrafast),
            });
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
                        provider: None,
                        effort: None,
                        fast_mode: None,
                        ultrafast_mode: None,
                    });
            }
        }
        outcome
    }

    /// The three speed keys as the session stands, as upstream writes them:
    /// the binding always `true`, because an interactive change is always made
    /// for the model in force ([`super::speed_save::switches`]).
    fn speed_preferences(&self) -> [(&'static [&'static str], serde_json::Value); 3] {
        super::speed_save::switches(self.settings.fast, self.settings.ultrafast)
    }

    /// Saves the speed switches with what upstream writes beside them, for
    /// the provider and the model in force -- under an `XFX_MODEL`, that model
    /// -- and says what upstream's report keeps ([`super::speed_save::save`]).
    /// `topic` is upstream's label for the write.
    fn save_speed(&self, topic: &'static str) -> Option<super::notice::Notice> {
        let path = self.config.user_settings_path.as_ref()?;
        super::speed_save::save(
            path,
            self.config.provider,
            self.model(),
            self.settings.fast,
            self.settings.ultrafast,
            &self.config.sources,
            topic,
        )
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
        provider: ProviderId,
        model: &str,
        effort: Option<&crate::provider::effort::Effort>,
        picker: bool,
    ) -> Option<super::notice::Notice> {
        let path = self.config.user_settings_path.as_ref()?;
        let topic = if picker { "model picker" } else { "model" };
        let model_key = [crate::config::MODELS_KEY, provider.label()];
        let provider_key = [crate::config::PROVIDER_KEY];
        let effort_key = [crate::config::EFFORT_KEY];
        let mut preferences: Vec<(&[&str], serde_json::Value)> =
            vec![(&model_key, serde_json::Value::String(model.to_string()))];
        // And the provider, as upstream's patch carries it (`selectModelFromPicker`,
        // `setResolvedModel`, `session_commands.zig:894-896`, `:1180-1181`) --
        // but not for a change chosen under a provider a switch has since
        // replaced, which would write the old provider back over the switch.
        if provider == self.config.provider {
            preferences.push((
                &provider_key,
                serde_json::Value::String(provider.label().to_string()),
            ));
        }
        if let Some(effort) = effort {
            preferences.push((
                &effort_key,
                serde_json::Value::String(effort.label().to_string()),
            ));
        }
        // The switches with it, as upstream writes them beside every model
        // change (`persistPreferenceTargets`; `fx-model-router-*` dumps:
        // `"fast_mode":false,"ultrafast_mode":false,"fast_mode_model_bound":true`).
        preferences.extend(self.speed_preferences());
        match crate::provider::profile::write_preferences(path, &preferences) {
            Ok(()) => super::speed_save::shadowed(&self.config.sources, effort.is_some())
                .map(|said| super::notice::Notice::neutral(topic, said)),
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
/// The whole of a catalog's telling that is not the load itself, split out so
/// that the mapping is a claim a test can make over all four states without a
/// socket. `Unavailable` and `NotLoaded` answer the same way because they are
/// the same fact from the UI's side -- there is nothing to browse -- and
/// answering rather than panicking keeps a report from taking the session down.
/// What the UI is told about `provider`'s catalog in `catalog`, `retry`
/// saying whether asking again could change a failure and `attempted` when
/// the read that failed began ([`Runtime::catalog_event`]). A failure with
/// no attempt on record -- only a test's -- is counted from now.
fn catalog_event_of(
    provider: ProviderId,
    catalog: &CatalogState,
    retry: Option<crate::provider::model::CatalogRetry>,
    attempted: Option<Instant>,
) -> UiEvent {
    match from_state(catalog) {
        CatalogAnswer::Rows(entries) => UiEvent::CatalogLoaded { provider, entries },
        // The provider has no catalog; nothing went wrong, and the menu
        // says there is nothing in it.
        CatalogAnswer::NoneAdvertised => UiEvent::CatalogLoaded {
            provider,
            entries: Vec::new(),
        },
        CatalogAnswer::Unread(_) => UiEvent::CatalogUnread {
            provider,
            retry,
            attempted: attempted.unwrap_or_else(Instant::now),
        },
    }
}

fn from_state(state: &CatalogState) -> CatalogAnswer {
    match state {
        CatalogState::Loaded(entries) => CatalogAnswer::Rows(in_menu_order(entries)),
        CatalogState::Failed(reason) => CatalogAnswer::Unread(reason.clone()),
        CatalogState::Unavailable | CatalogState::NotLoaded => CatalogAnswer::NoneAdvertised,
    }
}

/// The rows of `entries` a UI is given: **every one**, in the order the menu
/// lists them ([`crate::provider::model::picker_order`]).
///
/// Every one, because the band resolves a typed `/model <query>` against
/// these rows by the selector's own rule and announces what it finds -- and a
/// band holding fewer rows than the selector could resolve a query to a
/// different model than the one the runtime would have chosen. Upstream's
/// menu lists the whole catalog too. What bounds the list is the catalog
/// document itself: it is read through a 1 MiB cap
/// (`provider::model`'s `MAX_CATALOG_BODY_BYTES`), so no daemon can make this
/// event unbounded.
fn in_menu_order(entries: &[CatalogEntry]) -> Vec<CatalogEntry> {
    crate::provider::model::picker_order(entries)
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
        // Before anything else is taken: a switch the runtime has got past --
        // made, or dropped unstarted by an interrupt -- gives back the prompt
        // the band held for it, which asks now, ahead of its own turn
        // ([`Runtime::worked_through`]).
        state.worked_through(queue.taken);
        // **The catalog, read in the background** whenever the session holds
        // none and is not reading one: at launch, as upstream reads it on a
        // thread of its own once the session is up (`app.startModelCacheWarmup()`,
        // `vercel-labs/fx@c1db9191 src/core/app/app_entry_runtime.zig:335`;
        // `startWarmup`, `model_cache_runtime.zig:321-349`), and after a
        // provider switch, as it reads one when the credential a catalog is
        // read with changes (`applyCredentialChange`,
        // `app_auth_runtime.zig:2026-2036`). Nothing waits for it here: the
        // first thing that needs it takes its answer, waiting if it has not
        // come ([`await_catalog_for_window`]); the UI is told it the moment it
        // comes, a turn running or not. A failed one is not read again until `/model`
        // asks ([`ModelSelector::retry_after_failure`]).
        begin_catalog_read(&mut state, events);
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
                Some(TurnControl::Speed(change)) => Taken::Speed(change),
                // Nothing is running, so nothing is changing the files: the
                // undo is done now, as upstream's always is.
                Some(TurnControl::Undo { through }) => Taken::Undo(through),
                // Nothing is writing the conversation's log: the title is
                // written now.
                Some(TurnControl::Rename { title, announced }) => {
                    Taken::Rename { title, announced }
                }
                // Nothing holds the selector: asked of it now.
                Some(TurnControl::Catalog) => Taken::CatalogAsked,
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
            // **After control and before work.** The change is in force
            // already (the live authority); this is its record, and the order
            // keeps the record ahead of whatever was queued behind it -- the
            // configuration a conversation is opened from and the log's
            // `preferences_changed` land before the queued prompt's turn
            // does, as upstream syncs its queued prompts to a new mode
            // (`syncQueuedPermissionSnapshot`, `app_permission_runtime.zig`).
            change = changes.recv(), if changes_open => Taken::Change(change),
            // A bill the Gateway gave later, written down between pieces of
            // work by the recorder that owns the log; never during one, which
            // may be writing that log itself.
            answered = state.bills.next() => Taken::Bill(answered),
            // The catalog read in the background, answered while nothing
            // needed it yet: taken now, the UI having been told already.
            () = state.selector.settle_load(), if state.selector.is_loading() => Taken::Catalog,
            // A session title, written down and shown between pieces of work
            // for the reason a bill is: the recorder that owns the log.
            titled = next_title(&mut state.title_answers) => Taken::Title(titled),
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
                    Ended::Session => {
                        write_renamed_before_leaving(&mut state, events, &cancel).await;
                        return settled(state, changes);
                    }
                    Ended::Reset(through) => {
                        drop_conversation(&mut state, events, through).await;
                        write_renamed_after_reset(&mut state, events, &cancel).await;
                    }
                    Ended::Turn => {}
                }
                continue;
            }
            Taken::Speed(change) => {
                change_speed(&mut state, change);
                continue;
            }
            Taken::Undo(through) => {
                run_undo(&state, events, through, false).await;
                continue;
            }
            Taken::Nothing => continue,
            Taken::Bill(answered) => {
                state.settle_bill(&answered);
                continue;
            }
            Taken::Catalog => continue,
            Taken::CatalogAsked => {
                ask_catalog(&mut state, events).await;
                continue;
            }
            Taken::Title(titled) => {
                if let Some(title) = state.title_arrived(&titled, false) {
                    bridge::send_terminal(events, UiEvent::SessionTitled(title)).await;
                }
                continue;
            }
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
            Taken::Rename { title, announced } => {
                write_rename(&mut state, &title, announced, events, &cancel).await;
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
                run_turn(
                    &mut state,
                    prompt,
                    (Vec::new(), Vec::new()),
                    events,
                    &cancel,
                    &control,
                    &mut queue,
                )
                .await
            }
            TurnWork::Attached {
                prompt,
                images,
                skills,
            } => {
                run_turn(
                    &mut state,
                    prompt,
                    (images, skills),
                    events,
                    &cancel,
                    &control,
                    &mut queue,
                )
                .await
            }
            // This opens a socket, so it is work rather than a control
            // message, and it owes **exactly one** terminal event -- which is
            // what lets the UI's drain and the queue's accounting treat it
            // like any other item.
            TurnWork::Setup(provider) => {
                run_setup(&mut state, provider, events, &cancel, &control, &mut queue).await
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
            TurnWork::Workspace(action) => {
                let answer = state.change_workspace(&action);
                bridge::send_terminal(events, answer).await;
                Ended::Turn
            }
            TurnWork::Compact => {
                run_compact(&mut state, events, &cancel, &control, &mut queue).await
            }
        };
        // After **every** piece of work rather than in the arms that change
        // the conversation: a resume replaces it, and a piece of work added
        // later that does either is covered without remembering to be.
        // A reset drops it outside these arms and republishes there
        // ([`drop_conversation`]).
        state.publish_reply();
        // And a request this work recorded without its bill, or a session it
        // opened holding one, is asked about now.
        state.ask_for_bills();
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
            Ended::Session => {
                write_renamed_before_leaving(&mut state, events, &cancel).await;
                return settled(state, changes);
            }
            // A reset reached this work while it ran: the work has said its
            // last word, and the conversation goes before anything else is
            // taken -- the titles typed for it already written
            // ([`apply_deferred`]), and the ones typed after the reset written
            // into the session it starts.
            Ended::Reset(through) => {
                drop_conversation(&mut state, events, through).await;
                write_renamed_after_reset(&mut state, events, &cancel).await;
            }
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
    // A resume whose selection was still to be written down goes with it: the
    // next conversation's log starts with the selection in force.
    state.unrecorded_resume = None;
    state.bills.forget();
    // The band moved the title lineage on at the keystroke
    // ([`WorkHandle::new_title_lineage`]): what was asked about this
    // conversation is cancelled already, and what was asked since is the next
    // one's.
    state.conversation_lineage = None;
    state.began_ms = crate::session::event::system_now_ms();
    state.publish_reply();
    state.publish();
    bridge::send_terminal(events, UiEvent::Fresh { through }).await;
    // A title the conversation that starts now already has: asked by a prompt
    // typed after the reset, answered while the old conversation's work still
    // ran. The band set aside everything before the fence, so it is told
    // after it; the log takes it when the conversation opens.
    let waiting = state
        .pending_title
        .as_ref()
        .filter(|titled| titled.lineage == state.titles.lineage())
        .and_then(|titled| titled.title())
        .map(str::to_string);
    if let Some(title) = waiting {
        bridge::send_terminal(events, UiEvent::SessionTitled(title)).await;
    }
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
    // A title still being asked for is not waited for (`requestStop`,
    // `app_session_runtime.zig:3950`).
    state.titles.stop();
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
    /// `/fast` or `/ultrafast on|off`, between turns: applied now.
    Speed(SpeedChange),
    /// `/undo`, between turns: done now, as of the position it was typed at.
    Undo(crate::tools::undo::Mark),
    /// A message that means nothing here.
    Nothing,
    /// A change to the session's authority, or the channel that carries them
    /// closing.
    Change(Option<Change>),
    /// `/rename`'s title, to write now, and whether the shell has said so.
    Rename { title: String, announced: bool },
    /// What the Gateway said about a pending request's bill.
    Bill(super::reconcile::Answered),
    /// The catalog read in the background has answered, and its answer is
    /// taken.
    Catalog,
    /// `/model`'s menu or column asked for the catalog ([`TurnControl::Catalog`]).
    CatalogAsked,
    /// What a model said the session is called.
    Title(super::titles::Titled),
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
    attached: Attached,
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
    // Steers are taken from the moment the band can see this turn running
    // until it ends: opened before the row starts, closed before the
    // conclusion goes, so a steer the turn never took is one the band still
    // holds when it hears the turn is over ([`super::shell`]'s `TurnEnded`).
    let steering = state.steering.clone();
    steering.open();
    let _ = bridge::send_ui(events, &turn.token, UiEvent::TurnStarted).await;
    // The title answers, held by the turn while it holds the runtime: the
    // first prompt's question is asked as it is submitted, and an answer that
    // waited for the turn to end would keep the window on the workspace's
    // name for as long as the turn took.
    let mut answers = state.title_answers.take();
    let desk = state.titles.clone();
    let shown = desk.lineage();
    let asks = Arc::new(CatalogAsks::reading(state, events));
    let body = AssertUnwindSafe(ReportedByTheCatcher::around(one_turn(
        state, prompt, attached, events, &turn, control,
    )))
    .catch_unwind();
    let body = titled_while(body, answers.as_mut(), &desk, shown, control, events);
    // Both halves of one keystroke, and they have to happen **together, at the
    // moment the cancellation arrives**: the turn stops, and everything queued
    // behind it is dropped. Doing the second half after the body finished would
    // be too late for the case that matters most -- a provider that has gone
    // quiet without hanging up never lets the body finish at all, and the queue
    // would sit there claiming a place the band is still announcing.
    let ((caught, titled), ended) = raced_against_control(body, control, &asks, |through| {
        turn.cancel();
        queue.abandon(through);
    })
    .await;
    asks.finish(&mut state.selector);
    steering.close();
    if answers.is_some() {
        state.title_answers = answers;
    }
    // The titles that came back while the turn ran, written down now that the
    // turn has let go of the log -- into the conversation they are about,
    // when it is still the one the band shows. A `/new` typed during the turn
    // moved the lineage on at the keystroke: a title about the conversation
    // it let go of is dropped here, and one a prompt typed after it asked
    // waits for the conversation that `/new` starts, and the band is told of
    // it after the fence ([`drop_conversation`]).
    for titled in &titled {
        let _ = state.title_arrived(titled, true);
    }
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

/// `/compact`: compacts the conversation between turns and tells the band
/// how it ended ([`UiEvent::Compaction`]).
///
/// Raced against the control channel like a turn, so the Ctrl-C or Escape
/// pair that stops a turn stops this too (`cancelActiveOperation`'s
/// `context_compaction` target, `input_interrupt_runtime.zig:77-90`): the
/// notes request is cancelled with the work's own token, and what was queued
/// behind it goes with it. The ending is sent through `send_terminal`, so a
/// cancellation cannot drop the one event that clears the row. Nothing about
/// a compaction is written into the transcript, which is upstream's tested
/// contract (`tui-compaction-activity.test.ts:228-241`).
async fn run_compact(
    state: &mut Runtime,
    events: &Sender<UiEvent>,
    cancel: &Cancellation,
    control: &ControlChannel,
    queue: &mut Queue<'_>,
) -> Ended {
    let turn = cancel.turn();
    let asks = Arc::new(CatalogAsks::reading(state, events));
    let body = compact_between_turns(state, events, &turn);
    let (feedback, ended) = raced_against_control(body, control, &asks, |through| {
        turn.cancel();
        queue.abandon(through);
    })
    .await;
    asks.finish(&mut state.selector);
    // `/status` reads the conversation it compacted.
    state.publish();
    bridge::send_terminal(events, UiEvent::Compaction(Update::Settled(feedback))).await;
    ended
}

/// A manual compaction, in upstream's order (`request_context_compaction`,
/// `input_submit_runtime.zig:139-161`, then `compactContext`,
/// `orchestrator.zig:6206-6260`): nothing to compact is "No context to
/// compact."; then the provider it will ask, which failing is "Compaction was
/// not started."; then the window, which may still find nothing older than
/// the turns it keeps.
async fn compact_between_turns(
    state: &mut Runtime,
    events: &Sender<UiEvent>,
    turn: &TurnCancel,
) -> Feedback {
    if !state.has_context_to_compact() {
        return Feedback::NoOp;
    }
    // A compaction admitted as work takes the next turn id, as upstream's does
    // (`queued.turn_id = debug_trace.nextTurnId()`, `worker_runtime.zig:989`).
    let context = Context {
        turn: compaction_trace::next_turn_id(),
        step: 0,
    };
    if state.ready(&turn.mirror).is_err() {
        // Upstream's compaction is unavailable without a credential it may
        // ask with (`ContextCompactionUnavailable`, `compactor.zig:185-193`),
        // and `/trace` says which.
        compaction_trace::failure(
            context,
            TraceKind::CredentialUnauthorized,
            format!(
                "provider={} credential_source=none",
                state.config.provider.label()
            ),
        );
        transaction_failed(
            context,
            "preparation",
            Origin::Manual,
            "ContextCompactionUnavailable",
        );
        return Feedback::NotStarted;
    }
    let size = state.compaction_size();
    compact_conversation(state, size, Origin::Manual, context, events, turn).await
}

/// Upstream's name for a write to the log that may or may not have landed
/// (`rollbackAppend`, `vercel-labs/fx@c1db9191
/// src/core/session/session_log.zig:561-574`), and its row says to reopen
/// the session (`compaction_activity.zig:22`, `activity_status.zig:44-45`).
/// xfx's recorder takes no rollback: a write it could not finish stops every
/// write after it, so whether the checkpoint landed is not known either.
const UNCONFIRMED_WRITE: &str = "SessionPersistenceUncertain";

/// `/trace`'s record of a compaction that did not finish
/// (`compactContext`'s `errdefer`, `orchestrator.zig:6222-6228`): a failure,
/// unless the user stopped it.
fn transaction_failed(context: Context, stage: &str, origin: Origin, err: &str) {
    let detail = format!("stage={stage} origin={} err={err}", origin.name());
    if err == "Cancelled" {
        compaction_trace::info(context, TraceKind::TransactionFailed, detail);
    } else {
        compaction_trace::failure(context, TraceKind::TransactionFailed, detail);
    }
}

/// Plans, asks for, and saves one compaction of the open conversation, and
/// says how it ended. The band is told when it begins -- an automatic one
/// only once something has been chosen, as upstream's progress starts at
/// `.chosen` (`compactor.zig:181-184`) -- and when the notes request is about
/// to go.
async fn compact_conversation(
    state: &mut Runtime,
    size: Size,
    origin: Origin,
    context: Context,
    events: &Sender<UiEvent>,
    turn: &TurnCancel,
) -> Feedback {
    let Some(conversation) = state.conversation.as_ref() else {
        return Feedback::NoOp;
    };
    let plan = match compact::plan(conversation.recorder.state(), &size, context) {
        Ok(Some(plan)) => plan,
        Ok(None) => {
            compaction_trace::info(
                context,
                TraceKind::Decision,
                format!(
                    "decision=no_op origin={} reason=nothing_to_compact",
                    origin.name()
                ),
            );
            return Feedback::NoOp;
        }
        Err(failure) => {
            transaction_failed(context, "preparation", origin, failure.name());
            return feedback_of(failure);
        }
    };
    if origin != Origin::Manual {
        let _ = bridge::send_ui(
            events,
            &turn.token,
            UiEvent::Compaction(Update::Began { automatic: true }),
        )
        .await;
    }
    if plan.needs_model() {
        let _ = bridge::send_ui(
            events,
            &turn.token,
            UiEvent::Compaction(Update::Summarizing),
        )
        .await;
    }
    let spent = compact::Spent::default();
    let finished = {
        let model = state.model().to_string();
        let selector = &state.selector;
        let efforts = |name: &str| {
            selector
                .entry(name)
                .map(|entry| entry.efforts.clone())
                .unwrap_or_default()
        };
        let caller = Caller {
            provider: state
                .provider
                .as_ref()
                .expect("the bundle was built before compacting")
                .stream
                .as_ref(),
            model: &model,
            fallback: state.config.provider == ProviderId::Gateway,
            efforts: &efforts,
            spent: &spent,
            // Between turns there is no request to follow
            // (`app_agent_runtime.zig:1112-1129`).
            conversation: None,
        };
        compact::finish(&plan, &caller).await
    };
    let Some(conversation) = state.conversation.as_mut() else {
        return Feedback::Failed;
    };
    // What the notes requests cost goes to `/usage` however the compaction
    // ends: a request that was answered was billed (`spent`'s doc).
    if let Some(frame) = spent.frame() {
        conversation.recorder.commit(frame);
    }
    let event = match finished {
        Ok(event) => event,
        Err(failure) => {
            transaction_failed(context, "summary", origin, failure.name());
            return feedback_of(failure);
        }
    };
    // Stopped while the notes were written: nothing is saved
    // (`orchestrator.zig:6241`).
    if turn.mirror.is_cancelled() {
        transaction_failed(context, "summary", origin, "Cancelled");
        return Feedback::Cancelled;
    }
    let before = conversation.recorder.state().last_event_seq;
    let removed_turns = plan.turn_count();
    conversation.recorder.commit(event);
    let saved = conversation.recorder.state();
    let Some(checkpoint) = saved
        .checkpoint
        .as_ref()
        .filter(|_| saved.last_event_seq != before)
    else {
        transaction_failed(context, "publication", origin, UNCONFIRMED_WRITE);
        return Feedback::Unconfirmed;
    };
    compaction_trace::info(
        context,
        TraceKind::Committed,
        format!(
            "origin={} removed_turns={removed_turns} compaction_count={} summary_bytes={} \
             tools={}",
            origin.name(),
            checkpoint.count,
            crate::session::checkpoint::render(&checkpoint.compacted).len(),
            checkpoint.compacted.tool_count
        ),
    );
    Feedback::Succeeded
}

/// How many compactions the conversation has had: what says a provider's
/// count still describes the history it is sent.
fn compactions_of(state: &crate::session::DurableState) -> u64 {
    state.checkpoint.as_ref().map_or(0, |checkpoint| {
        u64::try_from(checkpoint.count).unwrap_or(u64::MAX)
    })
}

/// The band's sentence for a compaction that did not happen.
fn feedback_of(failure: Failure) -> Feedback {
    match failure {
        Failure::TooLarge => Feedback::TooLarge,
        Failure::Cancelled => Feedback::Cancelled,
        Failure::Summary(_) => Feedback::Failed,
    }
}

/// Starts the background read of the catalog, if the session holds none and
/// is not reading one ([`ModelSelector::begin_load`]), with the UI told its
/// answer the moment it arrives: upstream's interface takes a load's end on
/// its own loop's next tick and redraws an open menu and the footer then,
/// whatever its worker is doing (`pollLoadTransition`,
/// `vercel-labs/fx@c1db9191 src/core/app/model_cache_runtime.zig:549-557`,
/// from `loopCollectFacts`, `src/main.zig:3050-3083`). Here the read runs on
/// a thread of its own, so it tells the band itself -- through the send that
/// does not depend on a turn's token, since a turn may be running or none at
/// all, or a question may have parked this thread -- and the selector takes
/// the answer when it is next asked.
fn begin_catalog_read(state: &mut Runtime, events: &Sender<UiEvent>) {
    state
        .selector
        .begin_load(telling(state.config.provider, events.clone()));
}

/// How a read's answer is told to the band, from the thread the read ran on
/// ([`crate::provider::model::CatalogReader`]): `attempted` is when it began,
/// which a failure's floor counts from.
fn telling(
    provider: ProviderId,
    events: Sender<UiEvent>,
) -> impl FnOnce(
    CatalogState,
    Option<crate::provider::model::CatalogRetry>,
    Instant,
) -> std::pin::Pin<Box<dyn Future<Output = ()> + Send>>
       + Send
       + 'static {
    move |catalog, retry, attempted| {
        Box::pin(async move {
            let event = catalog_event_of(provider, &catalog, retry, Some(attempted));
            bridge::send_terminal(&events, event).await;
        })
    }
}

/// `/model`'s menu or column asking for the catalog while nothing holds the
/// runtime ([`TurnControl::Catalog`]): a read that failed is read again --
/// the `retry /model.` the menu said, as upstream's `ensureModelCache` starts
/// a load again after a failure, **but not within a second of the start of
/// the one that failed** (`beginLoad`, `vercel-labs/fx@c1db9191
/// src/core/app/model_cache_runtime.zig:441-442`; `RETRY_FLOOR`) -- and one
/// that has not started starts, both in the background and told when they
/// land ([`begin_catalog_read`]); a read already on its way is left to tell
/// its own answer; and a catalog the runtime holds -- or knows there is none
/// of, or a failure inside its floor -- is told now, for a UI that asked
/// without it.
async fn ask_catalog(state: &mut Runtime, events: &Sender<UiEvent>) {
    // A read that has answered and told is taken first, so the floor is
    // decided against its failure rather than waited out behind it.
    if state.selector.has_answered() {
        state.selector.settle_load().await;
    }
    state.selector.retry_after_failure_at(Instant::now());
    begin_catalog_read(state, events);
    if state.selector.is_loading() {
        return;
    }
    bridge::send_terminal(events, state.catalog_event()).await;
}

/// What a piece of work does with [`TurnControl::Catalog`] while it holds
/// the runtime, and so the selector -- read by the work's own race, or by a
/// prompter a question has parked the thread in
/// ([`super::approval::ControlChannel::lend_catalog`]).
///
/// fx's interface starts a catalog load on a thread of its own whatever its
/// worker is doing (`ensureModelCache` from `openModelBrowseCatalog`,
/// `vercel-labs/fx@c1db9191 src/core/app/app_input_runtime.zig:2346-2349`;
/// `startWarmup`, `src/core/app/model_cache_runtime.zig:321-349`), and the
/// open menu is refreshed when it lands (`pollLoadTransition`, `:549-557`).
/// Here the work holds the selector, so the read goes through what was taken
/// of the selector before the work began ([`CatalogReader`]), on a thread of
/// its own; its answer is told the moment it lands, and handed to the
/// selector once the work is over ([`Self::finish`]).
enum CatalogAsks {
    /// The read starts at once, beside the work.
    Reading {
        /// What the selector held when the work began.
        held: Box<HeldCatalog>,
        reader: CatalogReader,
        provider: ProviderId,
        events: Sender<UiEvent>,
        /// The read this work started, when it has started one.
        started: Mutex<Option<StartedLoad>>,
    },
    /// The read waits for the work: a provider switch, whose catalog is not
    /// the one in hand, or a load the work is making itself
    /// ([`ControlChannel::defer_catalog`]).
    AfterWork,
}

/// What a selector held when work that may be asked for its catalog began.
enum HeldCatalog {
    /// A read of its own on the way, which tells its answer itself -- and is
    /// decided by once it has ([`LoadView`]).
    Reading(LoadView),
    /// An answer, told again as it stands: rows, or that there is no catalog.
    Answer(Box<UiEvent>),
    /// A read that failed, begun at `attempted`: told again inside its floor,
    /// read again after it.
    Failed {
        answer: Box<UiEvent>,
        attempted: Option<Instant>,
    },
    /// Never read.
    Nothing,
}

impl CatalogAsks {
    /// For work that holds `state`: reads start at once.
    fn reading(state: &Runtime, events: &Sender<UiEvent>) -> Self {
        let held = if let Some(view) = state.selector.load_view() {
            HeldCatalog::Reading(view)
        } else {
            match state.selector.catalog() {
                CatalogState::Loaded(_) | CatalogState::Unavailable => {
                    HeldCatalog::Answer(Box::new(state.catalog_event()))
                }
                CatalogState::Failed(_) => HeldCatalog::Failed {
                    answer: Box::new(state.catalog_event()),
                    attempted: state.selector.last_attempt(),
                },
                CatalogState::NotLoaded => HeldCatalog::Nothing,
            }
        };
        Self::Reading {
            held: Box::new(held),
            reader: state.selector.reader(),
            provider: state.config.provider,
            events: events.clone(),
            started: Mutex::new(None),
        }
    }

    /// One request, read by the work's own race: what it says to tell now is
    /// told beside the work.
    fn ask(&self, control: &ControlChannel) {
        let Some(answer) = self.asked(control) else {
            return;
        };
        if let Self::Reading { events, .. } = self {
            let events = events.clone();
            tokio::task::spawn_local(async move {
                bridge::send_terminal(&events, answer).await;
            });
        }
    }

    /// One request, as upstream's `beginLoad` decides one (`:405-458`): the
    /// newest read in sight -- this work's, else the selector's own -- still
    /// on its way is not started twice; one that failed inside its floor is
    /// told again as it failed, and after it read again; anything else
    /// answered is told again. With no read in sight, what the selector held
    /// decides the same way. What to tell now, when it is not the read's own
    /// telling.
    ///
    /// The band asks again only after a failure's floor
    /// (`super::shell::Shell`'s `open_models_with`), so the floor here is the
    /// same rule held where the read is started, not a second one.
    fn asked(&self, control: &ControlChannel) -> Option<UiEvent> {
        self.asked_at(control, Instant::now())
    }

    /// [`Self::asked`], at `now`.
    fn asked_at(&self, control: &ControlChannel, now: Instant) -> Option<UiEvent> {
        let Self::Reading {
            held,
            reader,
            provider,
            events,
            started,
        } = self
        else {
            control.defer_catalog();
            return None;
        };
        let mut started = started.lock().unwrap_or_else(PoisonError::into_inner);
        let newest = match (started.as_ref(), held.as_ref()) {
            (Some(load), _) => Some(load.view()),
            (None, HeldCatalog::Reading(view)) => Some(view.clone()),
            (None, _) => None,
        };
        let failed_at = match &newest {
            Some(view) => match view.answered() {
                None => return None,
                Some((CatalogState::Failed(_), _)) => Some(view.began()),
                Some((catalog, retry)) => {
                    return Some(catalog_event_of(
                        *provider,
                        catalog,
                        *retry,
                        Some(view.began()),
                    ))
                }
            },
            None => match held.as_ref() {
                HeldCatalog::Answer(answer) => return Some((**answer).clone()),
                HeldCatalog::Failed { attempted, .. } => *attempted,
                HeldCatalog::Nothing | HeldCatalog::Reading(_) => None,
            },
        };
        if let Some(attempted) = failed_at {
            if !may_read_again(attempted, now) {
                return Some(match (&newest, held.as_ref()) {
                    (Some(view), _) => {
                        let (catalog, retry) = view.answered()?;
                        catalog_event_of(*provider, catalog, *retry, Some(attempted))
                    }
                    (None, HeldCatalog::Failed { answer, .. }) => (**answer).clone(),
                    (None, _) => return None,
                });
            }
        }
        *started = reader.begin(telling(*provider, events.clone()));
        None
    }

    /// Hands a read this work started to `selector`, now that the work has
    /// let it go ([`ModelSelector::adopt_load`]).
    fn finish(&self, selector: &mut ModelSelector) {
        if let Self::Reading { started, .. } = self {
            let load = started
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take();
            if let Some(load) = load {
                selector.adopt_load(load);
            }
        }
    }
}

impl super::approval::CatalogAsker for CatalogAsks {
    fn ask_catalog(&self, control: &ControlChannel) -> Option<UiEvent> {
        self.asked(control)
    }
}

/// Waits for the catalog the session began reading at launch
/// ([`ModelSelector::begin_load`]) when the turn cannot be sized without it,
/// as upstream's turn does: when what it knows of the model without a
/// catalog gives no window, it resolves the model's capabilities before it
/// plans the request's size, and that waits for a load still on its way
/// (`requiresResolvedRequestCapabilities`, `orchestrator.zig:5383-5395`,
/// `:5551-5586`; `resolveForRequest`, `model_cache_runtime.zig:614-669`).
/// A load that has failed, or none at all, is not waited for: the turn goes
/// with what it knows. Raced against the turn's own cancellation, as
/// upstream's wait is.
///
/// The other reasons upstream waits -- an effort, Fast or Ultra, an image
/// -- read the catalog where they are decided ([`one_turn`],
/// [`image_refusal`]), which waits for the same load.
async fn await_catalog_for_window(
    state: &mut Runtime,
    events: &Sender<UiEvent>,
    turn: &TurnCancel,
) {
    if state.context_window().is_some() || !state.selector.is_loading() {
        return;
    }
    let fresh = tokio::select! {
        biased;
        () = turn.token.cancelled() => false,
        fresh = state.load_catalog() => fresh,
    };
    if fresh {
        let _ = bridge::send_ui(events, &turn.token, state.catalog_event()).await;
    }
}

/// A turn's compactions: before each model step, whether the request it is
/// about to send has reached the automatic compaction point, and after a
/// request the provider refused as too large, once
/// (`orchestrator.zig:7286-7322`, `:8099-8124`). The turns before the one in
/// progress are compacted; the turn goes on with the compacted form in place
/// of them, on its own clock, and the row says `Compacting` meanwhile.
///
/// One that began and did not finish ends the turn: the row keeps the
/// sentence and the turn's conclusion says nothing more, as upstream keeps
/// the error on the row rather than in the transcript
/// (`compactionErrorHandled`, `app_process_runtime.zig:10-17`, `:55-56`) --
/// [`TurnCompactor::ended`] is how the turn knows.
struct TurnCompactor {
    size: Size,
    model: String,
    /// The efforts the conversation's model and its fallback list, read from
    /// the catalog as the turn began.
    efforts: Vec<(String, Vec<String>)>,
    fallback: bool,
    wire: crate::provider::Wire,
    /// What the turn's requests ask for, put back after a notes request.
    options: crate::gateway::protocol::RequestOptions,
    events: Sender<UiEvent>,
    token: tokio_util::sync::CancellationToken,
    mirror: CancelToken,
    /// The turn's number in the session, for `/trace`.
    turn: u64,
    spent: compact::Spent,
    /// Where each check's measurement of the fixed part is kept for a
    /// `/compact` after the turn ([`Runtime::fixed_tokens`]).
    fixed_tokens: Arc<AtomicUsize>,
    /// The conversation's last provider count ([`Runtime::calibration`]),
    /// read before each step and written after each answered one.
    calibration: Arc<std::sync::Mutex<Option<compact::Calibration>>>,
    /// How many compactions the conversation has had: a count from before
    /// the last one no longer applies.
    compactions: u64,
    /// How a compaction that ended the turn ended.
    ended: std::rc::Rc<std::cell::Cell<Option<Feedback>>>,
}

impl TurnCompactor {
    fn new(
        state: &Runtime,
        events: &Sender<UiEvent>,
        turn: &TurnCancel,
        options: crate::gateway::protocol::RequestOptions,
    ) -> Self {
        let model = state.model().to_string();
        let efforts = [model.clone(), compact::fallback_model(&model).to_string()]
            .into_iter()
            .map(|name| {
                let listed = state
                    .selector
                    .entry(&name)
                    .map(|entry| entry.efforts.clone())
                    .unwrap_or_default();
                (name, listed)
            })
            .collect();
        Self {
            size: state.compaction_size(),
            model,
            efforts,
            fallback: state.config.provider == ProviderId::Gateway,
            wire: state.config.provider.wire(),
            options,
            events: events.clone(),
            token: turn.token.clone(),
            mirror: turn.mirror.clone(),
            // Every turn takes the next id, compacting or not, so the ids
            // `/trace` prints are the ones upstream's would
            // (`debug_trace.nextTurnId`).
            turn: compaction_trace::next_turn_id(),
            spent: compact::Spent::default(),
            fixed_tokens: Arc::clone(&state.fixed_tokens),
            calibration: state
                .conversation
                .as_ref()
                .map(|conversation| Arc::clone(&conversation.calibration))
                .unwrap_or_default(),
            compactions: state.conversation.as_ref().map_or(0, |conversation| {
                compactions_of(conversation.recorder.state())
            }),
            ended: std::rc::Rc::default(),
        }
    }

    /// The count that applies to this turn's requests: the conversation's
    /// last, unless a compaction has replaced the history it was counted on.
    fn calibration(&self) -> Option<compact::Calibration> {
        self.calibration
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .filter(|calibration| calibration.compactions == self.compactions)
    }

    /// Tells the band how it ended; for one that did not succeed, the turn
    /// ends too and says nothing of its own.
    async fn settle(&self, feedback: Feedback) {
        if feedback != Feedback::Succeeded {
            self.ended.set(Some(feedback));
        }
        bridge::send_terminal(&self.events, UiEvent::Compaction(Update::Settled(feedback))).await;
    }

    /// The turn's error for a compaction that ended it: the row says why,
    /// and the turn says nothing (`compactionErrorHandled`).
    fn ended_turn(failure: &Failure) -> crate::agent::TurnError {
        match failure {
            Failure::Cancelled => crate::agent::TurnError::Cancelled,
            other => crate::agent::TurnError::CompactionEnded {
                reason: other.name(),
            },
        }
    }
}

#[async_trait::async_trait(?Send)]
impl crate::agent::StepCompactor for TurnCompactor {
    async fn before_step(
        &mut self,
        check: crate::agent::StepCheck<'_>,
    ) -> Result<Option<crate::agent::StepCompaction>, crate::agent::TurnError> {
        // The request by the estimate, scaled by the provider's count of the
        // one before it when that count still describes it, and the fixed
        // part with it (`orchestrator.zig:7292-7318`).
        let calibration = self.calibration();
        let applicable = calibration
            .as_ref()
            .filter(|calibration| calibration.applies(&self.model, &check.request));
        let estimated = applicable.map_or(check.request.text_tokens, |calibration| {
            calibration.estimate(&check.request)
        });
        let fixed = check.fixed.map(|fixed| {
            applicable
                .filter(|calibration| calibration.applies(&self.model, &fixed))
                .map_or(fixed.text_tokens, |calibration| {
                    calibration.estimate(&fixed)
                })
        });
        let mut size = self.size;
        size.request = Some(estimated);
        size.overflow = check.overflow;
        size.fixed = fixed;
        size.correction = calibration
            .as_ref()
            .and_then(|calibration| calibration.correction(&self.model));
        if let Some(fixed) = fixed {
            self.fixed_tokens.store(fixed, Ordering::Relaxed);
        }
        let context = Context {
            turn: self.turn,
            step: u64::from(check.step) + 1,
        };
        let origin = if check.overflow {
            Origin::ProviderOverflow
        } else {
            Origin::Automatic
        };
        let not_reduced = crate::agent::TurnError::ContextNotReduced {
            reason: "ContextCapacityExceeded",
        };
        if !(check.compactable && (check.overflow || size.due())) {
            if check.overflow {
                // `overflow_without_compaction`, `orchestrator.zig:7344-7346`.
                compaction_trace::failure(
                    context,
                    TraceKind::OverflowWithoutCompaction,
                    format!(
                        "estimated_tokens={estimated} usable_tokens={}",
                        compaction_trace::any(size.usable)
                    ),
                );
                return Err(not_reduced);
            }
            if let Some(usable) = size.usable.filter(|usable| estimated > *usable) {
                // Just compacted and still too large: not sent
                // (`no_compactable_context`, `orchestrator.zig:7347-7350`).
                compaction_trace::failure(
                    context,
                    TraceKind::NoCompactableContext,
                    format!("estimated_tokens={estimated} usable_tokens={usable}"),
                );
                return Err(not_reduced);
            }
            return Ok(None);
        }
        // Upstream's line, field for field (`orchestrator.zig:7323-7339`). xfx's
        // requests ask for no output limit.
        let has_images = check.request.images.is_some();
        compaction_trace::info(
            context,
            TraceKind::Decision,
            format!(
                "decision=compact overflow={} request_bytes={} estimated_tokens={estimated} \
                 text_tokens={} has_images={has_images} image_baseline={} \
                 prior_input_tokens={} usable_tokens={} compact_at_tokens={} \
                 compact_at_percent={} max_output_tokens=null",
                check.overflow,
                check.request.bytes,
                check.request.text_tokens,
                has_images && applicable.is_some(),
                compaction_trace::any(applicable.map(|calibration| calibration.exact)),
                compaction_trace::any(size.usable),
                compaction_trace::any(size.compact_at),
                compact::DEFAULT_PERCENT
            ),
        );
        let planned = match check.journal.state() {
            Some(state) => compact::plan_in_turn(state, &size, context),
            None => Ok(None),
        };
        let plan = match planned {
            Ok(Some(plan)) => plan,
            Ok(None) => {
                compaction_trace::info(
                    context,
                    TraceKind::Decision,
                    format!(
                        "decision=no_op origin={} reason=nothing_to_compact",
                        origin.name()
                    ),
                );
                if check.overflow {
                    // The refused request cannot go again as it was
                    // (`overflow_recovery_incomplete`, `orchestrator.zig:7487-7490`).
                    compaction_trace::failure(
                        context,
                        TraceKind::OverflowRecoveryIncomplete,
                        format!("estimated_tokens={estimated}"),
                    );
                    return Err(not_reduced);
                }
                return Ok(None);
            }
            // Nothing could be left out and the request still does not fit:
            // found before anything was chosen, so no row was begun and the
            // turn ends with upstream's error on its own line (`choose`,
            // `window.zig:297-318`; `compactionErrorHandled` finds no
            // operation, `app_process_runtime.zig:10-17`, and `formatErrorBody`
            // writes it, `:113-126`).
            Err(Failure::TooLarge) => {
                transaction_failed(context, "preparation", origin, Failure::TooLarge.name());
                return Err(not_reduced);
            }
            Err(failure) => {
                transaction_failed(context, "preparation", origin, failure.name());
                self.settle(feedback_of(failure.clone())).await;
                return Err(Self::ended_turn(&failure));
            }
        };
        let _ = bridge::send_ui(
            &self.events,
            &self.token,
            UiEvent::Compaction(Update::Began { automatic: true }),
        )
        .await;
        if plan.needs_model() {
            let _ = bridge::send_ui(
                &self.events,
                &self.token,
                UiEvent::Compaction(Update::Summarizing),
            )
            .await;
        }
        let finished = {
            let efforts = |name: &str| {
                self.efforts
                    .iter()
                    .find(|(model, _)| model == name)
                    .map(|(_, listed)| listed.clone())
                    .unwrap_or_default()
            };
            let caller = Caller {
                provider: check.provider,
                model: &self.model,
                fallback: self.fallback,
                efforts: &efforts,
                spent: &self.spent,
                // After a refusal the conversation no longer fits
                // (`orchestrator.zig:7390-7393`).
                conversation: (!check.overflow).then_some((check.conversation, &self.options)),
            };
            compact::finish(&plan, &caller).await
        };
        // The turn's own requests ask for what they asked for before.
        check.provider.set_options(self.options.clone());
        let event = match finished {
            Ok(event) if !self.mirror.is_cancelled() => event,
            Ok(_) => {
                transaction_failed(context, "summary", origin, "Cancelled");
                self.settle(Feedback::Cancelled).await;
                return Err(crate::agent::TurnError::Cancelled);
            }
            Err(failure) => {
                transaction_failed(context, "summary", origin, failure.name());
                self.settle(feedback_of(failure.clone())).await;
                return Err(Self::ended_turn(&failure));
            }
        };
        let before = check
            .journal
            .state()
            .map_or(0, |state| state.last_event_seq);
        let removed_turns = plan.turn_count();
        check.journal.record(event);
        let wire = self.wire.clone();
        let model = self.model.clone();
        let committed = check.journal.state().and_then(|state| {
            let checkpoint = state.checkpoint.as_ref()?;
            (state.last_event_seq != before).then(|| {
                (
                    checkpoint.count,
                    crate::session::checkpoint::render(&checkpoint.compacted),
                    checkpoint.compacted.tool_count,
                    state.history_messages_before_open(wire, &model).messages,
                )
            })
        });
        let Some((count, text, tools, history)) = committed else {
            transaction_failed(context, "publication", origin, UNCONFIRMED_WRITE);
            self.settle(Feedback::Unconfirmed).await;
            return Err(crate::agent::TurnError::CompactionEnded {
                reason: UNCONFIRMED_WRITE,
            });
        };
        compaction_trace::info(
            context,
            TraceKind::Committed,
            format!(
                "origin={} removed_turns={removed_turns} compaction_count={count} \
                 summary_bytes={} tools={tools}",
                origin.name(),
                text.len()
            ),
        );
        // `orchestrator.zig:7452-7457`. The count of the request before it
        // describes a history that is gone (`:7458`).
        compaction_trace::info(
            context,
            TraceKind::Installed,
            format!(
                "request_bytes_before={} estimated_tokens_before={estimated} summary_bytes={} \
                 tools={tools}",
                check.request.bytes,
                text.len()
            ),
        );
        self.compactions = u64::try_from(count).unwrap_or(u64::MAX);
        self.settle(Feedback::Succeeded).await;
        Ok(Some(crate::agent::StepCompaction {
            history,
            exchanges: plan.active_exchanges(),
        }))
    }

    fn spent(&mut self) -> (Vec<crate::session::RequestUsage>, bool) {
        self.spent.take()
    }

    fn stopped(&self) -> Option<String> {
        (self.ended.get() == Some(Feedback::Cancelled))
            .then(|| Feedback::Cancelled.label().to_string())
    }

    fn answered(&mut self, request: &compact::RequestCost, input_tokens: u64) {
        *self
            .calibration
            .lock()
            .unwrap_or_else(PoisonError::into_inner) =
            compact::Calibration::new(&self.model, *request, input_tokens, self.compactions);
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
        let (outcome, ended) = raced_against_control(
            body,
            control,
            &Arc::new(CatalogAsks::AfterWork),
            |through| {
                turn.cancel();
                queue.abandon(through);
            },
        )
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
            // The new provider's catalog is read in the background as the
            // loop takes its next piece of work ([`begin_catalog_read`]), as
            // upstream's switch reads the target's: the conversation goes on,
            // and the meter measures its tokens against the new window.
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
    // `/settings`' effort row: the effort is the session's from here on, and
    // that is all -- the band chose it, wrote it and said so
    // ([`ModelChange::effort_only`]). Recorded in the session, where a resume
    // reads it back. Whatever the provider: the effort is global.
    if change.effort_only {
        if let Some(effort) = change.effort {
            state.settings.effort = effort;
            state.record_settings();
            state.publish();
        }
        return Ended::Turn;
    }
    // **Chosen under another provider**: put aside while a `/provider` switch
    // ran, and read after it. Upstream refuses the switch while work is busy
    // and applies a model change at once, so its end state is the change made
    // for the provider it was chosen under and then the switch -- which is
    // what this leaves: that provider's model saved, the effort (global) taken,
    // and the provider now in force, its model and its switches untouched. The
    // band said `Switched to` the moment it was chosen, so it is told why the
    // footer still names this provider's model. A query nobody resolved cannot
    // be resolved against a catalog that has gone, and is dropped with a word.
    if change.provider != state.config.provider {
        let notice = if change.announced {
            if change.effort.is_some() {
                state.settings.effort = change.effort.clone().unwrap_or_default();
                state.record_settings();
            }
            state
                .save_change(
                    change.provider,
                    &change.name,
                    change.effort.as_ref(),
                    change.picker,
                )
                .or_else(|| {
                    Some(super::notice::Notice::warning(
                        "model",
                        format!(
                            "saved for {}; the provider changed to {} before it could be applied",
                            change.provider.label(),
                            state.config.provider.label()
                        ),
                    ))
                })
        } else {
            Some(super::notice::Notice::warning(
                "model",
                format!(
                    "not applied: the provider changed to {} before it could be",
                    state.config.provider.label()
                ),
            ))
        };
        bridge::send_terminal(
            events,
            UiEvent::ModelAnswered {
                model: state.model().to_string(),
                settings: state.settings.clone(),
                outcome: ModelAnswer::Unchanged,
                next_turn: change.next_turn,
                announced: true,
            },
        )
        .await;
        if let Some(notice) = notice {
            bridge::send_terminal(events, UiEvent::Notice(notice)).await;
        }
        return Ended::Turn;
    }
    let mut ended = Ended::Turn;
    if !change.announced {
        state.selector.retry_after_failure();
        let turn = cancel.turn();
        let (fresh, raced) = raced_against_control(
            state.load_catalog(),
            control,
            &Arc::new(CatalogAsks::AfterWork),
            |through| {
                turn.cancel();
                queue.abandon(through);
            },
        )
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
    // The effort and the speed go with the model they were chosen for, and
    // only when the model was taken: a refusal changed nothing. A picker's
    // choice is held to what the model's row offers (`selectModelFromPicker`);
    // a query turns both switches off when it changed the model
    // (`setResolvedModel`).
    let effort = match &outcome {
        ModelOutcome::Selected { .. } | ModelOutcome::Unchanged { .. } => change.effort.clone(),
        ModelOutcome::Refused { .. } | ModelOutcome::Reported { .. } => None,
    };
    let after = match &outcome {
        ModelOutcome::Refused { .. } | ModelOutcome::Reported { .. } => state.settings.clone(),
        _ if change.picker => {
            let entry = state.selector.entry(state.selector.model());
            state
                .settings
                .after_pick(effort.as_ref(), change.speed, entry)
        }
        ModelOutcome::Selected { .. } => state.settings.after_query(true),
        ModelOutcome::Unchanged { .. } => state.settings.after_query(false),
    };
    state.settings = after;
    state.record_settings();
    state.publish();
    // The effort a pick writes beside the model: the one it chose, or -- for
    // a pick that kept the session's, as `/settings`' model list does -- the
    // session's, for a model whose row lists levels (`selectModelFromPicker`
    // sets `patch.effort` whenever `reasoning_efforts` is not empty, with the
    // effort it was given or `app.effort`, `session_commands.zig:892-901`;
    // `applyInlineSettingsModelSelection`, `app_input_runtime.zig:2517-2529`).
    // A query writes none (`setResolvedModel`, `:1172-1188`).
    let effort = match effort {
        Some(chosen) => Some(chosen),
        None if change.picker
            && matches!(
                outcome,
                ModelOutcome::Selected { .. } | ModelOutcome::Unchanged { .. }
            )
            && state
                .selector
                .entry(state.selector.model())
                .is_some_and(|entry| !entry.efforts.is_empty()) =>
        {
            Some(state.settings.effort.clone())
        }
        None => None,
    };
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
    let saved = chosen.and_then(|model| {
        state.save_change(
            state.config.provider,
            &model,
            effort.as_ref(),
            change.picker,
        )
    });
    bridge::send_terminal(
        events,
        UiEvent::ModelAnswered {
            // **The model in force afterwards**, read back from the selector
            // rather than echoed from the request: on a refusal that is the
            // standing model, and it is the one the band must show.
            model: state.model().to_string(),
            settings: state.settings.clone(),
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

/// The turn's journal, with a call the stop refused at its approval recorded
/// as stopped rather than as refused.
///
/// A prompter parked at an approval answers a stop -- Escape at the panel, a
/// Ctrl-C, a reset -- with a refusal, and hands the stop back for the loop
/// (`super::approval::ControlChannel::give_back`): the call ends `Denied`
/// before the loop has read the stop. The screen shows it `Cancelled`, and
/// upstream keeps no result for it at all (`buildNormalExecutionMemory`,
/// `execution_memory.zig:449-452`), so a resumed session must not replay the
/// user's own refusal of a call they never decided. What the model was told
/// is unchanged: only the row's half of the result is rewritten.
struct StoppedAtApproval<'a> {
    inner: &'a mut dyn TurnJournal,
    control: &'a ControlChannel,
}

impl TurnJournal for StoppedAtApproval<'_> {
    fn record(&mut self, mut event: SessionEvent) {
        if let SessionEvent::ToolResult { ending, .. } = &mut event {
            if *ending == Some(crate::session::ToolEnding::Denied) && self.control.stopping() {
                *ending = Some(crate::session::ToolEnding::Cancelled);
            }
        }
        self.inner.record(event);
    }

    fn change_made(&mut self, call_id: &str, change: &crate::permission::ApprovalDiff) {
        self.inner.change_made(call_id, change);
    }

    // Every method the trait has, forwarded (the trait has no defaults, so a
    // method added to it stops this from compiling until it is).
    fn presented(&mut self, call_id: &str, tool: &str, text: &str) {
        self.inner.presented(call_id, tool, text);
    }

    fn attach_images(&mut self, images: &[crate::image::Image]) -> Option<std::path::PathBuf> {
        self.inner.attach_images(images)
    }

    fn attach_tool_images(&mut self, call_id: &str, images: &[crate::image::ToolImage]) {
        self.inner.attach_tool_images(call_id, images);
    }

    // The log as it stands, which a compaction between the turn's steps plans
    // from.
    fn state(&self) -> Option<&crate::session::DurableState> {
        self.inner.state()
    }
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
        let deferred = control.take_deferred();
        if deferred.is_empty() {
            return ended;
        }
        for item in deferred {
            let change = match item {
                // Done whatever ended: an undo needs no socket, and the file
                // is what the person asked about -- a `/quit` after it does
                // not take it back.
                Deferred::Undo {
                    through,
                    after_reset,
                } => {
                    run_undo(state, events, through, after_reset).await;
                    continue;
                }
                // Written whatever ended, as an undo is done: the shell
                // titled the window at once. One typed after the reset waits
                // for the drop ([`write_renamed_after_reset`]).
                Deferred::Rename {
                    title,
                    after_reset: false,
                    announced,
                } => {
                    write_rename(state, &title, announced, events, cancel).await;
                    continue;
                }
                Deferred::Rename {
                    title,
                    after_reset: true,
                    announced,
                } => {
                    state.renamed_after_reset.push((title, announced));
                    continue;
                }
                // The same for a speed: it needs no socket, and the band has
                // said it already.
                Deferred::Speed(change) => {
                    change_speed(state, change);
                    continue;
                }
                // Asked while a provider switch ran: asked now, of the
                // selector the switch left.
                Deferred::Catalog => {
                    ask_catalog(state, events).await;
                    continue;
                }
                Deferred::Model(change) => change,
            };
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

/// Applies one `/fast` or `/ultrafast on|off` ([`TurnControl::Speed`]): the
/// switches as the band decided them, recorded in the session log and saved
/// again, saying nothing.
///
/// **The band has saved it and said how that went** already, above its
/// `* fast: on`, as upstream persists before it announces
/// ([`super::speed_save`]). The write here is the file's order rather than a
/// second report: this thread writes the model changes, and one put aside
/// behind a turn is saved when the turn ends, with the switches as they then
/// stand -- so a `/fast` typed after it has to be written after it too, or
/// the profile would end on the switches the model change turned off. Its
/// outcome is the band's write's, said once.
///
/// A change the band decided under another provider is dropped: a
/// `/provider` switch took the switches off with the model it replaced, and
/// the band's own record of them went off with the same switch.
fn change_speed(state: &mut Runtime, change: SpeedChange) {
    if change.provider != state.config.provider {
        return;
    }
    state.settings.fast = change.fast && !change.ultrafast;
    state.settings.ultrafast = change.ultrafast;
    state.record_settings();
    state.publish();
    let _ = state.save_speed(change.topic);
}

/// `/undo`, and what became of it (`commandUndoLast`,
/// `vercel-labs/fx@c1db9191 src/core/app/app_commands.zig:1242-1290`): the
/// conversation's newest change as of `through` -- the position the keystroke
/// saw -- taken back ([`Runtime::undo`]) and upstream's sentence for it,
/// neutral whatever it says, as upstream's is.
///
/// `after_reset` is an undo typed after a reset this runtime has not acted on
/// yet ([`Deferred::Undo`]): it is answered for the conversation that reset
/// starts, which has nothing to take back, and the one about to be dropped is
/// not touched.
///
/// Nothing is written to the session log and nothing is said to the model:
/// upstream's tracker is not in its session store and its conversation is not
/// told (`change_tracker.zig`; `commandUndoLast` writes a notice and nothing
/// else). Through `send_terminal`, like every answer about something already
/// done: a file has changed by the time this is sent.
async fn run_undo(
    state: &Runtime,
    events: &Sender<UiEvent>,
    through: crate::tools::undo::Mark,
    after_reset: bool,
) {
    let undone = if after_reset {
        crate::tools::undo::Undone::Nothing
    } else {
        state.undo(through)
    };
    bridge::send_terminal(
        events,
        UiEvent::Undone(super::notice::Notice::neutral(
            crate::tools::undo::TOPIC,
            undone.sentence(),
        )),
    )
    .await;
}

/// Writes `/rename`'s title into the conversation's session
/// (`handleRenameCommand`, `app_commands.zig:3815-3858`). The shell has
/// already called the session that, as upstream caches the title before it
/// writes (`renameActiveSession`, `app_session_runtime.zig:3807`), and the
/// rename is said once, as what became of the write: a log that would not
/// take it in upstream's words ([`UiEvent::RenameUnsaved`]), and one that
/// did as `renamed to "…"` ([`UiEvent::Renamed`]) -- unless the shell has
/// said that already (`announced`), as it does while work runs.
async fn write_rename(
    state: &mut Runtime,
    title: &str,
    announced: bool,
    events: &Sender<UiEvent>,
    cancel: &Cancellation,
) {
    let written = state.rename(title, &cancel.mirror());
    state.publish();
    match written {
        Err(problem) => {
            bridge::send_terminal(events, UiEvent::RenameUnsaved(problem)).await;
        }
        Ok(()) if !announced => {
            bridge::send_terminal(events, UiEvent::Renamed(title.to_string())).await;
        }
        Ok(()) => {}
    }
}

/// The session is ending under a reset a shutdown outranked
/// ([`raced_against_control`]): a title typed after that reset still names
/// the session it would have started, and the shell has already said so.
/// That session is opened for it and titled, as a `/rename` before any
/// prompt opens one -- upstream's `/new` had created it, and keeps a titled
/// one on disk. Nothing to do when no such title is waiting.
async fn write_renamed_before_leaving(
    state: &mut Runtime,
    events: &Sender<UiEvent>,
    cancel: &Cancellation,
) {
    if state.renamed_after_reset.is_empty() {
        return;
    }
    state.conversation = None;
    write_renamed_after_reset(state, events, cancel).await;
}

/// Writes the titles typed after a reset the work they waited for ended
/// with, now that the reset has dropped the conversation it left
/// ([`Deferred::Rename`]'s `after_reset`): they name the session it starts.
async fn write_renamed_after_reset(
    state: &mut Runtime,
    events: &Sender<UiEvent>,
    cancel: &Cancellation,
) {
    for (title, announced) in std::mem::take(&mut state.renamed_after_reset) {
        write_rename(state, &title, announced, events, cancel).await;
    }
}

/// Runs `body` while reading the session-title answers the turn holds, and
/// gives back the ones that came.
///
/// Each is settled as it comes ([`super::titles::TitleDesk::settle`]), so the
/// band sees no question out once it is answered -- as upstream's poll takes
/// a finished task at once, whatever its worker is doing
/// (`src/main.zig:3140-3143`) -- and one that carries a title about the
/// conversation the turn is in is shown at once: the band is told, and the
/// runtime writes it into the log once the body has let go of it
/// ([`run_turn`]). The band can be told before the log has it because nothing
/// in the turn can give the session a title first: a `/rename` is work, and
/// waits for the turn -- and one that comes after a `/rename` the turn put
/// aside is dropped here, as upstream's finds the user's title on record. One
/// about a conversation the band has let go of is shown to nobody, and
/// dropped once the turn is over ([`run_turn`]).
///
/// **The answers are read before the body, never after it**, so the body is
/// polled at most once each time this is: a turn's call yields once when it
/// is done so that [`raced_against_control`], above this, reads a Ctrl-C the
/// call's question took before the turn goes on (`crate::agent::machine`'s
/// `yield_once`). Read after the body, an answer that came while the call had
/// the thread sent this loop round to poll the body again, straight past
/// that yield -- and the turn asked for its next step with the stop unread.
async fn titled_while<F: Future>(
    body: F,
    answers: Option<&mut super::titles::Answers>,
    desk: &super::titles::TitleDesk,
    shown: u64,
    control: &ControlChannel,
    events: &Sender<UiEvent>,
) -> (F::Output, Vec<super::titles::Titled>) {
    tokio::pin!(body);
    let Some(answers) = answers else {
        return (body.await, Vec::new());
    };
    let mut kept = Vec::new();
    loop {
        tokio::select! {
            biased;
            titled = answers.next() => {
                if !desk.settle(&titled) {
                    continue;
                }
                // A `/rename` typed during the turn is put aside until it
                // ends, and upstream would have written it at the keystroke:
                // a title that comes after it finds the user's on record
                // (`installGeneratedTitle`, `session_title_generation.zig:378-394`).
                if titled.lineage == shown && titled.title().is_some() && control.renaming() {
                    desk.record_dropped("user_title_present");
                    continue;
                }
                if titled.lineage == shown {
                    if let Some(title) = titled.title() {
                        // Not through the turn's token: a turn stopped after
                        // the title came still leaves the session titled.
                        bridge::send_terminal(events, UiEvent::SessionTitled(title.to_string()))
                            .await;
                    }
                }
                kept.push(titled);
            }
            output = &mut body => return (output, kept),
        }
    }
}

/// The next session-title answer, or never while the answers are lent to a
/// turn: an arm [`turn_loop`]'s `select!` can always carry.
async fn next_title(answers: &mut Option<super::titles::Answers>) -> super::titles::Titled {
    match answers.as_mut() {
        Some(answers) => answers.next().await,
        None => std::future::pending().await,
    }
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
    asks: &Arc<CatalogAsks>,
    mut stop: impl FnMut(usize),
) -> (F::Output, Ended) {
    // Lent for as long as the body runs, so a question it parks on answers
    // `/model`'s ask too ([`CatalogAsks`]).
    let _lent = control.lend_catalog(Arc::clone(asks) as Arc<dyn super::approval::CatalogAsker>);
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
                // `/model`, `/fast` or `/ultrafast` while this work runs: for
                // the turn after it, so put aside until it is over
                // ([`ControlChannel::defer`]). Nothing here stops.
                Some(TurnControl::Model(change)) => control.defer(change),
                Some(TurnControl::Speed(change)) => control.defer_speed(change),
                // `/undo` while this work runs: put aside the same way
                // ([`TurnControl::Undo`] says why not at once), and marked when
                // a reset has already been read here -- that undo was typed in
                // the conversation the reset starts, which has changed nothing
                // yet, rather than in the one it drops once this work is over.
                Some(TurnControl::Undo { through }) => {
                    control.defer_undo(through, matches!(ended, Ended::Reset(_)));
                }
                // `/rename` while this work runs: its log is this work's to
                // write until it is over, so the title is put aside the same
                // way, and marked when it was typed after a reset.
                Some(TurnControl::Rename { title, announced }) => {
                    control.defer_rename(title, matches!(ended, Ended::Reset(_)), announced);
                }
                // `/model`'s menu asking for the catalog while this work
                // holds the runtime: the read starts now, beside it, as
                // upstream's does on a thread of its own ([`CatalogAsks`]).
                Some(TurnControl::Catalog) => asks.ask(control),
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
/// Why a turn carrying an image may not be sent to the model in force, or
/// `None` when it may.
///
/// **On the Gateway, the catalog decides** (`image_input_support`,
/// `vercel-labs/fx@c1db9191 src/core/config/model_capabilities.zig:96-104`): a
/// row tagged `vision` and `file-input` both reads images inline; a row without
/// them would be upstream's second route, a forced `vision` tool call answered
/// by another model, which xfx does not have ([`crate::image::NOT_NATIVE`]); and
/// a model the catalog does not list -- or a catalog that could not be read --
/// is one whose support nobody can vouch for
/// ([`crate::image::CAPABILITY_UNKNOWN`]). The catalog is read for it like the
/// effort's: once, raced against this turn's cancellation, and told to the UI.
///
/// **On llmux, the daemon decides.** Its catalog has no capability field
/// (`2lab-ai/llmux@4b3d4a4 src/catalog.rs:1227`), so the image is sent and a
/// model that cannot read it is the daemon's 400, reported as any refusal it
/// sends is.
async fn image_refusal(
    state: &mut Runtime,
    events: &Sender<UiEvent>,
    turn: &TurnCancel,
) -> Option<String> {
    if state.config.provider != ProviderId::Gateway {
        return None;
    }
    let fresh = tokio::select! {
        biased;
        () = turn.token.cancelled() => false,
        fresh = state.load_catalog() => fresh,
    };
    if turn.token.is_cancelled() {
        return Some(crate::agent::TurnError::Cancelled.to_string());
    }
    if fresh {
        let _ = bridge::send_ui(events, &turn.token, state.catalog_event()).await;
    }
    match state.selector.entry(state.selector.model()) {
        Some(entry) if entry.facts.image_input => None,
        Some(_) => Some(crate::image::NOT_NATIVE.to_string()),
        None => Some(crate::image::CAPABILITY_UNKNOWN.to_string()),
    }
}

/// Whether a request's tool images go to the model, decided the way a
/// prompt's are and asked only when a request carries one.
///
/// **On the Gateway, the catalog decides** ([`image_refusal`]): a model it
/// tags `vision` and `file-input` is sent them; any other request goes with
/// the images taken out and upstream's sentence in front of each result that
/// had any -- [`crate::image::TOOL_IMAGES_NOT_NATIVE`] for a listed model
/// without the tags, [`crate::image::TOOL_IMAGES_UNKNOWN`] for one the catalog
/// does not list or could not be read (`projectToolImageMessages`,
/// `gateway_step.zig:256-283`). Upstream resolves the catalog the first time a
/// tool image appears (`orchestrator.zig:7206-7214`); so does this, raced
/// against the turn's cancellation and told to the UI when it is fresh.
///
/// **On llmux the daemon decides**, as it does a prompt's image: it is sent,
/// and a model that cannot read it is the daemon's 400.
struct ToolImageGate<'a> {
    inner: &'a dyn crate::gateway::Provider,
    provider: ProviderId,
    /// Lent out while the catalog is read, so no borrow is held across that
    /// wait.
    selector: std::cell::RefCell<Option<&'a mut ModelSelector>>,
    events: Sender<UiEvent>,
    token: tokio_util::sync::CancellationToken,
}

impl ToolImageGate<'_> {
    /// The note a request's tool images are replaced by, or `None` when the
    /// model is sent them.
    async fn refusal(&self) -> Option<&'static str> {
        if self.provider != ProviderId::Gateway {
            return None;
        }
        let selector = self.selector.borrow_mut().take()?;
        if matches!(selector.catalog(), CatalogState::NotLoaded) {
            // A read already on its way tells the UI itself.
            let told = selector.is_loading();
            tokio::select! {
                biased;
                () = self.token.cancelled() => {}
                _ = selector.ensure_catalog() => if !told {
                    let event = catalog_event_of(
                        self.provider,
                        selector.catalog(),
                        selector.failure_retry(),
                        selector.last_attempt(),
                    );
                    let _ = bridge::send_ui(&self.events, &self.token, event).await;
                }
            }
        }
        let note = match selector.entry(selector.model()) {
            Some(entry) if entry.facts.image_input => None,
            Some(_) => Some(crate::image::TOOL_IMAGES_NOT_NATIVE),
            None => Some(crate::image::TOOL_IMAGES_UNKNOWN),
        };
        *self.selector.borrow_mut() = Some(selector);
        note
    }
}

#[async_trait::async_trait(?Send)]
impl crate::gateway::Provider for ToolImageGate<'_> {
    async fn stream(
        &self,
        request: &crate::gateway::protocol::CompletionRequest,
        deltas: &mut dyn crate::gateway::DeltaSink,
    ) -> Result<crate::gateway::protocol::Completion, crate::gateway::ProviderError> {
        let carries = request.messages.iter().any(|message| {
            message.content.iter().any(|part| {
                matches!(part, crate::gateway::protocol::ContentPart::ToolResult { images, .. } if !images.is_empty())
            })
        });
        if carries {
            if let Some(note) = self.refusal().await {
                if let Some(stripped) = crate::image::strip_tool_images(request, note) {
                    return self.inner.stream(&stripped, deltas).await;
                }
            }
        }
        self.inner.stream(request, deltas).await
    }

    fn set_options(&self, options: crate::gateway::protocol::RequestOptions) {
        self.inner.set_options(options);
    }
}

/// What a prompt carries besides its words: the images its badges attach and
/// the skills its `$` tokens bound.
type Attached = (Vec<crate::image::Image>, Vec<crate::skills::Binding>);

/// What the band is told about the skills a turn named, before it is sent.
///
/// The load row under the card -- `* 1 requested skill loaded` and one `├`/`└`
/// row per skill, a warning with the failure count when one did not load --
/// is upstream's `load_notice` (`buildExplicitPromptSection`,
/// `skill_invocation.zig:267-288`, pushed at `orchestrator.zig:5648-5656`);
/// what each failure was, and every `[context]` line about a shortened
/// catalog or a skipped candidate, goes to the full transcript alone, as
/// upstream's `.full_only` notices do (`app_callbacks.zig:1321-1334`).
async fn say_skills(
    skills: &crate::skills::TurnSkills,
    events: &Sender<UiEvent>,
    turn: &TurnCancel,
) {
    use super::notice::Notice;
    let explicit = &skills.explicit;
    if let Some(details) = &explicit.details {
        let detail = Notice::warning("skills", details.trim_end_matches('\n'));
        let _ = bridge::send_ui(events, &turn.token, UiEvent::FullDetail(detail)).await;
    }
    if let Some((warning, said)) = &explicit.load_notice {
        let notice = if *warning {
            Notice::warning("", said.as_str())
        } else {
            Notice::neutral("", said.as_str())
        };
        let _ = bridge::send_ui(events, &turn.token, UiEvent::Notice(notice)).await;
    }
    let context = [
        skills.catalog.notice.as_deref(),
        skills.catalog.diagnostic_notice.as_deref(),
        explicit.notice.as_deref(),
        explicit.diagnostic_notice.as_deref(),
    ];
    for text in context.into_iter().flatten() {
        let body: Vec<&str> = text
            .split('\n')
            .filter(|line| *line != "[context]")
            .map(|line| line.strip_prefix("[context] ").unwrap_or(line))
            .collect();
        let detail = Notice::warning("context", body.join("\n").trim_end_matches('\n'));
        let _ = bridge::send_ui(events, &turn.token, UiEvent::FullDetail(detail)).await;
    }
}

/// Writes down, once, the provider, the model and the permission mode a
/// resumed conversation goes on under, where they differ from what its log
/// last recorded -- ahead of the first turn after the resume
/// ([`Runtime::resume`]), as `app::run_ask` writes them ahead of its turn.
///
/// Worked out now rather than at the open, from what is in force for this
/// turn: a `/model`, `/provider` or `/permissions` made since the open has
/// already written its own frame, and the log agrees with it, so only what
/// the open itself changed is left to say -- the launch's mode, which is
/// never restored from a log, and for a session written before the provider
/// was recorded, the provider it went on with. A crash before this turn loses
/// nothing: no turn ran under the selection, and the next open works it out
/// again from its own launch.
fn record_resumed_selection(
    conversation: &mut Conversation,
    unrecorded: &mut Option<crate::session::SessionId>,
    model: &str,
    mode: PermissionMode,
    provider: ProviderId,
) {
    // Only for the conversation that was resumed: a `/new` since gives the
    // next one a log that starts with the selection in force.
    if unrecorded.take().as_ref() != Some(conversation.recorder.id()) {
        return;
    }
    let provider = provider.label();
    let recorded = conversation.recorder.state();
    let model_changed = recorded.model != model;
    let mode_changed = recorded.permission_mode != mode;
    let provider_changed = recorded.provider.as_deref() != Some(provider);
    if model_changed || mode_changed || provider_changed {
        conversation
            .recorder
            .commit(SessionEvent::PreferencesChanged {
                model: model_changed.then(|| model.to_string()),
                permission_mode: mode_changed.then(|| mode.label().to_string()),
                provider: provider_changed.then(|| provider.to_string()),
                effort: None,
                fast_mode: None,
                ultrafast_mode: None,
            });
    }
}

async fn one_turn(
    state: &mut Runtime,
    prompt: String,
    attached: Attached,
    events: &Sender<UiEvent>,
    turn: &TurnCancel,
    control: &ControlChannel,
) -> Option<String> {
    let (images, bindings) = attached;
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
    if !state.settings.effort.is_default() || state.settings.fast || state.settings.ultrafast {
        let fresh = tokio::select! {
            biased;
            () = turn.token.cancelled() => false,
            fresh = state.load_catalog() => fresh,
        };
        if fresh {
            let _ = bridge::send_ui(events, &turn.token, state.catalog_event()).await;
        }
    }
    // **Whether the model reads images, before anything is sent or written.**
    // A turn whose prompt -- or whose history, which every request re-sends --
    // carries one goes inline only to a model the catalog tags `vision` and
    // `file-input`; any other is refused here, with upstream's words, and no
    // request leaves (`visionPolicy`, `orchestrator.zig:5971-5986`, `:7147-7158`).
    let carries_images = !images.is_empty()
        || state.conversation.as_ref().is_some_and(|conversation| {
            conversation
                .recorder
                .state()
                .turns
                .iter()
                .any(|turn| !turn.images.is_empty())
        });
    if carries_images {
        if let Some(refusal) = image_refusal(state, events, turn).await {
            return Some(refusal);
        }
    }
    // Project instructions are read now rather than remembered, so editing
    // `AGENTS.md` in another window takes effect on the next prompt -- and
    // before the automatic compaction below, which measures the request they
    // are part of.
    let context = ProjectContext::discover(
        state
            .conversation
            .as_ref()
            .expect("the conversation was just opened")
            .tools
            .scope(),
    );
    await_catalog_for_window(state, events, turn).await;
    // The skills this workspace has now -- one added in another window is in
    // this prompt's catalog -- and the ones the prompt or its `$` tokens name,
    // loaded whole for this turn (`crate::skills`). The catalog's budget is
    // sized by the model's window, as the catalog read at launch gives it
    // (`prepareSkillCatalog`'s `request_capabilities.context_window`).
    let skills = crate::skills::TurnSkills::prepare(
        state
            .conversation
            .as_ref()
            .expect("the conversation was just opened")
            .tools
            .scope()
            .primary(),
        state.config.home(),
        &prompt,
        &bindings,
        state.context_window(),
    );
    say_skills(&skills, events, turn).await;
    let options = match state.request_options() {
        Ok(options) => options,
        Err(refusal) => return Some(refusal),
    };
    // Fast asked for and dropped because the catalog is down: said once, at
    // the top of the answer, as upstream says it before the request.
    if fast_unavailable(state) {
        let _ = bridge::send_ui(
            events,
            &turn.token,
            UiEvent::Operational(FAST_UNAVAILABLE.to_string()),
        )
        .await;
    }
    let ultrafast = options.ultrafast;
    // Built before the conversation is borrowed for the turn: it reads the
    // catalog's window and efforts as they are now.
    let compactor = TurnCompactor::new(state, events, turn, options.clone());
    let compaction_ended = std::rc::Rc::clone(&compactor.ended);
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
    let provider_id = state.config.provider;
    // Two fields of one struct rather than one method returning both: the
    // provider is read and the conversation is written, and the borrow checker
    // is what keeps that honest. The selector is a third: the tool-image gate
    // below reads the catalog through it.
    let provider = state
        .provider
        .as_ref()
        .expect("the bundle was just built")
        .stream
        .as_ref();
    let selector = &mut state.selector;
    let conversation = state
        .conversation
        .as_mut()
        .expect("the conversation was just opened");

    record_resumed_selection(
        conversation,
        &mut state.unrecorded_resume,
        &model,
        state.config.permission_mode,
        provider_id,
    );
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
        .history_messages(state.config.provider.wire(), &model);
    // Once each for the conversation ([`Conversation::unsaid`]): the history
    // is rebuilt for every turn, and so are these.
    for notice in conversation.unsaid(replay.notices) {
        // Best effort by construction: a notice about the history is worth
        // less than the turn it precedes, and the only ways this fails are a
        // turn that was cancelled before it began and a UI that is gone --
        // both of which the turn below reports for itself.
        // A warning under `session`: the turn goes ahead, without what the
        // history could not carry. The store's sentence is the line shell's
        // too, so its `xfx: ` comes off here where the glyph says it.
        let notice =
            super::notice::Notice::warning("session", super::notice::unattributed(&notice));
        let _ = bridge::send_ui(events, &turn.token, UiEvent::Notice(notice)).await;
    }

    let request = TurnRequest {
        images,
        model,
        prompt,
        history: replay.messages,
        max_steps: state.config.max_agent_steps,
        max_attempts: DEFAULT_MAX_ATTEMPTS,
        // The same token the tools were given when the conversation was
        // opened, reset by `Cancellation::turn` for this turn.
        cancel: turn.mirror.clone(),
        tools: conversation
            .tools
            .clone()
            .with_skills(
                Some(std::sync::Arc::new(skills.catalog.locations.clone())),
                state.config.home().map(std::path::Path::to_path_buf),
            )
            .with_link_bases(crate::permission::LinkBase::from_environment())
            .with_git_environment(crate::permission::GitEnvironment::from_process()),
    };
    let mut sink = UiEventSink::new(events.clone(), turn.token.clone());
    // The recorder, with what the full transcript shows of each call -- its
    // arguments, its whole result -- handed to the UI as it is written down
    // (`super::full_transcript::Teed`). The log is written first and either
    // way: the view is best effort and the record is not.
    let mut teed = super::full_transcript::Teed::new(
        &mut conversation.recorder,
        events.clone(),
        turn.token.clone(),
    );
    // And a refusal the stop answered is recorded as the stop
    // ([`StoppedAtApproval`]).
    let mut journal = StoppedAtApproval {
        inner: &mut teed,
        control,
    };
    // And each request the turn settles, as the full transcript's `* network:`
    // record (`super::full_transcript::Networked`).
    let provider = super::full_transcript::Networked::new(
        provider,
        provider_id.label(),
        events.clone(),
        turn.token.clone(),
    );
    let provider = ToolImageGate {
        inner: &provider,
        provider: provider_id,
        selector: std::cell::RefCell::new(Some(selector)),
        events: events.clone(),
        token: turn.token.clone(),
    };
    // And, in Ultra, whether each response was served at it
    // ([`ServedUltra`]).
    let served = ServedUltra::new(&provider, ultrafast, events.clone(), turn.token.clone());
    // `run_turn_saved`'s machine, taking the person's steers as it runs, and
    // compacting before any step that needs it.
    let outcome = TurnMachine::new(request)
        .with_context(context)
        .with_skills(&skills)
        .with_steering(state.steering.clone())
        .with_compactor(Box::new(compactor))
        .run_journaled(&served, &mut sink, &mut journal)
        .await;

    // Approvals the user gave in the band become durable after the turn, once,
    // so an "always" answered here survives to the next resume -- the same step
    // `interactive::one_turn` and `app::ask` take, from the same recorder
    // method. **Whatever the turn did**: a turn that failed after the user
    // approved something still collected the approval, and asking again next
    // time would be asking a question the user has already answered.
    record_grants(conversation);

    // What the turn's answer read, from the **completed** turn's own outcome
    // (`agent::TurnOutcome::usage`, the same value the session log records at
    // `agent::machine`'s `UsageRecorded`). Only a turn that finished has one: a
    // turn that failed mid-stream has no total to report, and inventing one
    // from the deltas that did arrive would put a number on the hint row that
    // no provider ever said. What it wrote reached the band request by
    // request, as the machine settled each one (`UiEvent::Output`).
    //
    // Best effort, like the history notices above: the conclusion below is what
    // the turn owes, and a meter is worth less than the answer it measures.
    if let Ok(finished) = &outcome {
        let _ = bridge::send_ui(
            events,
            &turn.token,
            UiEvent::Usage {
                input: finished.usage.input_tokens,
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
        // A compaction ended it: the row says why, and there is nothing more
        // for its conclusion to say (`compactionErrorHandled`).
        Err(_) if compaction_ended.get().is_some() => None,
        Err(err) => Some(err.to_string()),
    }
}

/// Whether a turn in Fast mode goes at standard speed because the catalog
/// that would say the model offers Fast could not be read
/// (`orchestrator.zig:7253-7262`, `isModelCacheFailed`).
fn fast_unavailable(state: &Runtime) -> bool {
    state.settings.fast
        && !state.settings.ultrafast
        && state.request_options().is_ok_and(|options| !options.fast)
        && matches!(state.selector.catalog(), CatalogState::Failed(_))
}

/// The provider a turn streams from, saying once per turn -- when the turn is
/// in Ultra mode -- that a response's finish did not confirm the Ultra tier
/// (`orchestrator.zig:8015-8022`; the tier is the Gateway's own word,
/// [`crate::gateway::protocol::Completion::service_tier`]). After the
/// response, as upstream checks it once the stream has completed, so the
/// sentence lands ahead of the line the answer's presenter still holds.
struct ServedUltra<'a> {
    provider: &'a dyn crate::gateway::Provider,
    ultrafast: bool,
    said: std::cell::Cell<bool>,
    events: Sender<UiEvent>,
    cancel: tokio_util::sync::CancellationToken,
}

impl<'a> ServedUltra<'a> {
    fn new(
        provider: &'a dyn crate::gateway::Provider,
        ultrafast: bool,
        events: Sender<UiEvent>,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Self {
        Self {
            provider,
            ultrafast,
            said: std::cell::Cell::new(false),
            events,
            cancel,
        }
    }
}

#[async_trait::async_trait(?Send)]
impl crate::gateway::Provider for ServedUltra<'_> {
    async fn stream(
        &self,
        request: &crate::gateway::protocol::CompletionRequest,
        deltas: &mut dyn crate::gateway::DeltaSink,
    ) -> Result<crate::gateway::protocol::Completion, crate::gateway::ProviderError> {
        let settled = self.provider.stream(request, deltas).await;
        if let Ok(completion) = &settled {
            let confirmed =
                completion.service_tier == Some(crate::gateway::protocol::ServiceTier::Ultrafast);
            if self.ultrafast && !confirmed && !self.said.replace(true) {
                // Best effort, as the network record is: a turn already
                // cancelled or a UI already gone loses a sentence nobody sees.
                let _ = bridge::send_ui(
                    &self.events,
                    &self.cancel,
                    UiEvent::Operational(ULTRAFAST_UNCONFIRMED.to_string()),
                )
                .await;
            }
        }
        settled
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
    use crate::provider::effort::TurnSettings;
    use crate::provider::Wire;
    use crate::session::TurnConclusion;
    use crate::tools::undo::Mark;

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
    fn a_stop_raises_the_running_turns_flag_before_the_message_goes() {
        // A command holds the runtime's thread, so the message alone would be
        // read only once it ended by itself; the flag its supervisor polls is
        // raised here, on the UI thread, at the keystroke. Everything else the
        // UI sends leaves it alone.
        let (handle, _work, mut control, _changes) = WorkHandle::detached_with_changes();
        handle.control(TurnControl::QuestionCancelled {
            id: crate::tui::question::QuestionId(1),
        });
        assert!(!handle.stop.is_cancelled(), "an answer stopped the turn");
        handle.control(TurnControl::Cancel { through: 1 });
        assert!(handle.stop.is_cancelled());
        assert!(matches!(
            control.try_recv(),
            Ok(TurnControl::QuestionCancelled { .. })
        ));
        assert!(matches!(
            control.try_recv(),
            Ok(TurnControl::Cancel { through: 1 })
        ));
        for message in [TurnControl::Reset { through: 2 }, TurnControl::Shutdown] {
            handle.stop.reset();
            handle.control(message);
            assert!(handle.stop.is_cancelled());
        }
    }

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
                live: LiveAuthority::new(PermissionMode::Auto),
                rules: LiveRules::default(),
                grants: Grants::default(),
                stop: CancelToken::new(),
                steering: Steering::new(),
                steers: Arc::new(AtomicU64::new(0)),
                saved_rules: SavedRules::default(),
                spend: SessionSpend::default(),
                credits: CreditsBoard::default(),
                mcp: crate::mcp::McpRuntime::inert(),
                titles: super::super::titles::TitleDesk::new(true).0,
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
                live: LiveAuthority::new(PermissionMode::Auto),
                rules: LiveRules::default(),
                grants: Grants::default(),
                stop: CancelToken::new(),
                steering: Steering::new(),
                steers: Arc::new(AtomicU64::new(0)),
                saved_rules: SavedRules::default(),
                spend: SessionSpend::default(),
                credits: CreditsBoard::default(),
                mcp: crate::mcp::McpRuntime::inert(),
                titles: super::super::titles::TitleDesk::new(true).0,
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
                live: LiveAuthority::new(PermissionMode::Auto),
                rules: LiveRules::default(),
                grants: Grants::default(),
                stop: CancelToken::new(),
                steering: Steering::new(),
                steers: Arc::new(AtomicU64::new(0)),
                saved_rules: SavedRules::default(),
                spend: SessionSpend::default(),
                credits: CreditsBoard::default(),
                mcp: crate::mcp::McpRuntime::inert(),
                titles: super::super::titles::TitleDesk::new(true).0,
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
        let (was_stopped, ended) = on_a_runtime(raced_against_control(
            body,
            &control,
            &Arc::new(CatalogAsks::AfterWork),
            |through| {
                stopped.store(true, Ordering::Release);
                queue.abandon(through);
            },
        ));

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
            live: LiveAuthority::new(PermissionMode::Auto),
            rules: LiveRules::default(),
            grants: Grants::default(),
            stop: CancelToken::new(),
            steering: Steering::new(),
            steers: Arc::new(AtomicU64::new(0)),
            saved_rules: SavedRules::default(),
            spend: SessionSpend::default(),
            credits: CreditsBoard::default(),
            mcp: crate::mcp::McpRuntime::inert(),
            titles: super::super::titles::TitleDesk::new(true).0,
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

    /// Runs `work` on a runtime of its own, in a set of local tasks, as the
    /// worker thread does.
    fn on_a_runtime<T>(work: impl Future<Output = T>) -> T {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime");
        tokio::task::LocalSet::new().block_on(&runtime, work)
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

        let (output, ended) = on_a_runtime(raced_against_control(
            body,
            &control,
            &Arc::new(CatalogAsks::AfterWork),
            |_through| stopped.store(true, Ordering::Release),
        ));

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

        let (output, ended) = on_a_runtime(raced_against_control(
            body,
            &control,
            &Arc::new(CatalogAsks::AfterWork),
            |_through| stopped.store(true, Ordering::Release),
        ));

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
        // A conversation opened after it starts its log in it.
        assert_eq!(
            PermissionSession::new(state.config.permission_mode).mode(),
            PermissionMode::Yolo
        );
        // And the record never writes the live authority: a message drained
        // here can be older than what the person has chosen since, and the
        // handle -- the UI -- is the one that says what is in force.
        let (handle, _work, _control, _changes) = WorkHandle::detached_with_changes();
        state.live = handle.live().clone();
        handle
            .change_permissions(Change::Mode(PermissionMode::Ask))
            .expect("alive");
        assert_eq!(
            state.change_permissions(Change::Mode(PermissionMode::Yolo)),
            None
        );
        assert_eq!(state.authority().mode(), PermissionMode::Ask);
        assert_eq!(handle.live().mode(), PermissionMode::Ask);

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
        assert_eq!(
            grants.read().1,
            vec![Granted {
                tool: "write_file".to_string(),
                target: "a.txt".to_string(),
            }]
        );

        assert_eq!(state.change_permissions(Change::Reset), None);
        let conversation = state.conversation.as_ref().expect("still open");
        assert_eq!(conversation.tools.permissions().mode(), PermissionMode::Ask);
        assert!(conversation.tools.permissions().grants().is_empty());
        assert!(
            grants.read().1.is_empty(),
            "the board still lists the grant"
        );
        assert_eq!(state.config.permission_mode, PermissionMode::Ask);
        assert!(profile_text(&state).contains(r#""permission_mode": "ask""#));
    }

    #[test]
    fn a_reset_is_written_down_so_a_resume_does_not_bring_the_grants_back() {
        // Upstream's reset clears its session grants (`permission_engine.clear`,
        // `app_permission_runtime.zig:144-159`) and never persisted them, so a
        // resumed upstream session has none. xfx persists them; so the clearing
        // is persisted too, and a grant given *after* the reset -- mid-turn,
        // for a target the log already listed -- survives it.
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let mut config = config(home.path(), workspace.path());
        config.permission_mode = PermissionMode::Ask;
        let store = open_store(&config).expect("a store");
        let (events, _seen) = mpsc::channel(8);
        let (control_tx, control_rx) = mpsc::unbounded_channel();
        for id in 1..=3 {
            control_tx
                .send(TurnControl::Answer {
                    id: super::super::approval_readiness::ApprovalId(id),
                    answer: ApprovalAnswer::Always,
                    feedback: None,
                })
                .expect("the channel is open");
        }
        let prompter = TuiPrompter::new(
            events,
            ControlChannel::new(control_rx),
            Cancellation::new(CancelToken::new()),
        );
        let mut state = Runtime::new(config, store, prompter, a_questioner());
        let (handle, _work, _control, _changes) = WorkHandle::detached_with_changes();
        handle.live().set_mode(PermissionMode::Ask);
        state.live = handle.live().clone();
        let model = state.model().to_string();
        state.conversation = Some(
            open_conversation(
                &state.store,
                &state.config,
                &model,
                state.authority(),
                None,
                &CancelToken::new(),
            )
            .expect("open a conversation"),
        );
        let plan = |name: &str| {
            MutationPlan::new(
                MutationKind::Write,
                workspace.path().join(name),
                name.to_string(),
                TargetScope::PrimaryWorkspace,
                Preimage::Absent,
                b"x\n".to_vec(),
            )
        };
        let (kept, withdrawn) = (plan("kept.txt"), plan("withdrawn.txt"));
        let always = |state: &mut Runtime, plan: &MutationPlan| {
            let conversation = state.conversation.as_mut().expect("open");
            let decided = conversation
                .tools
                .permissions()
                .decide_with_feedback(ProposedAction::Mutation(plan))
                .decision;
            assert_eq!(
                decided,
                PolicyDecision::Allow {
                    source: AllowSource::InteractiveAlways
                }
            );
        };
        // Two "always" answers, written down at the end of their turn.
        always(&mut state, &kept);
        always(&mut state, &withdrawn);
        record_grants(state.conversation.as_mut().expect("open"));
        assert_eq!(
            state
                .conversation
                .as_ref()
                .expect("open")
                .recorder
                .state()
                .grants
                .len(),
            2
        );

        // The reset, as the UI makes it mid-turn: in force at once ...
        handle.change_permissions(Change::Reset).expect("alive");
        assert!(state
            .conversation
            .as_ref()
            .expect("open")
            .tools
            .permissions()
            .grants()
            .is_empty());
        // ... so the same turn asks again, and is told "always" again for one.
        always(&mut state, &kept);
        // The turn ends: the log already lists that target, so nothing is new.
        record_grants(state.conversation.as_mut().expect("open"));
        // Between pieces of work the runtime writes the reset down.
        assert_eq!(state.change_permissions(Change::Reset), None);
        let conversation = state.conversation.as_ref().expect("open");
        let key = |plan: &MutationPlan| ProposedAction::Mutation(plan).target();
        assert_eq!(
            conversation.recorder.state().grants,
            vec![Grant::new("write_file", key(&kept))],
            "the log does not say what the person is left with"
        );
        assert!(log_text(&state).contains(r#""kind":"permission_grants_cleared""#));

        // And a resume reads exactly that.
        let id = conversation.recorder.id().clone();
        state.conversation = None;
        let resumed = Resumption::open(&state.config, &crate::session::Selector::Id(id))
            .expect("the saved session opens");
        assert_eq!(
            resumed.session.state().grants,
            vec![Grant::new("write_file", key(&kept))],
            "a resume brought a reset grant back"
        );
    }

    #[test]
    fn a_mode_a_higher_layer_outranks_is_saved_and_says_so_in_upstreams_words() {
        // `appendShadowedUserSource` (`session_commands.zig:225-236`): the write
        // happened, and a fresh session here will still start in what the
        // environment -- or this workspace's own entry -- says.
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let env = Environment::new(
            Some(home.path().to_path_buf()),
            BTreeMap::from([("XFX_PERMISSION_MODE".to_string(), "ask".to_string())]),
        );
        let config = RuntimeConfig::load_with(&env, workspace.path()).expect("load");
        let store = open_store(&config).expect("a store");
        let mut state = Runtime::with_environment(config, env, store, a_prompter(), a_questioner());
        let said = state
            .change_permissions(Change::Mode(PermissionMode::Auto))
            .expect("the shadow is said");
        assert_eq!(
            said,
            super::super::notice::Notice::neutral(
                "permission-mode",
                "saved to user settings (scope=user); fresh sessions here use \
                 higher-precedence permission_mode=process_override"
            )
        );
        assert_eq!(
            profile_text(&state),
            "{\n  \"permission_mode\": \"auto\"\n}\n"
        );

        // A workspace entry outranks the global key the same way.
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let root = workspace.path().canonicalize().expect("a workspace");
        // Through the writer, so the profile home is born owner-only, as the
        // session store requires of it.
        crate::provider::profile::write_preference(
            &home.path().join(".xfx").join("settings.json"),
            &["workspaces"],
            serde_json::json!({root.to_string_lossy(): {"permission_mode": "ask"}}),
        )
        .expect("seed");
        let env = Environment::new(Some(home.path().to_path_buf()), BTreeMap::new());
        let config = RuntimeConfig::load_with(&env, workspace.path()).expect("load");
        assert_eq!(
            config.sources.permission_mode,
            crate::config::SettingSource::UserWorkspace
        );
        let store = open_store(&config).expect("a store");
        let mut state = Runtime::with_environment(config, env, store, a_prompter(), a_questioner());
        let said = state
            .change_permissions(Change::Reset)
            .expect("the shadow is said");
        assert!(
            said.body
                .ends_with("higher-precedence permission_mode=user_workspace"),
            "{said:?}"
        );

        // Nothing above the profile: nothing to say, as before.
        let (_home, _workspace, mut state) = recording();
        assert_eq!(
            state.change_permissions(Change::Mode(PermissionMode::Ask)),
            None
        );
    }

    #[test]
    fn a_keystroke_puts_the_mode_and_a_reset_in_force_before_the_runtime_hears_of_it() {
        let (handle, _work, _control, mut changes) = WorkHandle::detached_with_changes();
        let live = handle.live().clone();
        assert_eq!(live.mode(), PermissionMode::Auto);
        handle
            .change_permissions(Change::Mode(PermissionMode::Yolo))
            .expect("alive");
        assert_eq!(live.mode(), PermissionMode::Yolo, "not in force at once");
        handle.publish_grants(vec![Granted {
            tool: "write_file".to_string(),
            target: "a.txt".to_string(),
        }]);
        assert_eq!(handle.grants().len(), 1);
        assert_eq!(handle.resets(), 0);
        assert!(!handle.grants_reset_since_published());
        handle.change_permissions(Change::Reset).expect("alive");
        assert_eq!((live.mode(), live.resets()), (PermissionMode::Ask, 1));
        assert!(handle.grants().is_empty(), "a reset grant is still listed");
        assert!(handle.grants_reset_since_published());
        assert_eq!(handle.resets(), 1, "the panel's facts would not see it");
        handle
            .change_permissions(Change::Acknowledge)
            .expect("alive");
        assert_eq!(live.mode(), PermissionMode::Ask);
        assert_eq!(
            std::iter::from_fn(|| changes.try_recv().ok()).collect::<Vec<_>>(),
            vec![
                Change::Mode(PermissionMode::Yolo),
                Change::Reset,
                Change::Acknowledge
            ],
            "the record of each change, in the order they were made"
        );
        // A runtime that is gone: refused, and nothing moved.
        drop(changes);
        assert_eq!(
            handle.change_permissions(Change::Mode(PermissionMode::Yolo)),
            Err(Rejected::Gone)
        );
        assert_eq!(live.mode(), PermissionMode::Ask);
    }

    #[test]
    fn a_grant_is_published_as_it_is_held_for_the_panel_to_spell() {
        // A command a model wrote is not text the terminal may be handed; the
        // panel's rows are where it is made safe
        // (`info_panel::tests::a_permissions_item_names_what_a_model_or_a_file_put_in_it`),
        // so the publication keeps the command as the grant holds it.
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
        assert_eq!(
            grants.read().1,
            vec![Granted {
                tool: "terminal".to_string(),
                target: "printf '\u{1b}[2J' in /w".to_string(),
            }]
        );
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
        // after it was typed. Observed through a queued `/resume` of another
        // session, which replaces the conversation: the change reaches the
        // log of the conversation it replaces only if it was applied first.
        // (A queued `/model`, then a queued `/rename`, used to be the
        // witness; both are said off the work queue now.)
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
        let other = a_saved_session(&state.config, &Grant::new("edit_file", "x".to_string()));
        let (work_tx, mut work_rx) = mpsc::channel::<TurnWork>(WORK_LIMIT);
        work_tx
            .try_send(TurnWork::Resume(other.to_string()))
            .expect("room");
        let (changes_tx, mut changes) = mpsc::unbounded_channel::<Change>();
        changes_tx
            .send(Change::Mode(PermissionMode::Ask))
            .expect("open");
        drop(work_tx);
        let (events, _seen) = mpsc::channel::<UiEvent>(bridge::UI_EVENTS);
        let (_control_tx, control_rx) = mpsc::unbounded_channel::<TurnControl>();
        let outstanding = AtomicUsize::new(1);
        let left = on_a_runtime(turn_loop(
            state,
            &events,
            &mut work_rx,
            &mut changes,
            ControlChannel::new(control_rx),
            Cancellation::new(CancelToken::new()),
            &outstanding,
        ));
        assert_eq!(left, Some(other), "the queued work never ran");
        let log = std::fs::read_to_string(log).expect("the log");
        assert!(
            log.contains(r#""permission_mode":"ask""#),
            "the queued work ran before the change: {log}"
        );
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
            (Vec::new(), Vec::new()),
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

        let (output, ended) = on_a_runtime(raced_against_control(
            body,
            &control,
            &Arc::new(CatalogAsks::AfterWork),
            |_through| stopped.store(true, Ordering::Release),
        ));

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

        let (untouched, ended) = on_a_runtime(raced_against_control(
            body,
            &control,
            &Arc::new(CatalogAsks::AfterWork),
            |_through| stopped.store(true, Ordering::Release),
        ));

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
                (Vec::new(), Vec::new()),
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
            model: None,
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
                (Vec::new(), Vec::new()),
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
    fn the_session_scope_reads_the_conversations_spend_and_a_clock_a_reset_restarts() {
        let (_home, _workspace, mut state) = recording();
        let spend = SessionSpend::default();
        state.spend = spend.clone();
        // From the runtime's start, as upstream's session is from launch.
        state.began_ms = 1_000;
        let recorder = &mut state
            .conversation
            .as_mut()
            .expect("the fixture opened a conversation")
            .recorder;
        recorder.commit(SessionEvent::UserMessage {
            text: "asked".to_string(),
        });
        recorder.commit(SessionEvent::UsageRecorded {
            input_tokens: Some(1200),
            output_tokens: Some(40),
            turn_output_tokens: None,
            requests: vec![crate::session::RequestUsage {
                at_ms: 2_000,
                duration_ms: 700,
                model: "fake/model-a".to_string(),
                input_tokens: 1200,
                output_tokens: 40,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
                reasoning_tokens: None,
                cost: Some(0.01),
                generation_id: None,
                origin: None,
            }],
            unaccounted: false,
            lines_added: 4,
            lines_removed: 1,
            settled: Vec::new(),
            rejected: Vec::new(),
        });
        state.publish();
        let spent = spend.read();
        assert_eq!(spent.started_at_ms, Some(1_000));
        assert_eq!(spent.usage.requests.len(), 1);
        assert_eq!((spent.usage.lines_added, spent.usage.lines_removed), (4, 1));

        // `/new`: no spend, and a clock started again.
        let (events, _seen) = mpsc::channel::<UiEvent>(bridge::UI_EVENTS);
        on_a_runtime(async {
            drop_conversation(&mut state, &events, 0).await;
        });
        let spent = spend.read();
        assert!(spent.usage.requests.is_empty());
        assert!(
            spent.started_at_ms.is_some_and(|at| at > 1_000),
            "{spent:?}"
        );
    }

    /// A request whose stream named its generation and not its bill is asked
    /// about with the Gateway key, and the answer is written into the log of
    /// the session it belongs to -- `/usage`'s session scope reads it at once,
    /// and the log says it on the next read (`applyGeneration`,
    /// `session_usage.zig:1080-1126`). An answer about another session is not
    /// written into this one.
    #[test]
    fn a_pending_bill_is_asked_for_with_the_gateway_key_and_written_into_its_session() {
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let config = RuntimeConfig::load_with(
            &Environment::new(
                Some(home.path().to_path_buf()),
                BTreeMap::from([("AI_GATEWAY_API_KEY".to_string(), "key".to_string())]),
            ),
            workspace.path(),
        )
        .expect("load a configuration");
        let store = open_store(&config).expect("a store");
        let conversation = open_conversation(
            &store,
            &config,
            &config.model.clone(),
            PermissionSession::new(config.permission_mode),
            None,
            &CancelToken::new(),
        )
        .expect("open a conversation");
        let mut state = Runtime::new(config, store, a_prompter(), a_questioner());
        state.conversation = Some(conversation);
        let spend = SessionSpend::default();
        state.spend = spend.clone();
        const ID: &str = "gen_01JABCDEFGHJKMNPQRSTVWXYZ1";
        let bill = crate::gateway::protocol::Billing {
            at_ms: 1_900,
            model: "fake/model-a".to_string(),
            generation_id: Some(ID.to_string()),
            cost: 0.0125,
            input_tokens: 1340,
            output_tokens: 250,
            cache_read_tokens: 300,
            cache_write_tokens: 40,
            reasoning_tokens: Some(50),
        };
        let answered_bill = bill.clone();
        state.bills = super::super::reconcile::Reconciler::answering(
            move |origin, secret, id| {
                assert_eq!((origin, secret, id), ("http://127.0.0.1:4100", "key", ID));
                crate::gateway::generation::Lookup::Found(answered_bill.clone())
            },
            Duration::from_millis(1),
        );
        let recorder = &mut state.conversation.as_mut().expect("opened").recorder;
        recorder.commit(SessionEvent::UserMessage {
            text: "asked".to_string(),
        });
        recorder.commit(SessionEvent::UsageRecorded {
            input_tokens: Some(1200),
            output_tokens: Some(40),
            turn_output_tokens: None,
            requests: vec![crate::session::RequestUsage {
                at_ms: 2_000,
                duration_ms: 700,
                model: "fake/model-a".to_string(),
                input_tokens: 1200,
                output_tokens: 40,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
                reasoning_tokens: None,
                cost: None,
                generation_id: Some(ID.to_string()),
                origin: Some("http://127.0.0.1:4100".to_string()),
            }],
            unaccounted: false,
            lines_added: 0,
            lines_removed: 0,
            settled: Vec::new(),
            rejected: Vec::new(),
        });
        let session = recorder.id().clone();
        state.publish();
        assert_eq!(spend.read().usage.pending().count(), 1);

        state.ask_for_bills();
        let answered = on_a_runtime(async {
            tokio::time::timeout(TEST_DRAIN, state.bills.next())
                .await
                .expect("an answer")
        });
        assert_eq!(answered.session, session.as_str());

        let elsewhere = super::super::reconcile::Answered {
            session: "another".to_string(),
            ..answered.clone()
        };
        state.settle_bill(&elsewhere);
        assert_eq!(spend.read().usage.pending().count(), 1);

        state.settle_bill(&answered);
        let settled = &spend.read().usage.requests[0];
        assert_eq!(settled.cost, Some(0.0125));
        assert_eq!((settled.at_ms, settled.duration_ms), (1_900, 700));
        assert!(!settled.pending());
        state.conversation = None;
        let detail = state
            .store
            .detail(&crate::session::Selector::Id(session), workspace.path())
            .expect("the log reads back");
        assert_eq!(detail.state.usage.requests[0], *settled);
    }

    /// `signals::release`'s argument is that no handler runs off the UI
    /// thread, so the runtime thread is started with **every** owned signal
    /// blocked -- the four whose default ends or stops the process and the
    /// two whose handlers only set a flag -- read back by the thread itself.
    /// Started here from a test thread that blocks nothing, so what it has is
    /// what its own start gave it, not what it inherited.
    #[test]
    fn the_runtime_thread_is_started_with_every_owned_signal_blocked() {
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let (mut worker, mut events) = spawn(
            config(home.path(), workspace.path()),
            Cancellation::new(CancelToken::new()),
            None,
        )
        .expect("a runtime");
        let id = worker
            .thread
            .as_ref()
            .expect("the thread is held")
            .thread()
            .id();
        let deadline = Instant::now() + TEST_DRAIN;
        let mask = loop {
            let found = RUNTIME_MASKS
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .iter()
                .find(|(thread, _)| *thread == id)
                .map(|(_, mask)| mask.clone());
            if let Some(mask) = found {
                break mask;
            }
            assert!(Instant::now() < deadline, "the runtime never started");
            std::thread::sleep(Duration::from_millis(1));
        };
        worker.shutdown(&mut events, Instant::now() + TEST_DRAIN, |_| {});
        assert_eq!(mask.len(), 6, "{mask:?}");
        for (signal, blocked) in mask {
            assert!(
                blocked,
                "signal {signal} is deliverable on the runtime thread"
            );
        }
    }

    #[test]
    fn credits_on_llmux_asks_nobody_and_says_upstreams_sentence() {
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let mut config = config(home.path(), workspace.path());
        config.provider = crate::provider::ProviderId::Llmux;
        let answer = on_a_runtime(crate::credits::fetch(&config));
        assert_eq!(
            answer,
            crate::credits::Answer::Failed(crate::credits::UNAVAILABLE.to_string())
        );
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

    /// Bytes a terminal would obey rather than draw: an OSC title, a clear,
    /// a bell, a C1 CSI.
    const HOSTILE: &str = "x\u{1b}]0;pwned\u{7}y\u{1b}[2Jz\u{9b}31m";

    /// A saved session whose prompt, answers, call and result all carry
    /// [`HOSTILE`] -- a log is file input, and anyone who can write the file
    /// can put them there.
    fn a_hostile_session(config: &RuntimeConfig) -> SessionId {
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
        for event in [
            SessionEvent::UserMessage {
                text: format!("prompt {HOSTILE}"),
            },
            SessionEvent::AssistantMessage {
                text: format!("looking {HOSTILE}"),
                tool_calls: vec![crate::session::RecordedToolCall {
                    id: "r".to_string(),
                    name: "read_file".to_string(),
                    input: serde_json::json!({ "path": format!("p{HOSTILE}") }),
                }],
                raw_content: Vec::new(),
                responses_state: Vec::new(),
                wire: None,
                model: None,
            },
            SessionEvent::ToolResult {
                call_id: "r".to_string(),
                tool: "read_file".to_string(),
                ok: true,
                output: format!("read {HOSTILE}"),
                ending: None,
                lines: None,
                change: None,
            },
            SessionEvent::AssistantMessage {
                text: format!("done {HOSTILE}"),
                tool_calls: Vec::new(),
                raw_content: Vec::new(),
                responses_state: Vec::new(),
                wire: None,
                model: None,
            },
            SessionEvent::TurnConcluded {
                outcome: crate::session::TurnConclusion::Final {
                    finish_reason: "stop".to_string(),
                    steps: 2,
                },
            },
        ] {
            conversation.recorder.commit(event);
        }
        conversation.recorder.id().clone()
    }

    /// Every piece of text a replay carries to the screen or to the full
    /// transcript, whichever way it was built.
    fn replayed_text(replay: &super::super::hydration::Hydration) -> String {
        use super::super::hydration::HydratedStep;
        let mut all = format!("{:?}", replay.title);
        for turn in &replay.turns {
            all.push_str(&turn.sent);
            for step in &turn.steps {
                match step {
                    HydratedStep::Text(text)
                    | HydratedStep::Feedback(text)
                    | HydratedStep::Steer(text) => all.push_str(text),
                    HydratedStep::ToolStart { id, shown } => {
                        all.push_str(id);
                        all.push_str(shown.target.as_deref().unwrap_or(""));
                    }
                    HydratedStep::ToolResult { id, .. } => all.push_str(id),
                }
            }
            for (id, detail) in &turn.details {
                all.push_str(id);
                all.push_str(&format!("{detail:?}"));
            }
        }
        all
    }

    /// A resumed session is file input on both of its ways in: the picker's,
    /// whose replay crosses the channel, and the launch's, which is built
    /// before the terminal is taken and never crosses it. Both reach the UI
    /// through the same inert step ([`bridge::inert_hydration`]).
    #[test]
    fn a_hostile_log_reaches_the_ui_inert_at_launch_and_from_the_picker() {
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let config = config(home.path(), workspace.path());
        let id = a_hostile_session(&config);
        let obeyed = |text: &str| {
            text.chars()
                .filter(|c| c.is_control() && *c != '\n' && *c != '\r')
                .collect::<Vec<_>>()
        };

        // The launch's (`super::super::hold`).
        let launch = Resumption::open(&config, &crate::session::Selector::Id(id.clone()))
            .expect("the saved session opens")
            .hydration();
        let text = replayed_text(&launch);
        assert!(text.contains("prompt x ]0;pwned y [2Jz 31m"), "{text:?}");
        assert_eq!(obeyed(&text), Vec::<char>::new(), "{text:?}");

        // The picker's, across the channel as the runtime sends it.
        let store = open_store(&config).expect("a store");
        let mut state = Runtime::new(config, store, a_prompter(), a_questioner());
        // The picker's resume does not wait on a writer lock, and a sibling
        // test that forks a command in the instant this one closed the log
        // can hold the lock's descriptor until its child execs: a held lock
        // here is that, not this test's subject, so it is asked again.
        let deadline = Instant::now() + Duration::from_secs(5);
        let replay = loop {
            match state.resume_by_id(id.as_str(), &CancelToken::new()) {
                Ok(replay) => break replay,
                Err(super::super::session_picker::ResumeFailure::OpenElsewhere)
                    if Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(failure) => panic!("not resumed: {failure:?}"),
            }
        };
        let picked = on_a_runtime(async {
            let (events, mut received) = mpsc::channel(1);
            bridge::send_terminal(&events, UiEvent::Resumed(replay)).await;
            match received.recv().await {
                Some(UiEvent::Resumed(replay)) => replay,
                other => panic!("{other:?}"),
            }
        });
        assert_eq!(replayed_text(&picked), text, "the two ways in differ");
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
        // "At once" is one try at the lock: a resume that waited would try it
        // again until its grace period ran out (the store's own clock-free
        // cases: `a_budget_of_nothing_is_one_attempt_and_no_pause` against
        // `a_lock_that_frees_during_the_grace_is_acquired_after_the_documented_backoff`).
        // Counted, not timed, so a loaded machine can neither pass a wait nor
        // fail a refusal.
        let before_tries = crate::session::store::lock_attempts_on_this_thread();
        assert_eq!(
            state.resume_by_id(saved.as_str(), &CancelToken::new()),
            Err(ResumeFailure::OpenElsewhere)
        );
        assert_eq!(
            crate::session::store::lock_attempts_on_this_thread() - before_tries,
            1,
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
    fn a_resume_writes_nothing_and_its_first_turn_writes_down_what_it_runs_under() {
        // `app::run_ask`'s step, at the turn: a later resume reads what this
        // one used. Never at the open itself, which upstream's resume does
        // not write either (`hydrateResumedSession`,
        // `app_session_runtime.zig:2368-2420`). Red before: the resume
        // appended the model and the mode before anything was asked.
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let saved_under = config(home.path(), workspace.path());
        let grant = Grant::new("edit_file", "notes.txt".to_string());
        let id = a_saved_session(&saved_under, &grant);

        let mut config = saved_under.clone();
        config.model = "another-model".to_string();
        config.permission_mode = PermissionMode::Yolo;
        let resumed = Resumption::open(&config, &crate::session::Selector::Id(id))
            .expect("the saved session opens");
        assert_eq!(resumed.model(), saved_under.model);
        let store = open_store(&config).expect("a store");
        let mut state = Runtime::new(config.clone(), store, a_prompter(), a_questioner());
        let before = log_text(state_with_log(&mut state, resumed));
        let recorded = |state: &Runtime| {
            let recorded = state.conversation.as_ref().expect("bound").recorder.state();
            (recorded.model.clone(), recorded.permission_mode)
        };
        assert_eq!(
            recorded(&state),
            (saved_under.model.clone(), saved_under.permission_mode)
        );
        assert_eq!(log_text(&state), before, "the resume wrote to the log");

        // The turn's step writes down both, once.
        let turn = |state: &mut Runtime, model: &str| {
            let mode = state.config.permission_mode;
            let provider = state.config.provider;
            record_resumed_selection(
                state.conversation.as_mut().expect("bound"),
                &mut state.unrecorded_resume,
                model,
                mode,
                provider,
            );
        };
        turn(&mut state, "another-model");
        assert_eq!(
            recorded(&state),
            ("another-model".to_string(), PermissionMode::Yolo)
        );
        let once = log_text(&state);
        assert_ne!(once, before);
        turn(&mut state, "a-later-model");
        assert_eq!(log_text(&state), once, "written twice");
    }

    /// Runs one turn of `prompt` through the runtime's own path, its provider
    /// answering `answer`.
    fn one_turn_of(state: &mut Runtime, prompt: &str, answer: &str) -> Ended {
        one_turn_on(
            state,
            prompt,
            Box::new(ScriptedProvider::new(vec![answers(answer)])),
        )
    }

    /// [`one_turn_of`], its requests sent through `provider`.
    fn one_turn_on(
        state: &mut Runtime,
        prompt: &str,
        provider: Box<dyn crate::gateway::Provider>,
    ) -> Ended {
        state.provider = Some(Bundle {
            id: ProviderId::Gateway,
            stream: provider,
        });
        let (events_tx, _events_rx) = mpsc::channel(bridge::UI_EVENTS);
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
        on_a_runtime(run_turn(
            state,
            prompt.to_string(),
            (Vec::new(), Vec::new()),
            &events_tx,
            &cancel,
            &control,
            &mut queue,
        ))
    }

    /// A session's two files and its log's modification time.
    fn session_files(state: &Runtime, id: &SessionId) -> (Vec<u8>, Vec<u8>, std::time::SystemTime) {
        let dir = state.store.sessions_dir().join(id.as_str());
        let log = dir.join(crate::session::store::EVENTS_FILE);
        (
            std::fs::read(&log).expect("the log"),
            std::fs::read(dir.join(crate::session::store::MANIFEST_FILE)).expect("the manifest"),
            std::fs::metadata(&log)
                .and_then(|metadata| metadata.modified())
                .expect("an mtime"),
        )
    }

    #[test]
    fn a_picker_resume_and_a_refused_first_turn_write_nothing_and_the_next_turn_writes_the_mode() {
        // The picker's path (`resume_by_id`), not the launch's: the log's
        // bytes, its manifest and its modification time as they were. A first
        // turn refused before its prompt is recorded -- Ultra the model is
        // not offered -- writes nothing either; the turn that does run writes
        // the launch's mode just ahead of its prompt.
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let saved_under = config(home.path(), workspace.path());
        let grant = Grant::new("edit_file", "notes.txt".to_string());
        let saved = a_saved_session(&saved_under, &grant);
        let mut config = saved_under.clone();
        config.permission_mode = PermissionMode::Yolo;
        let store = open_store(&config).expect("a store");
        let mut state = Runtime::new(config, store, a_prompter(), a_questioner());
        let before = session_files(&state, &saved);

        state
            .resume_by_id(saved.as_str(), &CancelToken::new())
            .expect("resumed");
        assert_eq!(session_files(&state, &saved), before, "the picker wrote");

        state
            .selector
            .set_catalog_for_test(CatalogState::Loaded(speed_catalog()));
        state.settings.ultrafast = true;
        one_turn_of(&mut state, "refused", "never sent");
        assert_eq!(
            session_files(&state, &saved),
            before,
            "a refused turn wrote"
        );

        state.settings.ultrafast = false;
        assert_eq!(one_turn_of(&mut state, "answered", "ANSWERED"), Ended::Turn);
        let after = session_files(&state, &saved).0;
        assert!(after.starts_with(&before.0), "the log was rewritten");
        let appended: Vec<serde_json::Value> = String::from_utf8_lossy(&after[before.0.len()..])
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("a frame"))
            .map(|frame| frame["event"].clone())
            .collect();
        let kinds: Vec<&str> = appended
            .iter()
            .filter_map(|event| event["kind"].as_str())
            .collect();
        assert_eq!(
            kinds[..3],
            [
                "preferences_changed",
                "project_context_recorded",
                "user_message"
            ],
            "{kinds:?}"
        );
        assert_eq!(appended[0]["permission_mode"], "yolo", "{}", appended[0]);
        assert_eq!(
            kinds
                .iter()
                .filter(|kind| **kind == "preferences_changed")
                .count(),
            1,
            "{kinds:?}"
        );
    }

    #[test]
    fn a_new_conversation_after_a_resume_starts_its_own_log_and_leaves_the_resumed_one() {
        // `/new` after a resume, then a prompt: the new session is a log of
        // its own, opened under the selection in force -- the launch's mode
        // in `session_started` -- with no frame the resume still owed the
        // session it replaced, which is left as it was because no turn ran
        // in it. What a resume carries over into the new one is
        // `a_new_conversation_after_a_resume_records_the_settings_it_carries`.
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let saved_under = config(home.path(), workspace.path());
        let grant = Grant::new("edit_file", "notes.txt".to_string());
        let saved = a_saved_session(&saved_under, &grant);
        let mut config = saved_under.clone();
        config.permission_mode = PermissionMode::Yolo;
        let store = open_store(&config).expect("a store");
        let mut state = Runtime::new(config, store, a_prompter(), a_questioner());
        let before = session_files(&state, &saved);
        state
            .resume_by_id(saved.as_str(), &CancelToken::new())
            .expect("resumed");
        let (events_tx, _events_rx) = mpsc::channel(bridge::UI_EVENTS);
        on_a_runtime(drop_conversation(&mut state, &events_tx, 0));
        assert_eq!(state.unrecorded_resume, None);

        assert_eq!(one_turn_of(&mut state, "fresh", "FRESH"), Ended::Turn);
        let fresh = state
            .conversation
            .as_ref()
            .expect("a new conversation")
            .recorder
            .id()
            .clone();
        assert_ne!(fresh, saved);
        let events: Vec<serde_json::Value> =
            String::from_utf8_lossy(&session_files(&state, &fresh).0)
                .lines()
                .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("a frame"))
                .map(|frame| frame["event"].clone())
                .collect();
        let kinds: Vec<&str> = events
            .iter()
            .filter_map(|event| event["kind"].as_str())
            .collect();
        assert_eq!(kinds[0], "session_started", "{kinds:?}");
        assert_eq!(events[0]["permission_mode"], "yolo", "{}", events[0]);
        // The one `preferences_changed` is the first save's own: the effort
        // and the switches in force, and no model, mode or provider over
        // what `session_started` says.
        let changed: Vec<&serde_json::Value> = events
            .iter()
            .filter(|event| event["kind"] == "preferences_changed")
            .collect();
        assert_eq!(changed.len(), 1, "{kinds:?}");
        assert_eq!(kinds[1], "preferences_changed", "{kinds:?}");
        for field in ["model", "permission_mode"] {
            assert!(changed[0][field].is_null(), "{}", changed[0]);
        }
        assert!(changed[0].get("provider").is_none(), "{}", changed[0]);
        assert_eq!(session_files(&state, &saved), before);
    }

    /// Every frame of `id`'s log, its `event` alone, in order.
    fn logged_events(state: &Runtime, id: &SessionId) -> Vec<serde_json::Value> {
        String::from_utf8_lossy(&session_files(state, id).0)
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("a frame"))
            .map(|frame| frame["event"].clone())
            .collect()
    }

    /// A runtime with no conversation yet, that has been through `/model
    /// <id> max` from the column, `/settings`' effort row and `/fast` -- every
    /// one of them before a first prompt -- and then that prompt.
    fn chosen_before_the_first_prompt(config: RuntimeConfig) -> (Runtime, SessionId) {
        let store = open_store(&config).expect("a store");
        let mut state = Runtime::new(config, store, a_prompter(), a_questioner());
        state
            .selector
            .set_catalog_for_test(CatalogState::Loaded(speed_catalog()));
        let change = |name: &str, effort: &str, effort_only: bool| ModelChange {
            name: name.to_string(),
            effort: Some(crate::provider::effort::Effort::Named(effort.to_string())),
            speed: None,
            picker: !effort_only,
            announced: true,
            next_turn: false,
            provider: ProviderId::Gateway,
            effort_only,
        };
        ran_change(&mut state, change("anthropic/claude-opus-5", "max", false));
        ran_change(&mut state, change("", "low", true));
        change_speed(
            &mut state,
            SpeedChange {
                fast: true,
                ultrafast: false,
                topic: "fast",
                provider: ProviderId::Gateway,
            },
        );
        assert!(state.conversation.is_none(), "a choice opened the session");
        assert_eq!(one_turn_of(&mut state, "first", "FIRST"), Ended::Turn);
        let id = state
            .conversation
            .as_ref()
            .expect("the prompt opened a conversation")
            .recorder
            .id()
            .clone();
        (state, id)
    }

    #[test]
    fn settings_chosen_before_the_first_prompt_are_written_with_its_first_save() {
        // fx holds every setting made before a session's first turn in memory
        // and writes them with that turn (`hold`, `writeFirstTurn`,
        // `src/core/session_manager/session.zig:415-429`, `:503-539@481104fd`),
        // the session's own seed first (`appendSeed`, `session_adapter.zig:
        // 1289-1299`). The model, the provider and the mode in force are
        // `session_started`'s already; the effort and both switches are this
        // frame's, right behind it and ahead of the prompt. Red before: the
        // log said nothing of them until a change made after the first turn.
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let (state, id) = chosen_before_the_first_prompt(config(home.path(), workspace.path()));
        let events = logged_events(&state, &id);
        let kinds: Vec<&str> = events
            .iter()
            .filter_map(|event| event["kind"].as_str())
            .collect();
        assert_eq!(
            kinds[..4],
            [
                "session_started",
                "preferences_changed",
                "project_context_recorded",
                "user_message"
            ],
            "{kinds:?}"
        );
        assert_eq!(
            events[0]["model"], "anthropic/claude-opus-5",
            "{}",
            events[0]
        );
        assert_eq!(events[0]["provider"], "gateway", "{}", events[0]);
        assert_eq!(
            events[1],
            serde_json::json!({
                "kind": "preferences_changed",
                "model": null,
                "permission_mode": null,
                "effort": "low",
                "fast_mode": true,
                "ultrafast_mode": false,
            })
        );
        assert_eq!(
            kinds
                .iter()
                .filter(|kind| **kind == "preferences_changed")
                .count(),
            1,
            "{kinds:?}"
        );
    }

    #[test]
    fn a_resume_puts_back_the_first_saves_settings_and_nothing_it_does_not_restore() {
        // What a resume restores from that frame, and what it still does
        // not: the effort and the speed come back (`restoreRuntimePreferences`,
        // `app_session_runtime.zig:5110-5158@481104fd`), whatever the profile
        // says now; the launch's permission mode stays, and an `XFX_MODEL`
        // still outranks the session's model.
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let launched = config(home.path(), workspace.path());
        let (state, id) = chosen_before_the_first_prompt(launched.clone());
        drop(state);
        let chosen = TurnSettings {
            effort: crate::provider::effort::Effort::Named("low".to_string()),
            fast: true,
            ultrafast: false,
        };

        let mut later = launched.clone();
        later.permission_mode = PermissionMode::Yolo;
        later.effort = crate::provider::effort::Effort::Named("high".to_string());
        later.fast_mode = false;
        later.model = "fake/model-a".to_string();
        later.sources.model = crate::config::SettingSource::ProcessOverride;
        let resumed =
            Resumption::open(&later, &crate::session::Selector::Id(id.clone())).expect("opens");
        assert_eq!(resumed.settings(), Some(&chosen));
        assert_eq!(
            resumed.selection(&later, &later.model),
            (ProviderId::Gateway, "fake/model-a".to_string())
        );
        drop(resumed);

        let store = open_store(&later).expect("a store");
        let mut state = Runtime::new(later, store, a_prompter(), a_questioner());
        state
            .resume_by_id(id.as_str(), &CancelToken::new())
            .expect("resumed");
        assert_eq!(state.settings, chosen);
        assert_eq!(state.config.permission_mode, PermissionMode::Yolo);
        assert_eq!(state.model(), "fake/model-a");
    }

    #[test]
    fn a_rename_before_the_first_prompt_opens_the_log_with_the_settings_in_force() {
        // `/rename` opens the conversation before any prompt does: the frame
        // is written there, and a change after it is a change to a log that
        // already says what the session was in.
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let config = config(home.path(), workspace.path());
        let store = open_store(&config).expect("a store");
        let mut state = Runtime::new(config, store, a_prompter(), a_questioner());
        state.settings.effort = crate::provider::effort::Effort::Named("high".to_string());
        let id = state
            .conversation(&CancelToken::new())
            .expect("opened")
            .recorder
            .id()
            .clone();
        let events = logged_events(&state, &id);
        assert_eq!(events.len(), 2, "{events:?}");
        assert_eq!(events[1]["kind"], "preferences_changed", "{}", events[1]);
        assert_eq!(events[1]["effort"], "high", "{}", events[1]);
        assert_eq!(events[1]["fast_mode"], false, "{}", events[1]);
    }

    /// `line` -- a `/permissions remember …` or `revoke …` -- confirmed
    /// against the store as the UI holds it now, as the shell confirms one
    /// (`super::super::shell`'s `permissions`): prepared under the session's
    /// scope in `ask`, running nothing.
    fn confirmed(state: &Runtime, line: &str) -> (crate::permission::allowlist::Tone, String) {
        use crate::permission::slash::{self, Managed, Request};
        let store = state.saved_rules.view();
        let scope = conversation_scope(&state.config).expect("a scope");
        let managed = slash::manage_with(
            &Request::parse(line),
            Some(&store.at),
            |_| store.state.clone(),
            None,
            |tool, arguments| {
                crate::tools::prepare_rule_action(
                    tool,
                    arguments,
                    &crate::tools::ToolContext::new(scope)
                        .with_permissions(PermissionSession::new(PermissionMode::Ask)),
                )
            },
        );
        let Some(Managed::Confirm(confirmation)) = managed else {
            panic!("{line}: {managed:?}");
        };
        state.saved_rules.confirm(&confirmation)
    }

    /// Every session directory the store holds.
    fn session_dirs(state: &Runtime) -> Vec<std::path::PathBuf> {
        std::fs::read_dir(state.store.sessions_dir())
            .map(|entries| {
                entries
                    .flatten()
                    .map(|entry| entry.path())
                    .filter(|path| path.is_dir() && !path.ends_with(".tmp"))
                    .collect()
            })
            .unwrap_or_default()
    }

    const HELD_DENY: &str = r#"remember deny write_file {"path":"b.txt","content":"y"}"#;

    #[test]
    fn rules_held_before_the_first_prompt_are_its_sessions_and_decide_its_first_call() {
        // fx puts a confirmed rule in the session's state at once
        // (`replacePermissionStateOwned`, `input_approval_runtime.zig:
        // 289-293@481104fd`), which every approval reads, and writes it with
        // the session's first turn. Here: held until the first prompt opens
        // the session, written into its directory before its log, and the
        // store every decision of that first turn reads -- in `auto`, where
        // nothing else would stop the write. (A write's rule is applied past
        // upstream, whose TUI reads no saved rule for a file change:
        // `docs/parity.md`'s `/permissions` row.)
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let mut config = config(home.path(), workspace.path());
        config.permission_mode = PermissionMode::Auto;
        let store = open_store(&config).expect("a store");
        let mut state = Runtime::new(config, store, a_prompter(), a_questioner());
        let target = state.next_rules_path();
        assert_eq!(
            confirmed(&state, HELD_DENY).1,
            crate::permission::slash::UPDATED
        );
        assert!(session_dirs(&state).is_empty(), "a rule wrote a session");
        state.publish();
        assert_eq!(
            state.saved_rules.view().state.expect("held").rules.len(),
            1,
            "a publication between pieces of work let go of the held rule"
        );

        let provider = ScriptedProvider::new(vec![
            calls(vec![call(
                "call-1",
                "write_file",
                serde_json::json!({"path": "b.txt", "content": "y"}),
            )]),
            answers("DONE"),
        ]);
        assert_eq!(
            one_turn_on(&mut state, "write it", Box::new(provider)),
            Ended::Turn
        );
        assert!(
            !workspace.path().join("b.txt").exists(),
            "the held deny let the first turn's write by"
        );
        let conversation = state.conversation.as_ref().expect("opened");
        assert_eq!(
            conversation.tools.permissions().session_rules_path(),
            Some(target.as_path())
        );
        let saved = crate::permission::session_rules::load(&target).expect("the file");
        assert_eq!(saved.rules.len(), 1);
        assert_eq!(saved.rules[0].id, 1);
        assert_eq!(
            saved.rules[0].display,
            "write_file b.txt preimage=absent after=a1fce4363854ff88"
        );
        let id = conversation.recorder.id().clone();
        let log = String::from_utf8_lossy(&session_files(&state, &id).0).into_owned();
        assert!(
            log.contains("a saved-session permission rule denies"),
            "{log}"
        );
        // The file is the store from here on.
        assert_eq!(state.saved_rules.view().at, target);
        assert_eq!(
            confirmed(&state, "revoke 1").1,
            crate::permission::slash::UPDATED
        );
        assert!(crate::permission::session_rules::load(&target)
            .expect("the file")
            .rules
            .is_empty());
    }

    #[test]
    fn a_rename_before_the_first_prompt_writes_the_rules_held_for_its_session() {
        // `/rename` opens the session before any prompt (P-status decision
        // 1): that is the first save, and it carries the rules too.
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let config = config(home.path(), workspace.path());
        let store = open_store(&config).expect("a store");
        let mut state = Runtime::new(config.clone(), store, a_prompter(), a_questioner());
        let target = state.next_rules_path();
        confirmed(&state, HELD_DENY);
        state
            .rename("named first", &CancelToken::new())
            .expect("renamed");
        let saved = crate::permission::session_rules::load(&target).expect("the file");
        assert_eq!(saved.rules.len(), 1, "{saved:?}");
        // A session nothing was held for gets no file, as before.
        let mut clean = Runtime::new(
            config.clone(),
            open_store(&config).expect("a store"),
            a_prompter(),
            a_questioner(),
        );
        let untouched = clean.next_rules_path();
        clean
            .rename("nothing held", &CancelToken::new())
            .expect("renamed");
        assert!(!untouched.exists());
    }

    #[cfg(unix)]
    #[test]
    fn rules_held_for_a_session_the_store_refuses_go_with_the_next_one() {
        // A first save that cannot be made opens nothing and runs nothing, as
        // fx's failed first publish fails its turn before any call
        // (`appendRunningToolCalls`, `src/core/agent/runtime/orchestrator.zig:
        // 3894-3917@481104fd`); the rules stay held, and the next attempt --
        // under the next id -- writes them. Never in force unsaved, never lost.
        use std::os::unix::fs::PermissionsExt;
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let config = config(home.path(), workspace.path());
        let store = open_store(&config).expect("a store");
        let mut state = Runtime::new(config, store, a_prompter(), a_questioner());
        confirmed(&state, HELD_DENY);
        let sessions = state.store.sessions_dir().to_path_buf();
        let before = std::fs::metadata(&sessions)
            .expect("sessions")
            .permissions();
        std::fs::set_permissions(&sessions, std::fs::Permissions::from_mode(0o500))
            .expect("read-only");
        let refused = state.open_next(&CancelToken::new());
        std::fs::set_permissions(&sessions, before).expect("restored");
        assert!(refused.is_err());
        assert!(state.conversation.is_none());
        state.publish();
        let held = state.saved_rules.view();
        assert_eq!(held.at, state.next_rules_path(), "not moved to the next id");
        assert_eq!(
            held.state.expect("held").rules.len(),
            1,
            "the rule was lost"
        );

        state.open_next(&CancelToken::new()).expect("opened");
        let saved = crate::permission::session_rules::load(&held.at).expect("the file");
        assert_eq!(saved.rules.len(), 1, "{saved:?}");
    }

    /// A scripted provider that counts what it is asked, where the turn
    /// cannot hide it.
    struct Counted {
        inner: ScriptedProvider,
        asked: std::rc::Rc<std::cell::Cell<usize>>,
    }

    #[async_trait::async_trait(?Send)]
    impl crate::gateway::Provider for Counted {
        async fn stream(
            &self,
            request: &crate::gateway::protocol::CompletionRequest,
            deltas: &mut dyn crate::gateway::DeltaSink,
        ) -> Result<Completion, crate::gateway::ProviderError> {
            self.asked.set(self.asked.get() + 1);
            self.inner.stream(request, deltas).await
        }
    }

    /// [`one_turn_on`], and how the turn ended as the UI was told it.
    fn one_turn_failure(
        state: &mut Runtime,
        prompt: &str,
        provider: Box<dyn crate::gateway::Provider>,
    ) -> Option<String> {
        state.provider = Some(Bundle {
            id: ProviderId::Gateway,
            stream: provider,
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
            state,
            prompt.to_string(),
            (Vec::new(), Vec::new()),
            &events_tx,
            &cancel,
            &control,
            &mut queue,
        ));
        assert_eq!(ended, Ended::Turn);
        drop(events_tx);
        let mut failure = None;
        while let Ok(event) = events_rx.try_recv() {
            if let UiEvent::TurnEnded { failure: said } = event {
                failure = said;
            }
        }
        failure
    }

    #[test]
    fn rules_held_past_what_a_store_may_hold_refuse_the_prompt_run_nothing_and_stay_held() {
        // Review PERM-EARLY-b, item 4. The seed runs -- its directory made,
        // its file written -- and its read-back fails: the held rules encode
        // past the store's 1 MiB (`session_rules::MAX_FILE_BYTES`), which
        // `load` refuses. The open fails with the rules' own sentence
        // (`interactive::open_conversation_as`), the prompt is refused with
        // it, no provider is asked and no tool runs, as upstream's failed
        // first publish fails its turn before any call
        // (`appendRunningToolCalls`, `src/core/agent/runtime/orchestrator.zig:
        // 3894-3917@481104fd`). The rules stay held; revoked down to what a
        // store holds, the next prompt saves them and runs.
        use crate::permission::session_rules::{self, Decision, Event, KeyKind, RuleKey};
        use crate::permission::slash;
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let mut config = config(home.path(), workspace.path());
        config.permission_mode = PermissionMode::Auto;
        let store = open_store(&config).expect("a store");
        let mut state = Runtime::new(config, store, a_prompter(), a_questioner());
        let at = state.next_rules_path();
        let set = |n: usize| slash::Confirmation {
            question: "",
            explanation: "",
            display: String::new(),
            path: at.clone(),
            event: Event::Set {
                key: RuleKey::new(
                    KeyKind::FileMutation,
                    format!("{n:04}{}", "k".repeat(4000)).into_bytes(),
                )
                .expect("a key"),
                display: format!("filler {n} {}", "d".repeat(4000)),
                decision: Decision::Deny,
                expected: None,
            },
        };
        for n in 0..200 {
            assert_eq!(state.saved_rules.confirm(&set(n)).1, slash::UPDATED);
        }
        assert_eq!(confirmed(&state, HELD_DENY).1, slash::UPDATED);
        let held = state.saved_rules.view().state.expect("held");
        assert!(held.encode().len() as u64 > session_rules::MAX_FILE_BYTES);

        let asked = std::rc::Rc::new(std::cell::Cell::new(0));
        let provider = Counted {
            inner: ScriptedProvider::new(vec![
                calls(vec![call(
                    "call-1",
                    "write_file",
                    serde_json::json!({"path": "b.txt", "content": "y"}),
                )]),
                answers("DONE"),
            ]),
            asked: std::rc::Rc::clone(&asked),
        };
        let failure = one_turn_failure(&mut state, "write it", Box::new(provider));
        assert!(
            failure.as_deref().is_some_and(|failure| failure
                .starts_with("the saved-session permission rules cannot be written: ")),
            "{failure:?}"
        );
        assert_eq!(asked.get(), 0, "a provider was asked");
        assert!(!workspace.path().join("b.txt").exists(), "a tool ran");
        assert!(state.conversation.is_none(), "a session opened");
        let listed = state
            .store
            .list(&crate::session::ListFilter::new(
                crate::session::ListScope::AllWorkspaces,
            ))
            .expect("a listing");
        assert!(listed.sessions.is_empty(), "{:?}", listed.sessions);
        state.publish();
        let still = state.saved_rules.view();
        assert_eq!(
            still.at,
            state.next_rules_path(),
            "not moved to the next id"
        );
        assert_eq!(still.state.expect("held"), held, "the held rules moved");

        // Revoked down to the one deny, under the next id, and tried again.
        for rule in held
            .rules
            .iter()
            .filter(|rule| rule.display.starts_with("filler"))
        {
            let revoke = slash::Confirmation {
                question: "",
                explanation: "",
                display: String::new(),
                path: still.at.clone(),
                event: Event::Revoke {
                    id: rule.id,
                    expected: rule.generation,
                },
            };
            assert_eq!(state.saved_rules.confirm(&revoke).1, slash::UPDATED);
        }
        let asked = std::rc::Rc::new(std::cell::Cell::new(0));
        let provider = Counted {
            inner: ScriptedProvider::new(vec![
                calls(vec![call(
                    "call-1",
                    "write_file",
                    serde_json::json!({"path": "b.txt", "content": "y"}),
                )]),
                answers("DONE"),
            ]),
            asked: std::rc::Rc::clone(&asked),
        };
        assert_eq!(
            one_turn_failure(&mut state, "write it", Box::new(provider)),
            None
        );
        assert_eq!(asked.get(), 2);
        assert!(
            !workspace.path().join("b.txt").exists(),
            "the deny kept through the failed save let the write by"
        );
        let saved = session_rules::load(&still.at).expect("the file");
        assert_eq!(saved.rules.len(), 1, "{saved:?}");
        assert_eq!(
            saved.rules[0].display,
            "write_file b.txt preimage=absent after=a1fce4363854ff88"
        );
    }

    /// A provider that answers from a script and keeps every set of request
    /// options the turn gave it -- what a real wire would send beside the
    /// prompt (`crate::gateway::protocol::RequestOptions`).
    struct Sent {
        inner: ScriptedProvider,
        options: std::rc::Rc<std::cell::RefCell<Vec<crate::gateway::protocol::RequestOptions>>>,
    }

    #[async_trait::async_trait(?Send)]
    impl crate::gateway::Provider for Sent {
        async fn stream(
            &self,
            request: &crate::gateway::protocol::CompletionRequest,
            deltas: &mut dyn crate::gateway::DeltaSink,
        ) -> Result<Completion, crate::gateway::ProviderError> {
            self.inner.stream(request, deltas).await
        }

        fn set_options(&self, options: crate::gateway::protocol::RequestOptions) {
            self.options.borrow_mut().push(options);
        }
    }

    #[test]
    fn a_new_conversation_after_a_resume_records_the_settings_it_carries() {
        // `/resume` of a session saved in `low` and Fast, `/new`, one prompt.
        // The resume put the session's effort and Fast in force and `/new`
        // leaves them there, as upstream's does: its `/new`
        // (`commandNewSession`, `app_commands.zig:437-440@481104fd` ->
        // `newSession`, `app_session_runtime.zig:1524-1526` ->
        // `applyIdleLiveSessionTransition`, `:1788-1833`) resets neither the
        // effort, the switches nor the selection. Upstream then seeds the new
        // log from the workspace's preferences (`installFreshLiveSession`,
        // `:1713-1720` -> `beginFreshV2Session`, `:1481-1487`), which a
        // resume does not move -- a log that says what its turns are not
        // sent with. xfx records the settings in force, the ones the request
        // carries (`docs/parity.md`, `/new`).
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let launched = config(home.path(), workspace.path());
        let mut saved_under = launched.clone();
        saved_under.model = "openai/gpt-6-astra".to_string();
        let saved = {
            let store = open_store(&saved_under).expect("a store");
            let mut conversation = open_conversation(
                &store,
                &saved_under,
                &saved_under.model,
                PermissionSession::new(saved_under.permission_mode),
                None,
                &CancelToken::new(),
            )
            .expect("open a conversation");
            conversation.recorder.commit(SessionEvent::UserMessage {
                text: "first question alpha".to_string(),
            });
            conversation
                .recorder
                .commit(SessionEvent::PreferencesChanged {
                    model: None,
                    permission_mode: None,
                    provider: None,
                    effort: Some("low".to_string()),
                    fast_mode: Some(true),
                    ultrafast_mode: Some(false),
                });
            conversation.recorder.id().clone()
        };
        let store = open_store(&launched).expect("a store");
        let mut state = Runtime::new(launched, store, a_prompter(), a_questioner());
        assert_eq!(state.settings, TurnSettings::default());
        state
            .resume_by_id(saved.as_str(), &CancelToken::new())
            .expect("resumed");
        state
            .selector
            .set_catalog_for_test(CatalogState::Loaded(speed_catalog()));
        let (events_tx, _events_rx) = mpsc::channel(bridge::UI_EVENTS);
        on_a_runtime(drop_conversation(&mut state, &events_tx, 0));
        let carried = TurnSettings {
            effort: crate::provider::effort::Effort::Named("low".to_string()),
            fast: true,
            ultrafast: false,
        };
        assert_eq!(state.settings, carried, "`/new` reset the settings");
        assert_eq!(state.model(), "openai/gpt-6-astra");

        let sent = std::rc::Rc::default();
        let provider = Sent {
            inner: ScriptedProvider::new(vec![answers("FRESH")]),
            options: std::rc::Rc::clone(&sent),
        };
        assert_eq!(
            one_turn_on(&mut state, "fresh", Box::new(provider)),
            Ended::Turn
        );
        let fresh = state
            .conversation
            .as_ref()
            .expect("a new conversation")
            .recorder
            .id()
            .clone();
        assert_ne!(fresh, saved);
        let events = logged_events(&state, &fresh);
        assert_eq!(events[0]["kind"], "session_started", "{events:?}");
        assert_eq!(events[0]["model"], "openai/gpt-6-astra", "{}", events[0]);
        assert_eq!(
            events[1],
            serde_json::json!({
                "kind": "preferences_changed",
                "model": null,
                "permission_mode": null,
                "effort": "low",
                "fast_mode": true,
                "ultrafast_mode": false,
            })
        );
        // And the request was sent in them.
        let sent = sent.borrow();
        assert_eq!(sent.len(), 1, "{sent:?}");
        assert_eq!(sent[0].reasoning.as_deref(), Some("low"), "{sent:?}");
        assert!(sent[0].fast && !sent[0].ultrafast, "{sent:?}");
    }

    #[test]
    fn a_choice_made_after_a_resume_is_written_at_once_and_not_over_by_its_first_turn() {
        // `/model` and `/permissions` after the open write their own frames,
        // as they do in any session; the first turn then has only what the
        // open changed left to say, and says nothing over them.
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let saved_under = config(home.path(), workspace.path());
        let grant = Grant::new("edit_file", "notes.txt".to_string());
        let id = a_saved_session(&saved_under, &grant);
        let resumed = Resumption::open(&saved_under, &crate::session::Selector::Id(id))
            .expect("the saved session opens");
        let store = open_store(&saved_under).expect("a store");
        let mut state = Runtime::new(saved_under.clone(), store, a_prompter(), a_questioner());
        let before = log_text(state_with_log(&mut state, resumed));

        assert!(matches!(
            state.use_exact_model("chosen/model"),
            ModelOutcome::Selected { .. }
        ));
        assert_eq!(
            state.change_permissions(Change::Mode(PermissionMode::Ask)),
            None
        );
        let chosen = log_text(&state);
        assert!(chosen.len() > before.len(), "the choices were not written");
        assert!(chosen.contains(r#""model":"chosen/model""#), "{chosen}");
        assert!(chosen.contains(r#""permission_mode":"ask""#), "{chosen}");

        let model = state.model().to_string();
        let (mode, provider) = (state.config.permission_mode, state.config.provider);
        record_resumed_selection(
            state.conversation.as_mut().expect("bound"),
            &mut state.unrecorded_resume,
            &model,
            mode,
            provider,
        );
        assert_eq!(log_text(&state), chosen, "the turn wrote over them");
        let recorded = state.conversation.as_ref().expect("bound").recorder.state();
        assert_eq!(recorded.model, "chosen/model");
        assert_eq!(recorded.permission_mode, PermissionMode::Ask);
    }

    /// `state` with `resumed` bound, for a test that reads its log.
    fn state_with_log(state: &mut Runtime, resumed: Resumption) -> &Runtime {
        state.resume(resumed, &CancelToken::new());
        state
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
                live: LiveAuthority::new(PermissionMode::Auto),
                rules: LiveRules::default(),
                grants: Grants::default(),
                stop: CancelToken::new(),
                steering: Steering::new(),
                steers: Arc::new(AtomicU64::new(0)),
                saved_rules: SavedRules::default(),
                spend: SessionSpend::default(),
                credits: CreditsBoard::default(),
                mcp: crate::mcp::McpRuntime::inert(),
                titles: super::super::titles::TitleDesk::new(true).0,
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

    /// A refusal recorded while a parked prompter's stop waits for the loop
    /// is the stop's: the row's half says `Cancelled`, the model's half is
    /// what the model was told. Any other refusal is the user's.
    #[test]
    fn a_call_refused_by_a_stop_at_its_approval_is_recorded_as_stopped() {
        #[derive(Default)]
        struct Kept(Vec<SessionEvent>, Vec<String>);
        impl TurnJournal for Kept {
            fn record(&mut self, event: SessionEvent) {
                self.0.push(event);
            }
            fn change_made(&mut self, call_id: &str, _change: &crate::permission::ApprovalDiff) {
                self.1.push(format!("change {call_id}"));
            }
            fn presented(&mut self, call_id: &str, _tool: &str, _text: &str) {
                self.1.push(format!("presented {call_id}"));
            }
            fn attach_images(
                &mut self,
                images: &[crate::image::Image],
            ) -> Option<std::path::PathBuf> {
                self.1.push(format!("images {}", images.len()));
                Some(std::path::PathBuf::from("kept-images"))
            }
            fn attach_tool_images(&mut self, call_id: &str, images: &[crate::image::ToolImage]) {
                self.1
                    .push(format!("tool images {call_id} {}", images.len()));
            }
            fn state(&self) -> Option<&crate::session::DurableState> {
                Some(&KEPT_STATE)
            }
        }
        static KEPT_STATE: std::sync::LazyLock<crate::session::DurableState> =
            std::sync::LazyLock::new(crate::session::DurableState::empty_for_tests);
        let refused = || SessionEvent::ToolResult {
            call_id: "w".to_string(),
            tool: "write_file".to_string(),
            ok: false,
            output: "write_file was not permitted: you declined".to_string(),
            ending: Some(crate::session::ToolEnding::Denied),
            lines: None,
            change: None,
        };
        let (_tx, rx) = mpsc::unbounded_channel();
        let control = ControlChannel::new(rx);
        let mut kept = Kept::default();
        StoppedAtApproval {
            inner: &mut kept,
            control: &control,
        }
        .record(refused());
        control.give_back(TurnControl::Cancel { through: 1 });
        StoppedAtApproval {
            inner: &mut kept,
            control: &control,
        }
        .record(refused());
        let endings: Vec<_> = kept
            .0
            .iter()
            .map(|event| match event {
                SessionEvent::ToolResult { ending, output, .. } => (*ending, output.as_str()),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(
            endings,
            [
                (
                    Some(crate::session::ToolEnding::Denied),
                    "write_file was not permitted: you declined"
                ),
                (
                    Some(crate::session::ToolEnding::Cancelled),
                    "write_file was not permitted: you declined"
                ),
            ]
        );
        // Everything else the turn hands its journal goes through unchanged:
        // a default left in the wrapper would be the recorder's work undone.
        let mut wrapper = StoppedAtApproval {
            inner: &mut kept,
            control: &control,
        };
        wrapper.change_made("w", &crate::permission::ApprovalDiff::of("a\n", "b\n"));
        wrapper.presented("w", "write_file", "shown");
        // The directory the images were copied into is the recorder's
        // answer, which the card's badges link to.
        assert_eq!(
            wrapper.attach_images(&[crate::image::Image::new(
                1,
                std::path::PathBuf::from("shot.png"),
                crate::image::MediaType::Png,
                std::sync::Arc::from(&b"png"[..]),
            )]),
            Some(std::path::PathBuf::from("kept-images"))
        );
        wrapper.attach_tool_images(
            "r",
            &[crate::image::ToolImage {
                media_type: crate::image::MediaType::Png,
                data: std::sync::Arc::from(&b"png"[..]),
                path: "shot.png".to_string(),
            }],
        );
        // The log as it stands, which a compaction between the turn's steps
        // plans from.
        assert!(std::ptr::eq(
            wrapper.state().expect("forwarded"),
            &*KEPT_STATE
        ));
        assert_eq!(
            kept.1,
            ["change w", "presented w", "images 1", "tool images r 1"]
        );
        // And the loop reads its stop as before.
        assert!(control.stopping());
        assert_eq!(
            on_a_runtime(control.recv()),
            Some(TurnControl::Cancel { through: 1 })
        );
        assert!(!control.stopping());
    }

    /// The log of the session `id` names, under `home`.
    fn session_log(home: &std::path::Path, id: &str) -> String {
        std::fs::read_to_string(
            home.join(".xfx")
                .join("sessions")
                .join(id)
                .join(crate::session::store::EVENTS_FILE),
        )
        .expect("the log")
    }

    /// Between turns a `/rename` is written when it is read, in the order the
    /// control channel carries it with a reset: one typed before `/new` goes
    /// into the session `/new` drops, one typed after it into the session
    /// `/new` starts (opened for it, as a rename before any prompt opens one).
    #[test]
    fn between_turns_a_rename_is_written_into_the_session_it_was_typed_in() {
        let (home, _workspace, state) = recording();
        let before = state
            .conversation
            .as_ref()
            .expect("open")
            .recorder
            .id()
            .clone();
        let (events_tx, mut events_rx) = mpsc::channel(bridge::UI_EVENTS);
        let (work_tx, mut work_rx) = mpsc::channel::<TurnWork>(WORK_LIMIT);
        let (control_tx, control_rx) = mpsc::unbounded_channel();
        for message in [
            TurnControl::Rename {
                title: "Before New".to_string(),
                announced: false,
            },
            TurnControl::Reset { through: 0 },
            TurnControl::Rename {
                title: "After New".to_string(),
                announced: false,
            },
            TurnControl::Shutdown,
        ] {
            control_tx.send(message).expect("alive");
        }
        let (_changes_tx, mut changes) = mpsc::unbounded_channel::<Change>();
        let outstanding = AtomicUsize::new(0);
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
        // Titled and asked nothing, so no resume names it: it is the other
        // session on disk.
        let after = sessions_in(home.path())
            .into_iter()
            .find(|id| id.as_str() != before.as_str())
            .expect("the session the reset started");
        let dropped = session_log(home.path(), before.as_str());
        assert!(
            dropped.contains(r#""kind":"session_renamed","title":"Before New""#)
                && !dropped.contains("After New"),
            "{dropped}"
        );
        let started = session_log(home.path(), after.as_str());
        assert!(
            started.contains(r#""title":"After New""#) && !started.contains("Before New"),
            "{started}"
        );
        // Between turns the shell said nothing, so each written title is
        // said now, in the order the writes happened.
        assert_eq!(
            events_rx.try_recv(),
            Ok(UiEvent::Renamed("Before New".to_string()))
        );
        assert_eq!(events_rx.try_recv(), Ok(UiEvent::Fresh { through: 0 }));
        assert_eq!(
            events_rx.try_recv(),
            Ok(UiEvent::Renamed("After New".to_string()))
        );
    }

    /// One answer per rename, what became of its write: `Renamed` for one the
    /// shell has not said yet, nothing for one it has, and the refusal for
    /// either (`handleRenameCommand`, `app_commands.zig:3815-3858`).
    #[test]
    fn a_rename_is_answered_once_by_what_became_of_its_write() {
        use std::os::unix::fs::PermissionsExt;
        let (home, _workspace, mut state) = recording();
        let (events_tx, mut events_rx) = mpsc::channel(bridge::UI_EVENTS);
        let cancel = Cancellation::new(CancelToken::new());
        on_a_runtime(write_rename(
            &mut state, "Quiet", false, &events_tx, &cancel,
        ));
        assert_eq!(
            events_rx.try_recv(),
            Ok(UiEvent::Renamed("Quiet".to_string()))
        );
        on_a_runtime(write_rename(&mut state, "Said", true, &events_tx, &cancel));
        assert!(events_rx.try_recv().is_err(), "a rename was said twice");
        let id = state
            .conversation
            .as_ref()
            .expect("open")
            .recorder
            .state()
            .id
            .clone();
        let dir = home.path().join(".xfx").join("sessions").join(id);
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o500)).expect("chmod");
        on_a_runtime(write_rename(
            &mut state, "Refused", false, &events_tx, &cancel,
        ));
        on_a_runtime(write_rename(
            &mut state,
            "Refused too",
            true,
            &events_tx,
            &cancel,
        ));
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).expect("chmod");
        for _ in 0..2 {
            assert!(
                matches!(events_rx.try_recv(), Ok(UiEvent::RenameUnsaved(_))),
                "a refused write was not said"
            );
        }
        assert!(events_rx.try_recv().is_err());
    }

    /// While work runs a `/rename` is put aside, never a stop, and marked
    /// when it came behind a reset -- as an `/undo` is.
    #[test]
    fn a_rename_while_work_runs_waits_for_it_and_one_after_a_reset_is_the_fresh_ones() {
        let (control_tx, control_rx) = mpsc::unbounded_channel();
        let control = ControlChannel::new(control_rx);
        for message in [
            TurnControl::Rename {
                title: "a".to_string(),
                announced: true,
            },
            TurnControl::Reset { through: 0 },
            TurnControl::Rename {
                title: "b".to_string(),
                announced: false,
            },
        ] {
            control_tx.send(message).expect("alive");
        }
        let mut stops = Vec::new();
        let (_, ended) = on_a_runtime(raced_against_control(
            async {
                tokio::time::sleep(Duration::from_millis(50)).await;
            },
            &control,
            &Arc::new(CatalogAsks::AfterWork),
            |through| stops.push(through),
        ));
        assert_eq!(stops.len(), 1, "a rename stopped the work");
        assert_eq!(ended, Ended::Reset(0));
        assert_eq!(
            control.take_deferred(),
            vec![
                Deferred::Rename {
                    title: "a".to_string(),
                    after_reset: false,
                    announced: true,
                },
                Deferred::Rename {
                    title: "b".to_string(),
                    after_reset: true,
                    announced: false,
                },
            ]
        );
    }

    /// Once the work is over: a title typed before the reset is written into
    /// the session the reset drops, before the drop; one typed after it into
    /// the session the reset starts, after the drop -- and both even when the
    /// session is ending, because the shell has already said them.
    #[test]
    fn renames_put_aside_land_on_either_side_of_the_reset_they_were_typed_around() {
        let (home, _workspace, mut state) = recording();
        let before = state
            .conversation
            .as_ref()
            .expect("open")
            .recorder
            .id()
            .clone();
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
        // Said by the shell already, as one typed while work runs is.
        control.defer_rename("Typed Before".to_string(), false, true);
        control.defer_rename("Typed After".to_string(), true, true);
        let ended = on_a_runtime(async {
            let ended = apply_deferred(
                &mut state,
                Ended::Reset(0),
                &events_tx,
                &cancel,
                &control,
                &mut queue,
            )
            .await;
            if let Ended::Reset(through) = ended {
                drop_conversation(&mut state, &events_tx, through).await;
                write_renamed_after_reset(&mut state, &events_tx, &cancel).await;
            }
            ended
        });
        assert_eq!(ended, Ended::Reset(0));
        let dropped = session_log(home.path(), before.as_str());
        assert!(
            dropped.contains("Typed Before") && !dropped.contains("Typed After"),
            "{dropped}"
        );
        let after = state
            .conversation
            .as_ref()
            .expect("opened for the title")
            .recorder
            .id()
            .clone();
        assert_ne!(after, before);
        assert!(session_log(home.path(), after.as_str()).contains("Typed After"));
        assert_eq!(events_rx.try_recv(), Ok(UiEvent::Fresh { through: 0 }));

        // A `/quit` after the work: still written.
        control.defer_rename("Then Quit".to_string(), false, true);
        let ended = on_a_runtime(apply_deferred(
            &mut state,
            Ended::Session,
            &events_tx,
            &cancel,
            &control,
            &mut queue,
        ));
        assert_eq!(ended, Ended::Session);
        assert!(session_log(home.path(), after.as_str()).contains("Then Quit"));

        // A `/new`, a `/rename` and a `/quit` in one turn: the shutdown
        // outranks the reset, and the title still names the session the
        // reset would have started -- opened for it, not the one it left.
        control.defer_rename("New Then Quit".to_string(), true, true);
        on_a_runtime(async {
            let ended = apply_deferred(
                &mut state,
                Ended::Session,
                &events_tx,
                &cancel,
                &control,
                &mut queue,
            )
            .await;
            assert_eq!(ended, Ended::Session);
            write_renamed_before_leaving(&mut state, &events_tx, &cancel).await;
        });
        assert!(!session_log(home.path(), after.as_str()).contains("New Then Quit"));
        let last = state
            .conversation
            .as_ref()
            .expect("opened for the title")
            .recorder
            .id()
            .clone();
        assert!(last != after && last != before);
        assert!(session_log(home.path(), last.as_str()).contains("New Then Quit"));
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
            &Arc::new(CatalogAsks::AfterWork),
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
            &Arc::new(CatalogAsks::AfterWork),
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
                speed: None,
                picker: false,
                announced: false,
                next_turn,
                provider: ProviderId::Gateway,
                effort_only: false,
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

    /// [`Unreachable`], counting how often it was asked.
    struct CountedUnreachable {
        asked: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait(?Send)]
    impl crate::provider::model::ModelCatalog for CountedUnreachable {
        async fn fetch(&self) -> Result<Vec<CatalogEntry>, crate::provider::model::CatalogError> {
            self.asked.fetch_add(1, Ordering::SeqCst);
            Unreachable.fetch().await
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

    /// A catalog that answers `entries` once `gate` opens.
    struct Gated {
        entries: Vec<CatalogEntry>,
        gate: Mutex<Option<tokio::sync::oneshot::Receiver<()>>>,
    }

    #[async_trait::async_trait(?Send)]
    impl crate::provider::model::ModelCatalog for Gated {
        async fn fetch(&self) -> Result<Vec<CatalogEntry>, crate::provider::model::CatalogError> {
            let gate = self.gate.lock().expect("gate").take().expect("asked once");
            let _ = gate.await;
            Ok(self.entries.clone())
        }
    }

    /// A catalog that fails its first read and answers `entries` after it:
    /// the Gateway down at launch and back by the time `/model` asks again.
    struct BackAfterOne {
        entries: Vec<CatalogEntry>,
        asked: AtomicUsize,
    }

    #[async_trait::async_trait(?Send)]
    impl crate::provider::model::ModelCatalog for BackAfterOne {
        async fn fetch(&self) -> Result<Vec<CatalogEntry>, crate::provider::model::CatalogError> {
            if self.asked.fetch_add(1, Ordering::SeqCst) == 0 {
                return Unreachable.fetch().await;
            }
            Ok(self.entries.clone())
        }
    }

    #[test]
    fn a_catalog_asked_for_again_while_a_turn_runs_is_read_at_once_and_told_before_it_ends() {
        // Upstream's `/model` starts the load on a thread of its own whatever
        // its worker is doing -- again after one that failed (`ensureModelCache`
        // from `openModelBrowseCatalog`, `app_input_runtime.zig:2346-2349`;
        // `beginLoad`, `model_cache_runtime.zig:405-458`) -- and the open menu
        // takes the rows when they land (`pollLoadTransition`, `:549-557`).
        // Here a stand-in turn holds the runtime throughout: the request is a
        // control message the work's race reads, the read runs beside it, the
        // band is told before the turn ends, and the selector takes the
        // answer once it has.
        let (_home, _workspace, mut state) = recording();
        state.selector.set_fetcher_for_test(Box::new(BackAfterOne {
            entries: vec![published("anthropic/claude-opus-5")],
            asked: AtomicUsize::new(0),
        }));
        let (events, mut received) = mpsc::channel(bridge::UI_EVENTS);
        on_a_runtime(state.load_catalog());
        assert!(matches!(state.selector.catalog(), CatalogState::Failed(_)));
        // Past the second a failure is held for (`RETRY_FLOOR`).
        state.selector.backdate_attempt_for_test();
        let (control_tx, control_rx) = mpsc::unbounded_channel();
        let control = ControlChannel::new(control_rx);
        let asks = Arc::new(CatalogAsks::reading(&state, &events));
        let (turn_over, ended) = tokio::sync::oneshot::channel::<()>();
        let told = on_a_runtime(async {
            let turn = async {
                let _ = ended.await;
            };
            let watch = async {
                control_tx
                    .send(TurnControl::Catalog)
                    .expect("the work listens");
                let told = tokio::time::timeout(TEST_DRAIN, received.recv())
                    .await
                    .expect("the band was not told while the turn ran")
                    .expect("told");
                turn_over.send(()).expect("the turn is waiting");
                told
            };
            let (((), ended), told) = tokio::join!(
                raced_against_control(turn, &control, &asks, |_| {
                    panic!("nothing stopped the turn")
                }),
                watch
            );
            assert_eq!(ended, Ended::Turn);
            told
        });
        assert_eq!(
            told,
            UiEvent::CatalogLoaded {
                provider: state.config.provider,
                entries: vec![published("anthropic/claude-opus-5")],
            }
        );
        // The selector takes it once the work lets go, and the UI is not told
        // it a second time.
        asks.finish(&mut state.selector);
        assert!(state.selector.is_loading());
        let fresh = on_a_runtime(state.load_catalog());
        assert!(!fresh, "a read that told the UI itself was told again");
        assert!(state.selector.entry("anthropic/claude-opus-5").is_some());
        assert!(received.try_recv().is_err());
    }

    /// The wait a stand-in turn parks this thread in
    /// ([`catalog_told_while_parked`]).
    #[derive(Clone, Copy)]
    enum Parked {
        /// The model's question batch (`ask_user_question`).
        Question,
        /// A permission question.
        Approval,
    }

    /// Runs a stand-in turn whose body parks this thread in `parked`'s wait,
    /// as a real tool call does, with `/model`'s ask arriving while it is
    /// parked: the band asks for the catalog the moment it sees the question,
    /// and answers the question only once it has been told the rows -- or once
    /// a minute has gone by without them. What the band was told before it
    /// answered, how many reads the catalog served, and whether the selector
    /// then took the read's rows without telling them again.
    fn catalog_told_while_parked(parked: Parked) -> (Vec<UiEvent>, usize, bool) {
        use crate::permission::ApprovalPrompter;
        use crate::tools::question::{QuestionEntry, QuestionOption};
        use crate::tools::{QuestionReply, QuestionRequester};
        let (_home, _workspace, mut state) = recording();
        let reads = Arc::new(AtomicUsize::new(0));
        state.selector.set_fetcher_for_test(Box::new(Answering {
            entries: vec![published("anthropic/claude-opus-5")],
            asked: Arc::clone(&reads),
        }));
        let (events, mut received) = mpsc::channel(bridge::UI_EVENTS);
        let (control_tx, control_rx) = mpsc::unbounded_channel();
        let control = ControlChannel::new(control_rx);
        let cancel = Cancellation::new(CancelToken::new());
        let asks = Arc::new(CatalogAsks::reading(&state, &events));
        let band = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(60);
            let mut told = Vec::new();
            let mut answer = None;
            while Instant::now() < deadline {
                match received.try_recv() {
                    Ok(UiEvent::Question(request)) => {
                        answer = Some(TurnControl::QuestionAnswer {
                            id: request.id,
                            answers: vec!["Thorough".to_string()],
                        });
                        control_tx
                            .send(TurnControl::Catalog)
                            .expect("the work listens");
                    }
                    Ok(UiEvent::Approval(asked)) => {
                        answer = Some(TurnControl::Answer {
                            id: asked.id,
                            answer: ApprovalAnswer::Once,
                            feedback: None,
                        });
                        control_tx
                            .send(TurnControl::Catalog)
                            .expect("the work listens");
                    }
                    Ok(event) => {
                        let rows = matches!(event, UiEvent::CatalogLoaded { .. });
                        told.push(event);
                        if rows {
                            break;
                        }
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(5)),
                }
            }
            control_tx
                .send(answer.expect("the question was asked"))
                .expect("the work listens");
            told
        });
        let body = async {
            match parked {
                Parked::Question => {
                    let questioner =
                        TuiQuestioner::new(events.clone(), Arc::clone(&control), cancel.clone());
                    let reply = questioner.request(&[QuestionEntry {
                        question: "How deep?".to_string(),
                        options: vec![QuestionOption {
                            label: "Thorough".to_string(),
                            description: None,
                        }],
                    }]);
                    assert!(matches!(reply, QuestionReply::Answered(_)), "{reply:?}");
                }
                Parked::Approval => {
                    let mut prompter =
                        TuiPrompter::new(events.clone(), Arc::clone(&control), cancel.clone());
                    let answer = prompter
                        .request(&crate::permission::ApprovalRequest {
                            tool: "edit_file",
                            target: "notes.txt".into(),
                            summary: "replace `alpha` with `beta` in notes.txt".into(),
                            always_scope: "allow every future edit_file to `notes.txt`".into(),
                            diff: None,
                            subject: crate::permission::ApprovalSubject::Path("notes.txt".into()),
                        })
                        .expect("asked");
                    assert_eq!(answer, ApprovalAnswer::Once);
                }
            }
        };
        let ((), ended) = on_a_runtime(raced_against_control(body, &control, &asks, |_| {
            panic!("nothing stopped the turn")
        }));
        assert_eq!(ended, Ended::Turn);
        let told = band.join().expect("the band");
        // The selector takes the read once the work lets go, and the UI is not
        // told it a second time.
        asks.finish(&mut state.selector);
        let fresh = on_a_runtime(state.load_catalog());
        let taken = !fresh && state.selector.entry("anthropic/claude-opus-5").is_some();
        (told, reads.load(Ordering::SeqCst), taken)
    }

    #[test]
    fn a_catalog_asked_for_while_a_question_has_parked_the_turn_is_told_before_it_is_answered() {
        // Upstream's catalog load runs on a thread of its own, so the
        // model's question does not hold it: the load started by `/model`
        // lands, and the open menu takes its rows, whatever the worker is
        // waiting for (`ensureModelCache` -> `startWarmup`, spawning
        // `preloadThreadMain`, `model_cache_runtime.zig:321-349`, `:698-743`;
        // `pollLoadTransition`, `:549-557`). Here the question's wait holds
        // the runtime thread, and the read starts from it on a thread of its
        // own and tells the band while the question is still up.
        let (told, reads, taken) = catalog_told_while_parked(Parked::Question);
        assert_eq!(
            told,
            [UiEvent::CatalogLoaded {
                provider: crate::provider::ProviderId::Gateway,
                entries: vec![published("anthropic/claude-opus-5")],
            }],
            "the band was not told while the question was up"
        );
        assert_eq!(reads, 1);
        assert!(
            taken,
            "the selector did not take the read, or told it again"
        );
    }

    #[test]
    fn a_catalog_asked_for_while_an_approval_has_parked_the_turn_is_told_before_it_is_answered() {
        // The same for a permission question: its wait holds the thread as
        // the model's question's does, and the read is not held by it.
        let (told, reads, taken) = catalog_told_while_parked(Parked::Approval);
        assert_eq!(
            told,
            [UiEvent::CatalogLoaded {
                provider: crate::provider::ProviderId::Gateway,
                entries: vec![published("anthropic/claude-opus-5")],
            }],
            "the band was not told while the approval was up"
        );
        assert_eq!(reads, 1);
        assert!(
            taken,
            "the selector did not take the read, or told it again"
        );
    }

    /// The next event the band is told, waiting for it up to a minute.
    fn told_within_a_minute(received: &mut mpsc::Receiver<UiEvent>) -> Option<UiEvent> {
        let deadline = Instant::now() + Duration::from_secs(60);
        while Instant::now() < deadline {
            match received.try_recv() {
                Ok(event) => return Some(event),
                Err(_) => std::thread::sleep(Duration::from_millis(5)),
            }
        }
        None
    }

    #[test]
    fn the_sessions_own_read_failing_mid_work_is_read_again_once_its_second_is_up() {
        // The read at launch is still on its way when a turn begins, and fails
        // while the turn runs. Upstream's `/model` then decides by the cache's
        // state, whoever started the load: inside a second of its start the
        // menu opens over the failure, and after it a load starts again
        // (`beginLoad`, `model_cache_runtime.zig:441-447`). Round 4 left this
        // ask unanswered -- the work held "a read of its own on the way" --
        // and the menu said `Loading models…` until the turn was over.
        let (_home, _workspace, mut state) = recording();
        let reads = Arc::new(AtomicUsize::new(0));
        state
            .selector
            .set_fetcher_for_test(Box::new(CountedUnreachable {
                asked: Arc::clone(&reads),
            }));
        let (events, mut received) = mpsc::channel(bridge::UI_EVENTS);
        begin_catalog_read(&mut state, &events);
        let asks = Arc::new(CatalogAsks::reading(&state, &events));
        let failed = told_within_a_minute(&mut received).expect("the launch read told");
        let UiEvent::CatalogUnread { attempted, .. } = failed else {
            panic!("{failed:?}");
        };
        // The read tells before it records what it told: waited for, so the
        // ask below sees the answer and not a read still on its way.
        let deadline = Instant::now() + Duration::from_secs(60);
        while !state.selector.has_answered() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(state.selector.has_answered(), "the launch read never ended");
        let (_control_tx, control_rx) = mpsc::unbounded_channel();
        let control = ControlChannel::new(control_rx);
        // Inside its second: told again as it failed, and nothing read.
        assert_eq!(asks.asked_at(&control, attempted), Some(failed.clone()));
        assert_eq!(reads.load(Ordering::SeqCst), 1);
        // After it: read again, and the band told that read's answer.
        assert_eq!(
            asks.asked_at(&control, attempted + crate::provider::model::RETRY_FLOOR),
            None
        );
        let again = told_within_a_minute(&mut received).expect("the second read told");
        let UiEvent::CatalogUnread {
            attempted: second, ..
        } = again
        else {
            panic!("{again:?}");
        };
        assert!(second > attempted, "the first read's failure told again");
        assert_eq!(reads.load(Ordering::SeqCst), 2);
        // The selector takes the newer read, not the one it had.
        asks.finish(&mut state.selector);
        on_a_runtime(state.selector.settle_load());
        assert_eq!(state.selector.last_attempt(), Some(second));
    }

    #[test]
    fn a_catalog_request_during_a_provider_switch_waits_for_the_switch() {
        // A switch's catalog is the next provider's, not the one in hand: the
        // request is put aside and answered once the switch is over.
        let (_control_tx, control_rx) = mpsc::unbounded_channel();
        let control = ControlChannel::new(control_rx);
        CatalogAsks::AfterWork.ask(&control);
        assert!(matches!(
            control.take_deferred().as_slice(),
            [crate::tui::approval::Deferred::Catalog]
        ));
    }

    #[test]
    fn a_catalog_that_answers_while_a_turn_runs_is_told_to_the_band_at_once() {
        // Upstream's interface redraws an open menu and the footer on the
        // first tick after a load ends, whatever its worker is doing
        // (`pollLoadTransition`, `model_cache_runtime.zig:549-557`, from
        // `loopCollectFacts`, `main.zig:3050-3083`). Here a stand-in turn holds
        // the runtime the whole time; the rows reach the UI before it ends,
        // once, and the selector takes them when it is next asked.
        let (_home, _workspace, mut state) = recording();
        let (open, gate) = tokio::sync::oneshot::channel();
        state.selector.set_fetcher_for_test(Box::new(Gated {
            entries: vec![published("anthropic/claude-opus-5")],
            gate: Mutex::new(Some(gate)),
        }));
        let (events, mut received) = mpsc::channel(bridge::UI_EVENTS);
        let (turn_over, ended) = tokio::sync::oneshot::channel::<()>();
        let told = on_a_runtime(async {
            begin_catalog_read(&mut state, &events);
            let held = &mut state;
            let turn = async move {
                let _ = ended.await;
                held.selector.is_loading()
            };
            let watch = async {
                open.send(()).expect("the read is waiting");
                let told = tokio::time::timeout(TEST_DRAIN, received.recv())
                    .await
                    .expect("the band was not told while the turn ran")
                    .expect("told");
                turn_over.send(()).expect("the turn is waiting");
                told
            };
            let (still_loading, told) = tokio::join!(turn, watch);
            assert!(still_loading, "the selector took it before it was asked");
            told
        });
        assert_eq!(
            told,
            UiEvent::CatalogLoaded {
                provider: state.config.provider,
                entries: vec![published("anthropic/claude-opus-5")],
            }
        );
        // Taken when asked, and not told a second time.
        let fresh = on_a_runtime(state.load_catalog());
        assert!(!fresh, "a read that told the UI itself was told again");
        assert!(state.selector.entry("anthropic/claude-opus-5").is_some());
        assert!(received.try_recv().is_err());
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
                speed: None,
                provider: ProviderId::Gateway,
                effort_only: false,
            },
        );
        assert_eq!(
            said,
            [UiEvent::ModelAnswered {
                model: "openai/gpt-6-astra".to_string(),
                settings: TurnSettings {
                    effort: low.clone(),
                    ..TurnSettings::default()
                },
                // The fixture's Gateway has no credential, so no catalog to
                // check against: applied, and no caveat.
                outcome: ModelAnswer::Applied { unverified: None },
                next_turn: false,
                announced: true,
            }]
        );
        assert_eq!(state.settings.effort, low);
        assert_eq!(
            settings(home.path()),
            serde_json::json!({"theme": "dark", "effort": "low",
                               "models": {"gateway": "openai/gpt-6-astra"},
                               "provider": "gateway",
                               "fast_mode": false, "ultrafast_mode": false,
                               "fast_mode_model_bound": true})
        );
        // The next launch starts in it.
        let reloaded = config(home.path(), workspace.path());
        assert_eq!(reloaded.effort, low);
        assert_eq!(reloaded.model, "openai/gpt-6-astra");

        // A router-form change leaves the effort as it is, and does not write
        // it: it changed only the model (`setResolvedModel`).
        let said = ran_model(&mut state, "acme/one", false);
        let UiEvent::ModelAnswered { settings: kept, .. } = &said[0] else {
            panic!("{said:?}");
        };
        assert_eq!(
            kept.effort, low,
            "a model change dropped the session's effort"
        );
        assert_eq!(settings(home.path())["effort"], "low");
        assert_eq!(settings(home.path())["models"]["gateway"], "acme/one");
    }

    #[test]
    fn an_effort_alone_moves_the_sessions_effort_and_selects_writes_and_says_nothing() {
        // `/settings`' `Reasoning effort` (`selectEffortFromSettings`): the
        // band wrote `effort` and said so; the runtime only takes it up. The
        // model stays, and the profile is not touched -- in particular no
        // `models` entry is written for a model some other layer chose.
        let (home, _workspace, mut state) = recording();
        std::fs::create_dir_all(home.path().join(".xfx")).expect("a profile home");
        std::fs::write(
            home.path().join(".xfx").join("settings.json"),
            r#"{"theme":"dark"}"#,
        )
        .expect("seed");
        let model = state.model().to_string();
        let high = crate::provider::effort::Effort::Named("high".to_string());
        let said = ran_change(
            &mut state,
            ModelChange {
                name: "something/else".to_string(),
                effort: Some(high.clone()),
                speed: None,
                picker: false,
                announced: true,
                next_turn: false,
                // Whatever provider it names: the effort is global.
                provider: ProviderId::Llmux,
                effort_only: true,
            },
        );
        assert!(said.is_empty(), "{said:?}");
        assert_eq!(state.settings.effort, high);
        assert_eq!(
            recorded(&state).map(|settings| settings.effort),
            Some(high.clone()),
            "a resume would not read it back"
        );
        assert_eq!(state.model(), model, "the model moved");
        assert_eq!(settings(home.path()), serde_json::json!({"theme": "dark"}));
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
        let options = |state: &Runtime| state.request_options().expect("no Ultra").reasoning;
        assert_eq!(options(&state), None, "the default asks for nothing");
        state.settings.effort = crate::provider::effort::Effort::Named("high".to_string());
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
            speed: None,
            provider: ProviderId::Gateway,
            effort_only: false,
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
            &Arc::new(CatalogAsks::AfterWork),
            |through| stops.push(through),
        ));
        assert!(stops.is_empty(), "a model change stopped the work");
        assert_eq!(ended, Ended::Turn);
        assert_eq!(control.take_deferred(), vec![Deferred::Model(asked)]);
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
            speed: None,
            provider: ProviderId::Gateway,
            effort_only: false,
        });
        control.defer(ModelChange {
            name: "unresolved".to_string(),
            effort: None,
            picker: false,
            announced: false,
            next_turn: true,
            speed: None,
            provider: ProviderId::Gateway,
            effort_only: false,
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

    /// One catalog row offering `efforts`, and Fast and Ultra as told.
    fn sped(id: &str, efforts: &[&str], fast: bool, ultrafast: bool) -> CatalogEntry {
        let mut entry = leveled(id, efforts);
        entry.facts.fast = fast;
        entry.facts.ultrafast = ultrafast;
        entry
    }

    /// The capture harness's rows with their speeds.
    fn speed_catalog() -> Vec<CatalogEntry> {
        vec![
            sped("openai/gpt-6-astra", &["low", "high"], true, true),
            sped("anthropic/claude-opus-5", &["low", "max"], true, false),
            published("fake/model-a"),
        ]
    }

    /// What the session log holds as the effort and the switches.
    fn recorded(state: &Runtime) -> Option<TurnSettings> {
        state
            .conversation
            .as_ref()
            .expect("open")
            .recorder
            .state()
            .turn_settings
            .clone()
    }

    #[test]
    fn every_pick_writes_upstreams_fields_and_a_row_with_levels_takes_the_sessions_effort() {
        // `selectModelFromPicker` (`session_commands.zig:886-915`), the one
        // path a menu row, the column, `/model <id> <effort>`, a `C-p` row and
        // `/settings`' model list (`applyInlineSettingsModelSelection`,
        // `app_input_runtime.zig:2517-2529`) all reach: the provider, the
        // model, the switches and the binding -- and the effort whenever the
        // model's row lists levels, the session's when the pick chose none.
        let (home, _workspace, mut state) = recording();
        state
            .selector
            .set_catalog_for_test(CatalogState::Loaded(speed_catalog()));
        state.settings.effort = crate::provider::effort::Effort::Named("max".to_string());
        let kept = |name: &str| ModelChange {
            name: name.to_string(),
            effort: None,
            speed: None,
            picker: true,
            announced: true,
            next_turn: false,
            provider: ProviderId::Gateway,
            effort_only: false,
        };
        let said = ran_change(&mut state, kept("anthropic/claude-opus-5"));
        assert!(
            said.iter()
                .all(|event| !matches!(event, UiEvent::Notice(_))),
            "{said:?}"
        );
        assert_eq!(
            settings(home.path()),
            serde_json::json!({"models": {"gateway": "anthropic/claude-opus-5"},
                               "provider": "gateway", "effort": "max",
                               "fast_mode": false, "ultrafast_mode": false,
                               "fast_mode_model_bound": true})
        );
        // A row with no levels takes no effort, so none is written; the one
        // already there stays.
        std::fs::remove_file(home.path().join(".xfx").join("settings.json")).expect("emptied");
        ran_change(&mut state, kept("fake/model-a"));
        assert_eq!(
            settings(home.path()),
            serde_json::json!({"models": {"gateway": "fake/model-a"},
                               "provider": "gateway",
                               "fast_mode": false, "ultrafast_mode": false,
                               "fast_mode_model_bound": true})
        );
        // And a query writes none either (`setResolvedModel`).
        std::fs::remove_file(home.path().join(".xfx").join("settings.json")).expect("emptied");
        ran_model(&mut state, "openai/gpt-6-astra", false);
        assert_eq!(settings(home.path()).get("effort"), None);
    }

    #[test]
    fn a_pick_keeps_the_speed_its_row_offers_and_a_query_turns_both_off() {
        // `selectModelFromPicker` and `setResolvedModel`
        // (`session_commands.zig:886-915`, `:1168-1190`), recorded in the
        // session and saved beside the model.
        let (home, _workspace, mut state) = recording();
        state
            .selector
            .set_catalog_for_test(CatalogState::Loaded(speed_catalog()));
        let low = crate::provider::effort::Effort::Named("low".to_string());
        let picked =
            |name: &str, effort: Option<crate::provider::effort::Effort>, speed| ModelChange {
                name: name.to_string(),
                effort,
                speed,
                picker: true,
                announced: true,
                next_turn: false,
                provider: ProviderId::Gateway,
                effort_only: false,
            };
        ran_change(
            &mut state,
            picked(
                "openai/gpt-6-astra",
                Some(low.clone()),
                Some(crate::provider::effort::Speed::Ultrafast),
            ),
        );
        let ultra = TurnSettings {
            effort: low.clone(),
            fast: false,
            ultrafast: true,
        };
        assert_eq!(state.settings, ultra);
        assert_eq!(recorded(&state), Some(ultra));
        let saved = settings(home.path());
        assert_eq!(
            (
                &saved["ultrafast_mode"],
                &saved["fast_mode"],
                &saved["fast_mode_model_bound"]
            ),
            (
                &serde_json::json!(true),
                &serde_json::json!(false),
                &serde_json::json!(true)
            )
        );
        // Ultra on a row that does not offer it is no speed at all, and the
        // session's Fast stands only where the row offers it.
        ran_change(
            &mut state,
            picked(
                "anthropic/claude-opus-5",
                None,
                Some(crate::provider::effort::Speed::Ultrafast),
            ),
        );
        assert!(!state.settings.ultrafast && !state.settings.fast);
        state.settings.fast = true;
        ran_change(&mut state, picked("openai/gpt-6-astra", None, None));
        assert!(
            state.settings.fast,
            "no speed column keeps Fast where it is offered"
        );
        ran_change(&mut state, picked("fake/model-a", None, None));
        assert!(!state.settings.fast, "a row with no Fast");
        // A query that changes the model turns both off.
        state.settings.fast = true;
        ran_model(&mut state, "opus", false);
        assert_eq!(state.model(), "anthropic/claude-opus-5");
        assert!(!state.settings.fast);
        assert_eq!(recorded(&state).map(|settings| settings.fast), Some(false));
    }

    #[test]
    fn a_speed_change_is_applied_recorded_and_saved_and_one_overtaken_by_a_switch_is_not() {
        let (home, _workspace, mut state) = recording();
        change_speed(
            &mut state,
            SpeedChange {
                fast: true,
                ultrafast: false,
                topic: "fast",
                provider: ProviderId::Gateway,
            },
        );
        assert!(state.settings.fast && !state.settings.ultrafast);
        assert_eq!(recorded(&state).map(|settings| settings.fast), Some(true));
        assert_eq!(settings(home.path())["fast_mode"], serde_json::json!(true));
        // Decided under the provider a switch replaced: dropped, because the
        // switches went off with the model the switch replaced.
        change_speed(
            &mut state,
            SpeedChange {
                fast: false,
                ultrafast: true,
                topic: "ultrafast",
                provider: ProviderId::Llmux,
            },
        );
        assert!(state.settings.fast && !state.settings.ultrafast);
    }

    #[test]
    fn a_speed_the_band_saved_before_a_model_change_this_thread_saves_later_is_still_the_files() {
        // A `/model` put aside behind a turn, then `/fast`: the band writes
        // the speed at once (`super::speed_save`), the model change is saved
        // when the turn ends with the switches it turned off -- and the
        // speed, applied after it, is written again so the file ends where
        // the commands did.
        let (home, _workspace, mut state) = recording();
        let path = state.config.user_settings_path.clone().expect("a profile");
        assert_eq!(
            super::super::speed_save::save(
                &path,
                ProviderId::Gateway,
                "acme/two",
                true,
                false,
                &state.config.sources,
                "fast",
            ),
            None
        );
        ran_model(&mut state, "acme/two", false);
        assert_eq!(settings(home.path())["fast_mode"], serde_json::json!(false));
        change_speed(
            &mut state,
            SpeedChange {
                fast: true,
                ultrafast: false,
                topic: "fast",
                provider: ProviderId::Gateway,
            },
        );
        assert_eq!(
            settings(home.path()),
            serde_json::json!({"models": {"gateway": "acme/two"}, "provider": "gateway",
                               "fast_mode": true, "ultrafast_mode": false,
                               "fast_mode_model_bound": true})
        );
    }

    #[test]
    fn ultra_is_sent_only_through_the_gateway_to_an_openai_row_that_offers_it() {
        // `resolveUltrafastProviderOptions` (`model_capabilities.zig:209-224`):
        // no silent downgrade -- a turn in Ultra that cannot be sent in it is
        // refused. Fast is sent only where the row offers it
        // (`resolveProviderOptionsForCapabilities`, `:203`).
        let (_home, _workspace, mut state) = recording();
        state.settings.ultrafast = true;
        assert_eq!(
            state.request_options(),
            Err(ULTRAFAST_UNAVAILABLE.to_string()),
            "no row in hand"
        );
        state
            .selector
            .set_catalog_for_test(CatalogState::Loaded(speed_catalog()));
        state.use_model("openai/gpt-6-astra");
        let options = state.request_options().expect("offered");
        assert!(options.ultrafast && !options.fast);
        state.use_model("anthropic/claude-opus-5");
        assert_eq!(
            state.request_options(),
            Err(ULTRAFAST_UNAVAILABLE.to_string())
        );
        state.settings.ultrafast = false;
        state.settings.fast = true;
        assert!(state.request_options().expect("no Ultra").fast);
        state.use_model("fake/model-a");
        assert!(
            !state.request_options().expect("no Ultra").fast,
            "not offered"
        );
        // Through llmux, no Ultra at all: the daemon reads no speed.
        state.use_model("openai/gpt-6-astra");
        state.settings.ultrafast = true;
        state.config.provider = ProviderId::Llmux;
        assert_eq!(
            state.request_options(),
            Err(ULTRAFAST_UNAVAILABLE.to_string())
        );
    }

    #[test]
    fn a_change_chosen_under_another_provider_is_saved_for_it_and_lands_on_no_selector() {
        // Put aside while `/provider` switched: upstream refuses the switch
        // while work runs, so its end state is the change made for the old
        // provider and then the switch. The new provider's model is untouched.
        let (home, _workspace, mut state) = recording();
        let before = state.model().to_string();
        let high = crate::provider::effort::Effort::Named("high".to_string());
        let said = ran_change(
            &mut state,
            ModelChange {
                name: "fable".to_string(),
                effort: Some(high.clone()),
                speed: None,
                picker: true,
                announced: true,
                next_turn: true,
                provider: ProviderId::Llmux,
                effort_only: false,
            },
        );
        assert_eq!(
            state.model(),
            before,
            "it landed on this provider's selector"
        );
        assert_eq!(settings(home.path())["models"]["llmux"], "fable");
        assert_eq!(state.settings.effort, high, "the effort is the session's");
        assert!(
            matches!(&said[0], UiEvent::ModelAnswered { model, outcome: ModelAnswer::Unchanged, .. }
                if *model == before),
            "{said:?}"
        );
        assert!(
            matches!(&said[1], UiEvent::Notice(notice) if notice.tone == super::super::notice::Tone::Warning),
            "the band said `Switched to fable` and is owed why the footer did not: {said:?}"
        );
        // One the band never resolved cannot be resolved against a catalog
        // that has gone: dropped, with a word.
        let said = ran_change(
            &mut state,
            ModelChange {
                name: "fab".to_string(),
                effort: None,
                speed: None,
                picker: false,
                announced: false,
                next_turn: true,
                provider: ProviderId::Llmux,
                effort_only: false,
            },
        );
        assert_eq!(state.model(), before);
        assert!(
            matches!(&said[1], UiEvent::Notice(notice) if notice.tone == super::super::notice::Tone::Warning),
            "{said:?}"
        );
        assert_eq!(settings(home.path())["models"]["llmux"], "fable");
    }

    #[test]
    fn a_resume_puts_back_the_effort_and_the_speed_the_session_recorded() {
        // `restoreRuntimePreferences` (`app_session_runtime.zig:6126-6175`).
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let config = config(home.path(), workspace.path());
        let chosen = TurnSettings {
            effort: crate::provider::effort::Effort::Named("max".to_string()),
            fast: true,
            ultrafast: false,
        };
        let saved = {
            let store = open_store(&config).expect("a store");
            let mut conversation = open_conversation(
                &store,
                &config,
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
                .commit(SessionEvent::PreferencesChanged {
                    model: None,
                    permission_mode: None,
                    provider: None,
                    effort: Some("max".to_string()),
                    fast_mode: Some(true),
                    ultrafast_mode: Some(false),
                });
            conversation.recorder.id().clone()
        };
        // At launch, through the resumption.
        let resumed =
            Resumption::open(&config, &crate::session::Selector::Id(saved.clone())).expect("opens");
        assert_eq!(resumed.settings(), Some(&chosen));
        drop(resumed);
        // From the picker, into a runtime in other settings.
        let store = open_store(&config).expect("a store");
        let mut state = Runtime::new(config, store, a_prompter(), a_questioner());
        assert_eq!(state.settings, TurnSettings::default());
        let replay = state
            .resume_by_id(saved.as_str(), &CancelToken::new())
            .expect("resumed");
        assert_eq!(state.settings, chosen);
        assert_eq!(replay.settings, Some(chosen));
        // The runtime was in Normal: the session's Fast is not the configured
        // selection, so it comes back unbound (`restoredFastModeModelBound`).
        assert!(!replay.keeps_fast_binding);
    }

    #[test]
    fn a_restored_fast_binding_needs_the_provider_the_model_and_fast_the_session_ran_under() {
        // `restoredFastModeModelBound` (`app_session_runtime.zig:6318-6328`):
        // the provider's authority, the model and `fast_mode`.
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let config = config(home.path(), workspace.path());
        let store = open_store(&config).expect("a store");
        let recorded_fast = |provider: Option<ProviderId>| {
            let session = store
                .create(
                    SessionId::generate(),
                    crate::session::NewSession {
                        origin_workspace_root: config.workspace_root.clone(),
                        workspace_root: config.workspace_root.clone(),
                        model: config.model.clone(),
                        permission_mode: config.permission_mode,
                        provider,
                    },
                )
                .expect("a session");
            let mut recorder = crate::session::SessionRecorder::new(store.clone(), session);
            recorder.commit(SessionEvent::PreferencesChanged {
                model: None,
                permission_mode: None,
                provider: None,
                effort: Some("auto".to_string()),
                fast_mode: Some(true),
                ultrafast_mode: Some(false),
            });
            recorder.id().clone()
        };
        let opened = |id: &SessionId| {
            Resumption::open(&config, &crate::session::Selector::Id(id.clone())).expect("opens")
        };
        let model = config.model.as_str();

        let gateway = opened(&recorded_fast(Some(ProviderId::Gateway)));
        assert!(gateway.keeps_fast_binding(ProviderId::Gateway, model, true));
        assert!(!gateway.keeps_fast_binding(ProviderId::Llmux, model, true));
        assert!(!gateway.keeps_fast_binding(ProviderId::Gateway, "other/model", true));
        assert!(!gateway.keeps_fast_binding(ProviderId::Gateway, model, false));
        drop(gateway);

        // A session opened through the runtime's own path names its provider.
        let through_open = open_conversation(
            &store,
            &config,
            &config.model,
            PermissionSession::new(config.permission_mode),
            None,
            &CancelToken::new(),
        )
        .expect("open a conversation");
        assert_eq!(
            through_open.recorder.state().provider.as_deref(),
            Some(config.provider.label())
        );
        drop(through_open);

        // A log written before the provider was recorded: decided by the
        // model and Fast, as before -- and its first turn after a resume
        // writes down which provider it goes on under (the resume itself
        // writes nothing, [`record_resumed_selection`]).
        let older = recorded_fast(None);
        let resumed = opened(&older);
        assert!(resumed.keeps_fast_binding(ProviderId::Llmux, model, true));
        assert!(!resumed.keeps_fast_binding(ProviderId::Llmux, model, false));
        let mut state = Runtime::new(
            config.clone(),
            open_store(&config).expect("a store"),
            a_prompter(),
            a_questioner(),
        );
        state.resume(resumed, &CancelToken::new());
        let recorded = |state: &Runtime| {
            state
                .conversation
                .as_ref()
                .expect("bound")
                .recorder
                .state()
                .provider
                .clone()
        };
        assert_eq!(recorded(&state), None);
        record_resumed_selection(
            state.conversation.as_mut().expect("bound"),
            &mut state.unrecorded_resume,
            &config.model,
            config.permission_mode,
            config.provider,
        );
        assert_eq!(recorded(&state).as_deref(), Some(config.provider.label()));
    }

    /// A session saved in `home`, last on `provider` (a label, as the log
    /// holds it: `None` for a log written before the provider was recorded)
    /// in `model`.
    fn saved_on(config: &RuntimeConfig, provider: Option<&str>, model: &str) -> SessionId {
        let store = open_store(config).expect("a store");
        let session = store
            .create(
                SessionId::generate(),
                crate::session::NewSession {
                    origin_workspace_root: config.workspace_root.clone(),
                    workspace_root: config.workspace_root.clone(),
                    model: model.to_string(),
                    permission_mode: config.permission_mode,
                    provider: None,
                },
            )
            .expect("a session");
        let mut recorder = crate::session::SessionRecorder::new(store, session);
        recorder.commit(SessionEvent::PreferencesChanged {
            model: None,
            permission_mode: None,
            provider: provider.map(str::to_string),
            effort: None,
            fast_mode: None,
            ultrafast_mode: None,
        });
        assert_eq!(recorder.failure(), None);
        recorder.id().clone()
    }

    #[test]
    fn a_resume_continues_on_the_provider_the_session_recorded_and_in_its_model() {
        // Upstream puts back the selection the session recorded, provider and
        // model together (`restoreRuntimePreferences`,
        // `app_session_runtime.zig:6147`), and only a process override
        // outranks the model (`:6148-6150`). Red before: every case below
        // continued on the configured provider, the Gateway.
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let mut config = config(home.path(), workspace.path());
        // A profile on the Gateway that chose the Gateway's model.
        config.model = "zai/glm-5.2".to_string();
        config.sources.model = crate::config::SettingSource::UserGlobal;
        let opened = |config: &RuntimeConfig, id: SessionId| {
            Resumption::open(config, &crate::session::Selector::Id(id)).expect("opens")
        };

        // On llmux in its own model: the profile's model is the Gateway's,
        // and the daemon is not asked for it.
        let on_llmux = saved_on(&config, Some("llmux"), "m-1");
        let resumed = opened(&config, on_llmux.clone());
        assert_eq!(resumed.provider(), Some(ProviderId::Llmux));
        assert_eq!(
            resumed.selection(&config, &config.model),
            (ProviderId::Llmux, "m-1".to_string())
        );
        drop(resumed);
        // Unless the process names one (`XFX_MODEL`).
        let mut pinned = config.clone();
        pinned.model = "pinned/model".to_string();
        pinned.sources.model = crate::config::SettingSource::ProcessOverride;
        assert_eq!(
            opened(&pinned, on_llmux).selection(&pinned, &pinned.model),
            (ProviderId::Llmux, "pinned/model".to_string())
        );

        // On the configured provider the same rule: the session's model,
        // whatever the profile chose, and the process's over it. Red before:
        // the profile's `zai/glm-5.2` outranked the session's model.
        let on_gateway = saved_on(&config, Some("gateway"), "anthropic/claude-opus-5");
        assert_eq!(
            opened(&config, on_gateway.clone()).selection(&config, &config.model),
            (ProviderId::Gateway, "anthropic/claude-opus-5".to_string())
        );
        let mut unchosen = config.clone();
        unchosen.sources.model = crate::config::SettingSource::CompiledDefault;
        assert_eq!(
            opened(&unchosen, on_gateway.clone()).selection(&unchosen, &unchosen.model),
            (ProviderId::Gateway, "anthropic/claude-opus-5".to_string())
        );
        let mut pinned = config.clone();
        pinned.model = "pinned/model".to_string();
        pinned.sources.model = crate::config::SettingSource::ProcessOverride;
        assert_eq!(
            opened(&pinned, on_gateway).selection(&pinned, &pinned.model),
            (ProviderId::Gateway, "pinned/model".to_string())
        );

        // A log that recorded no provider, or one this build does not have
        // (upstream refuses to bind an unknown one, `model_provider.zig:53-55`;
        // xfx goes on with the configured provider, as for an older log).
        for recorded in [None, Some("codex")] {
            let id = saved_on(&config, recorded, "m-1");
            let resumed = opened(&config, id);
            assert_eq!(resumed.provider(), None, "{recorded:?}");
            assert_eq!(
                resumed.selection(&config, &config.model),
                (ProviderId::Gateway, "zai/glm-5.2".to_string()),
                "{recorded:?}"
            );
        }
    }

    #[test]
    fn a_resume_never_restores_a_model_with_a_control_character_in_it() {
        // The launch hands the restored model straight to the band -- the hint
        // row and `/settings` -- with no channel between them, so a model the
        // log holds is file input that nothing else makes inert. A model xfx
        // would not have written (`provider::model::model_id_problem`) is not
        // restored: the launch's own model is used instead, on both of the
        // rule's branches and whichever event recorded it. Red before: the
        // log's model came back with its escapes.
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let mut config = config(home.path(), workspace.path());
        config.model = "zai/glm-5.2".to_string();
        config.sources.model = crate::config::SettingSource::UserGlobal;
        let mut unchosen = config.clone();
        unchosen.sources.model = crate::config::SettingSource::CompiledDefault;
        let opened = |config: &RuntimeConfig, id: SessionId| {
            Resumption::open(config, &crate::session::Selector::Id(id)).expect("opens")
        };
        // The same model, recorded by a later `PreferencesChanged` over a
        // clean start.
        let changed_to = |provider: &str, model: &str| {
            let id = saved_on(&config, Some(provider), "m-1");
            let store = open_store(&config).expect("a store");
            let resumed = store
                .resume(
                    &crate::session::Selector::Id(id.clone()),
                    &config.workspace_root,
                )
                .expect("resumes");
            let mut recorder = crate::session::SessionRecorder::new(store, resumed.session);
            recorder.commit(SessionEvent::PreferencesChanged {
                model: Some(model.to_string()),
                permission_mode: None,
                provider: None,
                effort: None,
                fast_mode: None,
                ultrafast_mode: None,
            });
            assert_eq!(recorder.failure(), None);
            id
        };
        for hostile in [
            HOSTILE,
            "m\u{1b}]8;;mailto:a@b\u{1b}\\x\u{1b}]8;;\u{1b}\\",
            "m\u{9b}31m",
        ] {
            for (written_by, on_llmux, on_gateway) in [
                (
                    "SessionStarted",
                    saved_on(&config, Some("llmux"), hostile),
                    saved_on(&config, Some("gateway"), hostile),
                ),
                (
                    "PreferencesChanged",
                    changed_to("llmux", hostile),
                    changed_to("gateway", hostile),
                ),
            ] {
                let context = format!("{written_by} {hostile:?}");
                // A session that recorded a provider -- here another one than
                // the configured -- continues in the session's model unless
                // the process named one (the first branch).
                let resumed = opened(&config, on_llmux);
                assert_eq!(resumed.model(), hostile, "{context}: the log holds it");
                assert!(resumed.replaces_model(&config), "{context}");
                let (provider, model) = resumed.selection(&config, &config.model);
                assert!(!model.chars().any(char::is_control), "{context}: {model:?}");
                assert_eq!(
                    (provider, model.as_str()),
                    (ProviderId::Llmux, "zai/glm-5.2"),
                    "{context}"
                );
                drop(resumed);
                // The first branch again, on the configured provider and with
                // no model chosen; the opening below is checked on it.
                let resumed = opened(&unchosen, on_gateway.clone());
                assert!(resumed.replaces_model(&unchosen), "{context}");
                let (provider, model) = resumed.selection(&unchosen, &unchosen.model);
                assert!(!model.chars().any(char::is_control), "{context}: {model:?}");
                assert_eq!(
                    (provider, model.as_str()),
                    (ProviderId::Gateway, unchosen.model.as_str()),
                    "{context}"
                );
                // Opening it writes nothing ([`Runtime::resume`]): the log
                // still holds the hostile model, byte for byte.
                let mut launched = unchosen.clone();
                (launched.provider, launched.model) = (provider, model);
                let store = open_store(&launched).expect("a store");
                let events = store
                    .sessions_dir()
                    .join(on_gateway.as_str())
                    .join(crate::session::store::EVENTS_FILE);
                let before = std::fs::read(&events).expect("the log");
                let mut state = Runtime::new(launched, store, a_prompter(), a_questioner());
                state.resume(resumed, &CancelToken::new());
                assert_eq!(
                    std::fs::read(&events).expect("the log"),
                    before,
                    "{context}: the open wrote to the log"
                );
                let recorded = |state: &Runtime| {
                    let conversation = state.conversation.as_ref().expect("bound");
                    conversation.recorder.state().model.clone()
                };
                assert_eq!(recorded(&state), hostile, "{context}");
                // The first turn's step writes the model it runs under into
                // the log ([`record_resumed_selection`]) ...
                let model = state.model().to_string();
                let (mode, provider) = (state.config.permission_mode, state.config.provider);
                record_resumed_selection(
                    state.conversation.as_mut().expect("bound"),
                    &mut state.unrecorded_resume,
                    &model,
                    mode,
                    provider,
                );
                assert_eq!(recorded(&state), unchosen.model, "{context}");
                drop(state);
                // ... so the next resume reads a clean one.
                let again = opened(&unchosen, on_gateway);
                assert_eq!(again.model(), unchosen.model, "{context}");
                assert_eq!(
                    again.selection(&unchosen, &unchosen.model),
                    (ProviderId::Gateway, unchosen.model.clone()),
                    "{context}"
                );
                assert!(!again.replaces_model(&unchosen), "{context}");
            }
            // A session that recorded no provider -- written before the
            // provider was recorded -- goes on with the configured one, in
            // the session's model when no setting chose one (the second
            // branch): replaced there too.
            let legacy = saved_on(&config, None, hostile);
            let resumed = opened(&unchosen, legacy.clone());
            assert_eq!(resumed.provider(), None, "{hostile:?}");
            assert!(resumed.replaces_model(&unchosen), "{hostile:?}");
            assert_eq!(
                resumed.selection(&unchosen, &unchosen.model),
                (ProviderId::Gateway, unchosen.model.clone()),
                "{hostile:?}"
            );
            drop(resumed);
            // And where a setting chose the model, the session's was never
            // going to be restored: nothing is replaced, and nothing is said.
            let resumed = opened(&config, legacy);
            assert!(!resumed.replaces_model(&config), "{hostile:?}");
            assert_eq!(
                resumed.selection(&config, &config.model),
                (ProviderId::Gateway, config.model.clone()),
                "{hostile:?}"
            );
        }
    }

    #[test]
    fn a_picker_resume_moves_the_runtime_to_the_recorded_provider_and_says_what_it_lacks() {
        // `/resume` onto a session last on llmux, from a runtime on the
        // Gateway: the runtime goes on llmux (`hydrateResumedSession` ->
        // `restoreRuntimePreferences`, `app_session_runtime.zig:2404`,
        // `:6147`), its catalog that provider's, and the band is told where
        // and -- with no daemon url configured -- what it cannot authenticate
        // with (`restoreSessionCredential`'s `.missing`,
        // `app_auth_runtime.zig:172-182`). The log gains no provider change:
        // it already says llmux.
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let config = config(home.path(), workspace.path());
        let saved = saved_on(&config, Some("llmux"), "m-1");
        let mut state = Runtime::new(
            config.clone(),
            open_store(&config).expect("a store"),
            a_prompter(),
            a_questioner(),
        );
        let replay = state
            .resume_by_id(saved.as_str(), &CancelToken::new())
            .expect("resumed");
        assert_eq!(state.config.provider, ProviderId::Llmux);
        assert_eq!(state.selector.provider(), ProviderId::Llmux);
        assert_eq!(state.model(), "m-1");
        assert_eq!(replay.model.as_deref(), Some("m-1"));
        assert_eq!(replay.provider, Some((ProviderId::Llmux, true)));
        assert_eq!(replay.unauthenticated, Some(ProviderId::Llmux));
        let recorded = state.conversation.as_ref().expect("bound").recorder.state();
        assert_eq!(recorded.provider.as_deref(), Some("llmux"));
        assert_eq!(recorded.model, "m-1");
        // The next turn is refused with the words the band was told.
        assert_eq!(
            state.ready(&CancelToken::new()).err().as_deref(),
            Some(crate::llmux::MISSING_URL_HELP)
        );
    }

    #[test]
    fn a_resume_says_what_the_provider_in_force_lacks_whichever_it_is() {
        // Upstream says it on every resume, the provider the session recorded
        // or the one in force (`restoreSessionCredential`,
        // `app_auth_runtime.zig:151-182`).
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let mut config = config(home.path(), workspace.path());
        assert_eq!(config.provider, ProviderId::Gateway);
        assert_eq!(unauthenticated(&config), Some(ProviderId::Gateway));
        config.provider = ProviderId::Llmux;
        assert_eq!(unauthenticated(&config), Some(ProviderId::Llmux));
        config.llmux_url = Some("http://127.0.0.1:9".to_string());
        assert_eq!(unauthenticated(&config), None);
    }

    /// Runs one tool call through the conversation's own tools, as a turn's
    /// step would.
    fn ran(state: &Runtime, name: &str, input: serde_json::Value) -> crate::tools::ToolResult {
        let conversation = state.conversation.as_ref().expect("a conversation");
        crate::tools::Registry::builtin()
            .execute(
                &ToolCall {
                    id: format!("call-{name}"),
                    name: name.to_string(),
                    input,
                },
                &conversation.tools,
            )
            .expect("an advertised tool")
    }

    /// `name` as the conversation's tools locate it.
    fn in_workspace(state: &Runtime, name: &str) -> std::path::PathBuf {
        state
            .conversation
            .as_ref()
            .expect("a conversation")
            .tools
            .scope()
            .primary()
            .join(name)
    }

    /// The body of the next event, which has to be `/undo`'s answer, neutral.
    fn undo_said(events: &mut Receiver<UiEvent>) -> String {
        match events.try_recv() {
            Ok(UiEvent::Undone(notice)) => {
                assert_eq!(notice.topic, "undo");
                assert_eq!(notice.tone, super::super::notice::Tone::Neutral);
                notice.body
            }
            other => panic!("not an undo's answer: {other:?}"),
        }
    }

    #[test]
    fn an_undo_takes_back_the_conversations_newest_change_and_says_so() {
        let (_home, _workspace, state) = recording();
        let (events_tx, mut events_rx) = mpsc::channel(bridge::UI_EVENTS);
        let path = in_workspace(&state, "a.txt");
        let made = ran(
            &state,
            "write_file",
            serde_json::json!({"path": "a.txt", "content": "one\n"}),
        );
        assert!(made.ok, "{}", made.output);

        on_a_runtime(run_undo(&state, &events_tx, Mark::LATEST, false));
        assert_eq!(
            undo_said(&mut events_rx),
            format!("Deleted {} (was newly created)", path.display())
        );
        assert!(!path.exists(), "the created file is still there");
        on_a_runtime(run_undo(&state, &events_tx, Mark::LATEST, false));
        assert_eq!(undo_said(&mut events_rx), "Nothing to undo.");
    }

    #[test]
    fn a_session_that_has_asked_nothing_has_nothing_to_undo() {
        let (_home, _workspace, state) = a_runtime_that_can_open_one();
        assert!(state.conversation.is_none());
        let (events_tx, mut events_rx) = mpsc::channel(bridge::UI_EVENTS);
        on_a_runtime(run_undo(&state, &events_tx, Mark::LATEST, false));
        assert_eq!(undo_said(&mut events_rx), "Nothing to undo.");
    }

    #[test]
    fn a_reset_or_a_resume_leaves_the_changes_before_it_behind() {
        // Upstream clears its tracker on the transition `/new`, `/clear`,
        // `/reset` and `/resume` share (`app_session_runtime.zig:1922`, `:1999`),
        // and its own resume test is answered `Nothing to undo.` with the files
        // as they were (`tests/e2e/tui-resume.test.ts:6029-6037`).
        let (_home, _workspace, mut state) = recording();
        let (events_tx, mut events_rx) = mpsc::channel(bridge::UI_EVENTS);
        let path = in_workspace(&state, "a.txt");
        assert!(
            ran(
                &state,
                "write_file",
                serde_json::json!({"path": "a.txt", "content": "made\n"})
            )
            .ok
        );
        let id = state
            .conversation
            .as_ref()
            .expect("a conversation")
            .recorder
            .id()
            .to_string();

        on_a_runtime(drop_conversation(&mut state, &events_tx, 0));
        assert!(matches!(
            events_rx.try_recv(),
            Ok(UiEvent::Fresh { through: 0 })
        ));
        on_a_runtime(run_undo(&state, &events_tx, Mark::LATEST, false));
        assert_eq!(undo_said(&mut events_rx), "Nothing to undo.");

        state
            .resume_by_id(&id, &CancelToken::new())
            .expect("the session resumes");
        on_a_runtime(run_undo(&state, &events_tx, Mark::LATEST, false));
        assert_eq!(undo_said(&mut events_rx), "Nothing to undo.");
        assert_eq!(
            std::fs::read_to_string(&path).expect("still there"),
            "made\n"
        );
    }

    #[test]
    fn an_undo_while_work_runs_waits_for_it_and_one_after_a_reset_is_the_fresh_ones() {
        // Put aside, never a stop. Ahead of a reset it is about the
        // conversation that reset drops; behind one it is about the
        // conversation the reset starts.
        let typed = Mark::now();
        let undo = TurnControl::Undo { through: typed };
        for (messages, after_reset) in [
            (vec![undo.clone()], false),
            (vec![undo.clone(), TurnControl::Reset { through: 0 }], false),
            (vec![TurnControl::Reset { through: 0 }, undo.clone()], true),
        ] {
            let (control_tx, control_rx) = mpsc::unbounded_channel();
            let control = ControlChannel::new(control_rx);
            let resets = messages
                .iter()
                .filter(|message| matches!(message, TurnControl::Reset { .. }))
                .count();
            for message in messages {
                control_tx.send(message).expect("alive");
            }
            let mut stops = Vec::new();
            let _ = on_a_runtime(raced_against_control(
                async {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                },
                &control,
                &Arc::new(CatalogAsks::AfterWork),
                |through| stops.push(through),
            ));
            assert_eq!(stops.len(), resets, "an undo stopped the work");
            // The keystroke's position, carried as it came: the work that
            // runs on does not move it.
            assert_eq!(
                control.take_deferred(),
                vec![Deferred::Undo {
                    through: typed,
                    after_reset
                }]
            );
        }
    }

    #[test]
    fn an_undo_put_aside_is_done_once_the_work_is_over_even_when_the_session_is() {
        let (_home, _workspace, mut state) = recording();
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
        let path = in_workspace(&state, "a.txt");
        assert!(
            ran(
                &state,
                "write_file",
                serde_json::json!({"path": "a.txt", "content": "made\n"})
            )
            .ok
        );
        // One typed behind a reset: answered for the fresh conversation, and
        // the change it would have reached is left alone.
        control.defer_undo(Mark::now(), true);
        // One typed before it, and a `/quit` after both: still done.
        control.defer_undo(Mark::now(), false);
        let ended = on_a_runtime(apply_deferred(
            &mut state,
            Ended::Session,
            &events_tx,
            &cancel,
            &control,
            &mut queue,
        ));
        assert_eq!(ended, Ended::Session);
        assert_eq!(undo_said(&mut events_rx), "Nothing to undo.");
        assert_eq!(
            undo_said(&mut events_rx),
            format!("Deleted {} (was newly created)", path.display())
        );
        assert!(!path.exists());
    }

    #[test]
    fn an_undo_put_aside_takes_back_what_was_newest_when_it_was_typed() {
        // fx undoes at the keystroke; this runtime does it once the work is
        // over. The work committed a second change after the `/undo` was
        // typed: that one stays, and the one that was newest at the keystroke
        // is taken back.
        let (_home, _workspace, mut state) = recording();
        let (events_tx, mut events_rx) = mpsc::channel(bridge::UI_EVENTS);
        let (control_tx, control_rx) = mpsc::unbounded_channel();
        let control = ControlChannel::new(control_rx);
        let cancel = Cancellation::new(CancelToken::new());
        let (_work_tx, mut work_rx) = mpsc::channel::<TurnWork>(WORK_LIMIT);
        let outstanding = AtomicUsize::new(0);
        let mut queue = Queue {
            work: &mut work_rx,
            outstanding: &outstanding,
            taken: 0,
        };
        let first = in_workspace(&state, "a.txt");
        let second = in_workspace(&state, "b.txt");
        assert!(
            ran(
                &state,
                "write_file",
                serde_json::json!({"path": "a.txt", "content": "one\n"})
            )
            .ok
        );
        // The keystroke, as the band sends it, read while the work runs.
        control_tx
            .send(TurnControl::Undo {
                through: Mark::now(),
            })
            .expect("alive");
        let (_, ended) = on_a_runtime(raced_against_control(
            async {
                tokio::time::sleep(Duration::from_millis(20)).await;
            },
            &control,
            &Arc::new(CatalogAsks::AfterWork),
            |_| {},
        ));
        // The same work goes on to change another file.
        assert!(
            ran(
                &state,
                "write_file",
                serde_json::json!({"path": "b.txt", "content": "two\n"})
            )
            .ok
        );

        on_a_runtime(apply_deferred(
            &mut state, ended, &events_tx, &cancel, &control, &mut queue,
        ));
        assert_eq!(
            undo_said(&mut events_rx),
            format!("Deleted {} (was newly created)", first.display())
        );
        assert!(!first.exists(), "the change typed at was not taken back");
        assert_eq!(
            std::fs::read_to_string(&second).expect("the later change stays"),
            "two\n"
        );
        // And it is the next `/undo`'s.
        on_a_runtime(run_undo(&state, &events_tx, Mark::now(), false));
        assert_eq!(
            undo_said(&mut events_rx),
            format!("Deleted {} (was newly created)", second.display())
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
                settings: TurnSettings {
                    effort: crate::provider::effort::Effort::Auto,
                    ..TurnSettings::default()
                },
                outcome: ModelAnswer::Applied { unverified: None },
                next_turn: false,
                announced: false,
            }],
            "a write nothing outranks says nothing of its own"
        );
        assert_eq!(
            settings(home.path()),
            serde_json::json!({"permission_mode": "ask",
                               "models": {"llmux": "fable", "gateway": "acme/one"},
                               "provider": "gateway",
                               "fast_mode": false, "ultrafast_mode": false,
                               "fast_mode_model_bound": true})
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
                settings: TurnSettings {
                    effort: crate::provider::effort::Effort::Auto,
                    ..TurnSettings::default()
                },
                outcome: ModelAnswer::Unchanged,
                next_turn: true,
                announced: false,
            }]
        );
        assert_eq!(
            settings(home.path()),
            serde_json::json!({"models": {"gateway": "acme/one"}, "provider": "gateway",
                               "fast_mode": false, "ultrafast_mode": false,
                               "fast_mode_model_bound": true})
        );
    }

    #[test]
    fn a_speed_change_saves_what_upstream_saves_beside_it_under_a_model_override() {
        // `applyFastMode` (`session_commands.zig:781-790`): the provider, the
        // model in force -- here the one `XFX_MODEL` pinned, as upstream writes
        // `provider_runtime.model(app)` -- and the switches with the binding;
        // and the commit report keeps the shadowing suffix
        // (`appendShadowedUserSource`, `:225-240`).
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let env = Environment::new(
            Some(home.path().to_path_buf()),
            BTreeMap::from([("XFX_MODEL".to_string(), "pinned/model".to_string())]),
        );
        let config = RuntimeConfig::load_with(&env, workspace.path()).expect("load");
        let store = open_store(&config).expect("a store");
        let mut state = Runtime::with_environment(config, env, store, a_prompter(), a_questioner());
        change_speed(
            &mut state,
            SpeedChange {
                fast: true,
                ultrafast: false,
                topic: "fast",
                provider: ProviderId::Gateway,
            },
        );
        assert_eq!(
            settings(home.path()),
            serde_json::json!({"models": {"gateway": "pinned/model"}, "provider": "gateway",
                               "fast_mode": true, "ultrafast_mode": false,
                               "fast_mode_model_bound": true})
        );
        // What the band's write of the same change says, above its `* fast:
        // on` (`super::speed_save`).
        assert_eq!(
            state.save_speed("fast"),
            Some(super::super::notice::Notice::neutral(
                "fast",
                "saved to user settings (scope=user); fresh sessions here use \
                 higher-precedence model=process_override"
            ))
        );
    }

    #[test]
    fn a_save_names_every_field_this_workspaces_entry_still_decides_in_upstreams_order() {
        // `appendShadowedUserSources` (`session_commands.zig:170-186`): one
        // header, then `model`, `effort`, `fast_mode`, `ultrafast_mode` -- each
        // the save wrote and a layer above the profile still decides.
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let root = workspace.path().canonicalize().expect("a workspace");
        crate::provider::profile::write_preference(
            &home.path().join(".xfx").join("settings.json"),
            &["workspaces"],
            serde_json::json!({root.to_string_lossy(): {
                "effort": "high", "fast_mode": true, "ultrafast_mode": false
            }}),
        )
        .expect("seed");
        let env = Environment::new(Some(home.path().to_path_buf()), BTreeMap::new());
        let config = RuntimeConfig::load_with(&env, workspace.path()).expect("load");
        let store = open_store(&config).expect("a store");
        let state = Runtime::with_environment(config, env, store, a_prompter(), a_questioner());
        let shadow = "saved to user settings (scope=user); fresh sessions here use \
                      higher-precedence ";

        assert_eq!(
            state.save_speed("fast"),
            Some(super::super::notice::Notice::neutral(
                "fast",
                format!("{shadow}fast_mode=user_workspace, ultrafast_mode=user_workspace")
            ))
        );
        let high = crate::provider::effort::Effort::parse("high").expect("an effort");
        assert_eq!(
            state.save_change(ProviderId::Gateway, "fake/model-a", Some(&high), false),
            Some(super::super::notice::Notice::neutral(
                "model",
                format!(
                    "{shadow}effort=user_workspace, fast_mode=user_workspace, \
                     ultrafast_mode=user_workspace"
                )
            ))
        );
        // No effort written, none named.
        assert_eq!(
            state.save_change(ProviderId::Gateway, "fake/model-a", None, true),
            Some(super::super::notice::Notice::neutral(
                "model picker",
                format!("{shadow}fast_mode=user_workspace, ultrafast_mode=user_workspace")
            ))
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
    fn a_catalog_request_answers_the_menu_and_never_the_document() {
        // Each of the three answers the menu has, for the one request that
        // asked ([`TurnControl::Catalog`]); none of them is a notice, a failed
        // turn, or the end of any work -- a request is not work.
        let (_home, _workspace, mut state) = recording();
        let asked = |state: &mut Runtime| {
            let (events_tx, mut events_rx) = mpsc::channel(bridge::UI_EVENTS);
            on_a_runtime(async {
                ask_catalog(state, &events_tx).await;
                // A read it started tells its answer itself; taken here, so
                // the case reads what it told.
                state.selector.settle_load().await;
            });
            let mut said = Vec::new();
            while let Ok(event) = events_rx.try_recv() {
                said.push(event);
            }
            said
        };
        let provider = state.config.provider;
        // No catalog at all: an empty list.
        assert_eq!(
            asked(&mut state),
            [UiEvent::CatalogLoaded {
                provider,
                entries: Vec::new()
            }]
        );
        // Unread: the menu's failure -- and whether asking again could help
        // -- and no failed turn.
        let reads = Arc::new(AtomicUsize::new(0));
        state
            .selector
            .set_fetcher_for_test(Box::new(CountedUnreachable {
                asked: Arc::clone(&reads),
            }));
        let first = asked(&mut state);
        let [UiEvent::CatalogUnread {
            provider: told,
            retry: Some(crate::provider::model::CatalogRetry::Unreachable),
            ..
        }] = &first[..]
        else {
            panic!("{first:?}");
        };
        assert_eq!(*told, provider);
        assert_eq!(reads.load(Ordering::SeqCst), 1);
        // Asked again inside the second after that read began: told as it
        // stands -- the same attempt -- and nothing read (`beginLoad`,
        // `model_cache_runtime.zig:441-442`).
        assert_eq!(asked(&mut state), first);
        assert_eq!(reads.load(Ordering::SeqCst), 1, "read inside the floor");
        // After it, read again: the `retry /model.` the menu said.
        state.selector.backdate_attempt_for_test();
        let again = asked(&mut state);
        assert_eq!(reads.load(Ordering::SeqCst), 2, "not read after the floor");
        assert!(
            matches!(&again[..], [UiEvent::CatalogUnread { .. }]) && again != first,
            "{again:?}"
        );
        // Rows: in the order the menu lists them, told as they stand.
        state
            .selector
            .set_catalog_for_test(CatalogState::Loaded(vec![
                published("zeta/one"),
                published("alpha/two"),
            ]));
        let said = asked(&mut state);
        let [UiEvent::CatalogLoaded { entries, .. }] = &said[..] else {
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
    fn the_authority_decides_with_the_profiles_table_and_with_what_the_ui_puts_in_force() {
        use crate::permission::{ConfiguredRule, RuleAction, RuleSet};
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        crate::provider::profile::write_document(
            &home.path().join(".xfx").join("settings.json"),
            br#"{"permission":{"bash":{"git *":"allow"}}}"#,
        )
        .expect("an owner-only profile");
        let config = config(home.path(), workspace.path());
        let store = open_store(&config).expect("a store");
        let mut state = Runtime::new(config, store, a_prompter(), a_questioner());
        // At launch: the table the profile holds.
        assert_eq!(
            state.authority().rules().rules(),
            &[ConfiguredRule::new("bash", "git *", RuleAction::Allow)]
        );
        // The handle the UI writes is the one every authority reads, so a
        // `/allowlist` change reaches a conversation already open.
        let (handle, _work, _control, _changes) = WorkHandle::detached_with_changes();
        state.rules = handle.rules().clone();
        let authority = state.authority();
        handle
            .rules()
            .replace(RuleSet::new(vec![ConfiguredRule::new(
                "edit",
                "*",
                RuleAction::Deny,
            )]));
        assert_eq!(
            authority.rules().rules(),
            &[ConfiguredRule::new("edit", "*", RuleAction::Deny)]
        );
    }

    #[test]
    fn the_ui_is_given_every_row_so_the_band_and_the_selector_resolve_alike() {
        // It used to be cut at `MAX_RENDERED_MODELS`, and then a band
        // resolving `/model <query>` over 100 rows could announce a model the
        // selector -- over all of them -- would not have chosen. Every row
        // crosses now, in the menu's order; the catalog document's own 1 MiB
        // cap is what bounds it.
        use crate::provider::model::MAX_RENDERED_MODELS;
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
        let shown = in_menu_order(&many);
        assert_eq!(shown.len(), MAX_RENDERED_MODELS + 50);
        assert_eq!(shown[0].id, "model-0", "the provider's own order is kept");
        // The 150th row is reachable by the rule both sides use.
        let last = format!("model-{}", MAX_RENDERED_MODELS + 49);
        assert_eq!(
            crate::provider::model::resolve_in_order(&shown, &last),
            last
        );
        assert_eq!(in_menu_order(&[]).len(), 0);
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
    fn a_provider_switch_drops_the_old_bundle_and_keeps_the_conversation() {
        // Step (g). The bundle goes: it is a connection to an endpoint that is
        // no longer configured, and `ready()` builds whatever it does not
        // have, so dropping it is exactly what makes the next prompt select
        // the new one. **Changed claim**: the conversation stays, as
        // upstream's session does across `/provider` (`app_auth_runtime.zig:
        // 1377-1381`), with the switch written into its log, provider and
        // model both (`app_session_runtime.zig:3539-3550`), and its clock
        // where it was.
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

        let id = runtime
            .conversation
            .as_ref()
            .expect("opened")
            .recorder
            .id()
            .clone();
        runtime.began_ms = 1_234;
        // In Fast, and the log says so: the switch turns it off with the
        // model it replaces, and a resume reads the log's.
        runtime.settings.fast = true;
        runtime.record_settings();

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
        let kept = runtime
            .conversation
            .as_ref()
            .expect("the switch dropped the conversation");
        assert_eq!(kept.recorder.id(), &id, "the switch opened another session");
        assert_eq!(kept.recorder.state().provider.as_deref(), Some("llmux"));
        assert_eq!(kept.recorder.state().model, "fable");
        assert_eq!(runtime.began_ms, 1_234, "the switch restarted the clock");
        assert!(!runtime.settings.fast);
        assert_eq!(
            kept.recorder
                .state()
                .turn_settings
                .as_ref()
                .map(|settings| settings.fast),
            Some(false),
            "the log kept Fast on across the switch, for a resume to restore"
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
    fn status_names_the_session_the_first_prompt_opens_before_the_store_holds_it() {
        // Decided for xfx (`docs/parity.md`, `/status`): the id is allocated
        // at launch, in memory, and shown `not saved yet`; the first prompt's
        // session takes it, and the board says `saved` as soon as it is on
        // disk -- before that turn has a turn in it -- as upstream's
        // `open-first` capture shows. A reset shows the next session's id.
        let (home, _workspace, state) = a_runtime_that_can_open_one();
        let board = StatusBoard::unpublished();
        let mut state = state.publishing_to(board.clone());
        let Some(ShellSession::Open { id, saved: false }) = board.read().session else {
            panic!("no session to name at launch: {:?}", board.read().session);
        };
        assert_eq!(
            sessions_in(home.path()),
            Vec::<String>::new(),
            "naming it wrote it"
        );

        state.ready(&CancelToken::new()).expect("a conversation");
        let opened = state.conversation.as_ref().expect("open").recorder.id();
        assert_eq!(opened.as_str(), id, "the first prompt opened another id");
        assert_eq!(sessions_in(home.path()), vec![id.clone()]);
        let published = board.read();
        assert_eq!(
            published.session,
            Some(ShellSession::Open {
                id: id.clone(),
                saved: true
            }),
            "the open was not published before the turn"
        );
        assert_eq!(published.history_turns, 0);

        state.conversation = None;
        state.publish();
        let Some(ShellSession::Open { id: next, saved }) = board.read().session else {
            panic!("no session to name after a reset");
        };
        assert!(!saved);
        assert_ne!(next, id, "the next session would reuse the last one's id");
    }

    #[test]
    fn a_failure_before_the_store_is_asked_keeps_the_id_status_names() {
        // Review P-status-b item 1: a workspace that cannot be scoped fails
        // the open before the store is asked anything. The id `/status` names
        // must still be the one the next attempt opens.
        let (_home, workspace, state) = a_runtime_that_can_open_one();
        let board = StatusBoard::unpublished();
        let mut state = state.publishing_to(board.clone());
        let Some(ShellSession::Open {
            id: named,
            saved: false,
        }) = board.read().session
        else {
            panic!("no session to name at launch: {:?}", board.read().session);
        };
        let moved = workspace.path().with_extension("moved");
        std::fs::rename(workspace.path(), &moved).expect("move the workspace away");
        let failed = state.ready(&CancelToken::new());
        let scope_says = crate::interactive::conversation_scope(&state.config)
            .map(|_| ())
            .expect_err("a workspace that is not there was scoped");
        std::fs::rename(&moved, workspace.path()).expect("and back");
        // The failure is the scope's, in `open_next`, past the provider:
        // `ready` selected the bundle first, and the error is the scope's own.
        assert!(
            state.provider.is_some(),
            "the attempt failed in Bundle::select, before the open"
        );
        assert!(state.conversation.is_none());
        assert_eq!(
            failed,
            Err(scope_says),
            "the attempt failed somewhere other than conversation_scope"
        );
        assert_eq!(
            board.read().session,
            Some(ShellSession::Open {
                id: named.clone(),
                saved: false
            }),
            "a failure the store never saw changed what /status says"
        );

        state.ready(&CancelToken::new()).expect("a conversation");
        let opened = state.conversation.as_ref().expect("open").recorder.id();
        assert_eq!(
            opened.as_str(),
            named,
            "the next attempt opened an id /status never named"
        );
    }

    #[test]
    fn the_session_spawn_names_at_launch_is_the_one_the_runtime_opens() {
        // `spawn` seeds the board with the id it hands the runtime: the
        // runtime's first publication names the same session, and the one a
        // `/rename` before any prompt opens is it.
        //
        // **The seed is read before the runtime can publish, every run.** The
        // runtime thread's first statement takes `RUNTIME_MASKS`; held across
        // `spawn`, it keeps that thread from building anything, so the board
        // read under it is the seed's -- which its trace facts confirm: the
        // seed carries none, every publication of the runtime's carries the
        // settings files.
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let gate = RUNTIME_MASKS.lock().unwrap_or_else(PoisonError::into_inner);
        let spawned = spawn(
            config(home.path(), workspace.path()),
            Cancellation::new(CancelToken::new()),
            None,
        );
        let seed = spawned
            .as_ref()
            .map(|(worker, _)| worker.handle().trace_facts())
            .map_err(ToString::to_string);
        drop(gate);
        let (mut worker, mut events) = spawned.expect("a runtime");
        let (seed, seed_trace) = seed.expect("a runtime");
        assert_eq!(
            seed_trace.settings,
            crate::trace::Settings::default(),
            "the runtime published before the seed was read"
        );
        let Some(ShellSession::Open {
            id: seeded,
            saved: false,
        }) = seed.session
        else {
            panic!("the seed names no session: {:?}", seed.session);
        };

        let deadline = Instant::now() + TEST_DRAIN;
        let first = loop {
            let (published, trace) = worker.handle().trace_facts();
            if trace.settings != crate::trace::Settings::default() {
                break published;
            }
            assert!(Instant::now() < deadline, "the runtime never published");
            std::thread::sleep(Duration::from_millis(1));
        };
        assert_eq!(
            first.session,
            Some(ShellSession::Open {
                id: seeded.clone(),
                saved: false
            }),
            "the runtime's first publication named another session than the seed"
        );

        worker.control(TurnControl::Rename {
            title: "named before a prompt".to_string(),
            announced: true,
        });
        let opened = loop {
            let found = sessions_in(home.path());
            if !found.is_empty() {
                break found;
            }
            assert!(Instant::now() < deadline, "the rename opened no session");
            std::thread::sleep(Duration::from_millis(2));
        };
        worker.shutdown(&mut events, Instant::now() + TEST_DRAIN, |_| {});
        assert_eq!(
            opened,
            vec![seeded],
            "the runtime opened another id than the seed"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_store_that_refuses_the_session_is_a_shell_that_cannot_save_until_one_opens() {
        use std::os::unix::fs::PermissionsExt;
        // A `sessions` directory the store opened but cannot create in: the
        // first prompt's create fails, and `/status` says so
        // (`none · this shell can't save sessions`, `output_contracts.zig:734`).
        if rustix::process::geteuid().is_root() {
            return;
        }
        let (home, _workspace, state) = a_runtime_that_can_open_one();
        let board = StatusBoard::unpublished();
        let mut state = state.publishing_to(board.clone());
        let sessions = home.path().join(".xfx").join("sessions");
        let Some(ShellSession::Open { id: refused, .. }) = board.read().session else {
            panic!("no session to name at launch");
        };
        std::fs::set_permissions(&sessions, std::fs::Permissions::from_mode(0o500))
            .expect("read-only sessions");
        let failed = state.ready(&CancelToken::new());
        std::fs::set_permissions(&sessions, std::fs::Permissions::from_mode(0o700))
            .expect("writable again");
        assert!(failed.is_err(), "the store took a session it cannot write");
        assert_eq!(board.read().session, Some(ShellSession::Unavailable));

        // Writable again: the next prompt opens one, under a fresh id.
        state.ready(&CancelToken::new()).expect("a conversation");
        let Some(ShellSession::Open { id, saved: true }) = board.read().session else {
            panic!("the open was not published: {:?}", board.read().session);
        };
        assert_ne!(id, refused, "a refused id was asked for again");
        assert_eq!(sessions_in(home.path()), vec![id]);
    }

    /// The roots a conversation's next turn runs in, besides the workspace.
    fn turn_roots(state: &mut Runtime) -> Vec<std::path::PathBuf> {
        state.ready(&CancelToken::new()).expect("a conversation");
        state
            .conversation
            .as_ref()
            .expect("open")
            .tools
            .scope()
            .additional_roots()
            .to_vec()
    }

    #[test]
    fn a_workspace_change_is_saved_and_in_force_from_the_next_turn() {
        let (home, workspace, mut state) = a_runtime_that_can_open_one();
        let extra = tempfile::tempdir().expect("extra");
        let extra_path = extra.path().canonicalize().expect("canonical");
        assert!(turn_roots(&mut state).is_empty());

        let UiEvent::Workspace { notice, access } = state.change_workspace(
            &workspace_access::Action::Add(extra.path().to_string_lossy().into_owned()),
        ) else {
            panic!("not a workspace answer");
        };
        assert_eq!(notice.tone, super::super::notice::Tone::Neutral);
        assert_eq!(notice.topic, "workspace");
        assert!(
            notice
                .body
                .contains("saved_changed=true runtime_changed=true"),
            "{}",
            notice.body
        );
        assert_eq!(
            access.expect("in force").saved(),
            [extra_path.to_string_lossy().into_owned()]
        );
        // The open conversation's next turn, and the profile a launch reads.
        assert_eq!(turn_roots(&mut state), std::slice::from_ref(&extra_path));
        let reloaded = RuntimeConfig::load_with(
            &Environment::new(Some(home.path().to_path_buf()), BTreeMap::new()),
            workspace.path(),
        )
        .expect("load");
        assert_eq!(
            reloaded.additional_directories,
            [extra_path.to_string_lossy().into_owned()]
        );

        let UiEvent::Workspace { notice, access } =
            state.change_workspace(&workspace_access::Action::Clear)
        else {
            panic!("not a workspace answer");
        };
        assert!(
            notice.body.ends_with("additional directories: (none)"),
            "{}",
            notice.body
        );
        assert!(access.expect("in force").entries().is_empty());
        assert!(turn_roots(&mut state).is_empty());
    }

    #[test]
    fn a_refused_workspace_change_changes_nothing() {
        let (_home, _workspace, mut state) = a_runtime_that_can_open_one();
        let UiEvent::Workspace { notice, access } = state.change_workspace(
            &workspace_access::Action::Add("../no-such-directory".to_string()),
        ) else {
            panic!("not a workspace answer");
        };
        assert_eq!(notice.tone, super::super::notice::Tone::Error);
        assert_eq!(
            notice.body,
            "Workspace update rejected: directory does not exist"
        );
        assert!(access.is_none());
        assert!(state.config.additional_directories.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn a_saved_directory_swapped_for_a_link_is_out_of_the_next_turn() {
        // Resolved once, when it was added, and never again: the turn after
        // the swap does not run in the link's target.
        let (_home, _workspace, mut state) = a_runtime_that_can_open_one();
        let base = tempfile::tempdir().expect("base");
        let base = base.path().canonicalize().expect("canonical");
        let extra = base.join("extra");
        let elsewhere = base.join("elsewhere");
        std::fs::create_dir(&extra).expect("extra");
        std::fs::create_dir(&elsewhere).expect("elsewhere");
        state.change_workspace(&workspace_access::Action::Add(
            extra.to_string_lossy().into_owned(),
        ));
        assert_eq!(turn_roots(&mut state), std::slice::from_ref(&extra));

        std::fs::remove_dir(&extra).expect("remove");
        std::os::unix::fs::symlink(&elsewhere, &extra).expect("swap");
        assert!(turn_roots(&mut state).is_empty());
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
    fn a_conversation_kept_across_a_provider_switch_or_rebuilt_after_new_can_still_ask() {
        // `adopt` keeps the conversation, tools and requester with it, and
        // `ready()` builds whatever it does not have -- after `/new`, which
        // drops the field. A requester attached only at the first `ready`
        // would leave such a session unable to ask, and the failure would
        // look like a model that stopped using the tool.
        let (_home, _workspace, mut state) = a_runtime_that_can_open_one();
        state.ready(&CancelToken::new()).expect("a conversation");

        let mut after = state.config.clone();
        after.provider = ProviderId::Llmux;
        after.model = "fable".to_string();
        after.llmux_url = Some("http://127.0.0.1:3456".to_string());
        state.adopt(after);
        state.ready(&CancelToken::new()).expect("a conversation");
        assert!(
            state
                .conversation
                .as_ref()
                .expect("kept")
                .tools
                .questioner()
                .is_some(),
            "the conversation kept across the switch has nobody to ask"
        );

        state.conversation = None;
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
            billing: None,
            service_tier: None,
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
            billing: None,
            service_tier: None,
        }
    }

    #[test]
    fn a_turn_in_ultra_says_once_when_a_response_was_not_served_at_it() {
        // `orchestrator.zig:8015-8022`: checked per response, said once per
        // turn, and only in Ultra; a confirmed tier says nothing.
        let served = |tier| Completion {
            service_tier: tier,
            ..answers("x")
        };
        let said = |ultrafast: bool, steps: Vec<Completion>| {
            let (events_tx, mut events_rx) = mpsc::channel(bridge::UI_EVENTS);
            let provider = ScriptedProvider::new(steps.clone());
            let watched = ServedUltra::new(
                &provider,
                ultrafast,
                events_tx,
                tokio_util::sync::CancellationToken::new(),
            );
            let request = crate::gateway::protocol::CompletionRequest {
                model: "openai/gpt-6-astra".to_string(),
                messages: vec![crate::gateway::protocol::Message::user("go")],
                tools: Vec::new(),
                tool_choice: crate::gateway::protocol::ToolChoice::Auto,
            };
            struct Quiet;
            impl crate::gateway::DeltaSink for Quiet {
                fn text_delta(&mut self, _text: &str) -> io::Result<()> {
                    Ok(())
                }
            }
            for _ in &steps {
                on_a_runtime(crate::gateway::Provider::stream(
                    &watched, &request, &mut Quiet,
                ))
                .expect("answered");
            }
            let mut said = Vec::new();
            while let Ok(event) = events_rx.try_recv() {
                said.push(event);
            }
            said
        };
        use crate::gateway::protocol::ServiceTier;
        assert_eq!(
            said(
                true,
                vec![served(None), served(Some(ServiceTier::Priority))]
            ),
            [UiEvent::Operational(ULTRAFAST_UNCONFIRMED.to_string())],
            "once per turn"
        );
        assert!(said(true, vec![served(Some(ServiceTier::Ultrafast))]).is_empty());
        assert!(said(false, vec![served(None)]).is_empty(), "not in Ultra");
        assert!(ULTRAFAST_UNCONFIRMED.starts_with(
            "Ultrafast was requested, but Gateway did not confirm it was served; this response \
             may have used a standard or lower tier."
        ));
    }

    #[test]
    fn fast_with_no_catalog_to_offer_it_is_dropped_and_said_only_when_the_catalog_is_down() {
        // `orchestrator.zig:7253-7262`: a catalog that could not be read is
        // the case worth a word; one that was read and does not offer Fast for
        // the model drops it quietly.
        let (_home, _workspace, mut state) = recording();
        state.settings.fast = true;
        state
            .selector
            .set_catalog_for_test(CatalogState::Failed("nothing answered".to_string()));
        assert!(!state.request_options().expect("no Ultra").fast);
        assert!(fast_unavailable(&state));
        state
            .selector
            .set_catalog_for_test(CatalogState::Loaded(vec![published("other/model")]));
        assert!(!fast_unavailable(&state));
        assert_eq!(
            FAST_UNAVAILABLE,
            "Fast mode is unavailable for this model right now; continuing at standard speed.\n"
        );
    }

    fn call(id: &str, name: &str, input: serde_json::Value) -> ToolCall {
        ToolCall {
            id: id.to_string(),
            name: name.to_string(),
            input,
        }
    }

    /// A provider that, before answering its `at`-th request, makes the
    /// change the person would make mid-turn -- through the UI's own call,
    /// on the handle that shares the runtime's live authority.
    ///
    /// The hook stands where the keystroke lands in production: after the
    /// turn's first tool call has been judged and run, before its second one
    /// is. Made from inside the turn rather than from another thread so the
    /// case is about *what the second call is judged by*, not about a race.
    struct ChangesMidTurn<'a> {
        script: ScriptedProvider,
        handle: &'a WorkHandle,
        at: usize,
        change: Option<Change>,
    }

    #[async_trait::async_trait(?Send)]
    impl crate::gateway::Provider for ChangesMidTurn<'_> {
        async fn stream(
            &self,
            request: &crate::gateway::protocol::CompletionRequest,
            deltas: &mut dyn crate::gateway::DeltaSink,
        ) -> Result<Completion, crate::gateway::ProviderError> {
            if self.script.requests() == self.at {
                if let Some(change) = self.change {
                    self.handle
                        .change_permissions(change)
                        .expect("the runtime is listening");
                }
            }
            self.script.stream(request, deltas).await
        }
    }

    /// One turn of two steps, each writing one file, under a session opened
    /// the way `ready` opens one -- `Runtime::authority` -- in `mode`, with the
    /// band answering "no" to anything it is asked; when `granted`, the
    /// session already holds an "always" for both writes; `change`, if any,
    /// made between the two steps. Answers which files exist afterwards,
    /// whether the band was asked, and the mode in force at the end.
    fn two_writes(
        mode: PermissionMode,
        granted: bool,
        change: Option<Change>,
    ) -> (bool, bool, bool, PermissionMode) {
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let mut config = config(home.path(), workspace.path());
        config.permission_mode = mode;
        let store = open_store(&config).expect("a store");
        let (events_tx, mut events_rx) = mpsc::channel(64);
        let (control_tx, control_rx) = mpsc::unbounded_channel();
        // The band's "no", waiting for the question the second call asks.
        control_tx
            .send(TurnControl::Answer {
                id: super::super::approval_readiness::ApprovalId(1),
                answer: ApprovalAnswer::Deny,
                feedback: None,
            })
            .expect("the channel is open");
        let cancel = Cancellation::new(CancelToken::new());
        let prompter = TuiPrompter::new(
            events_tx.clone(),
            ControlChannel::new(control_rx),
            cancel.clone(),
        );
        let mut state = Runtime::new(config, store, prompter, a_questioner());
        // The UI's handle and the runtime share one live authority, as
        // `spawn` makes them.
        let (handle, _work, _control, _changes) = WorkHandle::detached_with_changes();
        handle.live().set_mode(mode);
        state.live = handle.live().clone();
        let scope = crate::workspace::AccessScope::primary_only(workspace.path()).expect("a scope");
        let mut authority = state.authority();
        if granted {
            // Keyed as a grant is: the write's canonical absolute target.
            let root = workspace.path().canonicalize().expect("a workspace");
            for name in ["first.txt", "second.txt"] {
                authority.grant(Grant::new(
                    "write_file",
                    root.join(name).to_string_lossy().into_owned(),
                ));
            }
        }
        let tools = crate::tools::ToolContext::new(scope).with_permissions(authority);

        let provider = ChangesMidTurn {
            script: ScriptedProvider::new(vec![
                calls(vec![call(
                    "call-1",
                    "write_file",
                    serde_json::json!({"path": "first.txt", "content": "one\n"}),
                )]),
                calls(vec![call(
                    "call-2",
                    "write_file",
                    serde_json::json!({"path": "second.txt", "content": "two\n"}),
                )]),
                answers("done"),
            ]),
            handle: &handle,
            at: 1,
            change,
        };
        let turn = cancel.turn();
        let request = TurnRequest {
            images: Vec::new(),
            model: "vendor/model".to_string(),
            prompt: "write two files".to_string(),
            history: Vec::new(),
            max_steps: 4,
            max_attempts: 1,
            cancel: turn.mirror.clone(),
            tools,
        };
        let mut sink = UiEventSink::new(events_tx.clone(), turn.token.clone());
        let outcome = on_a_runtime(crate::agent::run_turn(request, &provider, &mut sink));
        assert!(outcome.is_ok(), "{outcome:?}");
        assert_eq!(
            provider.script.requests(),
            3,
            "the turn did not run to its end"
        );
        let mut asked = false;
        while let Ok(event) = events_rx.try_recv() {
            asked |= matches!(event, UiEvent::Approval(_));
        }
        (
            workspace.path().join("first.txt").exists(),
            workspace.path().join("second.txt").exists(),
            asked,
            handle.live().mode(),
        )
    }

    #[test]
    fn a_switch_to_ask_mid_turn_gates_that_turns_next_tool_call() {
        // Upstream's `Shift+Tab tightening to ask gates a later tool call in
        // the active turn` (`yolo-permission-mode.test.ts:452-530`): the
        // person who switches to `ask` to stop a runaway turn is protected
        // from the very next call, not from the next turn.
        let (first, second, asked, mode) = two_writes(
            PermissionMode::Auto,
            false,
            Some(Change::Mode(PermissionMode::Ask)),
        );
        assert!(
            first,
            "the call before the switch was judged by the new mode"
        );
        assert!(asked, "the call after the switch was not put to the band");
        assert!(
            !second,
            "the call after the switch to ask ran without anybody saying yes"
        );
        assert_eq!(mode, PermissionMode::Ask);

        // The control: with no switch, `auto` writes both -- so the case above
        // is about the switch and not about `auto` refusing the second write.
        let (first, second, asked, mode) = two_writes(PermissionMode::Auto, false, None);
        assert!(first && second, "auto did not admit both writes");
        assert!(!asked, "auto asked the band");
        assert_eq!(mode, PermissionMode::Auto);
    }

    #[test]
    fn a_reset_mid_turn_takes_the_grants_away_from_that_turns_next_call() {
        // A reset is `ask` and no grants, at once: in an `ask` session that
        // had already been told "always" for both writes, the second is asked
        // about again -- a grant outranks the mode, so only the grants going
        // can make it ask.
        let (first, second, asked, mode) =
            two_writes(PermissionMode::Ask, true, Some(Change::Reset));
        assert!(first, "the grant before the reset was not honoured");
        assert!(asked, "the call after the reset was not put to the band");
        assert!(
            !second,
            "a grant the reset took away still allowed the call"
        );
        assert_eq!(mode, PermissionMode::Ask);

        // The control: the same session with no reset writes both unasked.
        let (first, second, asked, _) = two_writes(PermissionMode::Ask, true, None);
        assert!(first && second, "the grants did not admit both writes");
        assert!(!asked);
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
                images: Vec::new(),
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
                    &Arc::new(CatalogAsks::AfterWork),
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
                images: Vec::new(),
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
                &Arc::new(CatalogAsks::AfterWork),
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

    // ---- session titles (`super::titles`) ----

    /// A runtime on the Gateway with a key and nothing asked in it yet, its
    /// desk publishing where to ask and answered by `answer` from the excerpt.
    fn titled_runtime(
        answer: impl Fn(&str) -> super::super::titles::Outcome + Send + Sync + 'static,
    ) -> (tempfile::TempDir, tempfile::TempDir, Runtime) {
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let config = RuntimeConfig::load_with(
            &Environment::new(
                Some(home.path().to_path_buf()),
                BTreeMap::from([("AI_GATEWAY_API_KEY".to_string(), "key".to_string())]),
            ),
            workspace.path(),
        )
        .expect("load a configuration");
        assert_eq!(config.provider, ProviderId::Gateway);
        let store = open_store(&config).expect("a store");
        let mut state = Runtime::new(config, store, a_prompter(), a_questioner());
        let (desk, answers) = super::super::titles::TitleDesk::answering(true, answer);
        state.titles = desk;
        state.title_answers = Some(answers);
        state.publish();
        (home, workspace, state)
    }

    fn generated(title: &str) -> super::super::titles::Outcome {
        super::super::titles::Outcome::Generated(title.to_string())
    }

    /// The next title answer, or `None` when none comes.
    fn next_title_within(
        state: &mut Runtime,
        within: Duration,
    ) -> Option<super::super::titles::Titled> {
        on_a_runtime(async {
            tokio::time::timeout(within, next_title(&mut state.title_answers))
                .await
                .ok()
        })
    }

    /// Every `session_renamed` frame of the conversation's log, as written.
    fn rename_frames(state: &Runtime) -> Vec<String> {
        let conversation = state.conversation.as_ref().expect("a conversation");
        let log = std::fs::read_to_string(
            state
                .store
                .session_dir(conversation.recorder.id())
                .join(crate::session::EVENTS_FILE),
        )
        .expect("the log");
        log.lines()
            .filter(|line| line.contains("\"kind\":\"session_renamed\""))
            .map(|line| {
                let start = line.find("\"event\":").expect("an event");
                line[start..].to_string()
            })
            .collect()
    }

    /// A first turn that ran and ended in the conversation, so the session
    /// has a title derived from its prompt.
    fn concluded(state: &mut Runtime, prompt: &str) {
        let recorder = &mut state.conversation.as_mut().expect("open").recorder;
        recorder.commit(SessionEvent::UserMessage {
            text: prompt.to_string(),
        });
        recorder.commit(SessionEvent::TurnConcluded {
            outcome: TurnConclusion::Final {
                finish_reason: "stop".to_string(),
                steps: 1,
            },
        });
    }

    #[test]
    fn a_title_that_lands_while_a_call_has_the_thread_does_not_carry_the_turn_past_its_stop() {
        // A turn sees a Ctrl-C typed at a question it parked on only because
        // the call yields once when it is done (`crate::agent::machine`'s
        // `yield_once`), and the race above it reads the channel before it
        // polls the turn again. Red before: a title's answer that came while
        // the question had the thread was read below that race, and the turn
        // was polled straight past its yield -- the stop came after the next
        // request had already gone, and that request took the reply the
        // user's next prompt was owed.
        let (_control_tx, control_rx) = mpsc::unbounded_channel::<TurnControl>();
        let control = ControlChannel::new(control_rx);
        let (events, _seen) = mpsc::channel::<UiEvent>(bridge::UI_EVENTS);
        let (desk, mut answers) = super::super::titles::TitleDesk::new(true);
        let stopped = Arc::new(AtomicBool::new(false));
        let watched = Arc::clone(&stopped);
        let parked = Arc::clone(&control);
        let landing = desk.clone();
        let body = async move {
            // The question, while it has the thread: the Ctrl-C ends it and
            // is handed back to the loop that can stop the turn
            // (`super::super::question::TuiQuestioner`) -- and the title
            // asked at the first prompt's keystroke comes back meanwhile.
            parked.give_back(TurnControl::Cancel { through: 0 });
            landing.land(super::super::titles::Outcome::Unavailable(
                super::super::titles::Unavailable::Empty,
            ));
            // The call's one yield.
            let mut yielded = false;
            std::future::poll_fn(move |cx| {
                if yielded {
                    return std::task::Poll::Ready(());
                }
                yielded = true;
                cx.waker().wake_by_ref();
                std::task::Poll::Pending
            })
            .await;
            // What the turn's loop reads before it asks for the next step.
            watched.load(Ordering::Acquire)
        };
        let shown = desk.lineage();
        let ((was_stopped, _titled), ended) = on_a_runtime(raced_against_control(
            titled_while(body, Some(&mut answers), &desk, shown, &control, &events),
            &control,
            &Arc::new(CatalogAsks::AfterWork),
            |_through| stopped.store(true, Ordering::Release),
        ));
        assert!(
            was_stopped,
            "the turn went on to its next step with the stop still unread"
        );
        assert_eq!(ended, Ended::Turn);
    }

    #[test]
    fn a_title_that_comes_after_a_rename_the_turn_put_aside_is_dropped() {
        // Upstream writes a `/rename` at the keystroke, so a generated title
        // after it finds the user's title on record and is dropped
        // (`user_title_present`, `installGeneratedTitle`,
        // `session_title_generation.zig:378-394`). xfx's turn puts the rename
        // aside until it ends; an answer read meanwhile is dropped the same
        // way -- not shown, and not written ahead of the user's.
        let (_control_tx, control_rx) = mpsc::unbounded_channel::<TurnControl>();
        let control = ControlChannel::new(control_rx);
        let (events, mut seen) = mpsc::channel::<UiEvent>(bridge::UI_EVENTS);
        let (desk, mut answers) =
            super::super::titles::TitleDesk::answering(true, |_| generated("Generated"));
        desk.publish_asker(titled_runtime(|_| generated("x")).2.title_asker());
        assert!(desk.ask("fix the renderer"));
        control.defer_rename("Mine".to_string(), false, true);
        let landed = Arc::new(AtomicBool::new(false));
        let watched = Arc::clone(&landed);
        let probe = desk.clone();
        let body = async move {
            while probe.running() {
                tokio::task::yield_now().await;
            }
            watched.store(true, Ordering::Release);
        };
        let shown = desk.lineage();
        let (((), kept), _ended) = on_a_runtime(raced_against_control(
            titled_while(body, Some(&mut answers), &desk, shown, &control, &events),
            &control,
            &Arc::new(CatalogAsks::AfterWork),
            |_through| {},
        ));
        assert!(landed.load(Ordering::Acquire));
        assert!(kept.is_empty(), "kept for writing ahead of the rename");
        assert!(seen.try_recv().is_err(), "the band was shown it");
        let record = desk.generation();
        assert_eq!(record.status, crate::trace::TitleStatus::Dropped);
        assert_eq!(record.reason, Some("user_title_present"));
    }

    #[test]
    fn trace_names_the_session_a_failed_question_was_about() {
        // Upstream's record carries the session its task was started for,
        // whatever came back (`LastResult.copyIds`,
        // `app_session_runtime.zig:1110-1117`; `/trace` prints it as
        // `session=`, `app_commands.zig:2796-2800`).
        let (_home, _workspace, mut state) = titled_runtime(|_| {
            super::super::titles::Outcome::Unavailable(super::super::titles::Unavailable::Provider(
                500,
            ))
        });
        state.ready(&CancelToken::new()).expect("a conversation");
        assert!(state.titles.ask("fix the renderer"));
        let titled = next_title_within(&mut state, TEST_DRAIN).expect("an answer");
        assert_eq!(state.title_arrived(&titled, false), None);
        let id = state
            .conversation
            .as_ref()
            .expect("open")
            .recorder
            .id()
            .as_str()
            .to_string();
        let record = state.titles.generation();
        assert_eq!(record.status, crate::trace::TitleStatus::Failed);
        assert_eq!(record.detail.as_deref(), Some("server_error"));
        assert_eq!(record.session.as_deref(), Some(id.as_str()));
    }

    #[test]
    fn the_desk_asks_only_on_the_gateway_with_a_key() {
        // `sessionTitleModel` (`src/main.zig:471-475`): the Gateway names a
        // title model; a configured provider -- llmux's place -- names none.
        let (_home, _workspace, mut state) = titled_runtime(|_| generated("Asked"));
        assert!(state.title_asker().is_some());
        state.config.provider = ProviderId::Llmux;
        state.publish();
        assert!(!state.titles.ask("fix the renderer"), "llmux was asked");
        state.config.provider = ProviderId::Gateway;
        state.config.credential = None;
        state.publish();
        assert!(!state.titles.ask("fix the renderer"), "asked with no key");
    }

    #[test]
    fn an_answer_before_the_conversation_opens_waits_for_it_and_is_shown_at_once() {
        // The first prompt asks at the keystroke, before its turn opens the
        // session: the answer is the band's now, and the log's the moment the
        // session exists.
        let (_home, _workspace, mut state) = titled_runtime(|excerpt| {
            assert_eq!(excerpt, "fix the renderer");
            generated("Fix the renderer")
        });
        assert!(state.titles.ask("fix the renderer"));
        let titled = next_title_within(&mut state, TEST_DRAIN).expect("an answer");
        assert_eq!(
            state.title_arrived(&titled, false).as_deref(),
            Some("Fix the renderer"),
            "the band was not given the title"
        );
        assert!(state.conversation.is_none());
        state.ready(&CancelToken::new()).expect("a conversation");
        assert_eq!(
            rename_frames(&state),
            [
                r#""event":{"kind":"session_renamed","title":"Fix the renderer","source":"generated"}}"#
            ]
        );
        let recorded = state.conversation.as_ref().expect("open").recorder.state();
        assert_eq!(recorded.title().as_deref(), Some("Fix the renderer"));
        // A side call: nothing it cost is in what `/usage` reads
        // (`session_title_generation.zig:185-187`).
        assert!(recorded.usage.requests.is_empty());
        let record = state.titles.generation();
        assert_eq!(record.status, crate::trace::TitleStatus::Installed);
        assert_eq!(
            record.session.as_deref(),
            Some(
                state
                    .conversation
                    .as_ref()
                    .expect("open")
                    .recorder
                    .id()
                    .as_str()
            )
        );
    }

    #[test]
    fn a_users_title_is_kept_and_only_the_derived_one_is_replaced() {
        // `installGeneratedTitle` (`session_title_generation.zig:378-394`;
        // v2's, `session_adapter.zig:1675-1697`): the title on record may be
        // replaced when it is none, or the one the first prompt derives --
        // which upstream writes down at the first turn
        // (`session_adapter.zig:1300-1310`). Anything else is the user's.
        let (_home, _workspace, mut state) = titled_runtime(|_| generated("Generated"));
        state.ready(&CancelToken::new()).expect("a conversation");
        concluded(&mut state, "fix the renderer");
        state.rename("Mine", &CancelToken::new()).expect("renamed");
        assert!(state.titles.ask("fix the renderer"));
        let titled = next_title_within(&mut state, TEST_DRAIN).expect("an answer");
        assert_eq!(state.title_arrived(&titled, false), None);
        assert_eq!(
            rename_frames(&state),
            [r#""event":{"kind":"session_renamed","title":"Mine"}}"#]
        );
        let record = state.titles.generation();
        assert_eq!(record.status, crate::trace::TitleStatus::Dropped);
        assert_eq!(record.reason, Some("user_title_present"));

        // A title on record that is the derived one -- here the user retyped
        // it -- is replaced, as upstream replaces the derived title it wrote.
        let (_home, _workspace, mut state) = titled_runtime(|_| generated("Generated"));
        state.ready(&CancelToken::new()).expect("a conversation");
        concluded(&mut state, "fix the renderer");
        state
            .rename("fix the renderer", &CancelToken::new())
            .expect("renamed");
        assert!(state.titles.ask("fix the renderer"));
        let titled = next_title_within(&mut state, TEST_DRAIN).expect("an answer");
        assert_eq!(
            state.title_arrived(&titled, false).as_deref(),
            Some("Generated")
        );
        let recorded = state.conversation.as_ref().expect("open").recorder.state();
        assert_eq!(recorded.title().as_deref(), Some("Generated"));

        // A rename with no turn yet: upstream's derived title is not present,
        // so the rename stands (`!display.present`).
        let (_home, _workspace, mut state) = titled_runtime(|_| generated("Generated"));
        state
            .rename("Before asking", &CancelToken::new())
            .expect("renamed");
        assert!(state.titles.ask("fix the renderer"));
        let titled = next_title_within(&mut state, TEST_DRAIN).expect("an answer");
        assert_eq!(state.title_arrived(&titled, false), None);

        // And a rename after a generated title is the one in force.
        let (_home, _workspace, mut state) = titled_runtime(|_| generated("Generated"));
        state.ready(&CancelToken::new()).expect("a conversation");
        assert!(state.titles.ask("fix the renderer"));
        let titled = next_title_within(&mut state, TEST_DRAIN).expect("an answer");
        assert!(state.title_arrived(&titled, false).is_some());
        state.rename("Mine", &CancelToken::new()).expect("renamed");
        let recorded = state.conversation.as_ref().expect("open").recorder.state();
        assert_eq!(recorded.title().as_deref(), Some("Mine"));
    }

    #[test]
    fn a_title_for_a_conversation_let_go_of_names_neither_it_nor_the_next() {
        // A `/new` moves the lineage on at the keystroke; a resume moves it
        // in the runtime. The question runs on, and
        // its answer is about the conversation let go of: dropped
        // (`session_changed`, `pollSessionTitleGeneration`,
        // `app_session_runtime.zig:3720-3728`), which `/trace` says over the
        // title that came back (`recordDropped`, `:1153-1171`).
        let (_home, _workspace, mut state) = titled_runtime(|_| generated("Too late"));
        state.ready(&CancelToken::new()).expect("a conversation");
        assert!(state.titles.ask("fix the renderer"));
        // The band's `/new`, then the runtime's drop.
        state.titles.next_lineage();
        let (events_tx, _events_rx) = mpsc::channel(bridge::UI_EVENTS);
        on_a_runtime(drop_conversation(&mut state, &events_tx, 0));
        let titled = next_title_within(&mut state, TEST_DRAIN).expect("an answer");
        assert_eq!(titled.lineage, 0);
        assert_eq!(state.title_arrived(&titled, false), None);
        let record = state.titles.generation();
        assert_eq!(record.status, crate::trace::TitleStatus::Dropped);
        assert_eq!(record.reason, Some("session_changed"));
        state
            .ready(&CancelToken::new())
            .expect("the next conversation");
        assert!(rename_frames(&state).is_empty());

        // **Changed claim**: a provider switch is no longer one. It keeps
        // the conversation, as upstream's `/provider` keeps its session
        // (`app_auth_runtime.zig:1377-1381`), so the question out when it is
        // made is answered into that conversation.
        let (_home, _workspace, mut state) = titled_runtime(|_| generated("Still mine"));
        state.ready(&CancelToken::new()).expect("a conversation");
        assert!(state.titles.ask("fix the renderer"));
        let reloaded = state.config.clone();
        let _ = state.adopt(reloaded);
        let titled = next_title_within(&mut state, TEST_DRAIN).expect("an answer");
        assert_eq!(
            state.title_arrived(&titled, false).as_deref(),
            Some("Still mine")
        );
        let recorded = state.conversation.as_ref().expect("kept").recorder.state();
        assert_eq!(recorded.title().as_deref(), Some("Still mine"));

        // And one that was waiting for a conversation not yet open when the
        // switch was made goes on waiting for it, and is written when it
        // opens.
        let (_home, _workspace, mut state) = titled_runtime(|_| generated("Opened later"));
        assert!(state.titles.ask("fix the renderer"));
        let titled = next_title_within(&mut state, TEST_DRAIN).expect("an answer");
        assert_eq!(
            state.title_arrived(&titled, false).as_deref(),
            Some("Opened later"),
            "the band was not shown it"
        );
        let reloaded = state.config.clone();
        let _ = state.adopt(reloaded);
        state.ready(&CancelToken::new()).expect("a conversation");
        let recorded = state.conversation.as_ref().expect("open").recorder.state();
        assert_eq!(recorded.title().as_deref(), Some("Opened later"));
    }

    #[test]
    fn a_prompt_held_for_a_switch_asks_of_the_conversation_it_will_run_in() {
        // Upstream holds a prompt while its provider is prepared and asks
        // when it finalizes it, of the session in force then
        // (`app_auth_runtime.zig:1840-1842`; `src/main.zig:1238-1266`,
        // `:1479-1481`). A provider switch keeps the conversation, so a
        // prompt held for one made before any conversation was opened is
        // that conversation's first: asked.
        let (_home, _workspace, mut state) = titled_runtime(|_| generated("New provider"));
        state.titles.switching(1);
        assert!(state.titles.defer("fix the renderer", Some(2)));
        state.worked_through(0);
        assert!(!state.titles.running(), "asked before the switch was made");
        let reloaded = state.config.clone();
        let _ = state.adopt(reloaded);
        state.worked_through(1);
        assert!(
            state.titles.running(),
            "the first prompt after the switch asked nothing"
        );
        let titled = next_title_within(&mut state, TEST_DRAIN).expect("an answer");
        assert_eq!(titled.lineage, state.titles.lineage());

        // And one held for a switch of a conversation that has turns is not
        // its first prompt: nothing asked (`session_untitled`,
        // `app_session_runtime.zig:3674-3675`).
        let (_home, _workspace, mut state) = titled_runtime(|_| generated("Not asked"));
        state.ready(&CancelToken::new()).expect("a conversation");
        concluded(&mut state, "a turn in the conversation the switch keeps");
        state.titles.switching(1);
        assert!(state.titles.defer("fix the renderer", Some(2)));
        let reloaded = state.config.clone();
        let _ = state.adopt(reloaded);
        state.worked_through(1);
        assert!(!state.titles.running(), "a conversation with turns asked");

        // After a resume it is the resumed conversation, whose turns ask
        // nothing (`session_untitled`, `app_session_runtime.zig:3674-3675`).
        let (_home, _workspace, mut state) = titled_runtime(|_| generated("Not asked"));
        state.ready(&CancelToken::new()).expect("a conversation");
        concluded(&mut state, "the session resumed into");
        state.titles.switching(1);
        assert!(state.titles.defer("fix the renderer", Some(2)));
        state.worked_through(1);
        assert!(!state.titles.running(), "a conversation with turns asked");

        // A switch the interrupt dropped with the prompt: nothing asked.
        let (_home, _workspace, mut state) = titled_runtime(|_| generated("Not asked"));
        state.titles.switching(1);
        assert!(state.titles.defer("dropped", Some(2)));
        state.worked_through(2);
        assert!(!state.titles.running(), "a dropped prompt asked");
    }

    #[test]
    fn a_question_asked_after_a_new_is_the_next_conversations() {
        // A prompt typed straight after `/new`, before the runtime has
        // dropped the old conversation: its title is the next one's, held
        // until that one is open.
        let (_home, _workspace, mut state) = titled_runtime(|_| generated("Second session"));
        state.ready(&CancelToken::new()).expect("a conversation");
        state.titles.next_lineage();
        assert!(state.titles.ask("second session question"));
        let titled = next_title_within(&mut state, TEST_DRAIN).expect("an answer");
        // Still the old conversation open: not written into it.
        assert_eq!(
            state.title_arrived(&titled, false).as_deref(),
            Some("Second session")
        );
        assert!(rename_frames(&state).is_empty());
        // The band is told again after the fence, which sets aside everything
        // before it -- the case of an answer the old turn read.
        let (events_tx, mut events_rx) = mpsc::channel(bridge::UI_EVENTS);
        on_a_runtime(drop_conversation(&mut state, &events_tx, 0));
        let mut seen = Vec::new();
        while let Ok(event) = events_rx.try_recv() {
            seen.push(event);
        }
        assert_eq!(
            seen,
            [
                UiEvent::Fresh { through: 0 },
                UiEvent::SessionTitled("Second session".to_string())
            ]
        );
        state
            .ready(&CancelToken::new())
            .expect("the next conversation");
        assert_eq!(
            rename_frames(&state),
            [
                r#""event":{"kind":"session_renamed","title":"Second session","source":"generated"}}"#
            ]
        );
    }

    /// A provider that answers only once a title asked beside it has come
    /// back -- `answered` set by the asker -- and a moment more: the turn is
    /// provably still running when the title arrives, however loaded the
    /// machine is.
    struct AfterTheTitle {
        answer: std::cell::RefCell<Option<Completion>>,
        answered: Arc<std::sync::atomic::AtomicBool>,
    }

    #[async_trait::async_trait(?Send)]
    impl crate::gateway::Provider for AfterTheTitle {
        async fn stream(
            &self,
            _request: &crate::gateway::protocol::CompletionRequest,
            _deltas: &mut dyn crate::gateway::DeltaSink,
        ) -> Result<Completion, crate::gateway::ProviderError> {
            let deadline = Instant::now() + TEST_DRAIN;
            while !self.answered.load(Ordering::SeqCst) && Instant::now() < deadline {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(self.answer.borrow_mut().take().expect("one request"))
        }
    }

    #[test]
    fn a_title_that_comes_back_mid_turn_is_shown_then_and_written_when_the_turn_ends() {
        // Upstream installs it from its main loop whatever the worker is doing
        // (`src/main.zig:3140-3143`): the window is titled during the first
        // turn, not after it.
        let answered = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let says = Arc::clone(&answered);
        let (_home, _workspace, mut state) = titled_runtime(move |_| {
            says.store(true, Ordering::SeqCst);
            generated("Fix the renderer")
        });
        state.provider = Some(Bundle {
            id: ProviderId::Gateway,
            stream: Box::new(AfterTheTitle {
                answer: std::cell::RefCell::new(Some(answers("done"))),
                answered,
            }),
        });
        // Asked at the keystroke, before the turn takes the prompt.
        assert!(state.titles.ask("fix the renderer"));
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
            "fix the renderer".to_string(),
            (Vec::new(), Vec::new()),
            &events_tx,
            &cancel,
            &control,
            &mut queue,
        ));

        assert_eq!(ended, Ended::Turn);
        let mut seen = Vec::new();
        while let Ok(event) = events_rx.try_recv() {
            seen.push(event);
        }
        let titled = seen
            .iter()
            .position(|event| *event == UiEvent::SessionTitled("Fix the renderer".to_string()))
            .expect("the band was told the title");
        let ended_at = seen
            .iter()
            .position(|event| matches!(event, UiEvent::TurnEnded { .. }))
            .expect("the turn ended");
        assert!(titled < ended_at, "the title waited for the turn: {seen:?}");
        // Written down once the turn let go of the log, and nothing of it in
        // the turn's usage.
        let frames = rename_frames(&state);
        assert_eq!(frames.len(), 1, "{frames:?}");
        let recorded = state.conversation.as_ref().expect("open").recorder.state();
        assert_eq!(recorded.title().as_deref(), Some("Fix the renderer"));
        assert_eq!(recorded.turns.len(), 1);
        // The turn's own request, and none for the title.
        assert_eq!(recorded.usage.requests.len(), 1);
        assert!(!state.titles.running());
        assert!(
            state.title_answers.is_some(),
            "the answers were not given back"
        );
    }

    #[test]
    fn a_title_that_comes_back_between_turns_is_written_and_then_shown() {
        // The loop's own arm: the bill's way, the recorder that owns the log.
        let (home, _workspace, mut state) = titled_runtime(|_| generated("Between turns"));
        state.ready(&CancelToken::new()).expect("a conversation");
        assert!(state.titles.ask("fix the renderer"));
        let (events_tx, mut events_rx) = mpsc::channel(bridge::UI_EVENTS);
        let (control_tx, control_rx) = mpsc::unbounded_channel();
        let control = ControlChannel::new(control_rx);
        let (_work_tx, mut work_rx) = mpsc::channel::<TurnWork>(WORK_LIMIT);
        let (_changes_tx, mut changes_rx) = mpsc::unbounded_channel();
        let outstanding = AtomicUsize::new(0);
        let id = state
            .conversation
            .as_ref()
            .expect("open")
            .recorder
            .id()
            .clone();
        let log = home
            .path()
            .join(".xfx")
            .join("sessions")
            .join(id.as_str())
            .join(crate::session::EVENTS_FILE);
        let watched = log.clone();
        std::thread::scope(|scope| {
            let stopper = scope.spawn(move || {
                // Once the loop has written the title down, and not before.
                let deadline = Instant::now() + TEST_DRAIN;
                while !std::fs::read_to_string(&watched)
                    .is_ok_and(|log| log.contains("\"kind\":\"session_renamed\""))
                    && Instant::now() < deadline
                {
                    std::thread::sleep(Duration::from_millis(5));
                }
                let _ = control_tx.send(TurnControl::Shutdown);
            });
            let _ = on_a_runtime(turn_loop(
                state,
                &events_tx,
                &mut work_rx,
                &mut changes_rx,
                control,
                Cancellation::new(CancelToken::new()),
                &outstanding,
            ));
            stopper.join().expect("the stopper");
        });
        let mut seen = Vec::new();
        while let Ok(event) = events_rx.try_recv() {
            seen.push(event);
        }
        assert_eq!(
            seen,
            [UiEvent::SessionTitled("Between turns".to_string())],
            "{seen:?}"
        );
        let log = std::fs::read_to_string(&log).expect("the log");
        assert!(
            log.contains(
                r#""event":{"kind":"session_renamed","title":"Between turns","source":"generated"}}"#
            ),
            "{log}"
        );
    }
}
