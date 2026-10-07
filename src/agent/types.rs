//! What a turn is asked to do, what it produced, and how it can fail.

use std::fmt;
use std::io;

use crate::gateway::protocol::{FinishReason, Message, Usage};
use crate::gateway::{CancelToken, ProviderError};
use crate::session::SessionEvent;
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
    /// The images it carries, in the order they were attached: sent after its
    /// text on the user message, and recorded with it
    /// ([`TurnJournal::attach_images`]). Empty for a prompt with none.
    pub images: Vec<crate::image::Image>,
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

/// Where a turn's durable evidence goes.
///
/// The turn emits typed [`SessionEvent`]s and knows nothing about files: the
/// session store implements this, and so does a recorder in a test. Three
/// decisions are deliberate.
///
/// **It returns nothing.** A turn's outcome is what the user asked for, and it
/// must not change because a log could not be written. A persistence failure is
/// remembered by the implementation and reported by the caller alongside the
/// answer, which is the only honest report: the answer did arrive, and it was
/// not recorded.
///
/// **It is called as things happen, not once at the end.** A turn that is
/// interrupted -- by Ctrl-C, by a step limit, by a revoked authority -- has
/// still done real work, and the evidence of that work is exactly what a resume
/// needs. Events accumulated in memory until a tidy ending would be the events
/// most worth keeping and the ones most likely to be lost.
///
/// **No method has a default.** A journal that wraps another -- one that
/// rewrites a record, one that tees the turn to a surface -- answers each of
/// them, by forwarding or on purpose; a default would let a wrapper written
/// later drop the inner journal's answer without a word, as one dropped
/// [`Self::state`] and with it every compaction between a turn's steps. A
/// method added here is a compile error in every journal until each says
/// what it does with it. One that keeps no log answers that it has none.
///
/// So a wrapper that forwards only the record does not build:
///
/// ```compile_fail,E0046
/// struct Wrapper<'a>(&'a mut dyn xfx::agent::TurnJournal);
///
/// impl xfx::agent::TurnJournal for Wrapper<'_> {
///     fn record(&mut self, event: xfx::session::SessionEvent) {
///         self.0.record(event);
///     }
/// }
/// ```
pub trait TurnJournal {
    fn record(&mut self, event: SessionEvent);

    /// The change a call made, for a surface that shows it beside the log --
    /// the TUI's full transcript (`crate::tui`'s `full_transcript::Teed`).
    ///
    /// Handed over after the result's record, which carries the same change
    /// for a resumed session (`SessionEvent::ToolResult`'s `change`); this is
    /// how a surface watching the turn gets it as it happens. A journal that
    /// only keeps the log needs nothing from it.
    fn change_made(&mut self, call_id: &str, change: &crate::permission::ApprovalDiff);

    /// The images the user message just recorded carries.
    ///
    /// **A method rather than an event**, because recording them is two
    /// things in one order: their bytes saved beside the log, then the record
    /// that names them ([`crate::session::SessionRecorder::attach_images`]). A
    /// journal that keeps no log keeps no images either.
    ///
    /// Answers the directory their copies were saved in -- each under
    /// [`crate::image::Image::snapshot_name`] -- or `None` when none was.
    fn attach_images(&mut self, images: &[crate::image::Image]) -> Option<std::path::PathBuf>;

    /// The images the result of `call_id` carried, recorded with it so later
    /// requests send them again ([`crate::session::SessionRecorder::attach_tool_images`]).
    /// Called right after that result's [`SessionEvent::ToolResult`], and only
    /// for a result that carried any.
    fn attach_tool_images(&mut self, call_id: &str, images: &[crate::image::ToolImage]);

    /// The session as the log now holds it, this turn's steps so far
    /// included, for a journal that keeps one: what a compaction in the
    /// middle of a turn plans from ([`StepCompactor`]). `None` for one that
    /// keeps no log.
    fn state(&self) -> Option<&crate::session::DurableState>;
}

/// What a turn asks before each model step: whether its conversation is to be
/// compacted first (upstream's per-step check, `orchestrator.zig:7286-7322`).
///
/// The turn measures the request it is about to send and hands the answer
/// here; an implementation that compacts records the checkpoint through the
/// journal and returns the history the next request carries instead. It is
/// also asked, with `overflow` set, after the provider refused a request as
/// too large -- once per turn (`provider_overflow`, `:8099-8124`).
#[async_trait::async_trait(?Send)]
pub trait StepCompactor {
    /// `Ok(None)` when nothing was compacted, and the request goes as it was.
    async fn before_step(
        &mut self,
        check: StepCheck<'_>,
    ) -> Result<Option<StepCompaction>, TurnError>;

    /// What its notes requests cost since it was last asked, and whether one
    /// was lost after it was sent: the turn's `/usage` record carries them.
    fn spent(&mut self) -> (Vec<crate::session::RequestUsage>, bool) {
        (Vec::new(), false)
    }

    /// The provider counted `input_tokens` for the request measured as
    /// `request` and then sent, which later measurements are scaled by
    /// (`storeRequestTokenCalibration`, `orchestrator.zig:8873-8885`).
    fn answered(&mut self, _request: &crate::agent::compact::RequestCost, _input_tokens: u64) {}

    /// When a compaction it ran was stopped and stopped the turn with it, the
    /// sentence the person was shown for it: the turn's conclusion records it
    /// as a stop the compaction took (`persistCompactionInterruptedTurnOnce`,
    /// `orchestrator.zig:7413-7423`; `cancellation_origin`,
    /// `runtime/interruption.zig:51-77`).
    fn stopped(&self) -> Option<String> {
        None
    }
}

/// What a compaction before a step did to the request.
#[derive(Debug, Clone, PartialEq)]
pub struct StepCompaction {
    /// What the turn carries before its own message from now on: the
    /// compacted form, and the turns after it but the one in progress.
    pub history: Vec<crate::gateway::protocol::Message>,
    /// How many of the turn's own exchanges -- a step that called tools, with
    /// its results -- the compacted form now holds, counted from the turn's
    /// start. The turn sends its steps from the next one on
    /// (`retainedMessageOffset`, `orchestrator.zig:7432-7433`).
    pub exchanges: usize,
}

/// One step's question to a [`StepCompactor`].
pub struct StepCheck<'a> {
    /// The request about to be sent, as measured (`measureProviderRequest`,
    /// `prompt_context.zig:71-151`): its body's length, its text by the token
    /// estimate with images left out, and which images it carries.
    pub request: crate::agent::compact::RequestCost,
    /// The same request with no messages but its instructions, measured the
    /// same way: the part compaction cannot shrink (`request_fixed_tokens`,
    /// `orchestrator.zig:7307-7316`). `None` when it could not be built.
    pub fixed: Option<crate::agent::compact::RequestCost>,
    /// The request itself, which a notes request may follow so the provider
    /// can reuse what it cached of it (`CompactorCaller.conversation`,
    /// `orchestrator.zig:7390-7393`).
    pub conversation: &'a crate::gateway::protocol::CompletionRequest,
    /// The provider refused the last request as too large.
    pub overflow: bool,
    /// Whether anything new can be compacted: not straight after a
    /// compaction, before the request it rebuilt has gone
    /// (`has_new_compactable_context`, `orchestrator.zig:7304`).
    pub compactable: bool,
    /// How many model steps this turn has run.
    pub step: u32,
    pub provider: &'a dyn crate::gateway::Provider,
    pub journal: &'a mut dyn TurnJournal,
}

/// A journal that keeps nothing.
///
/// What `--no-save` and every unpersisted test turn run against. It exists so
/// "do not record this" is a real implementation rather than an `Option` every
/// call site has to remember to check.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoJournal;

impl TurnJournal for NoJournal {
    fn record(&mut self, _event: SessionEvent) {}

    fn change_made(&mut self, _call_id: &str, _change: &crate::permission::ApprovalDiff) {}

    fn attach_images(&mut self, _images: &[crate::image::Image]) -> Option<std::path::PathBuf> {
        None
    }

    fn attach_tool_images(&mut self, _call_id: &str, _images: &[crate::image::ToolImage]) {}

    fn state(&self) -> Option<&crate::session::DurableState> {
        None
    }
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
    /// Not a tool result: xfx told the model exactly which tools exist, so a
    /// call outside that set means the exchange is running on a premise xfx
    /// cannot correct from inside a result.
    ToolCallUnsupported { tool: String },
    /// Two tool calls in one step claimed the same identifier, so their results
    /// could not be told apart.
    DuplicateToolCallId { call_id: String },
    /// The model hit its output limit while writing a tool call, so the
    /// arguments may be cut short. They are not run.
    ToolCallTruncated { tool: String },
    /// A tool was authorized and then the world it was authorized against
    /// changed before it could run.
    ///
    /// Terminal rather than a tool result: the exchange rests on a fact that
    /// stopped being true, and offering the model a retry would let whoever won
    /// that race keep racing.
    ToolAuthorityRevoked { tool: String, detail: String },
    /// The model said it was calling tools and then named none.
    EmptyToolCallFinish,
    /// The provider reported its own failure as the terminal state.
    ProviderFailure { detail: String },
    /// The step budget ran out before the turn reached a terminal state.
    StepLimit { limit: u32 },
    /// The attempt budget was exhausted, or was zero to begin with.
    AttemptLimit { limit: u32 },
    /// The request is too large for the model's window even with
    /// everything before it compacted (`ContextCapacityExceeded`,
    /// `orchestrator.zig:7344-7350`).
    ContextNotReduced { reason: &'static str },
    /// A compaction the turn needed did not finish: its notes could not be
    /// written, or the log would not take it. `reason` is upstream's error
    /// name. **The row says what went wrong and the turn says nothing**
    /// (`compactionErrorHandled`, `app_process_runtime.zig:10-17`): the turn
    /// keeps the steps it ran, and concludes with no error of its own
    /// (`finishExecutionOnlyFailureIfNeeded`, `finalization.zig:213-246`).
    CompactionEnded { reason: &'static str },
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
                 xfx will not run a tool it did not offer"
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
            Self::ToolAuthorityRevoked { tool, detail } => write!(
                f,
                "`{tool}` was authorized and then the filesystem changed underneath it, so the \
                 turn stopped without making the change: {detail}"
            ),
            Self::EmptyToolCallFinish => write!(
                f,
                "the model finished with `tool-calls` but named no tool to call"
            ),
            Self::ProviderFailure { detail } => write!(f, "the provider failed: {detail}"),
            Self::ContextNotReduced { reason } => write!(f, "request failed: {reason}"),
            Self::CompactionEnded { reason } => write!(f, "the turn stopped ({reason})"),
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
