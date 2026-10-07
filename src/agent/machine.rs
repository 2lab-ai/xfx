//! The bounded turn state machine.
//!
//! A turn is a loop over model steps. Each step builds one request from the
//! conversation so far, spends at most `max_attempts` transport attempts on it,
//! and then either finishes the turn or extends the conversation. A completion
//! with no tool calls ends the turn; a completion with tool calls is executed
//! locally, appended to the conversation as one assistant message plus one
//! correlated result per call, and the loop asks the model what to do next
//! (`vercel-labs/fx@580a0c5d src/core/agent/runtime/orchestrator.zig:4624-4765`).
//!
//! Three rules are structural rather than conventional:
//!
//! - **Exactly one terminal event.** [`TurnMachine::drive`] emits only
//!   `assistant_delta`; the single `match` in [`TurnMachine::run`] emits
//!   exactly one `final` or one `error`. There is no path that emits both and
//!   no path that emits neither, and a machine refuses to run twice.
//! - **No blind replay.** An attempt is replayed only when the failure proves
//!   nothing was delivered *and* nothing has yet been written to the user this
//!   step. Upstream forces the transport's retry count to one when the agent
//!   owns attempts (`vercel-labs/fx@580a0c5d src/gateway/client.zig:1169-1172`);
//!   this is the layer that then decides.
//! - **No immediate replay.** A retry that fires the instant the last one failed
//!   is not a retry, it is a second request to a server that just said it could
//!   not take one. The turn waits, preferring the server's own `Retry-After`
//!   over its own guess, and the wait is interruptible.
//! - **Exactly once per call.** Every tool call is checked -- unique id, known
//!   tool -- before any of them runs, and then each runs once and appends
//!   exactly one correlated result. A call that cannot be checked stops the turn
//!   before the first execution, so a turn never half-runs a step.
//!
//! # The shape of a request
//!
//! A prompt is assembled in one fixed order, and each slot means something
//! different (`vercel-labs/fx@580a0c5d src/core/agent/runtime/prompt_context.zig:27-43`):
//!
//! | Slot | What it is | When it changes |
//! |---|---|---|
//! | static system/project | the project's instructions as of the turn's start | never, within a turn |
//! | transient overlay | instructions for a scope a tool call has just reached | grows before a target is admitted |
//! | durable history | earlier turns, restored from a session | never, within a turn |
//! | current user message | what was asked now | never |
//! | within-turn suffix | this turn's assistant steps and tool results | after each step |
//!
//! The order is load-bearing rather than cosmetic. The static prefix is never
//! rewritten mid-turn, so a scope discovered at step three arrives as an
//! *additional* message instead of silently editing something the model has
//! already read.

use std::io;
use std::path::PathBuf;
use std::task::Poll;
use std::time::{Duration, Instant};

use crate::gateway::protocol::{
    Bill, Completion, CompletionRequest, ContentPart, Deferred, FinishReason, Message, ToolChoice,
};
use crate::gateway::{CancelToken, DeltaSink, Provider, ProviderError};
use crate::output::{Event, EventSink};
use crate::session::{RecordedToolCall, RequestUsage, SessionEvent, TurnConclusion};
use crate::tools::{LineChanges, Registry};
use crate::workspace::ProjectContext;

use super::types::{
    NoJournal, StepCheck, StepCompaction, StepCompactor, TurnError, TurnJournal, TurnOutcome,
    TurnRequest,
};

/// The first backoff, doubled per failed attempt when the server names no delay.
///
/// An xfx value. Upstream backs off linearly from 150 ms
/// (`vercel-labs/fx@580a0c5d src/gateway/client.zig:180`, `:1827-1830`); xfx
/// starts a little later and doubles, so a second failure costs the server less.
const RETRY_BACKOFF_BASE: Duration = Duration::from_millis(250);

/// The longest a turn will wait between attempts, however long the server asks.
///
/// Upstream's ceiling (`src/gateway/client.zig:182`). A server may legitimately
/// ask for minutes; a foreground command that appears to hang for minutes is
/// indistinguishable from a broken one, so the turn fails instead of obeying.
const MAX_RETRY_DELAY: Duration = Duration::from_secs(5);

/// How often a wait notices that the turn was cancelled.
const CANCEL_POLL_INTERVAL: Duration = Duration::from_millis(25);

/// Whether a turn bounded by `limit` may take the step with index `step`.
///
/// `0` is unbounded (`vercel-labs/fx@580a0c5d src/core/config/agent_steps.zig:3-31`).
/// Records one assistant step, when there is one worth recording.
///
/// The one place that decides. "Is there a step here" is three questions rather
/// than one -- visible text, tool calls, or the provider's own content blocks --
/// and the terminal path used to ask only the first. A completion that reasoned
/// and hit `max_tokens` before saying anything visible has empty text, no calls,
/// and a signed `thinking` block; gating on the text dropped it from the session
/// entirely, so a later `--resume` replayed a history missing a block Anthropic
/// requires back unchanged, and the two user turns on either side of the hole
/// merged into one.
///
/// Nothing at all is still not evidence, and is still not recorded.
fn record_assistant(journal: &mut dyn TurnJournal, completion: &Completion) {
    if completion.text.is_empty()
        && completion.tool_calls.is_empty()
        && completion.raw_content.is_empty()
    {
        return;
    }
    journal.record(SessionEvent::AssistantMessage {
        text: completion.text.clone(),
        tool_calls: completion
            .tool_calls
            .iter()
            .map(|call| RecordedToolCall {
                id: call.id.clone(),
                name: call.name.clone(),
                input: call.input.clone(),
            })
            .collect(),
        // A terminal step has no continuation to satisfy, but a resumed
        // conversation replays it too, and the authority is checked there.
        raw_content: completion.raw_content.clone(),
        // Nothing on either of today's wires produces Responses items. The field
        // is written empty rather than omitted from the code, so the day a
        // Responses decoder exists there is one line to change and no second
        // recording path to discover.
        responses_state: Vec::new(),
        wire: (!completion.raw_content.is_empty()).then(|| completion.wire.clone()),
    });
}

pub fn allows_step(limit: u32, step: u32) -> bool {
    limit == 0 || step < limit
}

/// The filesystem target a tool call names, if it names one.
///
/// Every advertised tool spells its target `path`, so this reads one field
/// rather than knowing eight schemas. A call with no `path` -- `terminal`, or a
/// workspace-wide `glob_files` -- has no scope of its own to admit, and returns
/// `None` rather than being guessed at from a command line.
///
/// The value is *not* resolved here. This is a hint used to widen the model's
/// context; the security decision about the same path happens later, inside the
/// executor, against the scope. Confusing the two would make a rules lookup a
/// permission check.
/// Whether a request that failed this way may still have been billed.
///
/// The ones that provably never reached a model -- refused before it was
/// sent, never connected, answered with a status -- were not
/// (`DeliveryOutcome.unbilled`, `vercel-labs/fx@c1db9191
/// src/core/session/session_usage.zig:43-53`). Everything else left and was
/// lost on the way back, and a model that started answering bills for it.
pub(crate) fn may_have_billed(err: &ProviderError) -> bool {
    match err {
        ProviderError::Endpoint(_)
        | ProviderError::Request { .. }
        | ProviderError::InvalidHeader { .. }
        | ProviderError::Connect { .. }
        | ProviderError::Status { .. } => false,
        ProviderError::Transport { .. }
        | ProviderError::Protocol(_)
        | ProviderError::Sink(_)
        | ProviderError::Cancelled => true,
    }
}

/// What one completed request cost, as `/usage` keeps it: the provider's own
/// bill when it gave one ([`Completion::billing`]); otherwise the request as
/// asked of `model`, its token totals when they were said, and **no cost** --
/// a request whose spend nobody reported, which `/usage` counts as missing
/// rather than as free. One whose generation was named is pending rather than
/// missing: it keeps the id and where to ask, and the runtime asks
/// (upstream's `.deferred`, `gateway.zig:650-660`). A turn's steps and a
/// compaction's notes requests are kept alike (`crate::agent::compact`).
pub(crate) fn request_usage(completion: &Completion, model: &str, took: Duration) -> RequestUsage {
    let duration_ms = u64::try_from(took.as_millis()).unwrap_or(u64::MAX);
    let unbilled = |generation: Option<&Deferred>| RequestUsage {
        at_ms: crate::session::event::system_now_ms(),
        duration_ms,
        model: model.to_string(),
        input_tokens: completion.usage.input_tokens.unwrap_or(0),
        output_tokens: completion.usage.output_tokens.unwrap_or(0),
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        reasoning_tokens: None,
        cost: None,
        generation_id: generation.map(|deferred| deferred.generation_id.clone()),
        origin: generation.map(|deferred| deferred.origin.clone()),
    };
    match &completion.billing {
        Some(Bill::Deferred(deferred)) => unbilled(Some(deferred)),
        None => unbilled(None),
        Some(Bill::Exact(bill)) => RequestUsage {
            at_ms: bill.at_ms,
            duration_ms,
            model: bill.model.clone(),
            input_tokens: bill.input_tokens,
            output_tokens: bill.output_tokens,
            cache_read_tokens: bill.cache_read_tokens,
            cache_write_tokens: bill.cache_write_tokens,
            reasoning_tokens: bill.reasoning_tokens,
            cost: Some(bill.cost),
            generation_id: bill.generation_id.clone(),
            origin: None,
        },
    }
}

fn target_path(value: Option<&serde_json::Value>) -> Option<PathBuf> {
    let raw = value?.as_str()?.trim();
    (!raw.is_empty()).then(|| PathBuf::from(raw))
}

/// How long to wait before replaying `attempt`, which has just failed.
///
/// `attempt` is 1-based. A server-supplied delay always wins over the local
/// guess -- it is the only party that knows when it will be ready -- but it is
/// still capped, so a hostile or mistaken `Retry-After` cannot stall the turn.
pub fn retry_delay(attempt: u32, retry_after: Option<Duration>) -> Duration {
    if let Some(requested) = retry_after {
        return requested.min(MAX_RETRY_DELAY);
    }
    let doubling = attempt.saturating_sub(1).min(u32::BITS - 1);
    RETRY_BACKOFF_BASE
        .checked_mul(1u32 << doubling)
        .unwrap_or(MAX_RETRY_DELAY)
        .min(MAX_RETRY_DELAY)
}

/// Hands control back to whatever is driving this turn, exactly once.
///
/// **Not `tokio::task::yield_now`**, and the difference is the point: this turn
/// is driven from four places -- a current-thread runtime on the TUI's worker
/// thread, `xfx ask`, the line shell, and unit tests that poll it by hand --
/// and a yield that reached for a runtime context would be a yield that
/// depended on which of them was driving. All this needs is one `Pending` with
/// the waker already woken, which is a fact about futures rather than about
/// tokio.
async fn yield_once() {
    let mut yielded = false;
    std::future::poll_fn(move |cx| {
        if yielded {
            return Poll::Ready(());
        }
        yielded = true;
        // Woken before returning `Pending`, so nothing depends on another task
        // waking this one: the driver is free to poll whatever else it holds --
        // the control channel it is racing this turn against -- and then come
        // straight back.
        cx.waker().wake_by_ref();
        Poll::Pending
    })
    .await;
}

/// Waits `delay`, or stops early when the turn is cancelled.
///
/// The token is a flag rather than a channel, so the wait is sliced instead of
/// awaited once. A user who presses Ctrl-C during a backoff should not have to
/// wait out a delay that exists for the server's benefit.
async fn wait_before_retry(delay: Duration, cancel: &CancelToken) -> Result<(), TurnError> {
    let deadline = Instant::now() + delay;
    loop {
        if cancel.is_cancelled() {
            return Err(TurnError::Cancelled);
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Ok(());
        }
        tokio::time::sleep(remaining.min(CANCEL_POLL_INTERVAL)).await;
    }
}

/// Runs one turn to a terminal state, emitting exactly one terminal event.
///
/// Nothing is recorded: this is the entry point for a turn that carries no
/// project context and no session. [`run_turn_saved`] is the one `ask` uses.
pub async fn run_turn(
    request: TurnRequest,
    provider: &dyn Provider,
    events: &mut dyn EventSink,
) -> Result<TurnOutcome, TurnError> {
    TurnMachine::new(request).run(provider, events).await
}

/// What a flush of one step's amendments does when the stream refuses one.
///
/// The two callers want opposite things from the same failure, and the reason
/// is what each of them still has to lose.
///
/// A turn that is otherwise fine is about to ask for another completion, so a
/// reader who cannot be told what the user said is a turn that should stop --
/// and stopping before the journal keeps "recorded" meaning "delivered".
///
/// A turn that is already ending has no second request and no reader left to
/// protect: the log is the only copy of the sentence a resumed session can
/// read, so a broken stream must cost the echo and nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SinkRefusal {
    /// Report it at once. Nothing after the refusal is journaled or queued.
    Stop,
    /// Go on journaling the rest of the batch, and hand the failure back at
    /// the end for the caller to weigh against its own.
    JournalOn,
}

/// Runs one turn with project context and a durable journal.
pub async fn run_turn_saved(
    request: TurnRequest,
    context: ProjectContext,
    provider: &dyn Provider,
    events: &mut dyn EventSink,
    journal: &mut dyn TurnJournal,
) -> Result<TurnOutcome, TurnError> {
    TurnMachine::new(request)
        .with_context(context)
        .run_journaled(provider, events, journal)
        .await
}

/// One turn's conversation and bounds.
#[derive(Debug)]
pub struct TurnMachine {
    request: TurnRequest,
    /// The project's instructions, and the machinery to widen them when a tool
    /// call reaches a scope that has its own.
    context: ProjectContext,
    /// The static system message: the project context as of the turn's start.
    /// Empty when there is none.
    static_context: String,
    /// Instructions admitted mid-turn, in the order they were admitted. Each is
    /// its own message, so nothing already delivered is rewritten.
    overlay: Vec<String>,
    /// Earlier turns, restored from a session. Never changes within a turn.
    history: Vec<Message>,
    /// What was asked now.
    user: Message,
    /// This turn's own assistant steps and tool results.
    suffix: Vec<Message>,
    /// Where in [`Self::suffix`] the request starts: past the steps a
    /// compaction in this turn took into its compacted form.
    suffix_start: usize,
    /// A compaction ran in this turn.
    compacted_in_turn: bool,
    /// Every assistant fragment this turn delivered, concatenated. A turn that
    /// spans several steps answers with all of them, because that is what the
    /// user watched arrive.
    output: String,
    /// The text a step delivered before it failed, when one did.
    ///
    /// Recorded as the turn's last assistant step when the turn ends on that
    /// failure ([`Self::run_journaled`]), because it is what the user read and
    /// what upstream keeps: the next request carries the partial answer, closed
    /// by the sentence that says it ended early (`session.zig:3057-3069`;
    /// `crate::session::store`'s `close_interrupted`).
    partial: String,
    steps: u32,
    finalized: bool,
    /// Every model request this turn completed, as `/usage` keeps it
    /// ([`RequestUsage`]), in the order they were made.
    requests: Vec<RequestUsage>,
    /// A request was sent and then lost -- a broken stream, a stop -- so it
    /// may have been billed with nothing to say what it cost.
    unaccounted: bool,
    /// Lines this turn's file changes added and removed.
    lines: LineChanges,
    /// Asked before each step whether to compact first; `None` for a turn
    /// that never compacts.
    compactor: Option<Compactor>,
}

/// The finish reason a turn a compaction ended is concluded with: upstream's
/// word for its presentation (`TurnPresentationOutcome.failed`,
/// `types.zig:198-203`). Read by nothing but a person looking at the log.
pub const COMPACTION_ENDED: &str = "failed";

/// A [`StepCompactor`], so the machine stays `Debug`.
struct Compactor(Box<dyn StepCompactor>);

impl std::fmt::Debug for Compactor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Compactor")
    }
}

/// A request as a compaction check measures it ([`TurnMachine::measure`]).
struct Measured {
    tokens: usize,
    bytes: usize,
    has_images: bool,
    fixed: Option<usize>,
}

/// What was asked of the compactor before the turn opened in the log
/// ([`TurnMachine::run_journaled`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Start {
    /// Nothing: there is no compactor.
    Fresh,
    /// The first request was measured and nothing was compacted.
    Checked,
    /// A compaction rebuilt the first request, which is measured again
    /// before it goes and not compacted again.
    Rebuilt,
}

/// Where a turn is in its one recovery from a request the provider refused as
/// too large (`ContextOverflowRecoveryState`, `orchestrator.zig:4447-4451`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Overflow {
    Ready,
    /// Refused: the next check compacts, and the refusal stands if it cannot.
    Pending,
    Used,
}

/// Whether a provider's refusal says the request was too large for the
/// model's window (`isContextOverflowFailure`, `orchestrator.zig:4428-4445`):
/// a `413`, or a `400` whose words say so.
pub(crate) fn is_context_overflow(err: &ProviderError) -> bool {
    let ProviderError::Status { status, body, .. } = err else {
        return false;
    };
    match status {
        413 => true,
        400 => {
            let body = body.to_ascii_lowercase();
            [
                "context_length_exceeded",
                "exceeds the context window",
                "exceeded the context window",
                "maximum context length",
                "maximum prompt length",
                "input is too long",
                "prompt is too long",
                "too many input tokens",
            ]
            .iter()
            .any(|needle| body.contains(needle))
        }
        _ => false,
    }
}

impl TurnMachine {
    pub fn new(request: TurnRequest) -> Self {
        let history = request.history.clone();
        let user = if request.images.is_empty() {
            Message::user(request.prompt.clone())
        } else {
            Message::user_with_images(
                request.prompt.clone(),
                request
                    .images
                    .iter()
                    .map(|image| (image.media_type, &image.bytes)),
            )
        };
        Self {
            request,
            context: ProjectContext::none(),
            static_context: String::new(),
            overlay: Vec::new(),
            history,
            user,
            suffix: Vec::new(),
            suffix_start: 0,
            compacted_in_turn: false,
            output: String::new(),
            partial: String::new(),
            steps: 0,
            finalized: false,
            requests: Vec::new(),
            unaccounted: false,
            lines: LineChanges::default(),
            compactor: None,
        }
    }

    /// The same machine, asking `compactor` before each step whether to
    /// compact the conversation first.
    pub fn with_compactor(mut self, compactor: Box<dyn StepCompactor>) -> Self {
        self.compactor = Some(Compactor(compactor));
        self
    }

    /// The same machine, carrying `context` as its static system message.
    ///
    /// Rendered once, here, rather than per request: the static prefix is a
    /// snapshot of the project as the turn began, and re-rendering it each step
    /// would let a file edited mid-turn change what the model was told earlier.
    pub fn with_context(mut self, context: ProjectContext) -> Self {
        self.static_context = context.render();
        self.context = context;
        self
    }

    /// The prompt as it currently stands, in wire order.
    pub fn messages(&self) -> Vec<Message> {
        let mut messages = Vec::with_capacity(self.history.len() + self.suffix.len() + 3);
        if !self.static_context.is_empty() {
            messages.push(Message::system(self.static_context.clone()));
        }
        for overlay in &self.overlay {
            messages.push(Message::system(overlay.clone()));
        }
        messages.extend(self.history.iter().cloned());
        messages.push(self.user.clone());
        messages.extend(
            self.suffix[self.suffix_start.min(self.suffix.len())..]
                .iter()
                .cloned(),
        );
        messages
    }

    /// How many model steps have run.
    pub fn steps(&self) -> u32 {
        self.steps
    }

    /// Runs the turn and finalizes it exactly once, recording nothing.
    pub async fn run(
        &mut self,
        provider: &dyn Provider,
        events: &mut dyn EventSink,
    ) -> Result<TurnOutcome, TurnError> {
        self.run_journaled(provider, events, &mut NoJournal).await
    }

    /// Runs the turn, finalizes it exactly once, and records what happened.
    pub async fn run_journaled(
        &mut self,
        provider: &dyn Provider,
        events: &mut dyn EventSink,
        journal: &mut dyn TurnJournal,
    ) -> Result<TurnOutcome, TurnError> {
        if self.finalized {
            return Err(TurnError::AlreadyFinalized);
        }
        self.finalized = true;

        // **A compaction the first request needs comes before the turn opens
        // in the log**, as upstream's does: one that fails leaves no turn
        // behind -- upstream persists a failed turn only once it has
        // execution to keep (`finishExecutionOnlyFailureIfNeeded`,
        // `finalization.zig:213-246`) -- and only what its notes requests
        // cost is written. One the user stopped is a turn that was asked and
        // stopped, and is recorded as one (`persistCompactionInterruptedTurnOnce`,
        // `orchestrator.zig:7413-7423`).
        let mut start = Start::Fresh;
        let mut stopped = None;
        if self.compactor.is_some() {
            match self
                .compact_before_step(provider, journal, false, false)
                .await
            {
                Ok(Some(compaction)) => {
                    self.compacted(compaction);
                    start = Start::Rebuilt;
                }
                Ok(None) => start = Start::Checked,
                Err(TurnError::Cancelled) => stopped = Some(TurnError::Cancelled),
                Err(err) => {
                    let requests = std::mem::take(&mut self.requests);
                    if !requests.is_empty() || self.unaccounted {
                        journal.record(SessionEvent::UsageRecorded {
                            input_tokens: None,
                            output_tokens: None,
                            requests,
                            unaccounted: self.unaccounted,
                            lines_added: 0,
                            lines_removed: 0,
                            settled: Vec::new(),
                            rejected: Vec::new(),
                        });
                    }
                    events
                        .emit(&Event::Error {
                            message: err.to_string(),
                        })
                        .map_err(TurnError::Sink)?;
                    return Err(err);
                }
            }
        }

        // The user's message opens the turn in the log before anything can go
        // wrong, so a turn that fails on its first request is still a turn that
        // was asked.
        journal.record(SessionEvent::UserMessage {
            text: self.request.prompt.clone(),
        });
        journal.attach_images(&self.request.images);

        let result = match stopped {
            Some(err) => Err(err),
            None => self.drive(provider, events, journal, start).await,
        };

        // The answer a failed step had put in front of the user, as the turn's
        // last step: the conclusion closes it.
        if result.is_err() {
            let partial = std::mem::take(&mut self.partial);
            if !partial.is_empty() {
                journal.record(SessionEvent::AssistantMessage {
                    text: partial,
                    tool_calls: Vec::new(),
                    raw_content: Vec::new(),
                    responses_state: Vec::new(),
                    wire: None,
                });
            }
        }

        // What the turn cost, after its last step and just before it is
        // concluded. A turn that failed records one only when it has something
        // to say -- a request that completed before the failure, or one that
        // was lost after it was sent -- because a turn refused before any
        // request was made cost nothing, and a frame saying so would be a
        // frame about nothing. Its two totals stay absent: they are the
        // answering step's, and it had none.
        let (input_tokens, output_tokens) = match &result {
            Ok(outcome) => (outcome.usage.input_tokens, outcome.usage.output_tokens),
            Err(_) => (None, None),
        };
        if result.is_ok() || !self.requests.is_empty() || self.unaccounted {
            journal.record(SessionEvent::UsageRecorded {
                input_tokens,
                output_tokens,
                requests: std::mem::take(&mut self.requests),
                unaccounted: self.unaccounted,
                lines_added: u64::try_from(self.lines.added).unwrap_or(u64::MAX),
                lines_removed: u64::try_from(self.lines.removed).unwrap_or(u64::MAX),
                settled: Vec::new(),
                rejected: Vec::new(),
            });
        }

        // Exactly one conclusion, whichever way the turn went.
        journal.record(SessionEvent::TurnConcluded {
            outcome: match &result {
                Ok(outcome) => TurnConclusion::Final {
                    finish_reason: outcome.finish_reason.label().to_string(),
                    steps: outcome.steps,
                },
                // Upstream keeps the steps of a turn a compaction ended as a
                // turn that answered nothing, with no error of its own to show
                // or to replay (`finishExecutionOnlyFailureIfNeeded`,
                // `finalization.zig:213-246`: an empty reply, `.failed`).
                Err(TurnError::CompactionEnded { .. }) => TurnConclusion::Final {
                    finish_reason: COMPACTION_ENDED.to_string(),
                    steps: self.steps,
                },
                Err(err) => TurnConclusion::Interrupted {
                    reason: err.to_string(),
                },
            },
        });

        // The only place a terminal event is written. One match, two arms, no
        // early return in between: the "exactly once" property is the shape of
        // this code rather than a rule someone has to remember.
        let terminal = match &result {
            Ok(outcome) => Event::Final {
                output: outcome.output.clone(),
            },
            Err(err) => Event::Error {
                message: err.to_string(),
            },
        };
        events.emit(&terminal).map_err(TurnError::Sink)?;
        result
    }

    /// Runs the turn to a terminal state.
    ///
    /// Emits `assistant_delta`, `tool_start`, and `tool_result` events. Never
    /// emits a terminal event.
    ///
    /// The loop turns over on exactly one thing: a completion that named tool
    /// calls. Everything else -- an answer, a refusal, an exhausted budget --
    /// returns. Upstream draws the same line: zero tool calls is the terminal
    /// case and any tool call continues the turn
    /// (`orchestrator.zig:4624`, `:4761-4765`).
    async fn drive(
        &mut self,
        provider: &dyn Provider,
        events: &mut dyn EventSink,
        journal: &mut dyn TurnJournal,
        start: Start,
    ) -> Result<TurnOutcome, TurnError> {
        let mut overflow = Overflow::Ready;
        // The provider's refusal, while it is being recovered from.
        let mut refusal: Option<TurnError> = None;
        // A compaction has rebuilt the request, which has not gone yet.
        let mut rebuilt = start == Start::Rebuilt;
        // The first request was measured before the turn opened.
        let mut checked = start == Start::Checked;
        loop {
            if self.request.cancel.is_cancelled() {
                return Err(TurnError::Cancelled);
            }
            // The turn's own budget, consulted before spending a model call.
            // This is where the loop re-enters, so a tool loop that never
            // finishes stops here rather than running forever.
            if !allows_step(self.request.max_steps, self.steps) {
                return Err(TurnError::StepLimit {
                    limit: self.request.max_steps,
                });
            }

            if self.compactor.is_some() && !std::mem::take(&mut checked) {
                let pending = overflow == Overflow::Pending;
                match self
                    .compact_before_step(provider, journal, pending, rebuilt)
                    .await?
                {
                    Some(compaction) => {
                        self.compacted(compaction);
                        rebuilt = true;
                        if pending {
                            overflow = Overflow::Used;
                            refusal = None;
                        }
                        // Measured again before it goes: a request that still
                        // does not fit is not sent (`orchestrator.zig:7347-7350`).
                        continue;
                    }
                    // Nothing could be compacted, so the refusal stands
                    // (`overflow_without_compaction`, `:7344-7346`).
                    None if pending => {
                        return Err(refusal.take().unwrap_or(TurnError::ProviderFailure {
                            detail: "the request was too large for the model".to_string(),
                        }));
                    }
                    None => {}
                }
            }

            let began = Instant::now();
            let completion = match self.step(provider, events).await {
                Ok(completion) => completion,
                // Too large for the model's window: compacted once and asked
                // again, when there is something to compact and nothing of
                // the answer was shown (`shouldRecoverContextOverflow`,
                // `orchestrator.zig:4453-4466`, `:8099-8124`).
                Err(TurnError::Provider(err))
                    if self.compactor.is_some()
                        && overflow == Overflow::Ready
                        && self.has_compactable_context()
                        && self.partial.is_empty()
                        && !self.request.cancel.is_cancelled()
                        && is_context_overflow(&err) =>
                {
                    overflow = Overflow::Pending;
                    refusal = Some(TurnError::Provider(err));
                    rebuilt = false;
                    continue;
                }
                Err(err) => return Err(err),
            };
            rebuilt = false;
            self.account(&completion, began.elapsed());
            self.steps += 1;
            self.output.push_str(&completion.text);

            // A provider failure is terminal whatever else the completion
            // carried; running tools it named would be acting on a step the
            // provider says did not finish.
            if completion.finish_reason == FinishReason::ProviderError {
                return Err(TurnError::ProviderFailure {
                    detail: completion
                        .provider_detail
                        .unwrap_or_else(|| "the provider gave no detail".to_string()),
                });
            }

            if completion.tool_calls.is_empty() {
                // The answering step's own evidence, recorded through the same
                // guard the tool-calling step uses.
                record_assistant(journal, &completion);
                return match completion.finish_reason {
                    FinishReason::ToolCalls => Err(TurnError::EmptyToolCallFinish),
                    // `stop`, `length`, `content-filter`, and `other` all end
                    // the turn with whatever text arrived. Only `length` and
                    // `content-filter` mean the answer is cut short, and the
                    // finish reason travels in the outcome so a caller can say
                    // so.
                    _ => Ok(TurnOutcome {
                        output: std::mem::take(&mut self.output),
                        steps: self.steps,
                        usage: completion.usage,
                        finish_reason: completion.finish_reason,
                    }),
                };
            }

            // The model ran out of room mid-call, so the arguments it sent may
            // be a prefix of what it meant. Running a truncated path is worse
            // than failing (`orchestrator.zig:4581-4586`).
            if completion.finish_reason == FinishReason::Length {
                return Err(TurnError::ToolCallTruncated {
                    tool: completion.tool_calls[0].name.clone(),
                });
            }

            self.execute_tool_calls(&completion, events, journal)
                .await?;
        }
    }

    /// Runs one step's tool calls and extends the conversation with the result.
    ///
    /// Checks first, executions second. Both checks cover the whole step before
    /// anything runs, so a step with one bad call does not leave the workspace
    /// half-read and the conversation half-written.
    ///
    /// **What the user said while approving travels with the step, not with the
    /// call.** A sentence typed beside an approval is buffered per call and
    /// delivered after every result of the step; [`Self::flush_amendments`]
    /// says why after, and which exits deliver it. It is context and never
    /// authority: the call that ran is the call that was judged.
    ///
    /// **Async for one reason: the step has to be interruptible between its
    /// calls.** Every executor here is synchronous, and one of them --
    /// `ask_user_question`, and an `ask`-mode approval inside any of the others
    /// -- parks this thread for as long as a person takes to answer. The stop
    /// the person then types arrives while this loop is between two calls, and
    /// a loop with no await in it gives whoever is driving this turn no moment
    /// to record it: the flag this function reads is set by that driver
    /// (`crate::tui::worker`'s `raced_against_control`, `crate::app`'s signal
    /// handler), so without the yield below the check would read a flag nobody
    /// has been able to write yet and the rest of the step would run.
    async fn execute_tool_calls(
        &mut self,
        completion: &Completion,
        events: &mut dyn EventSink,
        journal: &mut dyn TurnJournal,
    ) -> Result<(), TurnError> {
        let registry = Registry::builtin();

        // A result correlates to a call by id, so two calls sharing one id
        // cannot both be answered. Ids already in the prompt count too: the
        // Gateway rejects a prompt that reuses one.
        let mut announced = self.announced_call_ids();
        for call in &completion.tool_calls {
            if announced.contains(&call.id) {
                return Err(TurnError::DuplicateToolCallId {
                    call_id: call.id.clone(),
                });
            }
            announced.push(call.id.clone());
        }
        for call in &completion.tool_calls {
            if registry.spec(&call.name).is_none() {
                return Err(TurnError::ToolCallUnsupported {
                    tool: call.name.clone(),
                });
            }
        }

        // The assistant turn goes in exactly as the provider sent it: its text,
        // then its calls in order. The next request has to show the model what
        // it asked for, not xfx's paraphrase of it.
        //
        // When the provider sent its own content blocks, "exactly as it sent
        // them" stops being a figure of speech. Anthropic signs its reasoning
        // blocks and verifies the signature when they come back in a tool
        // continuation, so a rebuilt-from-text assistant turn is answered with a
        // 400 at the next step. The blocks are replayed verbatim instead.
        self.suffix.push(if completion.raw_content.is_empty() {
            Message::assistant(Some(&completion.text), completion.tool_calls.clone())
        } else {
            Message::assistant_raw(
                completion.raw_content.clone(),
                completion.tool_calls.clone(),
            )
        });
        record_assistant(journal, completion);

        // What the user said while answering this step's approvals, in call
        // order. **Function-local, never a field**: a buffer on `self` would
        // survive an early return and hand one call's sentence to the next
        // call, or to the next step. Here, a panic unwinds it, a cancellation
        // drops it, and the two flush sites below consume it exactly once.
        let mut amendments: Vec<(String, String)> = Vec::new();

        for call in &completion.tool_calls {
            // **Before this call is admitted at all.** The turn was stopped
            // while an earlier call of this same step was running -- a Ctrl-C
            // typed at the panel that call was parked in, a shutdown, a signal
            // -- and the calls behind it are work nobody asked for any more. The
            // step is abandoned here rather than at the top of `drive`, because
            // by then the file would already be written.
            if self.request.cancel.is_cancelled() {
                return Err(TurnError::Cancelled);
            }

            // Before the call is admitted, not after it runs: a rule about the
            // directory this call is about to touch is worth nothing once the
            // file has already been written.
            if let Some(target) = target_path(call.input.get("path")) {
                if let Some(delta) = self.context.admit_target(&target) {
                    self.overlay.push(delta);
                }
            }

            // Checked above like the execution below it, and mapped rather than
            // unwrapped for the same reason.
            let Some(spec) = registry.spec(&call.name) else {
                return Err(TurnError::ToolCallUnsupported {
                    tool: call.name.clone(),
                });
            };
            events
                .emit(&Event::ToolStart {
                    call_id: call.id.clone(),
                    tool: call.name.clone(),
                    shown: spec.shown(&call.input),
                })
                .map_err(TurnError::Sink)?;

            // Checked above, so the registry cannot refuse the name here; the
            // error is still mapped rather than unwrapped, because a panic in a
            // turn would lose the terminal event the caller is owed.
            let result = registry
                .execute(call, &self.request.tools)
                .map_err(|err| TurnError::ToolCallUnsupported { tool: err.name })?;

            // Counted as soon as the file has changed, ahead of anything that
            // can still fail this step: the lines are on disk either way.
            if let Some(changes) = result.changes {
                self.lines.added = self.lines.added.saturating_add(changes.added);
                self.lines.removed = self.lines.removed.saturating_add(changes.removed);
            }
            events
                .emit(&Event::ToolResult {
                    call_id: call.id.clone(),
                    tool: call.name.clone(),
                    ok: result.ok,
                    detail: result.detail.clone(),
                    outcome: result.outcome,
                    changes: result.changes,
                })
                .map_err(TurnError::Sink)?;

            journal.record(SessionEvent::ToolResult {
                call_id: call.id.clone(),
                tool: call.name.clone(),
                ok: result.ok,
                output: result.output.clone(),
            });
            if let Some(change) = &result.change {
                journal.change_made(&call.id, change);
            }
            // A call that failed is said as one on the wire, as upstream says
            // every result whose status is a failure (`error-text`,
            // `vercel-labs/fx@c1db9191 src/gateway/vercel_protocol.zig:742-747`).
            self.suffix.push(if result.ok {
                Message::tool_result(&call.id, &call.name, result.output)
            } else {
                Message::tool_error(&call.id, &call.name, result.output)
            });

            // Buffered here, delivered after the loop. Directly after the
            // result push, so only a call that actually ran and was answered
            // can contribute one -- which is what makes the disposition of
            // every exit below decidable rather than incidental.
            if let Some(text) = result.feedback {
                amendments.push((call.id.clone(), text));
            }

            // Almost every refusal goes back to the model, which can correct
            // itself. One does not: an authority that stopped describing the
            // filesystem means the premise of the exchange is void, so the turn
            // ends here rather than running the rest of the step or asking for
            // another one.
            if result.fatal {
                // A turn that ends still owes the journal and the reader what
                // the user said about the calls that ran. Nothing is sent --
                // this returns an error, so there is no next request -- but a
                // resumed session can read it.
                //
                // **[`SinkRefusal::JournalOn`], and the journal is why.** This
                // turn is already ending for a reason of its own, and the log
                // is the only copy left: a stream that has gone must cost the
                // echo and nothing else, or a display failure would silently
                // take the sentence about the very call whose authority was
                // revoked. Under `Stop` the first refusal would drop every
                // amendment of the step, which is the defect this arm exists
                // to not have.
                let shown =
                    self.flush_amendments(events, journal, amendments, SinkRefusal::JournalOn);
                let revoked = TurnError::ToolAuthorityRevoked {
                    tool: call.name.clone(),
                    detail: result.detail,
                };
                // Two failures and one `TurnError`, so one of them is reported
                // and the other is **superseded** -- which is a different claim
                // from "handled", and is only defensible because the line above
                // has already put every sentence in the journal. The revocation
                // is what ends the turn and what the user must act on; a reader
                // whose stream refused the sentence cannot be told anything
                // either way. Written as a branch rather than as a discarded
                // binding so the ruling is read at the point it is made, and
                // total rather than wildcarded so an error kind added to the
                // flush later cannot be folded into the revocation by default.
                return Err(match shown {
                    Ok(()) | Err(TurnError::Sink(_)) => revoked,
                    Err(other) => other,
                });
            }

            // **The moment a stop can be recorded.** The call above ran to
            // completion and its result is already in the conversation and the
            // journal, so nothing is half-done; what this hands back is the
            // chance for the driver to see the message the user sent while that
            // call had this thread -- and, with it, to set the flag the top of
            // the next iteration reads. One poll, taken once per call, and the
            // only cost on a turn nobody interrupted.
            yield_once().await;
        }

        // **The last call needs this, or the cancellation contract is a
        // fiction.** A stop is *observed* at the yield above and *acted on* at
        // the top of the next iteration -- and after the final call there is no
        // next iteration. Without this check the loop would fall straight into
        // the flush and journal, then send, sentences from a turn the user had
        // already stopped. It is the same predicate the loop head reads, so the
        // one-call case behaves like every multi-call one.
        if self.request.cancel.is_cancelled() {
            drop(amendments);
            return Err(TurnError::Cancelled);
        }
        // **[`SinkRefusal::Stop`] on the ordinary path**, unchanged: this turn
        // is otherwise fine and is about to ask for another completion, so a
        // reader who cannot be told what the user said is a turn that should
        // not go on -- and the sentence is left out of the journal and the
        // prompt rather than recorded as delivered to somebody who never saw
        // it.
        self.flush_amendments(events, journal, amendments, SinkRefusal::Stop)
    }

    /// Puts every amendment of one step into the journal and the prompt, in
    /// call order, **after** every result of that step.
    ///
    /// After, because `llmux/protocol.rs:24-32` merges consecutive user
    /// messages and hoists `tool_result` blocks to the front of the merged one:
    /// a sentence interleaved between two results of a batch would arrive above
    /// the result it was about. The `call_id` keeps the origin, since the
    /// position no longer can.
    ///
    /// Called from exactly two places -- the normal end of a step, and the
    /// `fatal` return -- so that every other exit drops the buffer as a
    /// contract rather than as an accident of where the code happens to return.
    ///
    /// **The stream is written here and nowhere else**, which is what makes the
    /// transcript honest rather than merely prompt. A UI that echoed the
    /// sentence when the key was pressed would be claiming a delivery that had
    /// not happened yet -- and for the two exits that drop the buffer, one that
    /// never will. So all three copies of an amendment are made in this loop:
    /// the reader's, the journal's and the model's, per call, in call order.
    ///
    /// The **emit comes first**, which is the order the result of a call
    /// already uses a few lines above ([`Self::execute_tool_calls`]): a stream
    /// that has gone ends the turn with its own error, and the sentence is left
    /// out of the journal and out of the prompt rather than recorded as
    /// delivered to a reader who never saw it.
    fn flush_amendments(
        &mut self,
        events: &mut dyn EventSink,
        journal: &mut dyn TurnJournal,
        amendments: Vec<(String, String)>,
        refusal: SinkRefusal,
    ) -> Result<(), TurnError> {
        // The first refusal, kept rather than returned at once under
        // [`SinkRefusal::JournalOn`]. Once the stream has refused, the rest of
        // the batch is journaled without being offered to it again: a second
        // write to a pipe that has gone is noise, and this is what keeps
        // "shown" and "journaled" two answers rather than one.
        let mut refused: Option<io::Error> = None;
        for (call_id, text) in amendments {
            if refused.is_none() {
                if let Err(err) = events.emit(&Event::ToolFeedback {
                    call_id: call_id.clone(),
                    text: text.clone(),
                }) {
                    match refusal {
                        SinkRefusal::Stop => return Err(TurnError::Sink(err)),
                        SinkRefusal::JournalOn => refused = Some(err),
                    }
                }
            }
            journal.record(SessionEvent::ToolFeedback {
                call_id,
                text: text.clone(),
            });
            self.suffix.push(Message::user(text));
        }
        match refused {
            Some(err) => Err(TurnError::Sink(err)),
            None => Ok(()),
        }
    }

    /// Every tool call id already in the prompt.
    fn announced_call_ids(&self) -> Vec<String> {
        self.history
            .iter()
            .chain(self.suffix.iter())
            .flat_map(|message| message.content.iter())
            .filter_map(|part| match part {
                ContentPart::ToolCall(call) => Some(call.id.clone()),
                _ => None,
            })
            .collect()
    }

    /// Runs one model step, spending at most `max_attempts` transport attempts.
    /// Asks the compactor about the request this step would send, and keeps
    /// what its notes requests cost with the turn's own.
    async fn compact_before_step(
        &mut self,
        provider: &dyn Provider,
        journal: &mut dyn TurnJournal,
        overflow: bool,
        rebuilt: bool,
    ) -> Result<Option<StepCompaction>, TurnError> {
        let measured = self.measure();
        let Some(Compactor(compactor)) = self.compactor.as_mut() else {
            return Ok(None);
        };
        let answer = compactor
            .before_step(StepCheck {
                request_tokens: measured.tokens,
                request_bytes: measured.bytes,
                has_images: measured.has_images,
                fixed_tokens: measured.fixed,
                overflow,
                compactable: !rebuilt,
                step: self.steps,
                provider,
                journal,
            })
            .await;
        let (requests, unaccounted) = compactor.spent();
        self.requests.extend(requests);
        self.unaccounted |= unaccounted;
        answer
    }

    /// The request this step would send, measured as upstream measures one
    /// before deciding to compact (`measureProviderRequest`,
    /// `prompt_context.zig:83-120`): its body by the token estimate, with an
    /// image priced apart from the text around it -- here, left out -- so a
    /// picture's bytes are not counted as words.
    fn measure(&self) -> Measured {
        let mut request = self.completion_request();
        // Its whole length, images too, as upstream's `request_bytes` is.
        let bytes = request.body().map_or(0, |body| body.len());
        let mut has_images = false;
        for message in &mut request.messages {
            message.content.retain(|part| {
                let image = matches!(part, ContentPart::Image { .. });
                has_images |= image;
                !image
            });
        }
        let body = request.body().unwrap_or_default();
        // Its instructions stay: they are not the conversation.
        request
            .messages
            .retain(|message| message.role == crate::gateway::protocol::Role::System);
        let fixed = request
            .body()
            .ok()
            .map(|body| super::compact::text_tokens(&body));
        Measured {
            tokens: super::compact::text_tokens(&body),
            bytes,
            has_images,
            fixed,
        }
    }

    /// Whether anything is left that a compaction could take: before one in
    /// this turn, any history or any step of the turn's own; after one, a step
    /// the turn has made since (`has_compactable_context`,
    /// `orchestrator.zig:8095-8098`).
    fn has_compactable_context(&self) -> bool {
        if self.compacted_in_turn {
            self.suffix_start < self.suffix.len()
        } else {
            !self.history.is_empty() || !self.suffix.is_empty()
        }
    }

    /// Takes a compaction's history, and its share of this turn's own steps.
    fn compacted(&mut self, compaction: StepCompaction) {
        self.compacted_in_turn = true;
        self.history = compaction.history;
        // The start of exchange `exchanges`: the assistant message that made
        // that step's calls, counted from the turn's first.
        self.suffix_start = self
            .suffix
            .iter()
            .enumerate()
            .filter(|(_, message)| {
                message.role == crate::gateway::protocol::Role::Assistant
                    && message
                        .content
                        .iter()
                        .any(|part| matches!(part, ContentPart::ToolCall(_)))
            })
            .nth(compaction.exchanges)
            .map_or(self.suffix.len(), |(index, _)| index);
    }

    async fn step(
        &mut self,
        provider: &dyn Provider,
        events: &mut dyn EventSink,
    ) -> Result<Completion, TurnError> {
        let request = self.completion_request();
        let mut attempt = 0u32;
        loop {
            if attempt >= self.request.max_attempts {
                return Err(TurnError::AttemptLimit {
                    limit: self.request.max_attempts,
                });
            }
            attempt += 1;
            if attempt > 1 {
                events.retry_started(attempt, self.request.max_attempts);
            }

            let mut deltas = StepDeltas {
                events: &mut *events,
                delivered: false,
                text: String::new(),
                error: None,
            };
            let outcome = provider.stream(&request, &mut deltas).await;
            let delivered = deltas.delivered;
            // What reached the user, kept for a turn that ends here; a step
            // that completes records its own text instead.
            self.partial = std::mem::take(&mut deltas.text);
            // A sink failure is reported as itself rather than as whatever the
            // provider made of it, so a closed stdout is never read as a
            // protocol failure.
            if let Some(err) = deltas.error.take() {
                return Err(TurnError::Sink(err));
            }

            match outcome {
                Ok(completion) => {
                    self.partial.clear();
                    if attempt > 1 {
                        events.recovered(attempt, self.request.max_attempts);
                    }
                    return Ok(completion);
                }
                Err(err) => {
                    // `delivered` is the decisive fact: once part of an answer
                    // is in front of the user, a replay would produce a second
                    // answer to one question and bill for both.
                    if delivered || !err.is_replayable() || attempt >= self.request.max_attempts {
                        // A request that left and was lost may have been
                        // billed, and nothing will ever say for how much
                        // (`ambiguous_delivery`, `session_usage.zig:43-53`).
                        // One the endpoint refused, or that never left, was
                        // not.
                        if may_have_billed(&err) {
                            self.unaccounted = true;
                        }
                        return Err(TurnError::from(err));
                    }
                    // The failure survived every "do not replay" test, so this
                    // is the one place a retry is allowed -- and it waits first.
                    // Said before the wait, to whoever is showing the turn: the
                    // decision is already made, and the delay is the one
                    // `wait_for_retry` is about to spend (`retry_delay` is a
                    // function of exactly these two values).
                    events.retrying(
                        &err,
                        attempt,
                        self.request.max_attempts,
                        retry_delay(attempt, err.retry_after()),
                    );
                    self.wait_for_retry(&err, attempt).await?;
                }
            }
        }
    }

    /// Keeps what one completed request cost, for `/usage`
    /// ([`request_usage`]).
    fn account(&mut self, completion: &Completion, took: Duration) {
        self.requests
            .push(request_usage(completion, &self.request.model, took));
    }

    /// Waits out the backoff between two attempts.
    ///
    /// Split from [`Self::step`] so the wait is one named decision rather than
    /// something buried in a retry branch.
    async fn wait_for_retry(&self, err: &ProviderError, attempt: u32) -> Result<(), TurnError> {
        let delay = retry_delay(attempt, err.retry_after());
        if delay.is_zero() {
            // Still an explicit cancellation point: a zero delay must not be a
            // hole through which a cancelled turn issues another request.
            return if self.request.cancel.is_cancelled() {
                Err(TurnError::Cancelled)
            } else {
                Ok(())
            };
        }
        wait_before_retry(delay, &self.request.cancel).await
    }

    /// The request for the next model step -- before the first, the request
    /// the turn would open with, which is what an automatic compaction
    /// measures (`crate::agent::compact::Size::due`).
    pub fn completion_request(&self) -> CompletionRequest {
        CompletionRequest {
            model: self.request.model.clone(),
            messages: self.messages(),
            // The registry verbatim, every step. Re-advertising is not
            // redundant: the Gateway is stateless, so a schema omitted from
            // step two is a tool the model no longer has.
            tools: Registry::builtin().advertisement(),
            // `auto`, because every advertised tool is real and read-only.
            tool_choice: ToolChoice::Auto,
        }
    }
}

/// Forwards decoded assistant text to the turn's event sink.
///
/// It also records the one fact the retry decision needs: whether anything was
/// put in front of the user during this attempt.
struct StepDeltas<'a> {
    events: &'a mut dyn EventSink,
    delivered: bool,
    /// The text the user was shown, in order -- only what was emitted.
    text: String,
    /// The first write failure, kept so the turn can report it as its own.
    error: Option<io::Error>,
}

impl DeltaSink for StepDeltas<'_> {
    fn text_delta(&mut self, text: &str) -> io::Result<()> {
        self.delivered = true;
        let event = Event::AssistantDelta {
            text: text.to_string(),
        };
        match self.events.emit(&event) {
            Ok(()) => {
                self.text.push_str(text);
                Ok(())
            }
            Err(err) => {
                let reported = io::Error::new(err.kind(), err.to_string());
                if self.error.is_none() {
                    self.error = Some(err);
                }
                Err(reported)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::ToolContext;
    use crate::workspace::AccessScope;

    /// A tool context rooted at a real, empty directory.
    ///
    /// The `TempDir` is returned so the caller keeps it alive: a scope whose
    /// root has been deleted is a different test than the one being written.
    fn tools() -> (tempfile::TempDir, ToolContext) {
        let dir = tempfile::tempdir().expect("temporary workspace");
        let scope = AccessScope::primary_only(dir.path()).expect("a usable root");
        (dir, ToolContext::new(scope))
    }

    /// A provider that answers from a script and keeps what it was asked.
    struct Script {
        replies: std::cell::RefCell<Vec<Result<Completion, ProviderError>>>,
        asked: std::cell::RefCell<Vec<CompletionRequest>>,
    }

    impl Script {
        fn new(replies: Vec<Result<Completion, ProviderError>>) -> Self {
            Self {
                replies: std::cell::RefCell::new(replies),
                asked: std::cell::RefCell::new(Vec::new()),
            }
        }
    }

    #[async_trait::async_trait(?Send)]
    impl Provider for Script {
        async fn stream(
            &self,
            request: &CompletionRequest,
            _deltas: &mut dyn DeltaSink,
        ) -> Result<Completion, ProviderError> {
            self.asked.borrow_mut().push(request.clone());
            self.replies.borrow_mut().remove(0)
        }
    }

    fn answer(text: &str) -> Result<Completion, ProviderError> {
        Ok(Completion {
            text: text.to_string(),
            tool_calls: Vec::new(),
            finish_reason: FinishReason::Stop,
            usage: crate::gateway::protocol::Usage::default(),
            provider_detail: None,
            raw_content: Vec::new(),
            wire: crate::provider::Wire::VercelGateway,
            billing: None,
        })
    }

    fn too_large() -> Result<Completion, ProviderError> {
        Err(ProviderError::Status {
            subject: "the Gateway",
            status: 400,
            body: "AI_APICallError: input exceeds the context window".to_string(),
            retryable: false,
            retry_after: None,
        })
    }

    struct Quiet;

    impl EventSink for Quiet {
        fn emit(&mut self, _event: &Event) -> io::Result<()> {
            Ok(())
        }
    }

    /// Each check a compactor was asked: the request's tokens, `overflow`,
    /// `compactable` and the step.
    type Checks = std::rc::Rc<std::cell::RefCell<Vec<(usize, bool, bool, u32)>>>;

    /// Compacts when asked to, replacing the history with one message, and
    /// keeps every question it was asked.
    struct Compacting {
        asked: Checks,
        /// Which checks compact, by their order.
        compact_on: Vec<usize>,
    }

    #[async_trait::async_trait(?Send)]
    impl StepCompactor for Compacting {
        async fn before_step(
            &mut self,
            check: StepCheck<'_>,
        ) -> Result<Option<StepCompaction>, TurnError> {
            let mut asked = self.asked.borrow_mut();
            asked.push((
                check.request_tokens,
                check.overflow,
                check.compactable,
                check.step,
            ));
            Ok(self
                .compact_on
                .contains(&(asked.len() - 1))
                .then(|| StepCompaction {
                    history: vec![Message::user("<compacted_conversation>")],
                    exchanges: 0,
                }))
        }

        fn spent(&mut self) -> (Vec<RequestUsage>, bool) {
            (Vec::new(), self.asked.borrow().len() == 1)
        }
    }

    fn machine(tools: crate::tools::ToolContext) -> TurnMachine {
        TurnMachine::new(TurnRequest {
            images: Vec::new(),
            model: "m".to_string(),
            prompt: "now".to_string(),
            history: vec![
                Message::user("before"),
                Message::assistant(Some("BEFORE"), Vec::new()),
            ],
            max_steps: 4,
            max_attempts: 1,
            cancel: crate::gateway::CancelToken::new(),
            tools,
        })
    }

    #[tokio::test]
    async fn a_compaction_before_a_step_replaces_the_history_that_step_sends() {
        let (_dir, tools) = tools();
        let asked = std::rc::Rc::default();
        let mut turn = machine(tools).with_compactor(Box::new(Compacting {
            asked: std::rc::Rc::clone(&asked),
            compact_on: vec![0],
        }));
        let provider = Script::new(vec![answer("done")]);
        let outcome = turn.run(&provider, &mut Quiet).await.expect("answered");
        assert_eq!(outcome.output, "done");
        // Asked before the step, then again for the rebuilt request, which
        // may not be compacted again before it goes (`orchestrator.zig:7304`).
        let asked = asked.borrow();
        assert_eq!(asked.len(), 2, "{asked:?}");
        assert!(asked[0].2 && !asked[1].2, "{asked:?}");
        assert!(asked[1].0 < asked[0].0, "the rebuilt request is smaller");
        let sent = provider.asked.borrow();
        assert_eq!(sent[0].messages[0].text(), "<compacted_conversation>");
        assert_eq!(sent[0].messages[1].text(), "now");
        // What the notes requests cost is the turn's to record.
        assert!(turn.unaccounted);
    }

    #[tokio::test]
    async fn a_request_refused_as_too_large_is_compacted_and_asked_again_once() {
        let (_dir, tools) = tools();
        let asked = std::rc::Rc::default();
        let mut turn = machine(tools).with_compactor(Box::new(Compacting {
            asked: std::rc::Rc::clone(&asked),
            compact_on: vec![1],
        }));
        let provider = Script::new(vec![too_large(), answer("done")]);
        let outcome = turn.run(&provider, &mut Quiet).await.expect("recovered");
        assert_eq!(outcome.output, "done");
        let asked = asked.borrow();
        // Not due before the first; told of the refusal before the second.
        assert_eq!(
            asked
                .iter()
                .map(|(_, overflow, ..)| *overflow)
                .collect::<Vec<_>>(),
            [false, true, false]
        );
        assert_eq!(provider.asked.borrow().len(), 2);
        assert_eq!(
            provider.asked.borrow()[1].messages[0].text(),
            "<compacted_conversation>"
        );
    }

    #[tokio::test]
    async fn a_refusal_nothing_can_be_compacted_for_stands_and_a_second_is_not_recovered() {
        let (_one, context) = tools();
        // Nothing compacted: the refusal is the turn's error.
        let mut turn = machine(context).with_compactor(Box::new(Compacting {
            asked: std::rc::Rc::default(),
            compact_on: Vec::new(),
        }));
        let provider = Script::new(vec![too_large()]);
        let err = turn.run(&provider, &mut Quiet).await.expect_err("refused");
        assert!(
            err.to_string().contains("exceeds the context window"),
            "{err}"
        );
        assert_eq!(provider.asked.borrow().len(), 1);

        // Compacted once and refused again: the second stands.
        let (_two, context) = tools();
        let mut turn = machine(context).with_compactor(Box::new(Compacting {
            asked: std::rc::Rc::default(),
            compact_on: vec![1],
        }));
        let provider = Script::new(vec![too_large(), too_large()]);
        assert!(turn.run(&provider, &mut Quiet).await.is_err());
        assert_eq!(provider.asked.borrow().len(), 2);

        // And a turn with no compactor is not recovered at all.
        let (_three, context) = tools();
        let provider = Script::new(vec![too_large()]);
        assert!(machine(context).run(&provider, &mut Quiet).await.is_err());
    }

    /// Fails every check with `error`.
    struct Failing(fn() -> TurnError);

    #[async_trait::async_trait(?Send)]
    impl StepCompactor for Failing {
        async fn before_step(
            &mut self,
            _check: StepCheck<'_>,
        ) -> Result<Option<StepCompaction>, TurnError> {
            Err((self.0)())
        }
    }

    #[derive(Default)]
    struct Kept(Vec<SessionEvent>);

    impl TurnJournal for Kept {
        fn record(&mut self, event: SessionEvent) {
            self.0.push(event);
        }
    }

    #[tokio::test]
    async fn a_compaction_the_first_request_needed_that_failed_leaves_no_turn_and_a_stopped_one_does(
    ) {
        // `finishExecutionOnlyFailureIfNeeded`, `finalization.zig:213-246`:
        // nothing ran, so nothing of the turn is kept.
        let (_one, context) = tools();
        let mut turn =
            machine(context).with_compactor(Box::new(Failing(|| TurnError::ContextNotReduced {
                reason: "ModelFailed",
            })));
        let provider = Script::new(Vec::new());
        let mut journal = Kept::default();
        let err = turn
            .run_journaled(&provider, &mut Quiet, &mut journal)
            .await
            .expect_err("refused");
        assert!(matches!(err, TurnError::ContextNotReduced { .. }));
        assert!(journal.0.is_empty(), "{:?}", journal.0);
        assert!(provider.asked.borrow().is_empty());

        // Stopped: asked and stopped (`persistCompactionInterruptedTurnOnce`,
        // `orchestrator.zig:7413-7423`).
        let (_two, context) = tools();
        let mut turn = machine(context).with_compactor(Box::new(Failing(|| TurnError::Cancelled)));
        let mut journal = Kept::default();
        assert!(matches!(
            turn.run_journaled(&provider, &mut Quiet, &mut journal)
                .await,
            Err(TurnError::Cancelled)
        ));
        let kinds: Vec<&str> = journal.0.iter().map(SessionEvent::kind).collect();
        assert_eq!(kinds, ["user_message", "turn_concluded"]);
        assert!(provider.asked.borrow().is_empty());
    }

    #[test]
    fn a_refusal_is_an_overflow_by_its_status_or_its_words() {
        // `isContextOverflowFailure`, `orchestrator.zig:4428-4445`.
        let refused = |status: u16, body: &str| ProviderError::Status {
            subject: "the Gateway",
            status,
            body: body.to_string(),
            retryable: false,
            retry_after: None,
        };
        assert!(is_context_overflow(&refused(413, "")));
        for words in [
            "{\"error\":{\"code\":\"context_length_exceeded\"}}",
            "This model's MAXIMUM CONTEXT LENGTH is 8192 tokens",
            "prompt is too long: 220000 tokens > 200000 maximum",
            "Too many input tokens",
        ] {
            assert!(is_context_overflow(&refused(400, words)), "{words}");
        }
        assert!(!is_context_overflow(&refused(400, "invalid tool schema")));
        assert!(!is_context_overflow(&refused(429, "prompt is too long")));
        assert!(!is_context_overflow(&ProviderError::Cancelled));
    }

    #[test]
    fn zero_is_unbounded_and_a_bound_stops_at_its_own_count() {
        assert!(allows_step(0, 0));
        assert!(allows_step(0, u32::MAX));
        assert!(allows_step(2, 0) && allows_step(2, 1));
        assert!(!allows_step(2, 2));
    }

    #[test]
    fn a_new_machine_starts_from_history_plus_the_user_prompt() {
        let (_dir, tools) = tools();
        let machine = TurnMachine::new(TurnRequest {
            images: Vec::new(),
            model: "m".to_string(),
            prompt: "now".to_string(),
            history: vec![Message::user("before")],
            max_steps: 1,
            max_attempts: 1,
            cancel: crate::gateway::CancelToken::new(),
            tools,
        });
        assert_eq!(machine.messages().len(), 2);
        assert_eq!(machine.messages()[1].text(), "now");
        assert_eq!(machine.steps(), 0);
    }

    #[test]
    fn a_step_request_advertises_the_whole_registry_and_lets_the_model_choose() {
        let (_dir, tools) = tools();
        let machine = TurnMachine::new(TurnRequest {
            images: Vec::new(),
            model: "m".to_string(),
            prompt: "hi".to_string(),
            history: Vec::new(),
            max_steps: 1,
            max_attempts: 1,
            cancel: crate::gateway::CancelToken::new(),
            tools,
        });
        let request = machine.completion_request();
        assert_eq!(request.tools, Registry::builtin().advertisement());
        assert_eq!(request.tool_choice, ToolChoice::Auto);
    }

    #[test]
    fn the_prompt_reports_every_call_id_it_has_already_announced() {
        use crate::gateway::protocol::ToolCall;
        let (_dir, tools) = tools();
        let mut machine = TurnMachine::new(TurnRequest {
            images: Vec::new(),
            model: "m".to_string(),
            prompt: "hi".to_string(),
            history: Vec::new(),
            max_steps: 4,
            max_attempts: 1,
            cancel: crate::gateway::CancelToken::new(),
            tools,
        });
        assert!(machine.announced_call_ids().is_empty());
        machine.suffix.push(Message::assistant(
            None,
            vec![ToolCall {
                id: "c1".to_string(),
                name: "read_file".to_string(),
                input: serde_json::json!({}),
            }],
        ));
        machine
            .suffix
            .push(Message::tool_result("c1", "read_file", "x"));
        assert_eq!(machine.announced_call_ids(), ["c1"]);
    }

    #[test]
    fn a_request_runs_static_then_overlay_then_history_then_user_then_suffix() {
        use crate::gateway::protocol::Role;
        let (_dir, tools) = tools();
        let mut machine = TurnMachine::new(TurnRequest {
            images: Vec::new(),
            model: "m".to_string(),
            prompt: "now".to_string(),
            history: vec![
                Message::user("before"),
                Message::assistant(Some("ok"), vec![]),
            ],
            max_steps: 4,
            max_attempts: 1,
            cancel: crate::gateway::CancelToken::new(),
            tools,
        });
        machine.static_context = "STATIC".to_string();
        machine.overlay.push("OVERLAY".to_string());
        machine.suffix.push(Message::assistant(Some("mid"), vec![]));

        let messages = machine.messages();
        let roles: Vec<Role> = messages.iter().map(|message| message.role).collect();
        assert_eq!(
            roles,
            [
                Role::System,
                Role::System,
                Role::User,
                Role::Assistant,
                Role::User,
                Role::Assistant
            ]
        );
        assert_eq!(messages[0].text(), "STATIC");
        assert_eq!(messages[1].text(), "OVERLAY");
        assert_eq!(messages[4].text(), "now");
        assert_eq!(messages[5].text(), "mid");
    }

    #[test]
    fn a_machine_without_project_context_sends_no_system_message() {
        let (_dir, tools) = tools();
        let machine = TurnMachine::new(TurnRequest {
            images: Vec::new(),
            model: "m".to_string(),
            prompt: "now".to_string(),
            history: Vec::new(),
            max_steps: 1,
            max_attempts: 1,
            cancel: crate::gateway::CancelToken::new(),
            tools,
        })
        .with_context(crate::workspace::ProjectContext::none());
        assert_eq!(machine.messages().len(), 1);
    }

    #[test]
    fn a_tool_target_is_read_from_the_path_field_and_nowhere_else() {
        use serde_json::json;
        assert_eq!(
            target_path(json!({ "path": "src/a.rs" }).get("path")),
            Some(std::path::PathBuf::from("src/a.rs"))
        );
        assert_eq!(target_path(json!({}).get("path")), None);
        assert_eq!(target_path(json!({ "path": "  " }).get("path")), None);
        assert_eq!(target_path(json!({ "path": 7 }).get("path")), None);
        // A command is not a path, however path-shaped it looks.
        assert_eq!(
            target_path(json!({ "command": "ls src" }).get("path")),
            None
        );
    }

    #[test]
    fn an_absent_server_delay_backs_off_exponentially_up_to_the_cap() {
        assert_eq!(retry_delay(1, None), Duration::from_millis(250));
        assert_eq!(retry_delay(2, None), Duration::from_millis(500));
        assert_eq!(retry_delay(3, None), Duration::from_secs(1));
        assert_eq!(retry_delay(4, None), Duration::from_secs(2));
        // The cap holds however many attempts a caller allows, and the doubling
        // never overflows into a short wait.
        assert_eq!(retry_delay(9, None), MAX_RETRY_DELAY);
        assert_eq!(retry_delay(u32::MAX, None), MAX_RETRY_DELAY);
    }

    #[test]
    fn a_server_delay_outranks_the_local_backoff_and_is_still_capped() {
        // Shorter than the local guess: the server knows better in both
        // directions, so the turn does not sit out its own backoff.
        assert_eq!(
            retry_delay(3, Some(Duration::from_millis(100))),
            Duration::from_millis(100)
        );
        assert_eq!(
            retry_delay(1, Some(Duration::from_secs(2))),
            Duration::from_secs(2)
        );
        // Longer than the cap: bounded, not obeyed.
        assert_eq!(
            retry_delay(1, Some(Duration::from_secs(600))),
            MAX_RETRY_DELAY
        );
        assert_eq!(retry_delay(1, Some(Duration::MAX)), MAX_RETRY_DELAY);
        assert_eq!(retry_delay(2, Some(Duration::ZERO)), Duration::ZERO);
    }

    #[tokio::test]
    async fn a_wait_runs_for_its_full_delay_when_nothing_cancels_it() {
        let cancel = crate::gateway::CancelToken::new();
        let started = Instant::now();
        wait_before_retry(Duration::from_millis(120), &cancel)
            .await
            .expect("an uncancelled wait completes");
        assert!(
            started.elapsed() >= Duration::from_millis(110),
            "waited only {:?}",
            started.elapsed()
        );
    }

    #[tokio::test]
    async fn a_wait_stops_as_soon_as_the_turn_is_cancelled() {
        let cancel = crate::gateway::CancelToken::new();
        let watcher = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(40)).await;
            watcher.cancel();
        });

        let started = Instant::now();
        let err = wait_before_retry(Duration::from_secs(30), &cancel)
            .await
            .expect_err("a cancelled wait must not run to its deadline");
        assert!(matches!(err, TurnError::Cancelled));
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "the wait ignored cancellation for {:?}",
            started.elapsed()
        );
    }

    #[tokio::test]
    async fn a_wait_that_is_already_cancelled_never_sleeps() {
        let cancel = crate::gateway::CancelToken::new();
        cancel.cancel();
        let started = Instant::now();
        assert!(matches!(
            wait_before_retry(Duration::from_secs(30), &cancel).await,
            Err(TurnError::Cancelled)
        ));
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}
