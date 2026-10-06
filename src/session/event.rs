//! What a session records, and the exact bytes one record occupies.
//!
//! A session is an append-only log of typed events, one JSON object per line.
//! Nothing in this module touches the filesystem: a frame can be encoded,
//! decoded, and validated by a test that never opens a file, and the on-disk
//! format is therefore a thing that can be argued about in isolation.
//!
//! Three properties are structural rather than conventional:
//!
//! - **Every frame carries its own position.** Schema version, log generation,
//!   sequence number, event id, and timestamp travel with the payload, so a
//!   reader can prove a log is contiguous without consulting anything else
//!   (`vercel-labs/fx@580a0c5d src/core/session/session_event.zig:169-202`).
//! - **Encoding is deterministic.** Field order is declaration order and the
//!   line ends in exactly one newline, so the same event always produces the
//!   same bytes and a digest over the log means something.
//! - **The payload is typed, and it never carries xfx's own Gateway
//!   credential.** There is no free-form "state blob" event and no field for the
//!   token xfx authenticated with; what can be written is exactly the list in
//!   [`SessionEvent`]. That is a promise about *xfx's* secret and not about the
//!   reader's: [`SessionEvent::ToolResult`] stores a file's contents or a
//!   command's output verbatim, as owner-only plaintext, so a secret the model
//!   was asked to read is on disk until the session is deleted, and `--no-save`
//!   is the only way to record nothing at all.

use std::collections::hash_map::RandomState;
use std::fmt;
use std::hash::BuildHasher;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// The event frame schema this build writes and is willing to read.
///
/// A frame that names any other version is refused rather than guessed at: an
/// older xfx must not half-read a newer log and report the prefix as the whole
/// conversation.
///
/// **A new variant does not bump this, and the asymmetry is deliberate.**
/// [`EventEnvelope::validate`] compares versions for equality, so bumping would
/// make every log written before the bump unreadable by the new binary -- a
/// total break, to avoid a partial one. Adding [`SessionEvent::ToolFeedback`]
/// under version 1 instead means a new binary reads every old log, while an
/// older binary reading a log that contains a `tool_feedback` frame refuses that
/// frame as an unknown variant. Preview and stable mix forward, not backward,
/// across a session whose approvals were amended.
pub const EVENT_SCHEMA_VERSION: u32 = 1;

/// The most bytes one encoded frame may occupy.
///
/// A tool result is already capped at 256 KiB by the registry, so this is a
/// backstop against a pathological payload rather than the working bound.
pub const MAX_EVENT_FRAME_BYTES: usize = 1024 * 1024;

/// One model tool call, as it was actually requested.
///
/// The arguments are kept verbatim so a resumed conversation shows the model
/// what it asked for rather than xfx's paraphrase of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordedToolCall {
    pub id: String,
    pub name: String,
    pub input: Value,
}

/// One image a user message carried, as the log remembers it: which one, where
/// it came from, and how to know its saved copy when it is read back.
///
/// **References, never bytes.** The bytes are in a file of the session's own
/// -- `images/` + [`crate::image::snapshot_name`] of `id` and `sha256` -- which
/// a frame of at most [`MAX_EVENT_FRAME_BYTES`] could not hold, and the name of
/// that file is **derived** from these fields rather than written in them, so a
/// log cannot point a resume at any other file. Upstream records the same
/// facts on its user event (`images: [{id, path, media_type, snapshot_path,
/// snapshot_sha256}]`, `vercel-labs/fx@c1db9191
/// src/core/session/session_store.zig:102-118`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordedImage {
    /// The number the prompt's `[Image #N]` names it by.
    pub id: u32,
    /// The file it was read from, or the name of its saved copy for one that
    /// came off the clipboard.
    pub path: String,
    /// Its MIME type, one of [`crate::image::MediaType`]'s.
    pub media_type: String,
    /// The SHA-256 of its bytes, lowercase hex.
    pub sha256: String,
    /// How many bytes it is.
    pub bytes: u64,
}

/// One model request a turn made, as the provider accounted for it.
///
/// What `/usage` adds up (`GenerationFact`, `vercel-labs/fx@c1db9191
/// src/core/session/usage_report.zig:125-165`). **`cost` is the provider's
/// own number**, never one xfx worked out: upstream spends exactly what the
/// Gateway's `providerMetadata.gateway.cost` says (`src/gateway/client.zig:3352-3359`)
/// and reads no price list to do it, so neither does this.
///
/// A request with no `cost` is one the provider did not account for -- a
/// Gateway answer without its billing metadata. It is kept, because its
/// existence is the fact that some spend is unknown, and counted nowhere:
/// upstream's dashboard counts a generation only once its cost is known
/// (`session_usage.zig:110-175`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RequestUsage {
    /// When the request was billed: the Gateway's own `response-metadata`
    /// timestamp when it sent one, else when the answer arrived here. Which
    /// window of `/usage` the request falls in.
    pub at_ms: i64,
    /// How long the request took, retries included: `/usage`'s `API` time.
    pub duration_ms: u64,
    /// What the request was billed as: the Gateway's canonical slug when it
    /// said one, else the model that was asked for.
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// Part of `input_tokens`, never in addition to it.
    #[serde(default)]
    pub cache_read_tokens: u64,
    /// Part of `input_tokens`, never in addition to it.
    #[serde(default)]
    pub cache_write_tokens: u64,
    /// Part of `output_tokens`. `None` is "the provider did not say".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u64>,
    /// What it cost, in US dollars, as the provider said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<f64>,
    /// The Gateway's id for the generation (`gen_…`), which is what keeps one
    /// request from being counted twice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation_id: Option<String>,
}

/// For `skip_serializing_if`: a flag that is off is a flag that is not written.
fn is_false(value: &bool) -> bool {
    !*value
}

/// For `skip_serializing_if`: a count of nothing is not written.
fn is_zero(value: &u64) -> bool {
    *value == 0
}

/// How a turn ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TurnConclusion {
    /// The turn reached a terminal completion.
    Final { finish_reason: String, steps: u32 },
    /// The turn stopped before it finished: cancelled, out of budget, or
    /// refused. `reason` is the same sentence the user was shown.
    Interrupted { reason: String },
}

impl TurnConclusion {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Final { .. } => "final",
            Self::Interrupted { .. } => "interrupted",
        }
    }
}

/// Everything a session is allowed to remember.
///
/// The set is closed, and has no field for xfx's own Gateway credential, no
/// endpoint, and no environment capture. A session records what was asked, what
/// was done, and what it cost -- never what xfx authenticated with. What a tool
/// returned is recorded too, in [`SessionEvent::ToolResult`], so the reader's
/// own secrets can be in the log even though xfx's never are.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SessionEvent {
    /// The first event of every log. Establishes identity and both workspace
    /// roots (`session_event.zig:36-53`).
    SessionStarted {
        id: String,
        origin_workspace_root: String,
        workspace_root: String,
        model: String,
        permission_mode: String,
    },
    /// The session was deliberately moved to a different workspace
    /// (`session_event.zig:67-76`).
    WorkspaceRebound {
        previous_workspace_root: String,
        workspace_root: String,
    },
    /// A preference changed for later turns. Absent fields are unchanged.
    PreferencesChanged {
        model: Option<String>,
        permission_mode: Option<String>,
    },
    /// Which project-instruction files were in force for the turn that follows.
    ///
    /// Provenance and size only. The bytes are not persisted: project context is
    /// rediscovered on resume, so storing it would create a second, staler copy
    /// of a file that is already on disk.
    ProjectContextRecorded { sources: Vec<String>, bytes: u64 },
    /// The user's message. This is what opens a turn.
    UserMessage { text: String },
    /// The images the message just opened carries, in the order they were
    /// attached -- written immediately after it and before anything else of
    /// its turn, with their copies already saved beside the log.
    ///
    /// A frame of its own rather than a field on [`Self::UserMessage`], under
    /// schema 1 like [`Self::ToolFeedback`] and for its reason: a new binary
    /// reads every old log, and an older one refuses a log that holds this
    /// frame rather than resuming the conversation without the images its
    /// prompts name.
    ImagesAttached { images: Vec<RecordedImage> },
    /// One assistant step: its text, then the tools it asked for, in order.
    ///
    AssistantMessage {
        text: String,
        tool_calls: Vec<RecordedToolCall>,
        /// Anthropic Messages content blocks, verbatim, in arrival order.
        ///
        /// **Only ever written by the `anthropic_messages` wire** -- which is
        /// why an older record that has it can only have come from that wire,
        /// and why a second wire's replay state does not go here. Anthropic
        /// signs its reasoning blocks and verifies the signature when they come
        /// back in a continuation, so a rebuilt-from-text assistant turn is a
        /// 400 at the next step: xfx cannot reconstruct a signature it did not
        /// keep. Never displayed by any renderer; it exists to go back on the
        /// wire, and it can be large.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        raw_content: Vec<serde_json::Value>,
        /// OpenAI Responses replay items (`reasoning` with `encrypted_content`),
        /// verbatim, in arrival order.
        ///
        /// Disjoint from `raw_content` by construction: a turn writes at most
        /// one of the two. A separate field rather than a tagged shared one,
        /// because a tag's compatibility argument is **syntactic only** -- an
        /// older binary ignores an unknown tag, finds a non-empty `raw_content`,
        /// concludes "Anthropic blocks", and replays Responses items onto the
        /// Messages wire. Separated storage makes that binary see nothing at all
        /// and rebuild from text: degraded, never mis-wired.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        responses_state: Vec<serde_json::Value>,
        /// Which wire *and which authority* produced the state above.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        wire: Option<crate::provider::Wire>,
    },
    /// The evidence one tool call produced, correlated by `call_id`.
    ///
    /// `output` is what the tool actually returned -- a file's bytes, a
    /// command's output -- kept verbatim, which is why this is the one variant a
    /// reader's own secret can reach.
    ToolResult {
        call_id: String,
        tool: String,
        ok: bool,
        output: String,
    },
    /// What the user said when they answered the approval one call needed.
    ///
    /// Correlated by `call_id` and deliberately **not** a [`Self::UserMessage`]:
    /// a user message opens a turn in the reducer, and opening one in the middle
    /// of a tool batch would leave the batch's remaining results in a turn with
    /// no call ahead of them -- orphan results, refused on the next resume.
    ///
    /// Written only after every result of the step it belongs to, so a replayed
    /// prompt puts the sentence behind the result it was about. It is context,
    /// never authority: nothing reads it to widen a grant or re-run a call.
    ToolFeedback { call_id: String, text: String },
    /// An approval the user gave for the rest of the session.
    PermissionGrantRecorded { tool: String, target: String },
    /// `/permissions reset`: every approval recorded before this one is
    /// withdrawn, so a resume does not give it back.
    ///
    /// Upstream's reset clears its session grants (`permission_engine.clear`,
    /// `app_permission_runtime.zig:144-159`), which upstream never persists;
    /// xfx persists them ([`Self::PermissionGrantRecorded`]), so it persists
    /// their withdrawal. Added under [`EVENT_SCHEMA_VERSION`] 1 like
    /// [`Self::ToolFeedback`], for the reason given there: a new binary reads
    /// every old log, and an older binary refuses this frame as an unknown
    /// variant -- forward but not backward across a session that was reset.
    PermissionGrantsCleared,
    /// Tokens the provider reported for the turn that is ending. Added to the
    /// running totals; absent is "the provider did not say", not zero.
    ///
    /// The two totals are the **answering step's**, as they always were, and
    /// mean what they always meant: the context meter and a replayed turn's
    /// summary row read them. What `/usage` reads is beside them -- one entry
    /// per model request the turn made ([`RequestUsage`]), and what the turn
    /// changed on disk -- because a turn of three steps is three requests, and
    /// upstream bills, counts and windows each one
    /// (`vercel-labs/fx@c1db9191 src/core/session/usage_report.zig:125-165`).
    ///
    /// **Extended rather than joined by a new variant**, and every addition is
    /// skipped when empty: an older binary ignores a field it does not know in
    /// a variant it does, so a log written with these still opens there, and a
    /// turn that has none of them encodes to exactly the bytes it always did.
    /// A new variant would make every log with a turn in it unreadable to the
    /// binary before this one ([`EVENT_SCHEMA_VERSION`]).
    UsageRecorded {
        input_tokens: Option<u64>,
        output_tokens: Option<u64>,
        /// Every model request the turn completed, in order. Empty only on a
        /// frame written before requests were recorded, or on one that says
        /// the turn's only request was cut off (`unaccounted`): a turn that
        /// answered has at least one, and one that failed without either
        /// writes no frame. That is how `/usage` tells an older frame from a
        /// turn that cost nothing.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        requests: Vec<RequestUsage>,
        /// A request of this turn may have been billed without the provider
        /// saying what it cost: the stream broke, or was stopped, after the
        /// request had been sent (`DeliveryOutcome.ambiguous_delivery`,
        /// `session_usage.zig:43-53`).
        #[serde(default, skip_serializing_if = "is_false")]
        unaccounted: bool,
        /// Lines the turn's file changes added and removed, for `/usage`'s
        /// `Code +N · -N` (`recordCommittedLines`, `session_usage.zig:1584`).
        #[serde(default, skip_serializing_if = "is_zero")]
        lines_added: u64,
        #[serde(default, skip_serializing_if = "is_zero")]
        lines_removed: u64,
    },
    /// The turn ended. Exactly one per opened turn.
    TurnConcluded { outcome: TurnConclusion },
    /// The user named the session (`/rename <title>`).
    ///
    /// A title the user chose outranks the one a listing derives from the
    /// first prompt, and the last of these in a log is the one in force
    /// (`renameConversation`, `vercel-labs/fx@c1db9191
    /// src/core/session/session_log.zig:3298-3335`, which rewrites the title
    /// in the session's metadata). An event rather than a field the manifest
    /// alone carries, because the manifest is a projection of the log
    /// ([`super::store::SessionManifest`]): a title only the projection held
    /// would be the one fact a resume could not rebuild.
    ///
    /// Written under schema 1 for the reason [`Self::ToolFeedback`] is: a new
    /// binary reads every old log, and an older one refuses a log that holds
    /// this frame rather than half-reading it.
    SessionRenamed { title: String },
    /// The conversation was compacted (`/compact`, or a turn that reached the
    /// automatic compaction point): every turn opened at or before
    /// `covers_through_seq` is replaced, in what the model reads from here on,
    /// by `compacted` ([`super::checkpoint::render`]).
    ///
    /// Upstream's `context_checkpoint { covers_through_seq, summary }`
    /// (`vercel-labs/fx@c1db9191 src/core/session/session_event.zig:169-172`,
    /// `:182`), with the payload typed rather than a string behind a marker --
    /// see the module header on why there is no blob event here. The turns it
    /// covers stay in the log and on a resumed screen; only the next request
    /// carries the compacted form instead of them. The latest one in a log is
    /// the one in force, and each one carries everything the one before it
    /// kept.
    ///
    /// Written under schema 1 for the reason [`Self::ToolFeedback`] is: a new
    /// binary reads every old log, and an older one refuses a log that holds
    /// this frame rather than half-reading it -- which would be resuming with
    /// the whole history after the user compacted it.
    ContextCheckpoint {
        covers_through_seq: u64,
        compacted: super::checkpoint::Compacted,
    },
}

impl SessionEvent {
    /// The wire tag, for diagnostics that name what was refused.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::SessionStarted { .. } => "session_started",
            Self::WorkspaceRebound { .. } => "workspace_rebound",
            Self::PreferencesChanged { .. } => "preferences_changed",
            Self::ProjectContextRecorded { .. } => "project_context_recorded",
            Self::UserMessage { .. } => "user_message",
            Self::ImagesAttached { .. } => "images_attached",
            Self::AssistantMessage { .. } => "assistant_message",
            Self::ToolResult { .. } => "tool_result",
            Self::ToolFeedback { .. } => "tool_feedback",
            Self::PermissionGrantRecorded { .. } => "permission_grant_recorded",
            Self::PermissionGrantsCleared => "permission_grants_cleared",
            Self::UsageRecorded { .. } => "usage_recorded",
            Self::TurnConcluded { .. } => "turn_concluded",
            Self::SessionRenamed { .. } => "session_renamed",
            Self::ContextCheckpoint { .. } => "context_checkpoint",
        }
    }
}

/// One frame of the log: an event plus its position in it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventEnvelope {
    pub schema_version: u32,
    /// Identifies the log this frame belongs to. Every frame of one log carries
    /// the same value, so a frame copied in from elsewhere is detectable.
    pub log_generation: String,
    /// 1-based, contiguous. A gap is corruption, not a missing optional record.
    pub seq: u64,
    /// Unique within the log.
    pub event_id: String,
    pub timestamp_ms: i64,
    pub event: SessionEvent,
}

/// Why a frame could not be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameError {
    pub detail: String,
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.detail)
    }
}

impl std::error::Error for FrameError {}

impl EventEnvelope {
    /// The exact bytes of one log line, terminated by a single newline.
    pub fn encode(&self) -> Result<String, FrameError> {
        let mut line = serde_json::to_string(self).map_err(|err| FrameError {
            detail: format!("cannot encode a `{}` event: {err}", self.event.kind()),
        })?;
        line.push('\n');
        if line.len() > MAX_EVENT_FRAME_BYTES {
            return Err(FrameError {
                detail: format!(
                    "a `{}` event encodes to {} bytes, over the {MAX_EVENT_FRAME_BYTES}-byte frame limit",
                    self.event.kind(),
                    line.len()
                ),
            });
        }
        Ok(line)
    }

    /// Decodes one line, without its newline, and checks its own fields.
    pub fn decode(line: &str) -> Result<Self, FrameError> {
        let envelope: Self = serde_json::from_str(line).map_err(|err| FrameError {
            detail: format!("cannot decode an event frame: {err}"),
        })?;
        envelope.validate()?;
        Ok(envelope)
    }

    /// Checks the facts a frame asserts about itself.
    fn validate(&self) -> Result<(), FrameError> {
        if self.schema_version != EVENT_SCHEMA_VERSION {
            return Err(FrameError {
                detail: format!(
                    "event schema version {} is not the {EVENT_SCHEMA_VERSION} this build reads",
                    self.schema_version
                ),
            });
        }
        if self.seq == 0 {
            return Err(FrameError {
                detail: "an event sequence number starts at 1".to_string(),
            });
        }
        for (name, value) in [
            ("log_generation", &self.log_generation),
            ("event_id", &self.event_id),
        ] {
            if !is_identifier(value) {
                return Err(FrameError {
                    detail: format!("`{name}` must be 32 lowercase hex characters"),
                });
            }
        }
        Ok(())
    }
}

/// Whether `value` is the 32 lowercase hex characters an identifier is written as.
fn is_identifier(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// A fresh 128-bit identifier, written as 32 lowercase hex characters.
///
/// The entropy is the process's own randomly seeded hasher plus the current
/// nanosecond, the process id, and a monotonic counter, run through SHA-256. It
/// does not need to be unguessable -- an identifier is a correlation key inside
/// a private directory, never a capability -- but two of them must not collide
/// within a log, and a collision would be a refusal rather than silent reuse.
pub fn new_identifier() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    thread_local! {
        static SEED: RandomState = RandomState::new();
    }

    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_nanos())
        .unwrap_or(0);
    let seeded = SEED.with(|state| state.hash_one(counter));

    let mut hasher = Sha256::new();
    hasher.update(b"xfx:session-identifier:v1\0");
    hasher.update(seeded.to_be_bytes());
    hasher.update(nanos.to_be_bytes());
    hasher.update(std::process::id().to_be_bytes());
    hasher.update(counter.to_be_bytes());
    let digest = hasher.finalize();

    let mut out = String::with_capacity(32);
    for byte in &digest[..16] {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// The current wall clock in milliseconds since the Unix epoch.
pub fn system_now_ms() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(since) => i64::try_from(since.as_millis()).unwrap_or(i64::MAX),
        // A clock set before 1970 is not a reason to refuse to record a turn.
        Err(err) => -i64::try_from(err.duration().as_millis()).unwrap_or(i64::MAX),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn envelope(seq: u64, event: SessionEvent) -> EventEnvelope {
        EventEnvelope {
            schema_version: EVENT_SCHEMA_VERSION,
            log_generation: "0".repeat(32),
            seq,
            event_id: "1".repeat(32),
            timestamp_ms: 1_700_000_000_000,
            event,
        }
    }

    #[test]
    fn a_frame_is_one_line_and_round_trips() {
        let frame = envelope(
            1,
            SessionEvent::UserMessage {
                text: "line one\nline two".to_string(),
            },
        );
        let encoded = frame.encode().expect("encodes");
        assert!(encoded.ends_with('\n'));
        assert_eq!(encoded.matches('\n').count(), 1, "{encoded:?}");
        assert_eq!(
            EventEnvelope::decode(encoded.trim_end()).expect("decodes"),
            frame
        );
    }

    #[test]
    fn encoding_is_deterministic_and_starts_with_its_own_version() {
        let frame = envelope(
            2,
            SessionEvent::AssistantMessage {
                text: "hi".to_string(),
                tool_calls: vec![RecordedToolCall {
                    id: "c1".to_string(),
                    name: "read_file".to_string(),
                    input: json!({ "path": "a.txt" }),
                }],
                raw_content: Vec::new(),
                responses_state: Vec::new(),
                wire: None,
            },
        );
        assert_eq!(frame.encode().unwrap(), frame.encode().unwrap());
        assert!(
            frame
                .encode()
                .unwrap()
                .starts_with("{\"schema_version\":1,"),
            "{}",
            frame.encode().unwrap()
        );
    }

    #[test]
    fn a_frame_from_another_schema_version_is_refused_rather_than_read() {
        let mut frame = envelope(
            1,
            SessionEvent::UserMessage {
                text: "x".to_string(),
            },
        );
        frame.schema_version = 2;
        let line = serde_json::to_string(&frame).unwrap();
        let err = EventEnvelope::decode(&line).expect_err("a future schema is refused");
        assert!(err.to_string().contains("schema version 2"), "{err}");
    }

    #[test]
    fn a_frame_with_an_unknown_envelope_field_is_refused() {
        let line = r#"{"schema_version":1,"log_generation":"00000000000000000000000000000000","seq":1,"event_id":"11111111111111111111111111111111","timestamp_ms":0,"event":{"kind":"user_message","text":"x"},"extra":true}"#;
        assert!(EventEnvelope::decode(line).is_err());
    }

    #[test]
    fn an_unknown_event_kind_is_refused_rather_than_skipped() {
        let line = r#"{"schema_version":1,"log_generation":"00000000000000000000000000000000","seq":1,"event_id":"11111111111111111111111111111111","timestamp_ms":0,"event":{"kind":"credential_stored","value":"x"}}"#;
        assert!(EventEnvelope::decode(line).is_err());
    }

    #[test]
    fn an_identifier_is_thirty_two_lowercase_hex_characters_and_does_not_repeat() {
        let first = new_identifier();
        let second = new_identifier();
        assert!(is_identifier(&first), "{first}");
        assert!(is_identifier(&second), "{second}");
        assert_ne!(first, second);
        assert!(!is_identifier("ABCDEF00000000000000000000000000"));
        assert!(!is_identifier("abc"));
    }

    #[test]
    fn a_zero_sequence_number_is_refused() {
        let frame = envelope(
            0,
            SessionEvent::UserMessage {
                text: "x".to_string(),
            },
        );
        let line = serde_json::to_string(&frame).unwrap();
        assert!(EventEnvelope::decode(&line).is_err());
    }

    #[test]
    fn an_oversized_frame_is_refused_before_it_reaches_a_file() {
        let frame = envelope(
            1,
            SessionEvent::ToolResult {
                call_id: "c1".to_string(),
                tool: "read_file".to_string(),
                ok: true,
                output: "x".repeat(MAX_EVENT_FRAME_BYTES + 1),
            },
        );
        let err = frame.encode().expect_err("an oversized frame is refused");
        assert!(err.to_string().contains("frame limit"), "{err}");
    }

    #[test]
    fn every_event_kind_names_itself() {
        let events = [
            SessionEvent::SessionStarted {
                id: "s".to_string(),
                origin_workspace_root: "/w".to_string(),
                workspace_root: "/w".to_string(),
                model: "m".to_string(),
                permission_mode: "auto".to_string(),
            },
            SessionEvent::WorkspaceRebound {
                previous_workspace_root: "/w".to_string(),
                workspace_root: "/x".to_string(),
            },
            SessionEvent::PreferencesChanged {
                model: Some("m2".to_string()),
                permission_mode: None,
            },
            SessionEvent::ProjectContextRecorded {
                sources: Vec::new(),
                bytes: 0,
            },
            SessionEvent::UserMessage {
                text: "u".to_string(),
            },
            SessionEvent::AssistantMessage {
                text: "a".to_string(),
                tool_calls: Vec::new(),
                raw_content: Vec::new(),
                responses_state: Vec::new(),
                wire: None,
            },
            SessionEvent::ToolResult {
                call_id: "c".to_string(),
                tool: "t".to_string(),
                ok: true,
                output: "o".to_string(),
            },
            SessionEvent::ToolFeedback {
                call_id: "c".to_string(),
                text: "say it differently".to_string(),
            },
            SessionEvent::PermissionGrantRecorded {
                tool: "t".to_string(),
                target: "x".to_string(),
            },
            SessionEvent::PermissionGrantsCleared,
            SessionEvent::UsageRecorded {
                input_tokens: None,
                output_tokens: Some(1),
                requests: Vec::new(),
                unaccounted: false,
                lines_added: 0,
                lines_removed: 0,
            },
            SessionEvent::TurnConcluded {
                outcome: TurnConclusion::Interrupted {
                    reason: "cancelled".to_string(),
                },
            },
            SessionEvent::SessionRenamed {
                title: "My Title".to_string(),
            },
        ];
        for event in events {
            let kind = event.kind();
            let frame = envelope(1, event);
            let encoded = frame.encode().expect("encodes");
            assert!(
                encoded.contains(&format!("\"kind\":\"{kind}\"")),
                "{kind}: {encoded}"
            );
            assert_eq!(
                EventEnvelope::decode(encoded.trim_end()).expect("decodes"),
                frame
            );
        }
    }

    #[test]
    fn an_amendment_is_its_own_kind_and_not_a_user_message() {
        // The distinction is the whole point of the variant: a `user_message`
        // opens a turn when it is replayed, and one opened in the middle of a
        // tool batch orphans the results still to come.
        let amendment = SessionEvent::ToolFeedback {
            call_id: "c1".to_string(),
            text: "use tabs, not spaces".to_string(),
        };
        assert_eq!(amendment.kind(), "tool_feedback");
        assert_ne!(
            amendment.kind(),
            SessionEvent::UserMessage {
                text: "use tabs, not spaces".to_string()
            }
            .kind()
        );

        // Written under schema 1, so a new binary reads every old log. The
        // frame keeps its correlation, which is all that says which call the
        // sentence answered once it sits behind every result of its step.
        let frame = envelope(1, amendment);
        let encoded = frame.encode().expect("encodes");
        assert!(encoded.starts_with("{\"schema_version\":1,"), "{encoded}");
        assert!(encoded.contains("\"call_id\":\"c1\""), "{encoded}");
        assert_eq!(
            EventEnvelope::decode(encoded.trim_end()).expect("decodes"),
            frame
        );
    }

    #[test]
    fn a_rename_is_one_frame_with_the_title_and_nothing_else() {
        // The shape `/rename` writes, byte for byte: the kind and the title.
        let frame = envelope(
            7,
            SessionEvent::SessionRenamed {
                title: "My Title".to_string(),
            },
        );
        let encoded = frame.encode().expect("encodes");
        assert!(
            encoded.contains(r#""event":{"kind":"session_renamed","title":"My Title"}}"#),
            "{encoded}"
        );
        assert_eq!(
            EventEnvelope::decode(encoded.trim_end()).expect("decodes"),
            frame
        );
    }

    #[test]
    fn a_usage_frame_without_requests_encodes_exactly_as_it_always_did() {
        // The shape a binary before `/usage` wrote, byte for byte: the two
        // totals and nothing else. A turn written by this build has requests,
        // so this is the frame only an older log holds -- and it must still
        // decode, and say nothing new when written back.
        let line = r#"{"schema_version":1,"log_generation":"00000000000000000000000000000000","seq":3,"event_id":"11111111111111111111111111111111","timestamp_ms":1700000000000,"event":{"kind":"usage_recorded","input_tokens":11,"output_tokens":7}}"#;
        let frame = EventEnvelope::decode(line).expect("an older usage frame decodes");
        assert_eq!(
            frame.event,
            SessionEvent::UsageRecorded {
                input_tokens: Some(11),
                output_tokens: Some(7),
                requests: Vec::new(),
                unaccounted: false,
                lines_added: 0,
                lines_removed: 0,
            }
        );
        assert_eq!(frame.encode().expect("encodes").trim_end(), line);
    }

    #[test]
    fn a_usage_frame_keeps_each_request_and_what_the_turn_changed() {
        let frame = envelope(
            4,
            SessionEvent::UsageRecorded {
                input_tokens: Some(52000),
                output_tokens: Some(3100),
                requests: vec![RequestUsage {
                    at_ms: 1_700_000_000_123,
                    duration_ms: 812,
                    model: "anthropic/claude-opus-5".to_string(),
                    input_tokens: 52000,
                    output_tokens: 3100,
                    cache_read_tokens: 30000,
                    cache_write_tokens: 0,
                    reasoning_tokens: Some(900),
                    cost: Some(0.421),
                    generation_id: Some("gen_01JABCDEFGHJKMNPQRSTVWXYZ1".to_string()),
                }],
                unaccounted: true,
                lines_added: 3,
                lines_removed: 1,
            },
        );
        let encoded = frame.encode().expect("encodes");
        assert!(
            encoded.contains(
                r#""requests":[{"at_ms":1700000000123,"duration_ms":812,"model":"anthropic/claude-opus-5","input_tokens":52000,"output_tokens":3100,"cache_read_tokens":30000,"cache_write_tokens":0,"reasoning_tokens":900,"cost":0.421,"generation_id":"gen_01JABCDEFGHJKMNPQRSTVWXYZ1"}],"unaccounted":true,"lines_added":3,"lines_removed":1}"#
            ),
            "{encoded}"
        );
        assert_eq!(
            EventEnvelope::decode(encoded.trim_end()).expect("decodes"),
            frame
        );

        // A request the provider did not account for says so by having no
        // cost, rather than a cost of nought.
        let unbilled = RequestUsage {
            cost: None,
            generation_id: None,
            reasoning_tokens: None,
            ..match &frame.event {
                SessionEvent::UsageRecorded { requests, .. } => requests[0].clone(),
                _ => unreachable!(),
            }
        };
        let text = serde_json::to_string(&unbilled).expect("encodes");
        assert!(!text.contains("cost"), "{text}");
        assert!(!text.contains("reasoning"), "{text}");
    }

    #[test]
    fn a_conclusion_labels_itself_for_a_human_reader() {
        assert_eq!(
            TurnConclusion::Final {
                finish_reason: "stop".to_string(),
                steps: 2
            }
            .label(),
            "final"
        );
        assert_eq!(
            TurnConclusion::Interrupted {
                reason: "cancelled".to_string()
            }
            .label(),
            "interrupted"
        );
    }
}
