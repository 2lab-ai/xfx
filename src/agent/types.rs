//! What a turn is asked to do, what it produced, and how it can fail.

use std::fmt;
use std::io;

use crate::gateway::protocol::{FinishReason, Message, Usage};
use crate::gateway::{CancelToken, ProviderError};

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
    /// The model asked to run a tool. fxr advertises no tools yet, so there is
    /// nothing to run and nothing to pretend.
    ToolCallUnsupported { tool: String },
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
                "the model asked to run the `{tool}` tool, but this build advertises no tools \
                 and will not pretend to have run one"
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
        assert!(message.contains("no tools"), "{message}");
    }
}
