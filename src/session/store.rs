//! The durable session store: an append-only log, a published boundary, and the
//! projections rebuilt from them.
//!
//! # The one rule
//!
//! **A reader believes the manifest, and nothing past it.** Writing is two
//! steps that cannot be reordered:
//!
//! 1. append one encoded frame to `events.jsonl`, flush it, and `fsync` it;
//! 2. atomically replace `session.json` with a manifest naming the exact byte
//!    count and SHA-256 of the log it just made durable.
//!
//! Between those two steps the new bytes exist and are not yet true. A crash
//! there leaves a *tail*: bytes past the published boundary. Every reader stops
//! at the boundary, so the tail is invisible whether it is a well-formed event,
//! a half-written line, or garbage. A writer removes it, because a writer is
//! about to append and two truths cannot share one offset
//! (`vercel-labs/fx@580a0c5d src/core/session/session_log.zig:2185-2195`,
//! `src/core/session/session_projection.zig:248-256`).
//!
//! Everything else fails closed. A sequence gap, a repeated event id, a digest
//! that does not match, a boundary past the end of the file, a manifest whose
//! summary disagrees with the log it claims to describe, or a session id that
//! could name a path outside the store are all refusals with names -- never a
//! best-effort partial read, because a partial conversation resumed as if it
//! were whole is worse than no conversation at all.
//!
//! # Projections
//!
//! The manifest is a rebuildable projection of the log, and the listing is a
//! projection of the manifests. Nothing in a projection is authoritative: the
//! log is. That is why [`SessionStore::detail`] replays and cross-checks, while
//! [`SessionStore::list`] reads manifests only and reports how many entries it
//! had to skip rather than failing the whole listing.

use std::collections::BTreeSet;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
// Only the `flock` grace period measures time, and only unix has `flock`.
#[cfg(unix)]
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::agent::TurnJournal;
use crate::config::PermissionMode;
use crate::gateway::protocol::{Message, ToolCall};
use crate::permission::Grant;
use crate::provider::Wire;

use super::event::{
    new_identifier, system_now_ms, EventEnvelope, RecordedToolCall, SessionEvent, TurnConclusion,
    EVENT_SCHEMA_VERSION,
};

/// The directory holding every session, under the profile home.
pub const SESSIONS_DIR_NAME: &str = "sessions";

/// The canonical event log of one session.
pub const EVENTS_FILE: &str = "events.jsonl";

/// The manifest that publishes a boundary in that log.
pub const MANIFEST_FILE: &str = "session.json";

/// The manifest schema this build writes and reads.
pub const MANIFEST_SCHEMA_VERSION: u32 = 1;

/// The storage format the manifest names, so a future format is refused rather
/// than misread (`session_projection.zig:91`).
pub const STORAGE_FORMAT: &str = "event_log_v1";

/// The most characters a session id may have.
const MAX_SESSION_ID_BYTES: usize = 64;

/// The most bytes one session's log may reach.
const MAX_EVENT_LOG_BYTES: u64 = 64 * 1024 * 1024;

/// The most bytes a manifest may occupy (`session_projection.zig:11`).
const MAX_MANIFEST_BYTES: u64 = 64 * 1024;

/// How many sessions a listing shows when the caller does not say.
pub const DEFAULT_LIST_LIMIT: usize = 20;

/// The most a listing will ever show, however large a limit is asked for.
pub const MAX_LIST_LIMIT: usize = 200;

/// The most directory entries one listing will consider.
const MAX_SCANNED_SESSIONS: usize = 5_000;

/// The most bytes of a user message kept as a session's title.
const MAX_TITLE_BYTES: usize = 80;

/// The tail every staged manifest name ends in, so a leftover one is
/// recognizable as xfx's and never mistaken for session state.
pub const STAGE_SUFFIX: &str = ".staged";

// ---------------------------------------------------------------------------
// identity
// ---------------------------------------------------------------------------

/// A session id that has been proven safe to use as a directory name.
///
/// It can only be built by [`SessionId::parse`] or [`SessionId::generate`], so a
/// value of this type is itself the proof that joining it onto the sessions
/// directory cannot escape it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SessionId(String);

impl SessionId {
    /// Accepts `[A-Za-z0-9_-]{1,64}` and nothing else.
    ///
    /// The charset is deliberately smaller than "what a filesystem allows". A
    /// separator, a dot, a tilde, or a space each have a meaning to some layer
    /// between here and the disk, and none of them buy anything an id needs.
    pub fn parse(raw: &str) -> Result<Self, SessionError> {
        let unsafe_id = |detail: &'static str| SessionError::UnsafeId {
            requested: raw.to_string(),
            detail,
        };
        if raw.is_empty() {
            return Err(unsafe_id("a session id must not be empty"));
        }
        if raw.len() > MAX_SESSION_ID_BYTES {
            return Err(unsafe_id("a session id is at most 64 characters"));
        }
        if !raw
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        {
            return Err(unsafe_id(
                "a session id may only contain letters, digits, `-`, and `_`",
            ));
        }
        Ok(Self(raw.to_string()))
    }

    /// A fresh id: the creation time, then random bytes.
    ///
    /// The time prefix makes a directory listing sort chronologically, which is
    /// a convenience for a human reading the store by hand. It is not relied on:
    /// ordering comes from the manifest.
    pub fn generate() -> Self {
        let now = system_now_ms().max(0) as u64;
        Self(format!("{now:013}-{}", &new_identifier()[..16]))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Which session a command is talking about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selector {
    /// The most recent session of the current workspace. Never another one.
    Last,
    /// Exactly this session, wherever it is bound.
    Id(SessionId),
}

impl Selector {
    /// Parses the `<last|id>` grammar shared by `session` and `ask --resume`.
    pub fn parse(raw: &str) -> Result<Self, SessionError> {
        if raw == "last" {
            return Ok(Self::Last);
        }
        Ok(Self::Id(SessionId::parse(raw)?))
    }

    /// How a diagnostic names what the caller asked for.
    pub fn describe(&self) -> String {
        match self {
            Self::Last => "the most recent session of this workspace".to_string(),
            Self::Id(id) => format!("session `{id}`"),
        }
    }
}

// ---------------------------------------------------------------------------
// failures
// ---------------------------------------------------------------------------

/// Why a session operation could not be completed.
///
/// The `Display` text is what the user sees, so each variant says what happened
/// and, where there is one, what to do instead.
#[derive(Debug)]
pub enum SessionError {
    /// The requested id could name something other than a session directory.
    UnsafeId {
        requested: String,
        detail: &'static str,
    },
    /// A session with this id already exists; ids are claimed once.
    AlreadyExists { id: String },
    /// There is no such session.
    NoSession { detail: String },
    /// The session exists and cannot be trusted.
    Corrupt { id: String, detail: String },
    /// Another process holds this session open for writing.
    ///
    /// Distinct from [`Self::Corrupt`] on purpose: nothing is wrong with the
    /// session, and the right thing to do is wait or use another one.
    Busy { id: String },
    /// The log is not the length the open session believes it is, so someone
    /// else changed it underneath this writer.
    ///
    /// Refused rather than reconciled: writing at an offset that no longer means
    /// what it meant would interleave two conversations into one log.
    LogDiverged {
        id: String,
        expected: u64,
        actual: u64,
    },
    /// A file that must be private to its owner is not.
    InsecurePermissions { path: PathBuf, mode: u32 },
    /// A directory the store lives in is not a plain, private, owned directory.
    ///
    /// Separate from [`Self::InsecurePermissions`] because the consequence is
    /// different: a symlinked or foreign-owned `~/.xfx` does not leak one file,
    /// it redirects or exposes the whole store.
    InsecureParent { path: PathBuf, detail: String },
    /// The store cannot be used at all: no home directory, or a read-only store
    /// asked to write.
    Unavailable { detail: String },
    /// The filesystem refused.
    Io { path: PathBuf, source: io::Error },
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsafeId { requested, detail } => {
                write!(f, "`{requested}` is not a usable session id: {detail}")
            }
            Self::AlreadyExists { id } => write!(f, "session `{id}` already exists"),
            Self::NoSession { detail } => write!(f, "{detail}"),
            Self::Corrupt { id, detail } => write!(
                f,
                "session `{id}` cannot be trusted and was not read: {detail}"
            ),
            Self::Busy { id } => write!(
                f,
                "session `{id}` is already open for writing by another xfx process; \
                 finish or stop that turn first, or start a new session"
            ),
            Self::LogDiverged {
                id,
                expected,
                actual,
            } => write!(
                f,
                "session `{id}` was {expected} bytes when xfx opened it and is {actual} now, \
                 so something else is writing to it; this turn was not recorded"
            ),
            Self::InsecurePermissions { path, mode } => write!(
                f,
                "{} is mode {mode:o}, but session state must be private to its owner; \
                 xfx will not write through a file other accounts can read",
                path.display()
            ),
            Self::InsecureParent { path, detail } => {
                write!(f, "{} cannot hold session state: {detail}", path.display())
            }
            Self::Unavailable { detail } => write!(f, "{detail}"),
            Self::Io { path, source } => write!(f, "cannot use {}: {source}", path.display()),
        }
    }
}

impl std::error::Error for SessionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

fn io_error(path: &Path, source: io::Error) -> SessionError {
    SessionError::Io {
        path: path.to_path_buf(),
        source,
    }
}

// ---------------------------------------------------------------------------
// time
// ---------------------------------------------------------------------------

/// Where an event's timestamp comes from.
///
/// [`Clock::manual`] exists so a test can order two sessions deterministically
/// instead of sleeping. Nothing in the product builds one: [`SessionStore::open`]
/// and [`SessionStore::read_only`] both start from [`Clock::system`].
#[derive(Debug, Clone)]
pub struct Clock(ClockKind);

#[derive(Debug, Clone)]
enum ClockKind {
    System,
    Manual(Arc<AtomicI64>),
}

impl Clock {
    pub fn system() -> Self {
        Self(ClockKind::System)
    }

    /// A clock that only moves when a test moves it.
    pub fn manual(start_ms: i64) -> Self {
        Self(ClockKind::Manual(Arc::new(AtomicI64::new(start_ms))))
    }

    /// Moves a manual clock. A system clock ignores this.
    pub fn set(&self, ms: i64) {
        if let ClockKind::Manual(value) = &self.0 {
            value.store(ms, Ordering::SeqCst);
        }
    }

    pub fn now_ms(&self) -> i64 {
        match &self.0 {
            ClockKind::System => system_now_ms(),
            ClockKind::Manual(value) => value.load(Ordering::SeqCst),
        }
    }
}

impl Default for Clock {
    fn default() -> Self {
        Self::system()
    }
}

// ---------------------------------------------------------------------------
// the replayed state
// ---------------------------------------------------------------------------

/// One step inside a turn, in the order it happened.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum TurnStep {
    Assistant {
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
    ToolResult {
        call_id: String,
        tool: String,
        ok: bool,
        output: String,
    },
}

/// A history rebuilt for one active wire, and what the user has to be told
/// about what did not survive the rebuild.
#[derive(Debug, Clone, PartialEq)]
pub struct ReplayedHistory {
    pub messages: Vec<Message>,
    /// One line per assistant turn whose recorded state was not carried over.
    ///
    /// A drop is never silent: the user is entitled to know that the model
    /// resumed with less context than the log holds.
    pub notices: Vec<String>,
}

/// One complete exchange: what was asked, what happened, and how it ended.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HistoryTurn {
    pub user: String,
    pub steps: Vec<TurnStep>,
    /// `None` for a turn whose conclusion never reached the log, which is what a
    /// crash mid-turn looks like.
    pub outcome: Option<TurnConclusion>,
}

/// Everything a session's log adds up to.
#[derive(Debug, Clone, PartialEq)]
pub struct DurableState {
    pub id: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    /// Where the session was first opened. Never rewritten.
    pub origin_workspace_root: String,
    /// Where it is bound now. Only a [`SessionEvent::WorkspaceRebound`] moves it.
    pub workspace_root: String,
    pub model: String,
    pub permission_mode: PermissionMode,
    pub turns: Vec<HistoryTurn>,
    pub grants: Vec<Grant>,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    /// The project-instruction files that were in force at the last turn. Kept
    /// as provenance for `session` output; never used as context, because
    /// context is rediscovered.
    pub context_sources: Vec<String>,
    pub last_event_seq: u64,
}

impl DurableState {
    fn empty() -> Self {
        Self {
            id: String::new(),
            created_at_ms: 0,
            updated_at_ms: 0,
            origin_workspace_root: String::new(),
            workspace_root: String::new(),
            model: String::new(),
            permission_mode: PermissionMode::Ask,
            turns: Vec::new(),
            grants: Vec::new(),
            total_input_tokens: 0,
            total_output_tokens: 0,
            context_sources: Vec::new(),
            last_event_seq: 0,
        }
    }

    /// The session's first user message, clipped, for a listing.
    pub fn title(&self) -> Option<String> {
        self.turns.first().and_then(|turn| title_of(&turn.user))
    }

    /// The durable history, as the messages a next request would carry on
    /// `active`.
    ///
    /// Replay is keyed by **authority, not by shape**. Two wires can serialize
    /// state identically and still not be interchangeable, because the state is
    /// sealed by whoever issued the credential -- so the question is never "does
    /// this look like something I could send", it is "did the provider I am
    /// about to talk to produce it".
    ///
    /// Dropping shapes a *request*; it never mutates a record. The items stay on
    /// disk and a later resume back onto the original authority replays them.
    pub fn history_messages(&self, active: Wire) -> ReplayedHistory {
        let mut out = Vec::new();
        let mut notices = Vec::new();
        for turn in &self.turns {
            out.push(Message::user(turn.user.clone()));
            let mut pending: Vec<Message> = Vec::new();
            let mut awaiting: Vec<String> = Vec::new();
            for step in &turn.steps {
                match step {
                    TurnStep::Assistant {
                        text,
                        tool_calls,
                        raw_content,
                        responses_state,
                        wire,
                    } => {
                        if text.is_empty()
                            && tool_calls.is_empty()
                            && raw_content.is_empty()
                            && responses_state.is_empty()
                        {
                            continue;
                        }
                        let calls: Vec<ToolCall> = tool_calls
                            .iter()
                            .map(|call| ToolCall {
                                id: call.id.clone(),
                                name: call.name.clone(),
                                input: call.input.clone(),
                            })
                            .collect();
                        awaiting.extend(calls.iter().map(|call| call.id.clone()));
                        // The replay table of `.prd/04-providers.md` §Provenance,
                        // in one expression. `None` with blocks is legacy
                        // Anthropic, because that is the only wire that ever
                        // wrote the field; everything else must match exactly.
                        let recorded = wire.clone().unwrap_or_else(|| {
                            if !raw_content.is_empty() {
                                Wire::AnthropicMessages
                            } else if !responses_state.is_empty() {
                                // Wire-less responses_state is invalid: no pre-wire binary
                                // wrote this field. Treat as unknown wire so it drops.
                                Wire::Unrecognized("unknown_wire_with_responses_state".to_string())
                            } else {
                                active.clone()
                            }
                        });
                        let state: &[serde_json::Value] = match (&recorded, &active) {
                            (Wire::AnthropicMessages, Wire::AnthropicMessages) => raw_content,
                            (Wire::CodexResponses, Wire::CodexResponses)
                            | (Wire::GrokResponses, Wire::GrokResponses) => responses_state,
                            _ => &[],
                        };
                        if state.is_empty()
                            && !(raw_content.is_empty() && responses_state.is_empty())
                        {
                            notices.push(format!(
                                "xfx: this session recorded reasoning on the {} wire and is \
                                 resuming on {}, so that reasoning was not carried over",
                                recorded.label(),
                                active.label()
                            ));
                        }
                        pending.push(if state.is_empty() {
                            Message::assistant(Some(text), calls)
                        } else {
                            Message::assistant_raw(state.to_vec(), calls)
                        });
                    }
                    TurnStep::ToolResult {
                        call_id,
                        tool,
                        output,
                        ..
                    } => {
                        pending.push(Message::tool_result(call_id, tool, output.clone()));
                        awaiting.retain(|id| id != call_id);
                    }
                }
                if awaiting.is_empty() {
                    out.append(&mut pending);
                }
            }
        }
        ReplayedHistory {
            messages: out,
            notices,
        }
    }
}

/// The first line of `text`, trimmed and clipped to a character boundary.
fn title_of(text: &str) -> Option<String> {
    let line = text.trim().lines().next().unwrap_or("").trim();
    if line.is_empty() {
        return None;
    }
    if line.len() <= MAX_TITLE_BYTES {
        return Some(line.to_string());
    }
    let mut end = MAX_TITLE_BYTES;
    while end > 0 && !line.is_char_boundary(end) {
        end -= 1;
    }
    Some(format!("{}...", &line[..end]))
}

// ---------------------------------------------------------------------------
// the manifest
// ---------------------------------------------------------------------------

/// What the store publishes about a log: a summary, and the exact boundary the
/// summary was computed from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionManifest {
    pub schema_version: u32,
    pub storage_format: String,
    pub id: String,
    pub log_generation: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub origin_workspace_root: String,
    pub workspace_root: String,
    pub model: String,
    pub permission_mode: String,
    pub title: Option<String>,
    pub history_turns: u64,
    pub permission_grants: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    /// The sequence number of the last published event.
    pub last_event_seq: u64,
    /// How many bytes of `events.jsonl` are published. Everything past this is
    /// not part of the session.
    pub event_log_bytes: u64,
    /// SHA-256 of exactly those bytes.
    pub event_log_sha256: String,
}

impl SessionManifest {
    fn from_state(state: &DurableState, bytes: u64, digest: String, generation: &str) -> Self {
        Self {
            schema_version: MANIFEST_SCHEMA_VERSION,
            storage_format: STORAGE_FORMAT.to_string(),
            id: state.id.clone(),
            log_generation: generation.to_string(),
            created_at_ms: state.created_at_ms,
            updated_at_ms: state.updated_at_ms,
            origin_workspace_root: state.origin_workspace_root.clone(),
            workspace_root: state.workspace_root.clone(),
            model: state.model.clone(),
            permission_mode: state.permission_mode.label().to_string(),
            title: state.title(),
            history_turns: state.turns.len() as u64,
            permission_grants: state.grants.len() as u64,
            total_input_tokens: state.total_input_tokens,
            total_output_tokens: state.total_output_tokens,
            last_event_seq: state.last_event_seq,
            event_log_bytes: bytes,
            event_log_sha256: digest,
        }
    }

    /// Checks what the manifest says about itself, before it is believed.
    fn validate(&self, id: &SessionId) -> Result<(), String> {
        if self.schema_version != MANIFEST_SCHEMA_VERSION {
            return Err(format!(
                "manifest schema version {} is not the {MANIFEST_SCHEMA_VERSION} this build reads",
                self.schema_version
            ));
        }
        if self.storage_format != STORAGE_FORMAT {
            return Err(format!(
                "manifest storage format `{}` is not `{STORAGE_FORMAT}`",
                self.storage_format
            ));
        }
        if self.id != id.as_str() {
            return Err(format!(
                "the manifest names session `{}` but it is stored as `{id}`",
                self.id
            ));
        }
        if self.last_event_seq == 0 || self.event_log_bytes == 0 {
            return Err("the manifest publishes an empty boundary".to_string());
        }
        if self.event_log_bytes > MAX_EVENT_LOG_BYTES {
            return Err(format!(
                "the manifest publishes {} bytes, over the {MAX_EVENT_LOG_BYTES}-byte log limit",
                self.event_log_bytes
            ));
        }
        if !is_digest(&self.event_log_sha256) {
            return Err("the manifest digest is not 64 lowercase hex characters".to_string());
        }
        if PermissionMode::parse(&self.permission_mode).is_none() {
            return Err(format!(
                "the manifest names an unknown permission mode `{}`",
                self.permission_mode
            ));
        }
        Ok(())
    }

    /// Whether the manifest agrees with the log it claims to summarize.
    ///
    /// This is the check that makes the manifest a *projection* rather than a
    /// second source of truth: if the two ever disagree, the session is refused
    /// instead of one of them being quietly preferred.
    fn agrees_with(&self, state: &DurableState) -> Result<(), String> {
        let mismatch = |field: &str, published: String, replayed: String| {
            Err(format!(
                "the manifest says {field}={published} but the log replays {field}={replayed}"
            ))
        };
        if self.id != state.id {
            return mismatch("id", self.id.clone(), state.id.clone());
        }
        if self.origin_workspace_root != state.origin_workspace_root {
            return mismatch(
                "origin_workspace_root",
                self.origin_workspace_root.clone(),
                state.origin_workspace_root.clone(),
            );
        }
        if self.workspace_root != state.workspace_root {
            return mismatch(
                "workspace_root",
                self.workspace_root.clone(),
                state.workspace_root.clone(),
            );
        }
        if self.model != state.model {
            return mismatch("model", self.model.clone(), state.model.clone());
        }
        if self.permission_mode != state.permission_mode.label() {
            return mismatch(
                "permission_mode",
                self.permission_mode.clone(),
                state.permission_mode.label().to_string(),
            );
        }
        if self.history_turns != state.turns.len() as u64 {
            return mismatch(
                "history_turns",
                self.history_turns.to_string(),
                state.turns.len().to_string(),
            );
        }
        if self.permission_grants != state.grants.len() as u64 {
            return mismatch(
                "permission_grants",
                self.permission_grants.to_string(),
                state.grants.len().to_string(),
            );
        }
        if self.total_input_tokens != state.total_input_tokens
            || self.total_output_tokens != state.total_output_tokens
        {
            return mismatch(
                "usage",
                format!("{}/{}", self.total_input_tokens, self.total_output_tokens),
                format!("{}/{}", state.total_input_tokens, state.total_output_tokens),
            );
        }
        if self.last_event_seq != state.last_event_seq {
            return mismatch(
                "last_event_seq",
                self.last_event_seq.to_string(),
                state.last_event_seq.to_string(),
            );
        }
        if self.created_at_ms != state.created_at_ms || self.updated_at_ms != state.updated_at_ms {
            return mismatch(
                "timestamps",
                format!("{}/{}", self.created_at_ms, self.updated_at_ms),
                format!("{}/{}", state.created_at_ms, state.updated_at_ms),
            );
        }
        Ok(())
    }
}

fn is_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

// ---------------------------------------------------------------------------
// listing
// ---------------------------------------------------------------------------

/// What a listing shows about one session, without replaying its log.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SessionSummary {
    pub id: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub workspace_root: String,
    pub origin_workspace_root: String,
    pub history_turns: u64,
    pub title: Option<String>,
}

impl SessionSummary {
    fn from_manifest(manifest: &SessionManifest) -> Self {
        Self {
            id: manifest.id.clone(),
            created_at_ms: manifest.created_at_ms,
            updated_at_ms: manifest.updated_at_ms,
            workspace_root: manifest.workspace_root.clone(),
            origin_workspace_root: manifest.origin_workspace_root.clone(),
            history_turns: manifest.history_turns,
            title: manifest.title.clone(),
        }
    }
}

/// Which sessions a listing considers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListScope {
    /// Only sessions currently bound to this workspace.
    CurrentWorkspace(PathBuf),
    /// Every session in the store.
    AllWorkspaces,
}

impl ListScope {
    pub fn label(&self) -> &'static str {
        match self {
            Self::CurrentWorkspace(_) => "workspace",
            Self::AllWorkspaces => "all",
        }
    }
}

/// A bounded listing request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListFilter {
    pub scope: ListScope,
    pub limit: usize,
}

impl ListFilter {
    pub fn new(scope: ListScope) -> Self {
        Self {
            scope,
            limit: DEFAULT_LIST_LIMIT,
        }
    }

    /// A bound of at most [`MAX_LIST_LIMIT`]; zero means the default.
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = if limit == 0 {
            DEFAULT_LIST_LIMIT
        } else {
            limit.min(MAX_LIST_LIMIT)
        };
        self
    }
}

/// What a listing found.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionList {
    pub scope: &'static str,
    pub sessions: Vec<SessionSummary>,
    /// Whether the caller's limit cut the list short.
    pub has_more: bool,
    /// Whether the store's own scan cap cut the *candidate set* short, so there
    /// are sessions this listing did not even consider.
    ///
    /// Separate from `has_more` because the two mean different things to a user:
    /// one says "ask for more", the other says "this store is bigger than xfx
    /// will look at in one go".
    pub truncated: bool,
    /// How many session directories were skipped because they could not be
    /// trusted. Reported rather than hidden: a store that is quietly losing
    /// sessions should say so.
    pub skipped_invalid: usize,
}

/// One session read in full.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionDetail {
    pub summary: SessionSummary,
    pub manifest: SessionManifest,
    pub state: DurableState,
}

// ---------------------------------------------------------------------------
// the store
// ---------------------------------------------------------------------------

/// The facts needed to open a new session.
#[derive(Debug, Clone, PartialEq)]
pub struct NewSession {
    pub origin_workspace_root: PathBuf,
    pub workspace_root: PathBuf,
    pub model: String,
    pub permission_mode: PermissionMode,
}

/// A resumed session, and whether resuming it moved it.
#[derive(Debug)]
pub struct Resumed {
    pub session: WritableSession,
    /// True when the session was bound to another workspace and this resume
    /// rebound it, which is a durable event rather than a display detail.
    pub rebound: bool,
}

/// `~/.xfx/sessions`, and everything that may be done to it.
#[derive(Debug, Clone)]
pub struct SessionStore {
    profile_dir: PathBuf,
    sessions_dir: PathBuf,
    writable: bool,
    clock: Clock,
    scan_cap: usize,
}

impl SessionStore {
    /// Opens the store for writing, creating the private directories it needs.
    ///
    /// Both directories are checked before they are used, not only when they are
    /// created: a `~/.xfx` that has become a symlink, or that someone else owns,
    /// or that the group can write, redirects or exposes the entire store rather
    /// than one file, so it is refused instead of repaired.
    pub fn open(profile_dir: &Path) -> Result<Self, SessionError> {
        let store = Self::read_only(profile_dir);
        ensure_private_dir(&store.profile_dir)?;
        ensure_private_dir(&store.sessions_dir)?;
        Ok(Self {
            writable: true,
            ..store
        })
    }

    /// Opens the store for reading. Creates nothing, ever.
    ///
    /// `status`, `doctor`, `sessions`, and `session` all use this, so a machine
    /// that has never run `ask` still has an empty home after running them.
    pub fn read_only(profile_dir: &Path) -> Self {
        Self {
            profile_dir: profile_dir.to_path_buf(),
            sessions_dir: profile_dir.join(SESSIONS_DIR_NAME),
            writable: false,
            clock: Clock::system(),
            scan_cap: MAX_SCANNED_SESSIONS,
        }
    }

    /// The same store, timestamping events from `clock`.
    pub fn with_clock(mut self, clock: Clock) -> Self {
        self.clock = clock;
        self
    }

    /// The same store, considering at most `cap` session directories at once.
    ///
    /// A test seam. Proving the cap's behaviour with the shipped 5000 would mean
    /// building 5001 sessions, so the number is injectable and the *behaviour* --
    /// sort everything, keep the newest, say so -- is what gets tested.
    pub fn with_scan_cap(mut self, cap: usize) -> Self {
        self.scan_cap = cap.max(1);
        self
    }

    pub fn sessions_dir(&self) -> &Path {
        &self.sessions_dir
    }

    pub fn profile_dir(&self) -> &Path {
        &self.profile_dir
    }

    /// Where one session's files live. Safe by construction: the id is proven.
    pub fn session_dir(&self, id: &SessionId) -> PathBuf {
        self.sessions_dir.join(id.as_str())
    }

    // -- writing ------------------------------------------------------------

    /// Opens a new session, whose first event is its own creation.
    pub fn create(&self, id: SessionId, spec: NewSession) -> Result<WritableSession, SessionError> {
        self.require_writable()?;
        let dir = self.session_dir(&id);
        match fs::create_dir(&dir) {
            Ok(()) => {}
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
                return Err(SessionError::AlreadyExists {
                    id: id.as_str().to_string(),
                })
            }
            Err(err) => return Err(io_error(&dir, err)),
        }
        set_private_dir_mode(&dir)?;

        let log_path = dir.join(EVENTS_FILE);
        let file = create_private_file(&log_path)?;
        // Held for the whole life of the handle. A fresh id cannot already be
        // open, but taking the lock here rather than only in `resume` means
        // "a `WritableSession` owns its log" is a property of the type instead
        // of a property of one of its two constructors.
        lock_exclusive(&file, &log_path, &id)?;

        let mut session = WritableSession {
            id: id.clone(),
            dir,
            file,
            log_generation: new_identifier(),
            written_bytes: 0,
            published_bytes: 0,
            hasher: Sha256::new(),
            next_seq: 1,
            event_ids: BTreeSet::new(),
            state: DurableState::empty(),
            manifest: None,
            poisoned: None,
        };

        self.append(
            &mut session,
            SessionEvent::SessionStarted {
                id: id.as_str().to_string(),
                origin_workspace_root: workspace_key(&spec.origin_workspace_root),
                workspace_root: workspace_key(&spec.workspace_root),
                model: spec.model,
                permission_mode: spec.permission_mode.label().to_string(),
            },
        )?;
        self.publish(&mut session)?;
        Ok(session)
    }

    /// Appends one event and makes it durable, without publishing it.
    ///
    /// After this returns, the bytes are on the disk and `fsync`ed, and no
    /// reader can see them. That gap is the whole point: it is where a crash is
    /// allowed to happen.
    pub fn append(
        &self,
        session: &mut WritableSession,
        event: SessionEvent,
    ) -> Result<(), SessionError> {
        self.require_writable()?;
        session.check_usable()?;

        let mut event_id = new_identifier();
        while session.event_ids.contains(&event_id) {
            event_id = new_identifier();
        }
        let envelope = EventEnvelope {
            schema_version: EVENT_SCHEMA_VERSION,
            log_generation: session.log_generation.clone(),
            seq: session.next_seq,
            event_id,
            timestamp_ms: self.clock.now_ms(),
            event,
        };
        let line = envelope.encode().map_err(|err| SessionError::Corrupt {
            id: session.id.as_str().to_string(),
            detail: err.to_string(),
        })?;
        if session.written_bytes + line.len() as u64 > MAX_EVENT_LOG_BYTES {
            return Err(SessionError::Corrupt {
                id: session.id.as_str().to_string(),
                detail: format!(
                    "the event log would exceed its {MAX_EVENT_LOG_BYTES}-byte limit; \
                     start a new session"
                ),
            });
        }

        let log_path = session.dir.join(EVENTS_FILE);
        // The offset this append is about to claim has to still mean what it
        // meant when the handle was opened. The advisory lock keeps another
        // *xfx* out; this keeps anything else -- an editor, a sync client, a
        // second writer on a filesystem that does not honor `flock` -- from
        // being written over in silence. Refusing here is what makes the log's
        // "one writer per session" claim a checked fact rather than a hope.
        let actual = session
            .file
            .metadata()
            .map_err(|err| io_error(&log_path, err))?
            .len();
        if actual != session.written_bytes {
            session.poisoned = Some(format!(
                "the log was {} bytes when this turn started and is {actual} now",
                session.written_bytes
            ));
            return Err(SessionError::LogDiverged {
                id: session.id.as_str().to_string(),
                expected: session.written_bytes,
                actual,
            });
        }
        session
            .file
            .seek(SeekFrom::Start(session.written_bytes))
            .map_err(|err| io_error(&log_path, err))?;
        session
            .file
            .write_all(line.as_bytes())
            .map_err(|err| io_error(&log_path, err))?;
        session
            .file
            .flush()
            .map_err(|err| io_error(&log_path, err))?;
        // Durable before it is published, never the other way round.
        session
            .file
            .sync_data()
            .map_err(|err| io_error(&log_path, err))?;

        session.hasher.update(line.as_bytes());
        session.written_bytes += line.len() as u64;
        session.next_seq += 1;
        session.event_ids.insert(envelope.event_id.clone());

        // The event xfx just wrote is one xfx built, so a reduction failure here
        // is a defect rather than a bad input. It still fails closed: the
        // session stops accepting writes and the unpublished bytes stay
        // invisible to every reader.
        if let Err(detail) = apply(&mut session.state, &envelope) {
            session.poisoned = Some(detail.clone());
            return Err(SessionError::Corrupt {
                id: session.id.as_str().to_string(),
                detail,
            });
        }
        Ok(())
    }

    /// Publishes everything appended so far by atomically replacing the manifest.
    ///
    /// This is the only operation that changes what a reader sees.
    pub fn publish(&self, session: &mut WritableSession) -> Result<(), SessionError> {
        self.require_writable()?;
        session.check_usable()?;
        if session.written_bytes == session.published_bytes {
            return Ok(());
        }

        let digest = hex(&session.hasher.clone().finalize());
        let manifest = SessionManifest::from_state(
            &session.state,
            session.written_bytes,
            digest,
            &session.log_generation,
        );
        let mut body = serde_json::to_string(&manifest).map_err(|err| SessionError::Corrupt {
            id: session.id.as_str().to_string(),
            detail: format!("cannot encode the manifest: {err}"),
        })?;
        body.push('\n');
        if body.len() as u64 > MAX_MANIFEST_BYTES {
            return Err(SessionError::Corrupt {
                id: session.id.as_str().to_string(),
                detail: format!("the manifest exceeds its {MAX_MANIFEST_BYTES}-byte limit"),
            });
        }

        replace_private_file(&session.dir, MANIFEST_FILE, body.as_bytes())?;
        session.published_bytes = session.written_bytes;
        session.manifest = Some(manifest);
        Ok(())
    }

    // -- reading ------------------------------------------------------------

    /// Every session the filter admits, newest first.
    ///
    /// A session that cannot be trusted is counted and skipped rather than
    /// failing the listing: one damaged directory must not make the other
    /// twenty unreadable.
    ///
    /// # Determinism under the scan cap
    ///
    /// The whole directory is read and sorted *before* anything is dropped. A
    /// cap applied to `read_dir`'s own order would make which sessions exist a
    /// function of the filesystem's iteration order, which is the worst possible
    /// answer: `xfx session last` would silently continue an arbitrary old
    /// conversation on one run and a different one on the next.
    ///
    /// When the cap does bite, the *lexicographically greatest* names survive.
    /// A generated id begins with a zero-padded creation time, so that keeps the
    /// newest candidates, which is what `last` and a listing both want. The
    /// caller is told with `truncated` rather than left to infer it from a round
    /// number of rows.
    pub fn list(&self, filter: &ListFilter) -> Result<SessionList, SessionError> {
        // Before `read_dir`, and on both directories. Checking after the open
        // would mean following the link first and objecting afterwards, and
        // checking only `sessions` would leave a swapped `~/.xfx` unexamined --
        // which redirects `sessions` along with everything else under it.
        self.verify_store_dirs()?;

        let mut summaries: Vec<SessionSummary> = Vec::new();
        let mut skipped = 0usize;

        let entries = match fs::read_dir(&self.sessions_dir) {
            Ok(entries) => entries,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                return Ok(SessionList {
                    scope: filter.scope.label(),
                    sessions: Vec::new(),
                    has_more: false,
                    truncated: false,
                    skipped_invalid: 0,
                })
            }
            Err(err) => return Err(io_error(&self.sessions_dir, err)),
        };

        let mut names: Vec<String> = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|err| io_error(&self.sessions_dir, err))?;
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
        // Deterministic before anything else: two runs consider the same
        // directories in the same order, whatever the filesystem felt like.
        names.sort();
        let truncated = names.len() > self.scan_cap;
        if truncated {
            // Keep the tail, which is the newest under the id scheme.
            names.drain(..names.len() - self.scan_cap);
        }

        for name in names {
            let Ok(id) = SessionId::parse(&name) else {
                skipped += 1;
                continue;
            };
            match self.summary_of(&id) {
                Ok(summary) => summaries.push(summary),
                Err(_) => skipped += 1,
            }
        }

        if let ListScope::CurrentWorkspace(root) = &filter.scope {
            let key = workspace_key(root);
            summaries.retain(|summary| summary.workspace_root == key);
        }

        // Newest first; ties broken by id, descending, so the order is total.
        summaries.sort_by(|left, right| {
            right
                .updated_at_ms
                .cmp(&left.updated_at_ms)
                .then_with(|| right.id.cmp(&left.id))
        });

        let has_more = summaries.len() > filter.limit;
        summaries.truncate(filter.limit);
        Ok(SessionList {
            scope: filter.scope.label(),
            sessions: summaries,
            has_more,
            truncated,
            skipped_invalid: skipped,
        })
    }

    /// One session, replayed and cross-checked against its manifest.
    pub fn detail(
        &self,
        selector: &Selector,
        workspace: &Path,
    ) -> Result<SessionDetail, SessionError> {
        // At the entry, for every selector. `Selector::Last` reaches this check
        // through `list`, but an exact id does not go anywhere near a listing --
        // so relying on that would mean `xfx session --id X` read through a
        // swapped `~/.xfx` that `xfx sessions` had already refused.
        self.verify_store_dirs()?;
        let id = self.resolve(selector, workspace)?;
        let manifest = self.read_manifest(&id)?;
        let replay = self.replay(&id, &manifest)?;
        Ok(SessionDetail {
            summary: SessionSummary::from_manifest(&manifest),
            manifest,
            state: replay.state,
        })
    }

    /// Reopens a session for writing, truncating any unpublished crash tail.
    ///
    /// `Selector::Last` is scoped to `workspace` and never leaves it. An exact
    /// id may be resumed from anywhere, and doing so from a different workspace
    /// writes a [`SessionEvent::WorkspaceRebound`] before the turn runs, so the
    /// move is a fact in the log rather than a silent reinterpretation of every
    /// relative path in the history.
    pub fn resume(&self, selector: &Selector, workspace: &Path) -> Result<Resumed, SessionError> {
        self.require_writable()?;
        self.verify_store_dirs()?;
        let id = self.resolve(selector, workspace)?;
        let dir = self.session_dir(&id);
        verify_private(&dir, 0o700)?;
        verify_private(&dir.join(EVENTS_FILE), 0o600)?;
        verify_private(&dir.join(MANIFEST_FILE), 0o600)?;

        // The lock comes before the manifest is read and long before the log is
        // truncated. Ordering it the other way would let two writers each read
        // the same boundary, and then the loser would still have removed the
        // winner's unpublished bytes on its way to being refused.
        let log_path = dir.join(EVENTS_FILE);
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&log_path)
            .map_err(|err| io_error(&log_path, err))?;
        lock_exclusive(&file, &log_path, &id)?;

        let manifest = self.read_manifest(&id)?;
        let replay = self.replay(&id, &manifest)?;

        // The tail past the boundary is not part of this session and never will
        // be: the next append has to own the offset it starts at.
        file.set_len(manifest.event_log_bytes)
            .map_err(|err| io_error(&log_path, err))?;
        file.sync_data().map_err(|err| io_error(&log_path, err))?;

        let mut hasher = Sha256::new();
        hasher.update(&replay.bytes);

        let mut session = WritableSession {
            id,
            dir,
            file,
            log_generation: manifest.log_generation.clone(),
            written_bytes: manifest.event_log_bytes,
            published_bytes: manifest.event_log_bytes,
            hasher,
            next_seq: manifest.last_event_seq + 1,
            event_ids: replay.event_ids,
            state: replay.state,
            manifest: Some(manifest),
            poisoned: None,
        };

        let requested = workspace_key(workspace);
        let rebound = session.state.workspace_root != requested;
        if rebound {
            let previous = session.state.workspace_root.clone();
            self.append(
                &mut session,
                SessionEvent::WorkspaceRebound {
                    previous_workspace_root: previous,
                    workspace_root: requested,
                },
            )?;
            self.publish(&mut session)?;
        }
        Ok(Resumed { session, rebound })
    }

    // -- internals ----------------------------------------------------------

    fn require_writable(&self) -> Result<(), SessionError> {
        if self.writable {
            return Ok(());
        }
        Err(SessionError::Unavailable {
            detail: "this session store was opened read-only".to_string(),
        })
    }

    /// Checks both directories the store lives in, outermost first.
    ///
    /// Both, because they nest: a safe `sessions` inside a symlinked `~/.xfx` is
    /// not safe, it is somebody else's `sessions`. Outermost first, so the
    /// diagnostic names the outer problem rather than a symptom of it.
    ///
    /// Absence is not a failure -- a machine that has never run `ask` has
    /// neither directory, and reading from it is an empty answer rather than an
    /// error.
    fn verify_store_dirs(&self) -> Result<(), SessionError> {
        verify_store_dir(&self.profile_dir)?;
        verify_store_dir(&self.sessions_dir)
    }

    /// Turns a selector into an id, without reading any log.
    fn resolve(&self, selector: &Selector, workspace: &Path) -> Result<SessionId, SessionError> {
        match selector {
            Selector::Last => {
                let listed = self.list(&ListFilter::new(ListScope::CurrentWorkspace(
                    workspace.to_path_buf(),
                )))?;
                let Some(summary) = listed.sessions.first() else {
                    return Err(SessionError::NoSession {
                        detail: format!(
                            "no saved session belongs to {}; run `xfx ask` here first, \
                             or name a session id",
                            workspace.display()
                        ),
                    });
                };
                SessionId::parse(&summary.id)
            }
            Selector::Id(id) => {
                let dir = self.session_dir(id);
                if !dir.is_dir() {
                    return Err(SessionError::NoSession {
                        detail: format!("there is no session `{id}`"),
                    });
                }
                Ok(id.clone())
            }
        }
    }

    /// A listing entry, from the manifest alone.
    ///
    /// The log is stat'ed but not read: a listing is a projection, and paying
    /// for a full replay per row would make `xfx sessions` cost the whole store.
    fn summary_of(&self, id: &SessionId) -> Result<SessionSummary, SessionError> {
        let manifest = self.read_manifest(id)?;
        let log = self.session_dir(id).join(EVENTS_FILE);
        let metadata = fs::metadata(&log).map_err(|err| io_error(&log, err))?;
        if metadata.len() < manifest.event_log_bytes {
            return Err(SessionError::Corrupt {
                id: id.as_str().to_string(),
                detail: format!(
                    "the manifest publishes {} bytes but the log holds {}",
                    manifest.event_log_bytes,
                    metadata.len()
                ),
            });
        }
        Ok(SessionSummary::from_manifest(&manifest))
    }

    fn read_manifest(&self, id: &SessionId) -> Result<SessionManifest, SessionError> {
        let dir = self.session_dir(id);
        let corrupt = |detail: String| SessionError::Corrupt {
            id: id.as_str().to_string(),
            detail,
        };
        match fs::symlink_metadata(&dir) {
            Ok(metadata) if metadata.is_dir() => {}
            Ok(_) => {
                return Err(corrupt(
                    "the session path is not a directory; xfx will not follow it".to_string(),
                ))
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                return Err(SessionError::NoSession {
                    detail: format!("there is no session `{id}`"),
                })
            }
            Err(err) => return Err(io_error(&dir, err)),
        }

        let path = dir.join(MANIFEST_FILE);
        let metadata = match fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                return Err(corrupt("the session has no published manifest".to_string()))
            }
            Err(err) => return Err(io_error(&path, err)),
        };
        if !metadata.is_file() || metadata.len() > MAX_MANIFEST_BYTES {
            return Err(corrupt(
                "the manifest is not a bounded regular file".to_string(),
            ));
        }
        let body = fs::read_to_string(&path).map_err(|err| io_error(&path, err))?;
        let manifest: SessionManifest =
            serde_json::from_str(&body).map_err(|err| corrupt(format!("{err}")))?;
        manifest.validate(id).map_err(corrupt)?;
        Ok(manifest)
    }

    /// Reads exactly the published bytes and rebuilds the state from them.
    fn replay(&self, id: &SessionId, manifest: &SessionManifest) -> Result<Replay, SessionError> {
        let path = self.session_dir(id).join(EVENTS_FILE);
        let corrupt = |detail: String| SessionError::Corrupt {
            id: id.as_str().to_string(),
            detail,
        };

        let mut file = File::open(&path).map_err(|err| io_error(&path, err))?;
        let length = file.metadata().map_err(|err| io_error(&path, err))?.len();
        if length < manifest.event_log_bytes {
            return Err(corrupt(format!(
                "the manifest publishes {} bytes but the log holds {length}",
                manifest.event_log_bytes
            )));
        }

        let mut bytes = vec![0u8; manifest.event_log_bytes as usize];
        file.read_exact(&mut bytes)
            .map_err(|err| io_error(&path, err))?;
        let digest = hex(&Sha256::digest(&bytes));
        if digest != manifest.event_log_sha256 {
            return Err(corrupt(format!(
                "the published bytes hash to {digest}, not the {} the manifest names",
                manifest.event_log_sha256
            )));
        }

        let text = String::from_utf8(bytes.clone())
            .map_err(|_| corrupt("the published bytes are not UTF-8".to_string()))?;
        if !text.ends_with('\n') {
            return Err(corrupt(
                "the published boundary does not land on the end of a frame".to_string(),
            ));
        }

        let mut state = DurableState::empty();
        let mut event_ids: BTreeSet<String> = BTreeSet::new();
        let mut expected_seq = 1u64;
        for line in text.lines() {
            let envelope = EventEnvelope::decode(line).map_err(|err| corrupt(err.to_string()))?;
            if envelope.seq != expected_seq {
                return Err(corrupt(format!(
                    "the log jumps from sequence {} to {}",
                    expected_seq - 1,
                    envelope.seq
                )));
            }
            if envelope.log_generation != manifest.log_generation {
                return Err(corrupt(format!(
                    "event {} belongs to another log generation",
                    envelope.seq
                )));
            }
            if !event_ids.insert(envelope.event_id.clone()) {
                return Err(corrupt(format!(
                    "event id {} appears more than once",
                    envelope.event_id
                )));
            }
            apply(&mut state, &envelope).map_err(&corrupt)?;
            expected_seq += 1;
        }
        if expected_seq - 1 != manifest.last_event_seq {
            return Err(corrupt(format!(
                "the manifest publishes through sequence {} but the log ends at {}",
                manifest.last_event_seq,
                expected_seq - 1
            )));
        }
        manifest.agrees_with(&state).map_err(corrupt)?;

        Ok(Replay {
            state,
            event_ids,
            bytes,
        })
    }
}

struct Replay {
    state: DurableState,
    event_ids: BTreeSet<String>,
    bytes: Vec<u8>,
}

// ---------------------------------------------------------------------------
// the writable handle
// ---------------------------------------------------------------------------

/// An open session that may be appended to.
///
/// It owns the log's file handle, its running digest, and the state as of the
/// last appended event. Only [`SessionStore`] can move it forward, so a caller
/// cannot write a frame without going through the append/publish protocol.
#[derive(Debug)]
pub struct WritableSession {
    id: SessionId,
    dir: PathBuf,
    file: File,
    log_generation: String,
    /// Bytes appended and `fsync`ed, published or not.
    written_bytes: u64,
    /// Bytes a reader is allowed to see.
    published_bytes: u64,
    hasher: Sha256,
    next_seq: u64,
    event_ids: BTreeSet<String>,
    state: DurableState,
    manifest: Option<SessionManifest>,
    /// Set when an append produced a state xfx could not reduce. The session
    /// then accepts nothing more, so the damage cannot be published.
    poisoned: Option<String>,
}

impl WritableSession {
    pub fn id(&self) -> &SessionId {
        &self.id
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The state as of the last appended event, published or not.
    pub fn state(&self) -> &DurableState {
        &self.state
    }

    /// The last published manifest, when one exists.
    pub fn manifest(&self) -> Option<&SessionManifest> {
        self.manifest.as_ref()
    }

    /// How many bytes of the log a reader would currently see.
    pub fn published_bytes(&self) -> u64 {
        self.published_bytes
    }

    /// How many durable bytes are not yet published -- the crash window.
    pub fn pending_bytes(&self) -> u64 {
        self.written_bytes - self.published_bytes
    }

    fn check_usable(&self) -> Result<(), SessionError> {
        match &self.poisoned {
            None => Ok(()),
            Some(detail) => Err(SessionError::Corrupt {
                id: self.id.as_str().to_string(),
                detail: detail.clone(),
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// the journal adapter
// ---------------------------------------------------------------------------

/// Writes a running turn's events into a session, one durable step at a time.
///
/// Persistence is best-effort *from the turn's point of view*: a failure is
/// remembered here and reported by the caller rather than changing the turn's
/// outcome. That is the honest split. The user's answer already arrived; the
/// truthful report is "here is your answer, and xfx could not record it", not a
/// turn retroactively declared to have failed. Nothing is silently lost either:
/// unpublished bytes are invisible, so a partial record is never resumed as a
/// whole one.
#[derive(Debug)]
pub struct SessionRecorder {
    store: SessionStore,
    session: WritableSession,
    failure: Option<String>,
}

impl SessionRecorder {
    pub fn new(store: SessionStore, session: WritableSession) -> Self {
        Self {
            store,
            session,
            failure: None,
        }
    }

    pub fn id(&self) -> &SessionId {
        self.session.id()
    }

    pub fn state(&self) -> &DurableState {
        self.session.state()
    }

    /// The first persistence failure, if there was one.
    pub fn failure(&self) -> Option<&str> {
        self.failure.as_deref()
    }

    /// Appends one event and publishes it.
    pub fn commit(&mut self, event: SessionEvent) {
        if self.failure.is_some() {
            return;
        }
        let kind = event.kind();
        let outcome = self
            .store
            .append(&mut self.session, event)
            .and_then(|()| self.store.publish(&mut self.session));
        if let Err(err) = outcome {
            self.failure = Some(format!("cannot record the `{kind}` session event: {err}"));
        }
    }
}

impl TurnJournal for SessionRecorder {
    fn record(&mut self, event: SessionEvent) {
        self.commit(event);
    }
}

// ---------------------------------------------------------------------------
// reduction
// ---------------------------------------------------------------------------

/// Folds one event into the state, or says why it cannot be part of this log.
///
/// Every refusal here is a structural impossibility rather than a taste: an
/// assistant step with no turn to belong to, a turn that concludes twice, a
/// rebind that names a binding the session never had. A log that contains one is
/// not a log xfx wrote.
fn apply(state: &mut DurableState, envelope: &EventEnvelope) -> Result<(), String> {
    let is_start = matches!(envelope.event, SessionEvent::SessionStarted { .. });
    if (envelope.seq == 1) != is_start {
        return Err(
            "the first event of a log is `session_started`, and only the first one is".to_string(),
        );
    }

    match &envelope.event {
        SessionEvent::SessionStarted {
            id,
            origin_workspace_root,
            workspace_root,
            model,
            permission_mode,
        } => {
            let mode = PermissionMode::parse(permission_mode)
                .ok_or_else(|| format!("unknown permission mode `{permission_mode}`"))?;
            state.id = id.clone();
            state.created_at_ms = envelope.timestamp_ms;
            state.origin_workspace_root = origin_workspace_root.clone();
            state.workspace_root = workspace_root.clone();
            state.model = model.clone();
            state.permission_mode = mode;
        }
        SessionEvent::WorkspaceRebound {
            previous_workspace_root,
            workspace_root,
        } => {
            if *previous_workspace_root != state.workspace_root {
                return Err(format!(
                    "a rebind names `{previous_workspace_root}` as the previous workspace, \
                     but the session was bound to `{}`",
                    state.workspace_root
                ));
            }
            state.workspace_root = workspace_root.clone();
        }
        SessionEvent::PreferencesChanged {
            model,
            permission_mode,
        } => {
            if let Some(model) = model {
                state.model = model.clone();
            }
            if let Some(raw) = permission_mode {
                state.permission_mode = PermissionMode::parse(raw)
                    .ok_or_else(|| format!("unknown permission mode `{raw}`"))?;
            }
        }
        SessionEvent::ProjectContextRecorded { sources, .. } => {
            state.context_sources = sources.clone();
        }
        SessionEvent::UserMessage { text } => state.turns.push(HistoryTurn {
            user: text.clone(),
            steps: Vec::new(),
            outcome: None,
        }),
        SessionEvent::AssistantMessage {
            text,
            tool_calls,
            raw_content,
            responses_state,
            wire,
        } => {
            open_turn(state, "an assistant step")?
                .steps
                .push(TurnStep::Assistant {
                    text: text.clone(),
                    tool_calls: tool_calls.clone(),
                    raw_content: raw_content.clone(),
                    responses_state: responses_state.clone(),
                    wire: wire.clone(),
                });
        }
        SessionEvent::ToolResult {
            call_id,
            tool,
            ok,
            output,
        } => {
            open_turn(state, "a tool result")?
                .steps
                .push(TurnStep::ToolResult {
                    call_id: call_id.clone(),
                    tool: tool.clone(),
                    ok: *ok,
                    output: output.clone(),
                });
        }
        SessionEvent::PermissionGrantRecorded { tool, target } => {
            let grant = Grant::new(tool.clone(), target.clone());
            if !state.grants.contains(&grant) {
                state.grants.push(grant);
            }
        }
        SessionEvent::UsageRecorded {
            input_tokens,
            output_tokens,
        } => {
            state.total_input_tokens = state
                .total_input_tokens
                .saturating_add(input_tokens.unwrap_or(0));
            state.total_output_tokens = state
                .total_output_tokens
                .saturating_add(output_tokens.unwrap_or(0));
        }
        SessionEvent::TurnConcluded { outcome } => {
            let turn = open_turn(state, "a turn conclusion")?;
            turn.outcome = Some(outcome.clone());
        }
    }

    state.updated_at_ms = envelope.timestamp_ms;
    state.last_event_seq = envelope.seq;
    Ok(())
}

/// The turn currently open, or why there is not one.
fn open_turn<'a>(state: &'a mut DurableState, what: &str) -> Result<&'a mut HistoryTurn, String> {
    let Some(turn) = state.turns.last_mut() else {
        return Err(format!("{what} appears before any user message"));
    };
    if turn.outcome.is_some() {
        return Err(format!("{what} appears after its turn already concluded"));
    }
    Ok(turn)
}

// ---------------------------------------------------------------------------
// filesystem
// ---------------------------------------------------------------------------

/// How a workspace root is written into a session, and compared.
///
/// Trailing separators are removed so that two spellings of one directory are
/// one key (`session_store_paths.zig:43-49`).
pub fn workspace_key(path: &Path) -> String {
    let raw = path.to_string_lossy();
    let trimmed = raw.trim_end_matches('/');
    if trimmed.is_empty() {
        raw.into_owned()
    } else {
        trimmed.to_string()
    }
}

/// Ensures `path` is a real, private, owned directory, creating it if absent.
///
/// The existing case is checked rather than trusted. `~/.xfx` and
/// `~/.xfx/sessions` are the two directories every session's privacy rests on:
/// if one is a symlink, every later `0600` file is created wherever the link
/// points; if another account owns it or can write it, that account chooses what
/// xfx resumes. Neither is repaired -- silently `chmod`ing a directory xfx does
/// not own would be a worse answer than stopping.
fn ensure_private_dir(path: &Path) -> Result<(), SessionError> {
    match fs::symlink_metadata(path) {
        Ok(_) => return verify_store_dir(path),
        Err(err) if err.kind() == io::ErrorKind::NotFound => {}
        Err(err) => return Err(io_error(path, err)),
    }
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.is_dir() {
            fs::create_dir_all(parent).map_err(|err| io_error(parent, err))?;
        }
    }
    match fs::create_dir(path) {
        Ok(()) => {
            set_private_dir_mode(path)?;
            verify_store_dir(path)
        }
        // Lost a race with another xfx creating the same directory. It still
        // has to pass the same check.
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => verify_store_dir(path),
        Err(err) => Err(io_error(path, err)),
    }
}

/// Requires an existing store directory to be a plain directory, owned by this
/// user, with nothing granted to group or other.
#[cfg(unix)]
fn verify_store_dir(path: &Path) -> Result<(), SessionError> {
    use std::os::unix::fs::MetadataExt;
    use std::os::unix::fs::PermissionsExt;

    let insecure = |detail: String| {
        Err(SessionError::InsecureParent {
            path: path.to_path_buf(),
            detail,
        })
    };
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(io_error(path, err)),
    };
    if metadata.file_type().is_symlink() {
        return insecure(
            "it is a symbolic link, and xfx will not follow one to decide where session state lives"
                .to_string(),
        );
    }
    if !metadata.is_dir() {
        return insecure("it is not a directory".to_string());
    }
    let owner = metadata.uid();
    let current = rustix::process::geteuid().as_raw();
    if owner != current {
        return insecure(format!(
            "it is owned by uid {owner}, not by uid {current} running xfx"
        ));
    }
    let mode = metadata.permissions().mode() & 0o777;
    if mode & 0o077 != 0 {
        return insecure(format!(
            "it is mode {mode:o}; session state must not be reachable by group or other"
        ));
    }
    Ok(())
}

#[cfg(not(unix))]
fn verify_store_dir(path: &Path) -> Result<(), SessionError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => Err(SessionError::InsecureParent {
            path: path.to_path_buf(),
            detail: "it is not a directory".to_string(),
        }),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(io_error(path, err)),
    }
}

#[cfg(unix)]
fn set_private_dir_mode(path: &Path) -> Result<(), SessionError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|err| io_error(path, err))
}

#[cfg(not(unix))]
fn set_private_dir_mode(_path: &Path) -> Result<(), SessionError> {
    Ok(())
}

/// Creates a new, empty, owner-only file, refusing to reuse an existing one.
fn create_private_file(path: &Path) -> Result<File, SessionError> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).map_err(|err| io_error(path, err))
}

/// A staged file that removes itself unless it was renamed into place.
///
/// The stage exists for the few microseconds between "written" and "renamed". If
/// anything in between fails, the partial file must not be left behind to be
/// mistaken for state -- and equally must not be cleaned up by *deleting a fixed
/// name*, because a fixed name is something another process might own.
struct StagedFile {
    path: PathBuf,
    committed: bool,
}

impl StagedFile {
    fn commit(mut self) {
        self.committed = true;
    }
}

impl Drop for StagedFile {
    fn drop(&mut self) {
        if !self.committed {
            // Only ever this process's own uniquely named stage.
            let _ = fs::remove_file(&self.path);
        }
    }
}

/// Replaces `name` inside `dir` with `bytes`, atomically and privately.
///
/// Staged in the same directory so the rename cannot cross a filesystem, and the
/// directory itself is synced afterwards so the *name* is durable and not only
/// the bytes it points at.
///
/// The stage name carries this process's id and a fresh nonce. A fixed
/// `<name>.new` would have to be deleted before use, and deleting a fixed name
/// means deleting a file another writer may be in the middle of -- the exact
/// interference the writer lock exists to prevent, reintroduced by the cleanup
/// path. xfx therefore never unlinks a stage it did not create; a leftover one
/// from a killed process is inert (it is not `session.json`, so no reader looks
/// at it) and is reported by `doctor` rather than silently removed.
fn replace_private_file(dir: &Path, name: &str, bytes: &[u8]) -> Result<(), SessionError> {
    let staged = StagedFile {
        path: dir.join(format!(
            "{name}.{}.{}{STAGE_SUFFIX}",
            std::process::id(),
            &new_identifier()[..16]
        )),
        committed: false,
    };
    {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&staged.path)
            .map_err(|err| io_error(&staged.path, err))?;
        file.write_all(bytes)
            .map_err(|err| io_error(&staged.path, err))?;
        file.sync_all().map_err(|err| io_error(&staged.path, err))?;
    }
    let target = dir.join(name);
    fs::rename(&staged.path, &target).map_err(|err| io_error(&target, err))?;
    staged.commit();
    sync_directory(dir)
}

/// Claims a session's log for this handle, or reports who has it.
///
/// `flock(LOCK_EX | LOCK_NB)` on the log's own file description. Two properties
/// make it the right primitive here:
///
/// - **It is released by the kernel**, when the last descriptor for the open
///   file description closes. A `WritableSession` therefore cannot leak a lock
///   by panicking, by being forgotten, or by the process being killed -- which
///   is exactly the situation a crash-safe store has to survive.
/// - **It is per open file description, not per process.** Opening the same
///   session twice in one process is refused too, which is what makes the
///   guarantee testable without spawning anything.
///
/// It is advisory: it stops xfx, not `cat`. That is why [`SessionStore::append`]
/// also verifies the log's length before every write -- the lock is the polite
/// mechanism, the length check is the one that cannot be talked out of.
///
/// # Why a refusal is not immediate
///
/// The same property that makes the lock crash-safe -- it lives on the open
/// file description, and the kernel drops it when the *last* descriptor for
/// that description closes -- means a `fork` can outlive the writer that took
/// it. `fork` duplicates every open descriptor into the child, and `O_CLOEXEC`
/// closes those copies at `execve`, not at `fork`. xfx forks constantly: every
/// `terminal` command is a child, and a child forked by one thread inherits the
/// session another thread has open. In the window between that `fork` and its
/// `exec`, the lock is held by a process that is not writing to the session and
/// never will, and it stays held even after the real writer closes.
///
/// So `WOULDBLOCK` opens a **bounded grace period** rather than a refusal: the
/// lock is retried across [`LOCK_RETRY_BUDGET`] by [`acquire_with_grace`], and
/// only a lock still held when the budget runs out is reported as
/// [`SessionError::Busy`]. What that buys, exactly:
///
/// - A fork-inheritance window of ordinary length is suppressed, because it is
///   over long before the budget is.
/// - Contention that clears inside the budget is serialized instead of refused
///   -- including a genuine writer that finishes in that time. That is the
///   right outcome for a lock: the second writer waited its turn and then had
///   it.
/// - A `Busy` that is really warranted is reported roughly a budget later than
///   it used to be -- a budget, plus whatever the last `sleep` overslept by.
///   That is the price, and it is why the budget is milliseconds rather than
///   seconds.
///
/// What it does **not** do is tell the two kinds of holder apart. Nothing here
/// can: `flock` says a lock is held, never by whom or why. A forked child that
/// is descheduled long enough will still produce a `Busy` about a session
/// nobody is writing, and this only makes that rarer. The grace period is a
/// reduction in a race's blast radius, not a classifier.
///
/// Blocking `flock` is a different instrument, and the wrong one: it would wait
/// out a writer that holds the session for a whole turn instead of reporting
/// it, which is the answer a person needs.
#[cfg(unix)]
fn lock_exclusive(file: &File, path: &Path, id: &SessionId) -> Result<(), SessionError> {
    use rustix::fs::{flock, FlockOperation};

    let outcome = acquire_with_grace(
        || match flock(file, FlockOperation::NonBlockingLockExclusive) {
            Ok(()) => AttemptOutcome::Acquired,
            // `EAGAIN` and `EWOULDBLOCK` are the same value on every platform
            // xfx builds for, so matching one covers both.
            Err(rustix::io::Errno::WOULDBLOCK) => AttemptOutcome::WouldBlock,
            Err(err) => AttemptOutcome::Failed(io::Error::from(err)),
        },
        std::thread::sleep,
        Instant::now,
    );
    match outcome {
        Ok(()) => Ok(()),
        Err(AcquireFailure::HeldThroughGrace) => Err(SessionError::Busy {
            id: id.as_str().to_string(),
        }),
        Err(AcquireFailure::Failed(err)) => Err(io_error(path, err)),
    }
}

/// What one attempt at a lock did, in the only three shapes the grace period
/// distinguishes.
#[cfg(unix)]
enum AttemptOutcome {
    /// The lock is now held by the caller.
    Acquired,
    /// Someone else holds it at this instant.
    WouldBlock,
    /// The attempt could not be made at all, which is not a lock question.
    Failed(io::Error),
}

/// Why a grace period ended without the lock.
#[cfg(unix)]
enum AcquireFailure {
    /// Someone held it for the whole budget. The caller names the session and
    /// reports [`SessionError::Busy`].
    HeldThroughGrace,
    Failed(io::Error),
}

/// Retries `attempt` across [`LOCK_RETRY_BUDGET`], waiting through `sleep`.
///
/// Separate from [`lock_exclusive`], with all three of its effects injected --
/// the attempt, the pause, and the clock -- because the part worth being sure
/// about is arithmetic: the backoff sequence, that a failure is not retried,
/// and that no pause is ever requested once the deadline has passed. Arithmetic
/// should be provable without a lock, a real clock, or a second thread to race,
/// and the tests below drive this directly.
///
/// The deadline is fixed once, from the clock, and the time left is re-read
/// from the clock before every pause. That matters because neither a `sleep`
/// nor a thread is punctual: an oversleeping pause, or a descheduling long
/// enough to cross the deadline, ends the grace at the next look instead of
/// asking for another pause the deadline can no longer pay for. That is the
/// guarantee, and it is deliberately the modest one -- this function never
/// *asks* to wait past the budget, and it cannot promise that the elapsed time
/// stayed under it, because a `sleep` returns when the OS says so. A caller who
/// has a real refusal coming is kept waiting for one budget plus whatever the
/// last pause overran by.
#[cfg(unix)]
fn acquire_with_grace(
    mut attempt: impl FnMut() -> AttemptOutcome,
    mut sleep: impl FnMut(Duration),
    mut now: impl FnMut() -> Instant,
) -> Result<(), AcquireFailure> {
    let deadline = now() + LOCK_RETRY_BUDGET;
    let mut backoff = LOCK_FIRST_BACKOFF;
    loop {
        match attempt() {
            AttemptOutcome::Acquired => return Ok(()),
            AttemptOutcome::Failed(err) => return Err(AcquireFailure::Failed(err)),
            AttemptOutcome::WouldBlock => {
                let remaining = deadline.saturating_duration_since(now());
                if remaining.is_zero() {
                    return Err(AcquireFailure::HeldThroughGrace);
                }
                // Clamped, so the last pause of a grace period cannot spend
                // time the deadline does not have.
                let pause = backoff.min(remaining);
                sleep(pause);
                backoff = (backoff * 2).min(LOCK_MAX_BACKOFF);
            }
        }
    }
}

/// How long [`lock_exclusive`] waits out a held lock before calling it `Busy`.
///
/// A compromise between two costs, not a threshold with meaning. Longer covers
/// more fork windows on a loaded machine; longer also delays every `Busy` a
/// person is actually waiting on, and blurs a real refusal further into a
/// wait. This is enough for a child that reaches `execve` promptly, and short
/// enough to stay under the time a keystroke feels answered -- give or take
/// however long the final `sleep` overran, which is not this code's to bound.
#[cfg(unix)]
const LOCK_RETRY_BUDGET: Duration = Duration::from_millis(250);

/// The first pause between attempts, doubled up to [`LOCK_MAX_BACKOFF`].
///
/// It starts small because the hold this is aimed at is usually already
/// closing, and grows so that a longer hold is not spun on.
#[cfg(unix)]
const LOCK_FIRST_BACKOFF: Duration = Duration::from_millis(1);

/// The longest pause between attempts, so the budget is spent on several looks
/// rather than one long nap that could end well after the lock was released.
#[cfg(unix)]
const LOCK_MAX_BACKOFF: Duration = Duration::from_millis(50);

/// No advisory locking is available, so concurrent writers are caught by the
/// length check in [`SessionStore::append`] instead of being prevented.
#[cfg(not(unix))]
fn lock_exclusive(_file: &File, _path: &Path, _id: &SessionId) -> Result<(), SessionError> {
    Ok(())
}

#[cfg(unix)]
fn sync_directory(dir: &Path) -> Result<(), SessionError> {
    let handle = File::open(dir).map_err(|err| io_error(dir, err))?;
    handle.sync_all().map_err(|err| io_error(dir, err))
}

#[cfg(not(unix))]
fn sync_directory(_dir: &Path) -> Result<(), SessionError> {
    Ok(())
}

/// Requires `path` to be exactly `expected` mode before it is written through.
#[cfg(unix)]
fn verify_private(path: &Path, expected: u32) -> Result<(), SessionError> {
    use std::os::unix::fs::PermissionsExt;
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        // Absence is not an insecurity; the caller reports it as "no session".
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(io_error(path, err)),
    };
    let mode = metadata.permissions().mode() & 0o777;
    if mode != expected {
        return Err(SessionError::InsecurePermissions {
            path: path.to_path_buf(),
            mode,
        });
    }
    Ok(())
}

#[cfg(not(unix))]
fn verify_private(_path: &Path, _expected: u32) -> Result<(), SessionError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::event::TurnConclusion;

    fn frame(seq: u64, event: SessionEvent) -> EventEnvelope {
        EventEnvelope {
            schema_version: EVENT_SCHEMA_VERSION,
            log_generation: "a".repeat(32),
            seq,
            event_id: format!("{seq:032x}"),
            timestamp_ms: 1_000 + seq as i64,
            event,
        }
    }

    fn started() -> SessionEvent {
        SessionEvent::SessionStarted {
            id: "s1".to_string(),
            origin_workspace_root: "/w".to_string(),
            workspace_root: "/w".to_string(),
            model: "m".to_string(),
            permission_mode: "auto".to_string(),
        }
    }

    fn reduce(events: Vec<SessionEvent>) -> Result<DurableState, String> {
        let mut state = DurableState::empty();
        for (index, event) in events.into_iter().enumerate() {
            apply(&mut state, &frame(index as u64 + 1, event))?;
        }
        Ok(state)
    }

    #[test]
    fn a_log_must_start_with_exactly_one_creation_event() {
        assert!(reduce(vec![SessionEvent::UserMessage {
            text: "x".to_string()
        }])
        .is_err());
        assert!(reduce(vec![started(), started()]).is_err());
        assert!(reduce(vec![started()]).is_ok());
    }

    #[test]
    fn a_step_without_a_turn_is_refused() {
        for event in [
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
            SessionEvent::TurnConcluded {
                outcome: TurnConclusion::Final {
                    finish_reason: "stop".to_string(),
                    steps: 1,
                },
            },
        ] {
            assert!(
                reduce(vec![started(), event.clone()]).is_err(),
                "{:?} must need an open turn",
                event.kind()
            );
        }
    }

    #[test]
    fn a_turn_concludes_once() {
        let concluded = SessionEvent::TurnConcluded {
            outcome: TurnConclusion::Final {
                finish_reason: "stop".to_string(),
                steps: 1,
            },
        };
        assert!(reduce(vec![
            started(),
            SessionEvent::UserMessage {
                text: "u".to_string()
            },
            concluded.clone(),
            concluded,
        ])
        .is_err());
    }

    #[test]
    fn a_rebind_must_name_the_binding_it_replaces() {
        assert!(reduce(vec![
            started(),
            SessionEvent::WorkspaceRebound {
                previous_workspace_root: "/somewhere-else".to_string(),
                workspace_root: "/x".to_string(),
            },
        ])
        .is_err());

        let state = reduce(vec![
            started(),
            SessionEvent::WorkspaceRebound {
                previous_workspace_root: "/w".to_string(),
                workspace_root: "/x".to_string(),
            },
        ])
        .expect("a truthful rebind is accepted");
        assert_eq!(state.workspace_root, "/x");
        assert_eq!(
            state.origin_workspace_root, "/w",
            "the origin is never rewritten"
        );
    }

    #[test]
    fn usage_accumulates_and_an_absent_count_adds_nothing() {
        let state = reduce(vec![
            started(),
            SessionEvent::UsageRecorded {
                input_tokens: Some(3),
                output_tokens: Some(4),
            },
            SessionEvent::UsageRecorded {
                input_tokens: None,
                output_tokens: Some(1),
            },
        ])
        .expect("reduces");
        assert_eq!(state.total_input_tokens, 3);
        assert_eq!(state.total_output_tokens, 5);
    }

    #[test]
    fn a_repeated_grant_is_recorded_once() {
        let state = reduce(vec![
            started(),
            SessionEvent::PermissionGrantRecorded {
                tool: "edit_file".to_string(),
                target: "a.txt".to_string(),
            },
            SessionEvent::PermissionGrantRecorded {
                tool: "edit_file".to_string(),
                target: "a.txt".to_string(),
            },
        ])
        .expect("reduces");
        assert_eq!(state.grants.len(), 1);
    }

    #[test]
    fn history_drops_a_tool_call_that_never_got_its_result() {
        let state = reduce(vec![
            started(),
            SessionEvent::UserMessage {
                text: "go".to_string(),
            },
            SessionEvent::AssistantMessage {
                text: "reading".to_string(),
                tool_calls: vec![RecordedToolCall {
                    id: "c1".to_string(),
                    name: "read_file".to_string(),
                    input: serde_json::json!({ "path": "a.txt" }),
                }],
                raw_content: Vec::new(),
                responses_state: Vec::new(),
                wire: None,
            },
            SessionEvent::TurnConcluded {
                outcome: TurnConclusion::Interrupted {
                    reason: "the turn was cancelled".to_string(),
                },
            },
        ])
        .expect("reduces");
        let replay = state.history_messages(Wire::AnthropicMessages);
        assert_eq!(replay.messages.len(), 1, "{:?}", replay.messages);
        assert_eq!(replay.messages[0].text(), "go");
    }

    #[test]
    fn history_keeps_a_complete_group_that_precedes_an_incomplete_one() {
        let state = reduce(vec![
            started(),
            SessionEvent::UserMessage {
                text: "go".to_string(),
            },
            SessionEvent::AssistantMessage {
                text: "first".to_string(),
                tool_calls: vec![RecordedToolCall {
                    id: "c1".to_string(),
                    name: "read_file".to_string(),
                    input: serde_json::json!({}),
                }],
                raw_content: Vec::new(),
                responses_state: Vec::new(),
                wire: None,
            },
            SessionEvent::ToolResult {
                call_id: "c1".to_string(),
                tool: "read_file".to_string(),
                ok: true,
                output: "bytes".to_string(),
            },
            SessionEvent::AssistantMessage {
                text: "second".to_string(),
                tool_calls: vec![RecordedToolCall {
                    id: "c2".to_string(),
                    name: "read_file".to_string(),
                    input: serde_json::json!({}),
                }],
                raw_content: Vec::new(),
                responses_state: Vec::new(),
                wire: None,
            },
        ])
        .expect("reduces");
        let replay = state.history_messages(Wire::AnthropicMessages);
        assert_eq!(replay.messages.len(), 3, "{:?}", replay.messages);
        assert_eq!(replay.messages[1].text(), "first");
    }

    #[test]
    fn a_title_is_the_first_line_of_the_first_prompt_and_is_bounded() {
        assert_eq!(title_of("  hello \n world "), Some("hello".to_string()));
        assert_eq!(title_of("   "), None);
        let long = title_of(&"x".repeat(200)).expect("a title");
        assert!(long.len() <= MAX_TITLE_BYTES + 3, "{}", long.len());
        assert!(long.ends_with("..."));
    }

    #[test]
    fn a_workspace_key_ignores_trailing_separators() {
        assert_eq!(workspace_key(Path::new("/w/x/")), "/w/x");
        assert_eq!(workspace_key(Path::new("/w/x")), "/w/x");
        assert_eq!(workspace_key(Path::new("/")), "/");
    }

    #[test]
    fn a_list_limit_is_bounded_and_zero_means_the_default() {
        let filter = ListFilter::new(ListScope::AllWorkspaces);
        assert_eq!(filter.limit, DEFAULT_LIST_LIMIT);
        assert_eq!(filter.clone().with_limit(0).limit, DEFAULT_LIST_LIMIT);
        assert_eq!(filter.clone().with_limit(5).limit, 5);
        assert_eq!(filter.with_limit(100_000).limit, MAX_LIST_LIMIT);
    }

    #[test]
    fn a_selector_parses_the_last_keyword_and_nothing_unsafe() {
        assert_eq!(Selector::parse("last").unwrap(), Selector::Last);
        assert_eq!(
            Selector::parse("abc").unwrap(),
            Selector::Id(SessionId::parse("abc").unwrap())
        );
        assert!(Selector::parse("../escape").is_err());
        assert!(Selector::parse("").is_err());
    }

    #[test]
    fn a_generated_id_is_safe_and_sorts_by_time() {
        let id = SessionId::generate();
        SessionId::parse(id.as_str()).expect("a generated id is a safe id");
        assert!(id.as_str().len() > 13, "{id}");
    }

    #[test]
    fn a_manifest_disagreeing_with_its_log_is_refused_field_by_field() {
        let state = reduce(vec![
            started(),
            SessionEvent::UserMessage {
                text: "u".to_string(),
            },
        ])
        .expect("reduces");
        let good = SessionManifest::from_state(&state, 100, "0".repeat(64), &"a".repeat(32));
        assert!(good.agrees_with(&state).is_ok());

        for mutate in [
            |m: &mut SessionManifest| m.history_turns = 9,
            |m: &mut SessionManifest| m.model = "other".to_string(),
            |m: &mut SessionManifest| m.workspace_root = "/elsewhere".to_string(),
            |m: &mut SessionManifest| m.total_input_tokens = 5,
            |m: &mut SessionManifest| m.last_event_seq = 7,
            |m: &mut SessionManifest| m.updated_at_ms = 0,
            |m: &mut SessionManifest| m.permission_grants = 3,
        ] {
            let mut manifest = good.clone();
            mutate(&mut manifest);
            assert!(manifest.agrees_with(&state).is_err());
        }
    }

    #[test]
    fn a_manifest_validates_its_own_bounds_before_it_is_believed() {
        let state = reduce(vec![started()]).expect("reduces");
        let id = SessionId::parse("s1").expect("id");
        let good = SessionManifest::from_state(&state, 100, "0".repeat(64), &"a".repeat(32));
        assert!(good.validate(&id).is_ok());

        for mutate in [
            |m: &mut SessionManifest| m.schema_version = 2,
            |m: &mut SessionManifest| m.storage_format = "other".to_string(),
            |m: &mut SessionManifest| m.id = "elsewhere".to_string(),
            |m: &mut SessionManifest| m.event_log_bytes = 0,
            |m: &mut SessionManifest| m.event_log_sha256 = "NOTHEX".to_string(),
            |m: &mut SessionManifest| m.permission_mode = "sideways".to_string(),
            |m: &mut SessionManifest| m.event_log_bytes = MAX_EVENT_LOG_BYTES + 1,
        ] {
            let mut manifest = good.clone();
            mutate(&mut manifest);
            assert!(manifest.validate(&id).is_err());
        }
    }

    #[test]
    fn hex_renders_lowercase_fixed_width_bytes() {
        assert_eq!(hex(&[0x00, 0x0f, 0xff]), "000fff");
        assert!(is_digest(&hex(&Sha256::digest(b"x"))));
        assert!(!is_digest("abc"));
    }

    // --- the lock's grace period ------------------------------------------
    //
    // Driven through `acquire_with_grace`'s injected effects, so every case
    // here is a decision the machine made rather than a race it won: no lock,
    // no real clock, no second thread. The clock is a cell these tests move,
    // and how far it moves for a requested pause is the interesting variable --
    // a punctual sleep advances it by exactly what was asked, and a real one is
    // free not to.

    /// Runs the grace period against a scripted sequence of attempts on a
    /// punctual clock, and reports what it decided, every pause it asked for,
    /// and how many looks it took.
    #[cfg(unix)]
    fn grace(outcomes: Vec<AttemptOutcome>) -> (Result<(), AcquireFailure>, Vec<Duration>, usize) {
        grace_on_a_clock(outcomes, |pause| pause)
    }

    /// The same, with `advance` deciding how much time each requested pause
    /// really costs -- which is where an oversleep is expressed.
    #[cfg(unix)]
    fn grace_on_a_clock(
        outcomes: Vec<AttemptOutcome>,
        mut advance: impl FnMut(Duration) -> Duration,
    ) -> (Result<(), AcquireFailure>, Vec<Duration>, usize) {
        use std::cell::Cell;
        use std::rc::Rc;

        // Only the origin comes from the real clock; every reading after it is
        // arithmetic these tests did.
        let clock = Rc::new(Cell::new(Instant::now()));
        let reader = Rc::clone(&clock);
        let mut remaining = outcomes.into_iter();
        let mut attempts = 0usize;
        let mut slept = Vec::new();
        let result = acquire_with_grace(
            || {
                attempts += 1;
                remaining
                    .next()
                    .unwrap_or_else(|| panic!("the machine attempted more times than scripted"))
            },
            |pause| {
                slept.push(pause);
                clock.set(clock.get() + advance(pause));
            },
            move || reader.get(),
        );
        (result, slept, attempts)
    }

    #[cfg(unix)]
    fn would_block(times: usize) -> Vec<AttemptOutcome> {
        (0..times).map(|_| AttemptOutcome::WouldBlock).collect()
    }

    #[cfg(unix)]
    #[test]
    fn a_lock_that_frees_during_the_grace_is_acquired_after_the_documented_backoff() {
        let mut script = would_block(6);
        script.push(AttemptOutcome::Acquired);
        let (result, slept, attempts) = grace(script);

        assert!(result.is_ok(), "the lock came free inside the budget");
        assert_eq!(
            slept,
            [1, 2, 4, 8, 16, 32].map(Duration::from_millis),
            "the backoff doubles from `LOCK_FIRST_BACKOFF`"
        );
        assert_eq!(attempts, 7, "one attempt more than there were pauses");
    }

    #[cfg(unix)]
    #[test]
    fn a_lock_held_throughout_spends_the_budget_exactly_and_then_reports_busy() {
        let (result, slept, attempts) = grace(would_block(32));

        assert!(
            matches!(result, Err(AcquireFailure::HeldThroughGrace)),
            "a lock held for the whole budget is the caller's `Busy`"
        );
        assert_eq!(
            slept,
            [1, 2, 4, 8, 16, 32, 50, 50, 50, 37].map(Duration::from_millis),
            "the backoff caps at `LOCK_MAX_BACKOFF`, and the last pause is \
             clamped to what the budget had left"
        );
        assert_eq!(
            slept.iter().sum::<Duration>(),
            LOCK_RETRY_BUDGET,
            "on a punctual clock the pauses add up to the budget, and no further"
        );
        assert_eq!(
            attempts,
            slept.len() + 1,
            "every pause is followed by a look"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_pause_that_overshoots_the_deadline_ends_the_grace_instead_of_pausing_again() {
        // One 1ms pause that really costs twice the whole budget: a sleep that
        // overslept, or a thread that was descheduled through it.
        let (result, slept, attempts) =
            grace_on_a_clock(would_block(32), |_| LOCK_RETRY_BUDGET * 2);

        assert!(
            matches!(result, Err(AcquireFailure::HeldThroughGrace)),
            "past the deadline there is nothing left to wait with"
        );
        assert_eq!(
            slept,
            [Duration::from_millis(1)],
            "the machine must not issue pauses the deadline can no longer pay for"
        );
        assert_eq!(attempts, 2, "one look, one pause, one look that gives up");
    }

    #[cfg(unix)]
    #[test]
    fn a_free_lock_costs_no_wait_at_all() {
        let (result, slept, attempts) = grace(vec![AttemptOutcome::Acquired]);

        assert!(result.is_ok());
        assert!(slept.is_empty(), "an uncontended lock must not pause");
        assert_eq!(attempts, 1);
    }

    #[cfg(unix)]
    #[test]
    fn an_error_that_is_not_contention_is_not_retried() {
        let (result, slept, attempts) = grace(vec![AttemptOutcome::Failed(io::Error::from(
            io::ErrorKind::PermissionDenied,
        ))]);

        match result {
            Err(AcquireFailure::Failed(err)) => {
                assert_eq!(err.kind(), io::ErrorKind::PermissionDenied)
            }
            _ => panic!("a failed attempt is the caller's io error, not a wait"),
        }
        assert!(slept.is_empty(), "waiting cannot fix a refused syscall");
        assert_eq!(attempts, 1);
    }
}
