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
//! - **No immediate replay.** A retry that fires the instant the last one failed
//!   is not a retry, it is a second request to a server that just said it could
//!   not take one. The turn waits, preferring the server's own `Retry-After`
//!   over its own guess, and the wait is interruptible.

use std::io;
use std::time::{Duration, Instant};

use crate::gateway::protocol::{Completion, CompletionRequest, FinishReason, Message, ToolChoice};
use crate::gateway::{CancelToken, DeltaSink, Provider, ProviderError};
use crate::output::{Event, EventSink};

use super::types::{TurnError, TurnOutcome, TurnRequest};

/// The first backoff, doubled per failed attempt when the server names no delay.
///
/// An fxr value. Upstream backs off linearly from 150 ms
/// (`vercel-labs/fx@580a0c5d src/gateway/client.zig:180`, `:1827-1830`); fxr
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
