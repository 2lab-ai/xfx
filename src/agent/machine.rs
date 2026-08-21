//! The bounded turn state machine.
//!
//! A turn is a loop over model steps. Each step builds one request from the
//! conversation so far, spends at most `max_attempts` transport attempts on it,
//! and then either finishes the turn or extends the conversation. This release
//! only implements the finishing half: a content-only completion writes its
//! deltas and returns. A tool call is refused, not simulated, because no tool is
//! advertised yet (`docs/parity.md`).
//!
//! Two rules are structural rather than conventional:
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

use std::io;

use crate::gateway::protocol::{Completion, CompletionRequest, FinishReason, Message, ToolChoice};
use crate::gateway::{DeltaSink, Provider};
use crate::output::{Event, EventSink};

use super::types::{TurnError, TurnOutcome, TurnRequest};

/// Whether a turn bounded by `limit` may take the step with index `step`.
///
/// `0` is unbounded (`vercel-labs/fx@580a0c5d src/core/config/agent_steps.zig:3-31`).
pub fn allows_step(limit: u32, step: u32) -> bool {
    limit == 0 || step < limit
}

/// Runs one turn to a terminal state, emitting exactly one terminal event.
pub async fn run_turn(
    request: TurnRequest,
    provider: &dyn Provider,
    events: &mut dyn EventSink,
) -> Result<TurnOutcome, TurnError> {
    TurnMachine::new(request).run(provider, events).await
}

/// One turn's conversation and bounds.
#[derive(Debug)]
pub struct TurnMachine {
    request: TurnRequest,
    /// The prompt sent to the model: history, then this turn's user message,
    /// then whatever the turn appends as it runs.
    messages: Vec<Message>,
    steps: u32,
    finalized: bool,
}

impl TurnMachine {
    pub fn new(request: TurnRequest) -> Self {
        let mut messages = request.history.clone();
        messages.push(Message::user(request.prompt.clone()));
        Self {
            request,
            messages,
            steps: 0,
            finalized: false,
        }
    }

    /// The prompt as it currently stands.
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// How many model steps have run.
    pub fn steps(&self) -> u32 {
        self.steps
    }

    /// Runs the turn and finalizes it exactly once.
    pub async fn run(
        &mut self,
        provider: &dyn Provider,
        events: &mut dyn EventSink,
    ) -> Result<TurnOutcome, TurnError> {
        if self.finalized {
            return Err(TurnError::AlreadyFinalized);
        }
        self.finalized = true;

        let result = self.drive(provider, events).await;
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
    /// Emits `assistant_delta` events only. Never emits a terminal event.
    ///
    /// This is deliberately straight-line rather than a loop. Every transition a
    /// content-only build knows about is terminal, so a loop here would be a
    /// loop that cannot turn over -- a shape that says "multi-step" while
    /// meaning "one step". The continuation that makes it a real loop is a tool
    /// result, and it arrives with the tool registry; the budget check below is
    /// already the place that loop will re-enter.
    async fn drive(
        &mut self,
        provider: &dyn Provider,
        events: &mut dyn EventSink,
    ) -> Result<TurnOutcome, TurnError> {
        if self.request.cancel.is_cancelled() {
            return Err(TurnError::Cancelled);
        }
        // The turn's own budget, consulted before spending a model call. A
        // content-only turn spends exactly one, so today this admits every
        // configured bound; it is the guard, not a formality, and it is where
        // the step loop re-enters.
        if !allows_step(self.request.max_steps, self.steps) {
            return Err(TurnError::StepLimit {
                limit: self.request.max_steps,
            });
        }

        let completion = self.step(provider, events).await?;
        self.steps += 1;

        // A tool call is refused before anything else is decided: whatever the
        // finish reason says, fxr cannot honor it and must not answer as if it
        // had.
        if let Some(call) = completion.tool_calls.first() {
            return Err(TurnError::ToolCallUnsupported {
                tool: call.name.clone(),
            });
        }
        match completion.finish_reason {
            FinishReason::ToolCalls => Err(TurnError::EmptyToolCallFinish),
            FinishReason::ProviderError => Err(TurnError::ProviderFailure {
                detail: completion
                    .provider_detail
                    .unwrap_or_else(|| "the provider gave no detail".to_string()),
            }),
            // `stop`, `length`, `content-filter`, and `other` all end the turn
            // with whatever text arrived. Only `length` and `content-filter`
            // mean the answer is cut short, and the finish reason travels in
            // the outcome so a caller can say so.
            FinishReason::Stop
            | FinishReason::Length
            | FinishReason::ContentFilter
            | FinishReason::Other => Ok(TurnOutcome {
                output: completion.text,
                steps: self.steps,
                usage: completion.usage,
                finish_reason: completion.finish_reason,
            }),
        }
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
                }
            }
        }
    }

    /// The request for the next model step.
    fn completion_request(&self) -> CompletionRequest {
        CompletionRequest {
            model: self.request.model.clone(),
            messages: self.messages.clone(),
            // No registry exists yet, so no tool is advertised and the model is
            // told it may not call one. An empty list with `auto` would invite
            // a call fxr would then have to refuse.
            tools: Vec::new(),
            tool_choice: ToolChoice::None,
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

    #[test]
    fn zero_is_unbounded_and_a_bound_stops_at_its_own_count() {
        assert!(allows_step(0, 0));
        assert!(allows_step(0, u32::MAX));
        assert!(allows_step(2, 0) && allows_step(2, 1));
        assert!(!allows_step(2, 2));
    }

    #[test]
    fn a_new_machine_starts_from_history_plus_the_user_prompt() {
        let machine = TurnMachine::new(TurnRequest {
            model: "m".to_string(),
            prompt: "now".to_string(),
            history: vec![Message::user("before")],
            max_steps: 1,
            max_attempts: 1,
            cancel: crate::gateway::CancelToken::new(),
        });
        assert_eq!(machine.messages().len(), 2);
        assert_eq!(machine.messages()[1].text(), "now");
        assert_eq!(machine.steps(), 0);
    }

    #[test]
    fn a_step_request_advertises_no_tools_and_forbids_calling_one() {
        let machine = TurnMachine::new(TurnRequest {
            model: "m".to_string(),
            prompt: "hi".to_string(),
            history: Vec::new(),
            max_steps: 1,
            max_attempts: 1,
            cancel: crate::gateway::CancelToken::new(),
        });
        let request = machine.completion_request();
        assert!(request.tools.is_empty());
        assert_eq!(request.tool_choice, ToolChoice::None);
    }
}
