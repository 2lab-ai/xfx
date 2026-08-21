//! What a turn is asked to do, what it produced, and how it can fail.

use std::fmt;
use std::io;

use crate::gateway::protocol::{FinishReason, Message, Usage};
use crate::gateway::{CancelToken, ProviderError};
use crate::tools::ToolContext;

/// One user request, with the bounds it must run inside.
///
/// A turn owns its own limits rather than reading them from global state, so a
/// test can bound a turn exactly and the interactive shell can run two turns
/// with different budgets in one process.
#[derive(Debug, Clone)]
pub struct TurnRequest {
    pub model: String,
    /// The user's message for this turn.
    pub prompt: String,
    /// Durable history, oldest first. Empty until sessions exist.
    pub history: Vec<Message>,
    /// The most model steps this turn may take. `0` means unbounded, matching
    /// the configured semantics
    /// (`vercel-labs/fx@580a0c5d src/core/config/agent_steps.zig:3-31`).
    pub max_steps: u32,
    /// The most transport attempts one step may spend.
    pub max_attempts: u32,
    pub cancel: CancelToken,
    /// Where this turn's tool calls may read, and the bounds they run under.
    ///
    /// The registry itself is a compile-time constant, so a turn chooses the
    /// *scope* of its tools, never their identity: two turns in one process
    /// cannot advertise different tool sets.
    pub tools: ToolContext,
}

/// What a completed turn produced.
#[derive(Debug, Clone, PartialEq)]
pub struct TurnOutcome {
    /// The assistant's answer: every delta, concatenated.
    pub output: String,
    /// How many model steps ran.
    pub steps: u32,
    pub usage: Usage,
    pub finish_reason: FinishReason,
}

/// Why a turn did not complete.
///
/// The `Display` text is what the user sees in the `error` event, so each
/// variant says what happened rather than naming an internal state.
#[derive(Debug)]
pub enum TurnError {
    /// The transport failed and could not be safely replayed.
    Provider(ProviderError),
    /// The model asked to run a tool this build does not advertise.
    ///
    /// Not a tool result: fxr told the model exactly which tools exist, so a
    /// call outside that set means the exchange is running on a premise fxr
    /// cannot correct from inside a result.
    ToolCallUnsupported { tool: String },
    /// Two tool calls in one step claimed the same identifier, so their results
    /// could not be told apart.
    DuplicateToolCallId { call_id: String },
    /// The model hit its output limit while writing a tool call, so the
    /// arguments may be cut short. They are not run.
    ToolCallTruncated { tool: String },
    /// The model said it was calling tools and then named none.
    EmptyToolCallFinish,
    /// The provider reported its own failure as the terminal state.
    ProviderFailure { detail: String },
    /// The step budget ran out before the turn reached a terminal state.
    StepLimit { limit: u32 },
    /// The attempt budget was exhausted, or was zero to begin with.
    AttemptLimit { limit: u32 },
    /// The turn was cancelled.
    Cancelled,
    /// Turn events could not be written.
    Sink(io::Error),
    /// The turn was already finalized; it cannot run twice.
    AlreadyFinalized,
}

impl fmt::Display for TurnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Provider(err) => write!(f, "{err}"),
            Self::ToolCallUnsupported { tool } => write!(
                f,
                "the model asked to run the `{tool}` tool, which this build does not advertise; \
                 fxr will not run a tool it did not offer"
            ),
            Self::DuplicateToolCallId { call_id } => write!(
                f,
                "the model made two tool calls with the id `{call_id}`, so their results could \
                 not be told apart; none were run"
            ),
            Self::ToolCallTruncated { tool } => write!(
                f,
                "the model reached its output limit while calling `{tool}`, so the arguments may \
                 be incomplete and were not run"
            ),
            Self::EmptyToolCallFinish => write!(
                f,
                "the model finished with `tool-calls` but named no tool to call"
            ),
            Self::ProviderFailure { detail } => write!(f, "the provider failed: {detail}"),
            Self::StepLimit { limit } => {
                write!(f, "the turn reached its limit of {limit} model steps")
            }
            Self::AttemptLimit { limit } => write!(
                f,
                "the turn reached its limit of {limit} Gateway attempts for one step"
            ),
            Self::Cancelled => write!(f, "the turn was cancelled"),
            Self::Sink(err) => write!(f, "cannot write turn output: {err}"),
            Self::AlreadyFinalized => write!(f, "this turn has already finished"),
        }
    }
}

impl std::error::Error for TurnError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Provider(err) => Some(err),
            Self::Sink(err) => Some(err),
            _ => None,
        }
    }
}

impl From<ProviderError> for TurnError {
    fn from(err: ProviderError) -> Self {
        match err {
            // Cancellation is a turn outcome, not a transport detail.
            ProviderError::Cancelled => Self::Cancelled,
            other => Self::Provider(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cancelled_transport_is_reported_as_a_cancelled_turn() {
        assert!(matches!(
            TurnError::from(ProviderError::Cancelled),
            TurnError::Cancelled
        ));
    }

    #[test]
    fn an_unsupported_tool_call_names_the_tool_and_refuses_to_fake_it() {
        let message = TurnError::ToolCallUnsupported {
            tool: "write_file".to_string(),
        }
        .to_string();
        assert!(message.contains("write_file"), "{message}");
        assert!(message.contains("does not advertise"), "{message}");
    }

    #[test]
    fn a_duplicate_call_id_says_that_nothing_ran() {
        let message = TurnError::DuplicateToolCallId {
            call_id: "c1".to_string(),
        }
        .to_string();
        assert!(message.contains("c1"), "{message}");
        assert!(message.contains("none were run"), "{message}");
    }

    #[test]
    fn a_truncated_tool_call_is_reported_as_unrun_rather_than_attempted() {
        let message = TurnError::ToolCallTruncated {
            tool: "read_file".to_string(),
        }
        .to_string();
        assert!(message.contains("read_file"), "{message}");
        assert!(message.contains("were not run"), "{message}");
    }
}
