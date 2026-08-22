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
use std::time::{Duration, Instant};

use crate::gateway::protocol::{
    Completion, CompletionRequest, ContentPart, FinishReason, Message, ToolChoice,
};
use crate::gateway::{CancelToken, DeltaSink, Provider, ProviderError};
use crate::output::{Event, EventSink};
use crate::session::{RecordedToolCall, SessionEvent, TurnConclusion};
use crate::tools::Registry;
use crate::workspace::ProjectContext;

use super::types::{NoJournal, TurnError, TurnJournal, TurnOutcome, TurnRequest};

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
    /// Every assistant fragment this turn delivered, concatenated. A turn that
    /// spans several steps answers with all of them, because that is what the
    /// user watched arrive.
    output: String,
    steps: u32,
    finalized: bool,
}

impl TurnMachine {
    pub fn new(request: TurnRequest) -> Self {
        let history = request.history.clone();
        let user = Message::user(request.prompt.clone());
        Self {
            request,
            context: ProjectContext::none(),
            static_context: String::new(),
            overlay: Vec::new(),
            history,
            user,
            suffix: Vec::new(),
            output: String::new(),
            steps: 0,
            finalized: false,
        }
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
        messages.extend(self.suffix.iter().cloned());
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

        // The user's message opens the turn in the log before anything can go
        // wrong, so a turn that fails on its first request is still a turn that
        // was asked.
        journal.record(SessionEvent::UserMessage {
            text: self.request.prompt.clone(),
        });

        let result = self.drive(provider, events, journal).await;

        // Exactly one conclusion, whichever way the turn went.
        match &result {
            Ok(outcome) => {
                journal.record(SessionEvent::UsageRecorded {
                    input_tokens: outcome.usage.input_tokens,
                    output_tokens: outcome.usage.output_tokens,
                });
                journal.record(SessionEvent::TurnConcluded {
                    outcome: TurnConclusion::Final {
                        finish_reason: outcome.finish_reason.label().to_string(),
                        steps: outcome.steps,
                    },
                });
            }
            Err(err) => journal.record(SessionEvent::TurnConcluded {
                outcome: TurnConclusion::Interrupted {
                    reason: err.to_string(),
                },
            }),
        }

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
    ) -> Result<TurnOutcome, TurnError> {
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

            let completion = self.step(provider, events).await?;
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
                // The answering step's own text, recorded as the assistant
                // evidence of this turn. An empty one is not evidence.
                if !completion.text.is_empty() {
                    journal.record(SessionEvent::AssistantMessage {
                        text: completion.text.clone(),
                        tool_calls: Vec::new(),
                        // The final step has no continuation to satisfy, so the
                        // blocks are not needed -- but a resumed conversation
                        // replays this turn too, and Anthropic checks the
                        // signature there as well.
                        raw_content: completion.raw_content.clone(),
                    });
                }
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

            self.execute_tool_calls(&completion, events, journal)?;
        }
    }

    /// Runs one step's tool calls and extends the conversation with the result.
    ///
    /// Checks first, executions second. Both checks cover the whole step before
    /// anything runs, so a step with one bad call does not leave the workspace
    /// half-read and the conversation half-written.
    fn execute_tool_calls(
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
            raw_content: completion.raw_content.clone(),
        });

        for call in &completion.tool_calls {
            // Before the call is admitted, not after it runs: a rule about the
            // directory this call is about to touch is worth nothing once the
            // file has already been written.
            if let Some(target) = target_path(call.input.get("path")) {
                if let Some(delta) = self.context.admit_target(&target) {
                    self.overlay.push(delta);
                }
            }

            events
                .emit(&Event::ToolStart {
                    call_id: call.id.clone(),
                    tool: call.name.clone(),
                })
                .map_err(TurnError::Sink)?;

            // Checked above, so the registry cannot refuse the name here; the
            // error is still mapped rather than unwrapped, because a panic in a
            // turn would lose the terminal event the caller is owed.
            let result = registry
                .execute(call, &self.request.tools)
                .map_err(|err| TurnError::ToolCallUnsupported { tool: err.name })?;

            events
                .emit(&Event::ToolResult {
                    call_id: call.id.clone(),
                    tool: call.name.clone(),
                    ok: result.ok,
                    detail: result.detail.clone(),
                })
                .map_err(TurnError::Sink)?;

            journal.record(SessionEvent::ToolResult {
                call_id: call.id.clone(),
                tool: call.name.clone(),
                ok: result.ok,
                output: result.output.clone(),
            });
            self.suffix
                .push(Message::tool_result(&call.id, &call.name, result.output));

            // Almost every refusal goes back to the model, which can correct
            // itself. One does not: an authority that stopped describing the
            // filesystem means the premise of the exchange is void, so the turn
            // ends here rather than running the rest of the step or asking for
            // another one.
            if result.fatal {
                return Err(TurnError::ToolAuthorityRevoked {
                    tool: call.name.clone(),
                    detail: result.detail,
                });
            }
        }
        Ok(())
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

            let mut deltas = StepDeltas {
                events: &mut *events,
                delivered: false,
                error: None,
            };
            let outcome = provider.stream(&request, &mut deltas).await;
            let delivered = deltas.delivered;
            // A sink failure is reported as itself rather than as whatever the
            // provider made of it, so a closed stdout is never read as a
            // protocol failure.
            if let Some(err) = deltas.error.take() {
                return Err(TurnError::Sink(err));
            }

            match outcome {
                Ok(completion) => return Ok(completion),
                Err(err) => {
                    // `delivered` is the decisive fact: once part of an answer
                    // is in front of the user, a replay would produce a second
                    // answer to one question and bill for both.
                    if delivered || !err.is_replayable() || attempt >= self.request.max_attempts {
                        return Err(TurnError::from(err));
                    }
                    // The failure survived every "do not replay" test, so this
                    // is the one place a retry is allowed -- and it waits first.
                    self.wait_for_retry(&err, attempt).await?;
                }
            }
        }
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

    /// The request for the next model step.
    fn completion_request(&self) -> CompletionRequest {
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
            Ok(()) => Ok(()),
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
