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
    /// One assistant step: its text, then the tools it asked for, in order.
    ///
    /// `raw_content` is the provider's own content blocks when it sent them,
    /// and it is recorded for one reason: the Anthropic wire verifies the
    /// signature on a reasoning block replayed in a continuation, so a resumed
    /// conversation that rebuilt the assistant turn from `text` would be
    /// answered with a 400. It can be **large** -- reasoning is not bounded by
    /// the answer's length -- and it is never displayed by any renderer; it
    /// exists to go back on the wire. Absent on old records and on the Gateway
    /// wire, which has no such contract; those rebuild from text and calls as
    /// before.
    AssistantMessage {
        text: String,
        tool_calls: Vec<RecordedToolCall>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        raw_content: Vec<serde_json::Value>,
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
    /// An approval the user gave for the rest of the session.
    PermissionGrantRecorded { tool: String, target: String },
    /// Tokens the provider reported for the turn that is ending. Added to the
    /// running totals; absent is "the provider did not say", not zero.
    UsageRecorded {
        input_tokens: Option<u64>,
        output_tokens: Option<u64>,
    },
    /// The turn ended. Exactly one per opened turn.
    TurnConcluded { outcome: TurnConclusion },
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
            Self::AssistantMessage { .. } => "assistant_message",
            Self::ToolResult { .. } => "tool_result",
            Self::PermissionGrantRecorded { .. } => "permission_grant_recorded",
            Self::UsageRecorded { .. } => "usage_recorded",
            Self::TurnConcluded { .. } => "turn_concluded",
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
            },
            SessionEvent::ToolResult {
                call_id: "c".to_string(),
                tool: "t".to_string(),
                ok: true,
                output: "o".to_string(),
            },
            SessionEvent::PermissionGrantRecorded {
                tool: "t".to_string(),
                target: "x".to_string(),
            },
            SessionEvent::UsageRecorded {
                input_tokens: None,
                output_tokens: Some(1),
            },
            SessionEvent::TurnConcluded {
                outcome: TurnConclusion::Interrupted {
                    reason: "cancelled".to_string(),
                },
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
