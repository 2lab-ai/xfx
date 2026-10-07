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
use crate::gateway::protocol::{Message, ResultStatus, ToolCall};
use crate::permission::Grant;
use crate::provider::Wire;

use super::event::{
    new_identifier, system_now_ms, EventEnvelope, RecordedImage, RecordedToolCall, SessionEvent,
    TurnConclusion, EVENT_SCHEMA_VERSION,
};

/// The directory beside a session's log that holds the copies of its images.
pub const IMAGES_DIR: &str = "images";

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

/// The most bytes a title the user chose may have (`max_title_bytes`,
/// `vercel-labs/fx@c1db9191 src/core/session/session_display_metadata.zig:12`).
///
/// Wider than [`MAX_TITLE_BYTES`] on purpose: that one clips a prompt nobody
/// meant as a title, and this one bounds a title somebody typed as one, which
/// upstream lets run to 240 bytes and refuses past them rather than clipping.
pub const MAX_RENAMED_TITLE_BYTES: usize = 240;

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
    /// What the user said when they answered the approval `call_id` needed.
    ///
    /// A step of the open turn rather than a turn of its own: a user message
    /// opens a turn here, and one opened mid-batch would leave the results still
    /// to come with no call ahead of them.
    ToolFeedback { call_id: String, text: String },
    /// A prompt the person sent into the turn while it ran
    /// ([`SessionEvent::Steered`]), in the words they typed.
    Steered { text: String },
}

/// What the model is told after a turn that did not finish, before the next
/// prompt (`interrupted_turn_context`, `vercel-labs/fx@c1db9191
/// src/core/session/session.zig:52-56`).
pub const TURN_ABORTED: &str = "<turn_aborted>\nThe previous turn ended before completion. \
Any tools or commands may have partially executed. Do not continue this request unless the \
user explicitly asks to continue.\n</turn_aborted>";

/// What closes the answer of a turn that did not finish
/// (`interrupted_before_completion_output`, `session.zig:63`).
pub const ENDED_BEFORE_COMPLETION: &str = "The previous response ended before completion.";

/// What a call its turn stopped under is answered with (`aborted_tool_output`,
/// `session.zig:64`) -- the same words a question the user cancelled answers
/// with, so there is one copy of them.
pub const ABORTED_TOOL_OUTPUT: &str = crate::tools::question::ABORTED_OUTPUT;

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

/// One image a turn's prompt carried, and its saved copy once read back.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TurnImage {
    #[serde(flatten)]
    pub recorded: RecordedImage,
    /// The copy the session kept, as far as this process knows it.
    #[serde(skip)]
    pub snapshot: Snapshot,
}

/// Where a turn image's bytes stand.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Snapshot {
    /// Not read: a listing reads the log and not the images beside it.
    #[default]
    Unread,
    /// Read, and the digest it was recorded with: what a later request sends.
    Loaded(Arc<[u8]>),
    /// Could not be used, for this reason: it is not sent, and the user is
    /// told so ([`DurableState::history_messages`]).
    Unusable(String),
}

/// One complete exchange: what was asked, what happened, and how it ended.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HistoryTurn {
    pub user: String,
    /// The images the prompt carried, in the order they were attached.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<TurnImage>,
    pub steps: Vec<TurnStep>,
    /// `None` for a turn whose conclusion never reached the log, which is what a
    /// crash mid-turn looks like.
    pub outcome: Option<TurnConclusion>,
    /// When it was asked and when it ended, and what the provider said it
    /// wrote: the facts a resumed screen's turn summary is made of
    /// ([`TurnTiming`]).
    #[serde(skip)]
    pub timing: TurnTiming,
    /// Where the turn is in its log: what a [`SessionEvent::ContextCheckpoint`]
    /// is measured against ([`TurnSeqs`]).
    #[serde(skip)]
    pub seqs: TurnSeqs,
}

impl HistoryTurn {
    /// Adds a step recorded in frame `seq`.
    fn push_step(&mut self, seq: u64, step: TurnStep) {
        self.steps.push(step);
        self.seqs.steps.push(seq);
    }

    /// The turn from its step `first` on: its message and images, and the
    /// steps a checkpoint that cut before `first` left.
    pub fn from_step(&self, first: usize) -> HistoryTurn {
        let first = first.min(self.steps.len());
        let mut seqs = self.seqs.clone();
        seqs.steps = seqs.steps.get(first..).unwrap_or_default().to_vec();
        HistoryTurn {
            user: self.user.clone(),
            images: self.images.clone(),
            steps: self.steps[first..].to_vec(),
            outcome: self.outcome.clone(),
            timing: self.timing,
            seqs,
        }
    }

    /// The turn up to its step `end`, as an unfinished turn: what a
    /// checkpoint that cuts there covers of it, with no conclusion to close.
    pub fn until_step(&self, end: usize) -> HistoryTurn {
        let end = end.min(self.steps.len());
        let mut seqs = self.seqs.clone();
        seqs.steps.truncate(end);
        if let Some(last) = seqs.steps.last() {
            seqs.last = *last;
        }
        HistoryTurn {
            user: self.user.clone(),
            images: self.images.clone(),
            steps: self.steps[..end].to_vec(),
            outcome: None,
            timing: self.timing,
            seqs,
        }
    }

    /// Whether step `index` is an assistant step that called tools: the step
    /// each exchange is built around (upstream's `ToolExecutionStep`).
    pub fn calls_tools(&self, index: usize) -> bool {
        matches!(
            self.steps.get(index),
            Some(TurnStep::Assistant { tool_calls, .. }) if !tool_calls.is_empty()
        )
    }

    /// Where each of the turn's exchanges starts: the first at its first
    /// step, each later one right after the results of the step before it.
    /// So a steer, and the answer it cut short, go with the tool step that
    /// came after them -- upstream keeps a steer with the step its
    /// `after_tool_step_count` reaches, and those after the last step with
    /// the last (`selectRecentContext`, `window.zig:219-224`). Empty for a
    /// turn that called no tools. A cut inside a turn falls at one of these
    /// but the first, or at the end of a turn still running
    /// (`ContextHistoryCut`'s `tool_steps` and `steering`).
    pub fn exchange_starts(&self) -> Vec<usize> {
        let calls: Vec<usize> = (0..self.steps.len())
            .filter(|index| self.calls_tools(*index))
            .collect();
        let Some((_, before_last)) = calls.split_last() else {
            return Vec::new();
        };
        std::iter::once(0)
            .chain(before_last.iter().map(|call| self.results_end(*call)))
            .collect()
    }

    /// The step after the results of the tool step at `call`, and after what
    /// the user said beside them.
    fn results_end(&self, call: usize) -> usize {
        let mut end = call + 1;
        while matches!(
            self.steps.get(end),
            Some(TurnStep::ToolResult { .. } | TurnStep::ToolFeedback { .. })
        ) {
            end += 1;
        }
        end
    }
}

/// The sequence numbers of a turn's first and last frames.
///
/// A checkpoint names the last frame it covers, as upstream's does
/// (`covers_through_seq`, `vercel-labs/fx@c1db9191
/// src/core/session/session_event.zig:169-172`); these are what say which
/// turns that is. Read off the envelopes, so nothing new is written for them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TurnSeqs {
    /// The `user_message` frame's.
    pub opened: u64,
    /// The last frame that belongs to the turn.
    pub last: u64,
    /// Each step's frame, in [`HistoryTurn::steps`] order: where a checkpoint
    /// that cuts inside the turn falls among them.
    pub steps: Vec<u64>,
}

/// The compaction in force: which turns it covers and what replaces them in
/// what the model reads ([`SessionEvent::ContextCheckpoint`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Checkpoint {
    /// The last frame it covers.
    pub covers_through_seq: u64,
    /// How many of [`DurableState::turns`] it covers whole, oldest first.
    pub covered_turns: usize,
    /// How many steps of the turn after those it covers, when it cut inside
    /// that turn: the turn's first user message, and its steps from this
    /// one on, are what the model reads after the compacted form.
    pub covered_steps: usize,
    pub compacted: super::checkpoint::Compacted,
    /// How many compactions the session has had, this one counted: what
    /// `/trace` calls `compaction_count` (`compactor.zig:207`).
    pub count: usize,
}

/// When one turn was asked, when it ended, and the tokens its provider
/// reported writing, as its frames recorded them.
///
/// Read off the envelopes' own timestamps and the turn's `usage_recorded`
/// frame, so nothing new is written for it: a log from before this existed
/// replays with the same numbers. What a resumed TUI writes under each turn
/// (`  0s (↑6 ↓40)`), and nothing else reads it -- which is why it is not part
/// of a turn's serialized shape.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TurnTiming {
    /// The `user_message` frame's timestamp.
    pub started_at_ms: i64,
    /// The `turn_concluded` frame's, for a turn that has one.
    pub concluded_at_ms: Option<i64>,
    /// Out as the turn's summary row said it (`turn_output_tokens`), or, on
    /// a frame from before that was kept, what the provider reported for the
    /// answering step. `None` when its `usage_recorded` frame said neither or
    /// there was none.
    pub output_tokens: Option<u64>,
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
    /// The title the user gave the session, the last `session_renamed` in the
    /// log. Outranks the one derived from the first prompt ([`Self::title`]).
    pub renamed: Option<String>,
    /// The effort and speed switches the session last recorded, or `None` for
    /// a session that never recorded them -- a resume then keeps what the
    /// profile says.
    pub turn_settings: Option<crate::provider::effort::TurnSettings>,
    /// What the session's `usage_recorded` frames say about its spend, for
    /// `/usage` ([`crate::usage`]).
    pub usage: RecordedUsage,
    /// The last `context_checkpoint` in the log, when there is one: the turns
    /// it covers are still in [`Self::turns`] -- a resumed screen shows them
    /// -- and [`Self::history_messages`] sends its compacted form instead.
    pub checkpoint: Option<Checkpoint>,
}

/// What a session's `usage_recorded` frames add up to, kept per request
/// rather than summed: `/usage` windows them by time, and a sum cannot be cut
/// at a window's edge.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RecordedUsage {
    /// Every request the session's turns completed, in log order.
    pub requests: Vec<crate::session::RequestUsage>,
    /// When each frame that predates request records was written: a turn's
    /// usage that was recorded only as totals, which no window can place
    /// (upstream's `legacy` completeness, `usage_report.zig:90-95`).
    pub legacy_at_ms: Vec<i64>,
    /// When each turn that may have been billed without a record ended.
    pub unaccounted_at_ms: Vec<i64>,
    pub lines_added: u64,
    pub lines_removed: u64,
}

impl RecordedUsage {
    /// The requests whose bill is still to be asked for
    /// ([`crate::session::RequestUsage::pending`]).
    pub fn pending(&self) -> impl Iterator<Item = &crate::session::RequestUsage> {
        self.requests.iter().filter(|request| request.pending())
    }

    /// Puts what the Gateway said later in place of the pending requests it
    /// was about (`applyGeneration`, `session_usage.zig:1080-1126`, and
    /// `rejectPendingGeneration`, `:2171-2187`).
    ///
    /// A bill takes the request's place under the same generation id and keeps
    /// how long the request took, which only this side measured. A rejection
    /// leaves the request with no cost and nowhere to ask: spend that stays
    /// missing. Either applies only to a request still pending, so a second
    /// answer for one generation changes nothing.
    fn settle(&mut self, settled: &[crate::session::RequestUsage], rejected: &[String]) {
        for bill in settled {
            let Some(id) = bill.generation_id.as_deref() else {
                continue;
            };
            if let Some(request) = self
                .requests
                .iter_mut()
                .find(|request| request.pending() && request.generation_id.as_deref() == Some(id))
            {
                let duration_ms = request.duration_ms;
                *request = crate::session::RequestUsage {
                    duration_ms,
                    origin: None,
                    ..bill.clone()
                };
            }
        }
        for id in rejected {
            if let Some(request) = self
                .requests
                .iter_mut()
                .find(|request| request.pending() && request.generation_id.as_deref() == Some(id))
            {
                request.origin = None;
            }
        }
    }
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
            renamed: None,
            turn_settings: None,
            usage: RecordedUsage::default(),
            checkpoint: None,
        }
    }

    /// An empty state, for a test that builds one by hand.
    #[cfg(test)]
    pub(crate) fn empty_for_tests() -> Self {
        Self::empty()
    }

    /// The turns a compaction has not covered yet: what the model reads word
    /// for word, and what the next compaction may compact.
    pub fn uncovered_turns(&self) -> &[HistoryTurn] {
        let covered = self
            .checkpoint
            .as_ref()
            .map_or(0, |checkpoint| checkpoint.covered_turns);
        &self.turns[covered.min(self.turns.len())..]
    }

    /// [`Self::uncovered_turns`] as the model reads them: the first one cut
    /// to what a checkpoint that cut inside it left -- its first user
    /// message, its images, and its steps from the first one the checkpoint
    /// does not cover (`split`'s `retained_history`,
    /// `vercel-labs/fx@c1db9191 src/core/compactor/window.zig:286-290`).
    pub fn retained_turns(&self) -> Vec<std::borrow::Cow<'_, HistoryTurn>> {
        let covered_steps = self
            .checkpoint
            .as_ref()
            .map_or(0, |checkpoint| checkpoint.covered_steps);
        self.uncovered_turns()
            .iter()
            .enumerate()
            .map(|(index, turn)| {
                if index == 0 && covered_steps > 0 {
                    std::borrow::Cow::Owned(turn.from_step(covered_steps))
                } else {
                    std::borrow::Cow::Borrowed(turn)
                }
            })
            .collect()
    }

    /// What the session is called: the title the user gave it, or else its
    /// first user message, clipped.
    ///
    /// The order is upstream's (`readNativeResumeDisplay`,
    /// `app_session_runtime.zig:4644-4655`: the conversation title when there
    /// is one, a title derived from the history when there is not), and the
    /// manifest publishes this, so a listing, a resume and the window title all
    /// say the same thing.
    pub fn title(&self) -> Option<String> {
        self.renamed
            .clone()
            .or_else(|| self.turns.first().and_then(|turn| title_of(&turn.user)))
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
    ///
    /// **A turn that did not finish is closed the way upstream closes one**
    /// (`appendHistoryMessages`' `.interrupted` arm, `vercel-labs/fx@c1db9191
    /// src/core/session/session.zig:2695-2747`) -- whether the user stopped it
    /// or it failed, as upstream's arm reads neither reason: a call that never
    /// got its result is answered `aborted by user` as an error, and otherwise
    /// the turn's last answer is closed by a sentence saying it ended before
    /// completion; then the model is told, in a message of its own, not to
    /// carry the turn on unasked ([`TURN_ABORTED`]). A turn with no conclusion
    /// in the log -- a crash -- is replayed as it always was, the incomplete
    /// group dropped.
    ///
    /// **A compacted conversation is sent in place of the turns it covers**
    /// ([`Self::checkpoint`]): one user message holding its text, then the
    /// turns after it as they are -- the shape upstream sends after a
    /// compaction (`receipts/w4/README.md`, "The next request").
    pub fn history_messages(&self, active: Wire) -> ReplayedHistory {
        self.history_of(&self.retained_turns(), active)
    }

    /// The history a turn still in progress carries once a compaction in its
    /// middle has replaced what came before: the compacted form and every
    /// later turn but the open one, whose own messages the turn keeps -- and
    /// trims itself, when the compaction took its earlier steps.
    pub fn history_messages_before_open(&self, active: Wire) -> ReplayedHistory {
        let mut turns = self.retained_turns();
        if turns.last().is_some_and(|turn| turn.outcome.is_none()) {
            turns.pop();
        }
        self.history_of(&turns, active)
    }

    fn history_of(
        &self,
        turns: &[std::borrow::Cow<'_, HistoryTurn>],
        active: Wire,
    ) -> ReplayedHistory {
        let mut out = Vec::new();
        let mut notices = Vec::new();
        if let Some(checkpoint) = &self.checkpoint {
            out.push(Message::user(super::checkpoint::render(
                &checkpoint.compacted,
            )));
        }
        for turn in turns {
            out.extend(turn_messages(turn.as_ref(), &active, &mut notices));
        }
        ReplayedHistory {
            messages: out,
            notices,
        }
    }
}

/// Whether a recorded result is the answer of a call its turn was stopped
/// under: a failure whose first line is [`ABORTED_TOOL_OUTPUT`] -- alone, or
/// followed by xfx's own note that a stopped command left something running
/// (`crate::tools::terminal`). No tool's own failure begins with those words:
/// every other failure is a sentence xfx writes about the call.
fn stopped_answer(ok: bool, output: &str) -> bool {
    !ok && output.split('\n').next() == Some(ABORTED_TOOL_OUTPUT)
}

/// One turn as the messages a request carries for it on `active`, its user
/// message first ([`DurableState::history_messages`]). A reasoning state that
/// does not replay on `active` is dropped with a line in `notices`.
pub fn turn_messages(turn: &HistoryTurn, active: &Wire, notices: &mut Vec<String>) -> Vec<Message> {
    let mut out = Vec::new();
    // Every image the prompt carried goes with it again, as upstream
    // re-sends the history's images on every request
    // (`cap/fx-image-40x120-bodies/002-completion.json`) -- the bytes
    // the session saved, checked against the digest it saved them
    // with. One it could not read back is left out and said so, the
    // way a dropped reasoning record is: the prompt still names it.
    let mut images = Vec::new();
    for image in &turn.images {
        let label = crate::image::MediaType::parse(&image.recorded.media_type);
        match (&image.snapshot, label) {
            (Snapshot::Loaded(bytes), Some(media_type)) => images.push((media_type, bytes)),
            (Snapshot::Unusable(reason), _) => notices.push(format!(
                "xfx: image #{} of this session could not be read back ({reason}), \
                 so it was not sent again",
                image.recorded.id
            )),
            _ => notices.push(format!(
                "xfx: image #{} of this session was not read back, so it was not sent \
                 again",
                image.recorded.id
            )),
        }
    }
    out.push(if images.is_empty() {
        Message::user(turn.user.clone())
    } else {
        Message::user_with_images(turn.user.clone(), images)
    });
    let opened = out.len();
    let mut pending: Vec<Message> = Vec::new();
    let mut awaiting: Vec<String> = Vec::new();
    // The calls of the step being answered, by id, for the aborted
    // results an interrupted turn owes them; and every tool that did
    // return in this turn, in order, for the sentence that names them.
    let mut named: Vec<(String, String)> = Vec::new();
    let mut finished: Vec<String> = Vec::new();
    // Whether the step being answered has a call that was stopped
    // rather than finished (see the `ToolResult` arm).
    let mut stopped = false;
    for step in &turn.steps {
        match step {
            TurnStep::Assistant {
                text,
                tool_calls,
                raw_content,
                responses_state,
                wire,
            } => {
                stopped = false;
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
                named.extend(
                    calls
                        .iter()
                        .map(|call| (call.id.clone(), call.name.clone())),
                );
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
                let state: &[serde_json::Value] = match (&recorded, active) {
                    (Wire::AnthropicMessages, Wire::AnthropicMessages) => raw_content,
                    (Wire::CodexResponses, Wire::CodexResponses)
                    | (Wire::GrokResponses, Wire::GrokResponses) => responses_state,
                    _ => &[],
                };
                if state.is_empty() && !(raw_content.is_empty() && responses_state.is_empty()) {
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
                ok,
                output,
            } => {
                // Replayed as it was sent: a failure as a failure, a
                // refusal as a refusal.
                let status = if *ok {
                    ResultStatus::Success
                } else if crate::tools::is_refusal(tool, output) {
                    ResultStatus::Denied
                } else {
                    ResultStatus::Failure
                };
                pending.push(Message::tool_ended(call_id, tool, output.clone(), status));
                awaiting.retain(|id| id != call_id);
                // A call that answered `aborted by user` did not
                // finish: its turn was stopped under it (a question the
                // user cancelled, `crate::tools::question`; a command
                // the user stopped, `crate::tools::terminal`). It is the
                // call upstream keeps on the interrupted turn rather
                // than among the completed ones
                // (`InterruptedHistoryTurn.tool_call`,
                // `session.zig:2701-2736`).
                if stopped_answer(*ok, output) {
                    stopped = true;
                } else {
                    finished.push(tool.clone());
                }
            }
            TurnStep::ToolFeedback { text, .. } => {
                // Into `pending`, with `awaiting` untouched: this is
                // not an answer to a call, so it cannot complete a
                // group. The machine emits it only after every result
                // of its step, so `awaiting` is already empty here on a
                // complete step and the flush below carries the
                // sentence out with its group -- and on an incomplete
                // one it is dropped with the group, rather than
                // arriving as a user message about a result the model
                // was never shown.
                pending.push(Message::user(text.clone()));
            }
            // The person's words as a user message of their own, as
            // upstream's history replays a steer: plain, not in the
            // wrapper the model was given live (`fx-interrupt-steer-
            // 40x120-bodies/005-completion.json`: `user "what are you
            // doing"`). Delivered only after every result of its step,
            // so it goes out with the group.
            TurnStep::Steered { text } => {
                pending.push(Message::user(text.clone()));
            }
        }
        if awaiting.is_empty() {
            out.append(&mut pending);
        }
    }
    if matches!(turn.outcome, Some(TurnConclusion::Interrupted { .. })) {
        let stopped_under = stopped || !awaiting.is_empty();
        close_interrupted(
            &mut out,
            opened,
            pending,
            &awaiting,
            &named,
            &finished,
            stopped_under,
        );
    }
    out
}

/// Closes a turn that did not finish, as upstream's history does
/// ([`DurableState::history_messages`]): `opened` is where the turn's own
/// messages begin in `out`, `pending` and `awaiting` are the step that was
/// still waiting for results, `named` every call of the turn by id,
/// `finished` the tools that returned, and `stopped_under` whether the turn
/// stopped under a call -- one never answered, or one that answered
/// `aborted by user`.
///
/// Upstream's two shapes (`session.zig:2695-2747`): a turn stopped under a
/// call ends with that call answered `aborted by user` as a failure, the tools
/// that did return named on the message that made it; any other ends with an
/// assistant message closing the answer that was cut off. Both are followed by
/// the [`TURN_ABORTED`] user message.
fn close_interrupted(
    out: &mut Vec<Message>,
    opened: usize,
    mut pending: Vec<Message>,
    awaiting: &[String],
    named: &[(String, String)],
    finished: &[String],
    stopped_under: bool,
) {
    use crate::gateway::protocol::{ContentPart, Role};
    // `formatCompletedToolSummary` (`session.zig:3020-3032`).
    let summary = (!finished.is_empty()).then(|| {
        format!(
            "Interrupted by user after completing {} tool call{}: {}.",
            finished.len(),
            if finished.len() == 1 { "" } else { "s" },
            finished.join(", ")
        )
    });
    if stopped_under {
        // The call the turn stopped under, answered as an error so the group
        // is whole (`formatInterruptedToolOutput`, `session.zig:82-108`) --
        // beside the step's other results, ahead of any sentence the user said
        // about them, so no user message stands between a call and an answer.
        let aborted: Vec<Message> = awaiting
            .iter()
            .map(|id| {
                let tool = named
                    .iter()
                    .rev()
                    .find(|(call, _)| call == id)
                    .map(|(_, tool)| tool.as_str())
                    .unwrap_or_default();
                Message::tool_error(id.clone(), tool, ABORTED_TOOL_OUTPUT)
            })
            .collect();
        let at = pending
            .iter()
            .rposition(|message| message.role != Role::User)
            .map_or(pending.len(), |last| last + 1);
        pending.splice(at..at, aborted);
        out.append(&mut pending);
        // `formatInterruptedAssistantToolContent` (`session.zig:3034-3055`):
        // the tools that did return are named after the text of the message
        // that made the call. Not on a message carried as the provider's own
        // blocks, which are sent back exactly as they arrived or not at all.
        if let Some(summary) = summary {
            let caller = out[opened..].iter_mut().rev().find(|message| {
                message.role == Role::Assistant
                    && message
                        .content
                        .iter()
                        .any(|part| matches!(part, ContentPart::ToolCall(_)))
            });
            if let Some(caller) = caller.filter(|caller| caller.raw_blocks().is_none()) {
                match caller.content.first_mut() {
                    Some(ContentPart::Text { text }) => {
                        text.push_str("\n\n");
                        text.push_str(&summary);
                    }
                    _ => caller
                        .content
                        .insert(0, ContentPart::Text { text: summary }),
                }
            }
        }
    } else {
        // The answer that was cut off: the turn's last message when it is the
        // assistant's text and nothing else -- a step that answered with text
        // alone would have ended the turn, so one here is the partial answer
        // the turn stopped in.
        let partial = match out.last() {
            Some(last)
                if out.len() > opened
                    && last.role == Role::Assistant
                    && last
                        .content
                        .iter()
                        .all(|part| matches!(part, ContentPart::Text { .. })) =>
            {
                out.pop().map(|message| message.text())
            }
            _ => None,
        }
        .filter(|text| !text.is_empty());
        // `formatInterruptedAssistantClosedContent` (`session.zig:3057-3069`).
        let closing = match (partial, summary) {
            (Some(text), Some(summary)) => format!("{text}\n\n{summary}"),
            (None, Some(summary)) => summary,
            (Some(text), None) => format!("{text}\n\n{ENDED_BEFORE_COMPLETION}"),
            (None, None) => ENDED_BEFORE_COMPLETION.to_string(),
        };
        out.push(Message::assistant(Some(&closing), Vec::new()));
    }
    out.push(Message::user(TURN_ABORTED));
}

/// The first line of `text`, trimmed and clipped to a character boundary.
///
/// Also the TUI's window title for a session (`crate::tui`'s shell), so the
/// window and a listing of the session say the same thing.
pub(crate) fn title_of(text: &str) -> Option<String> {
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

/// Why a title the user typed cannot be a session's title.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitleError {
    /// Nothing but blanks.
    Empty,
    /// Over [`MAX_RENAMED_TITLE_BYTES`].
    TooLong,
    /// A control character: a line break, a tab, an escape, a DEL.
    NotPrintable,
}

/// The title `raw` names, trimmed, or why it cannot be one
/// (`validateSessionTitle`, `vercel-labs/fx@c1db9191
/// src/core/app/app_session_runtime.zig:3790-3799`).
///
/// Trimmed of spaces, tabs and line breaks at both ends, then refused when
/// nothing is left, when it is over [`MAX_RENAMED_TITLE_BYTES`], or when any
/// byte of it is below `0x20` or is `0x7f` -- in that order, so a title that is
/// both too long and unprintable is called too long, as upstream calls it. A
/// `&str` is UTF-8 already, which is upstream's fourth refusal.
///
/// The store's own rule, because the store is what the title is written into:
/// both shells and the runtime ask it rather than keeping a copy each.
pub fn validate_title(raw: &str) -> Result<&str, TitleError> {
    let title = raw.trim_matches([' ', '\t', '\r', '\n']);
    if title.is_empty() {
        return Err(TitleError::Empty);
    }
    if title.len() > MAX_RENAMED_TITLE_BYTES {
        return Err(TitleError::TooLong);
    }
    if title.bytes().any(|byte| byte < 0x20 || byte == 0x7f) {
        return Err(TitleError::NotPrintable);
    }
    Ok(title)
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

/// What [`SessionStore::usage_since`] found.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UsageScan {
    /// Each readable session's recorded spend.
    pub sessions: Vec<RecordedUsage>,
    /// One entry per session that could not be read: when its manifest says it
    /// was last written, or `None` when not even that could be read.
    pub unreadable: Vec<Option<i64>>,
    /// Whether the scan cap left sessions unread.
    pub truncated: bool,
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

    /// What every session written since `since_ms` recorded about its spend,
    /// for `/usage` ([`crate::usage`]).
    ///
    /// Each session is replayed through its published boundary and
    /// cross-checked against its manifest, as [`Self::detail`] does, so a
    /// window adds up only what a resume would believe. A session last written
    /// before `since_ms` is not opened: nothing it holds is that recent. One
    /// that cannot be read is **reported, not skipped** -- its spend is missing
    /// from whichever windows it could be in, and the dashboard has to say so.
    /// The store's scan cap applies, newest kept, as it does to a listing.
    ///
    /// `Ok(None)` once `cancel` is set: it is looked at before each session is
    /// opened, so a reader nobody wants any more stops after the one it is in
    /// (`collectFromHomeConservativeCancelable`, `vercel-labs/fx@c1db9191
    /// src/core/session/usage_recovery.zig:43-47`).
    pub fn usage_since(
        &self,
        since_ms: i64,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<Option<UsageScan>, SessionError> {
        let cancelled = || cancel.load(std::sync::atomic::Ordering::Acquire);
        if cancelled() {
            return Ok(None);
        }
        self.verify_store_dirs()?;
        let mut scan = UsageScan::default();
        let entries = match fs::read_dir(&self.sessions_dir) {
            Ok(entries) => entries,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Some(scan)),
            Err(err) => return Err(io_error(&self.sessions_dir, err)),
        };
        let mut names: Vec<String> = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|err| io_error(&self.sessions_dir, err))?;
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
        names.sort();
        if names.len() > self.scan_cap {
            names.drain(..names.len() - self.scan_cap);
            scan.truncated = true;
        }
        for name in names {
            if cancelled() {
                return Ok(None);
            }
            // Not a session's name is not a session: a stray file beside them
            // holds no spend.
            let Ok(id) = SessionId::parse(&name) else {
                continue;
            };
            let manifest = match self.read_manifest(&id) {
                Ok(manifest) => manifest,
                Err(_) => {
                    scan.unreadable.push(None);
                    continue;
                }
            };
            if manifest.updated_at_ms < since_ms {
                continue;
            }
            match self.replay(&id, &manifest) {
                Ok(replay) => scan.sessions.push(replay.state.usage),
                Err(_) => scan.unreadable.push(Some(manifest.updated_at_ms)),
            }
        }
        Ok(Some(scan))
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
        self.resume_within(selector, workspace, Wait::Grace)
    }

    /// [`Self::resume`], refusing at once when another process holds the
    /// session rather than waiting out [`LOCK_RETRY_BUDGET`] for it.
    ///
    /// The `/resume` picker's (`resumeSelectedSession` takes the lock with a
    /// zero deadline, `app_session_runtime.zig:2263-2265`): what a person
    /// picking from a list is owed for a session open elsewhere is the reason,
    /// at once, on the row they picked -- and the runtime thread that asks is
    /// one a prompt typed next would otherwise wait behind.
    pub fn resume_without_waiting(
        &self,
        selector: &Selector,
        workspace: &Path,
    ) -> Result<Resumed, SessionError> {
        self.resume_within(selector, workspace, Wait::None)
    }

    fn resume_within(
        &self,
        selector: &Selector,
        workspace: &Path,
        wait: Wait,
    ) -> Result<Resumed, SessionError> {
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
        lock_exclusive_within(&file, &log_path, &id, wait)?;

        let manifest = self.read_manifest(&id)?;
        let replay = self.replay(&id, &manifest)?;

        // The tail past the boundary is not part of this session and never will
        // be: the next append has to own the offset it starts at.
        file.set_len(manifest.event_log_bytes)
            .map_err(|err| io_error(&log_path, err))?;
        file.sync_data().map_err(|err| io_error(&log_path, err))?;

        let mut hasher = Sha256::new();
        hasher.update(&replay.bytes);

        // The images its prompts carried, read back now so every request of
        // the continued conversation can send them again.
        let mut state = replay.state;
        read_snapshots(&dir, &mut state);

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
            state,
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
            apply(&mut state, &envelope).map_err(corrupt)?;
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

    /// Where this session's saved-session permission rules are kept
    /// (`crate::permission::session_rules`), beside its log.
    pub fn permission_rules_path(&self) -> std::path::PathBuf {
        crate::permission::session_rules::path_in(&self.store.session_dir(self.session.id()))
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
        // A turn that recorded what its requests cost is the moment `/usage`
        // began counting, if none has been before it ([`crate::usage::start_tracking`]).
        let counted = matches!(
            &event,
            SessionEvent::UsageRecorded { requests, unaccounted, .. }
                if !requests.is_empty() || *unaccounted
        );
        let outcome = self
            .store
            .append(&mut self.session, event)
            .and_then(|()| self.store.publish(&mut self.session));
        match outcome {
            Err(err) => {
                self.failure = Some(format!("cannot record the `{kind}` session event: {err}"));
            }
            Ok(()) if counted => crate::usage::start_tracking(
                self.store.profile_dir(),
                self.session.state().updated_at_ms,
            ),
            Ok(()) => {}
        }
    }

    /// Makes durable every approval in `granted` this log does not already
    /// carry, so an "always" answered during a turn survives to the next
    /// resume.
    ///
    /// **One method rather than one per front end.** `xfx ask`, the line shell
    /// and the TUI all collect approvals into a shared permission ledger and
    /// all owe the same step after the turn; three copies of it is three places
    /// for a surface to quietly stop taking it, which is exactly how the TUI
    /// shipped without one. Callers pass the ledger's grants (`to_vec` first --
    /// a commit is a disk write and the ledger is behind a lock) and this
    /// decides what is new.
    ///
    /// **Judged against the log's own grants, not against a snapshot the caller
    /// took.** The log is what a resume reads, so "already recorded" is a
    /// question only the log can answer -- and a surface that holds one
    /// conversation across many turns would otherwise have to keep its snapshot
    /// in step by hand, turn after turn, forever.
    pub fn record_new_grants(&mut self, granted: &[Grant]) {
        let unrecorded: Vec<Grant> = {
            let recorded = &self.state().grants;
            granted
                .iter()
                .filter(|grant| !recorded.contains(grant))
                .cloned()
                .collect()
        };
        for grant in unrecorded {
            self.commit(SessionEvent::PermissionGrantRecorded {
                tool: grant.tool,
                target: grant.target,
            });
        }
    }
}

impl SessionRecorder {
    /// Saves a copy of each of `images` beside the log and records them on the
    /// turn the last user message opened.
    ///
    /// The copies come first and the record after, so a log never names a copy
    /// that is not there; each is `images/` + [`crate::image::snapshot_name`],
    /// owner-only, written once and `fsync`ed, as upstream keeps its own
    /// (`session_store.zig:102-118`, mode 0600). A copy that cannot be written
    /// is this recorder's failure, reported after the turn like any other --
    /// the turn still sends the images, which it holds in memory.
    pub fn attach_images(&mut self, images: &[crate::image::Image]) {
        if images.is_empty() || self.failure.is_some() {
            return;
        }
        if let Err(err) = save_snapshots(self.session.dir(), images) {
            self.failure = Some(format!("cannot save the attached images: {err}"));
            return;
        }
        self.commit(SessionEvent::ImagesAttached {
            images: images
                .iter()
                .map(|image| RecordedImage {
                    id: image.id,
                    path: image.path.display().to_string(),
                    media_type: image.media_type.label().to_string(),
                    sha256: image.sha256.clone(),
                    bytes: image.bytes.len() as u64,
                })
                .collect(),
        });
        if self.failure.is_some() {
            return;
        }
        // The bytes are the ones just saved, so the state that re-sends them
        // holds them without reading them back.
        if let Some(turn) = self.session.state.turns.last_mut() {
            for (kept, image) in turn.images.iter_mut().zip(images) {
                kept.snapshot = Snapshot::Loaded(Arc::clone(&image.bytes));
            }
        }
    }
}

impl TurnJournal for SessionRecorder {
    fn record(&mut self, event: SessionEvent) {
        self.commit(event);
    }

    fn state(&self) -> Option<&DurableState> {
        Some(SessionRecorder::state(self))
    }

    fn attach_images(&mut self, images: &[crate::image::Image]) {
        SessionRecorder::attach_images(self, images);
    }
}

/// Writes each image's copy into `dir`'s [`IMAGES_DIR`], creating it
/// owner-only. A copy already there with the same name is the same bytes by
/// construction -- the name carries the digest -- and is checked rather than
/// trusted.
fn save_snapshots(dir: &Path, images: &[crate::image::Image]) -> Result<(), SessionError> {
    let folder = dir.join(IMAGES_DIR);
    match fs::create_dir(&folder) {
        Ok(()) => set_private_dir_mode(&folder)?,
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {}
        Err(err) => return Err(io_error(&folder, err)),
    }
    verify_private(&folder, 0o700)?;
    for image in images {
        let path = folder.join(image.snapshot_name());
        match create_private_file(&path) {
            Ok(mut file) => {
                file.write_all(&image.bytes)
                    .and_then(|()| file.sync_all())
                    .map_err(|err| io_error(&path, err))?;
            }
            Err(SessionError::Io { source, .. })
                if source.kind() == io::ErrorKind::AlreadyExists =>
            {
                if let Err(reason) = read_snapshot(&path, &image.sha256, image.bytes.len() as u64) {
                    return Err(SessionError::Io {
                        path,
                        source: io::Error::new(io::ErrorKind::AlreadyExists, reason),
                    });
                }
            }
            Err(err) => return Err(err),
        }
    }
    sync_directory(&folder)
}

/// Reads back the copy of every image `state`'s turns carried, from `dir`.
///
/// Each is checked before it is believed: a regular file reached without
/// following a symlink, the size it was recorded with, and the digest. One that
/// fails any of them is [`Snapshot::Unusable`] with the reason, and the
/// conversation goes on without it.
fn read_snapshots(dir: &Path, state: &mut DurableState) {
    let folder = dir.join(IMAGES_DIR);
    for turn in &mut state.turns {
        for image in &mut turn.images {
            let path = folder.join(crate::image::snapshot_name(
                image.recorded.id,
                &image.recorded.sha256,
            ));
            image.snapshot =
                match read_snapshot(&path, &image.recorded.sha256, image.recorded.bytes) {
                    Ok(bytes) => Snapshot::Loaded(bytes),
                    Err(reason) => Snapshot::Unusable(reason),
                };
        }
    }
}

/// One copy, checked, or why it cannot be used.
fn read_snapshot(path: &Path, sha256: &str, size: u64) -> Result<Arc<[u8]>, String> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = options.open(path).map_err(|err| match err.kind() {
        io::ErrorKind::NotFound => "its saved copy is missing".to_string(),
        _ => format!("its saved copy cannot be opened: {err}"),
    })?;
    let metadata = file
        .metadata()
        .map_err(|err| format!("its saved copy cannot be read: {err}"))?;
    if !metadata.is_file() || metadata.len() != size || size > crate::image::MAX_IMAGE_BYTES {
        return Err("its saved copy is not the file that was saved".to_string());
    }
    let mut bytes = Vec::with_capacity(usize::try_from(size).unwrap_or(0));
    (&mut file)
        .take(size)
        .read_to_end(&mut bytes)
        .map_err(|err| format!("its saved copy cannot be read: {err}"))?;
    if crate::image::digest(&bytes) != sha256 {
        return Err("its saved copy is not the file that was saved".to_string());
    }
    Ok(Arc::from(bytes))
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
            effort,
            fast_mode,
            ultrafast_mode,
        } => {
            if let Some(model) = model {
                state.model = model.clone();
            }
            if let Some(raw) = permission_mode {
                state.permission_mode = PermissionMode::parse(raw)
                    .ok_or_else(|| format!("unknown permission mode `{raw}`"))?;
            }
            if effort.is_some() || fast_mode.is_some() || ultrafast_mode.is_some() {
                let mut settings = state.turn_settings.clone().unwrap_or_default();
                if let Some(raw) = effort {
                    settings.effort = crate::provider::effort::Effort::parse(raw)
                        .ok_or_else(|| format!("unknown reasoning effort `{raw}`"))?;
                }
                if let Some(on) = fast_mode {
                    settings.fast = *on;
                }
                if let Some(on) = ultrafast_mode {
                    settings.ultrafast = *on;
                }
                state.turn_settings = Some(settings);
            }
        }
        SessionEvent::ProjectContextRecorded { sources, .. } => {
            state.context_sources = sources.clone();
        }
        SessionEvent::UserMessage { text } => state.turns.push(HistoryTurn {
            user: text.clone(),
            images: Vec::new(),
            steps: Vec::new(),
            outcome: None,
            timing: TurnTiming {
                started_at_ms: envelope.timestamp_ms,
                ..TurnTiming::default()
            },
            seqs: TurnSeqs {
                opened: envelope.seq,
                last: envelope.seq,
                steps: Vec::new(),
            },
        }),
        SessionEvent::ImagesAttached { images } => {
            let turn = open_turn(state, "an image", envelope.seq)?;
            // Right behind the message that names them, once: anything else is
            // not the order a turn writes them in.
            if !turn.steps.is_empty() || !turn.images.is_empty() {
                return Err("a turn's images appear after its first step or twice".to_string());
            }
            for image in images {
                if crate::image::MediaType::parse(&image.media_type).is_none() {
                    return Err(format!("an image names the type `{}`", image.media_type));
                }
                if !is_digest(&image.sha256) || image.bytes > crate::image::MAX_IMAGE_BYTES {
                    return Err(format!(
                        "image #{} has a digest or a size xfx would not have written",
                        image.id
                    ));
                }
                turn.images.push(TurnImage {
                    recorded: image.clone(),
                    snapshot: Snapshot::Unread,
                });
            }
        }
        SessionEvent::AssistantMessage {
            text,
            tool_calls,
            raw_content,
            responses_state,
            wire,
        } => {
            open_turn(state, "an assistant step", envelope.seq)?.push_step(
                envelope.seq,
                TurnStep::Assistant {
                    text: text.clone(),
                    tool_calls: tool_calls.clone(),
                    raw_content: raw_content.clone(),
                    responses_state: responses_state.clone(),
                    wire: wire.clone(),
                },
            );
        }
        SessionEvent::ToolResult {
            call_id,
            tool,
            ok,
            output,
        } => {
            open_turn(state, "a tool result", envelope.seq)?.push_step(
                envelope.seq,
                TurnStep::ToolResult {
                    call_id: call_id.clone(),
                    tool: tool.clone(),
                    ok: *ok,
                    output: output.clone(),
                },
            );
        }
        SessionEvent::ToolFeedback { call_id, text } => {
            // A step of the open turn, exactly as a result is. Reducing it the
            // way a `UserMessage` is reduced would push a new turn and leave
            // the rest of the batch's results with no call ahead of them.
            open_turn(state, "an approval amendment", envelope.seq)?.push_step(
                envelope.seq,
                TurnStep::ToolFeedback {
                    call_id: call_id.clone(),
                    text: text.clone(),
                },
            );
        }
        SessionEvent::Steered { text } => {
            open_turn(state, "a steer", envelope.seq)?
                .push_step(envelope.seq, TurnStep::Steered { text: text.clone() });
        }
        SessionEvent::PermissionGrantRecorded { tool, target } => {
            let grant = Grant::new(tool.clone(), target.clone());
            if !state.grants.contains(&grant) {
                state.grants.push(grant);
            }
        }
        // Every approval recorded so far is withdrawn; one recorded after
        // this is a new answer and stands.
        SessionEvent::PermissionGrantsCleared => state.grants.clear(),
        SessionEvent::UsageRecorded {
            input_tokens,
            output_tokens,
            turn_output_tokens,
            requests,
            unaccounted,
            lines_added,
            lines_removed,
            settled,
            rejected,
        } => {
            // A frame with neither requests nor a cut-off one is one a binary
            // before request records wrote: usage that happened and cannot be
            // placed in any window. One that settles bills is the runtime's
            // answer about earlier requests, and is none of those.
            let settles = !settled.is_empty() || !rejected.is_empty();
            if requests.is_empty() && !unaccounted && !settles {
                state.usage.legacy_at_ms.push(envelope.timestamp_ms);
            }
            state.usage.settle(settled, rejected);
            if *unaccounted {
                state.usage.unaccounted_at_ms.push(envelope.timestamp_ms);
            }
            state.usage.requests.extend(requests.iter().cloned());
            state.usage.lines_added = state.usage.lines_added.saturating_add(*lines_added);
            state.usage.lines_removed = state.usage.lines_removed.saturating_add(*lines_removed);
            state.total_input_tokens = state
                .total_input_tokens
                .saturating_add(input_tokens.unwrap_or(0));
            state.total_output_tokens = state
                .total_output_tokens
                .saturating_add(output_tokens.unwrap_or(0));
            // The turn it is about, when one is open: it is written just
            // before that turn's conclusion (`agent::machine`'s
            // `run_journaled`). One outside a turn still counts toward the
            // totals, as it always has. What its summary row said, where the
            // frame kept it; an older frame's answering step otherwise, which
            // is what the row said when that binary wrote it.
            if let Some(turn) = state.turns.last_mut().filter(|turn| turn.outcome.is_none()) {
                if let Some(output) = turn_output_tokens.or(*output_tokens) {
                    turn.timing.output_tokens = Some(
                        turn.timing
                            .output_tokens
                            .unwrap_or(0)
                            .saturating_add(output),
                    );
                }
            }
        }
        SessionEvent::TurnConcluded { outcome } => {
            let turn = open_turn(state, "a turn conclusion", envelope.seq)?;
            turn.outcome = Some(outcome.clone());
            turn.timing.concluded_at_ms = Some(envelope.timestamp_ms);
        }
        SessionEvent::SessionRenamed { title } => {
            // A title the reader would have to second-guess is not one this
            // build wrote: [`validate_title`] runs before every write.
            if validate_title(title) != Ok(title.as_str()) {
                return Err("a rename names a title xfx would not have written".to_string());
            }
            state.renamed = Some(title.clone());
        }
        SessionEvent::ContextCheckpoint {
            covers_through_seq,
            compacted,
        } => {
            // Upstream's coverage rule (`session_event.zig:245-256`): never
            // behind the checkpoint in force, never past the log's end -- and
            // never over itself, since this frame is not in the log yet.
            let in_force = state
                .checkpoint
                .as_ref()
                .map_or(0, |checkpoint| checkpoint.covers_through_seq);
            if *covers_through_seq < in_force || *covers_through_seq > state.last_event_seq {
                return Err(format!(
                    "a checkpoint covers through frame {covers_through_seq}, outside frames \
                     {in_force} to {}",
                    state.last_event_seq
                ));
            }
            // Upstream's coverage is a frame, and it may fall inside a turn
            // (`appendContextCompaction`, `session_log.zig:343-360`): the
            // turns it covers whole are the concluded ones it reaches the
            // end of; the turn after them it may cover the first steps of, up
            // to where an exchange starts, or every step so far of a turn
            // still running. That turn's first message stays after the
            // compacted form, so a running turn is never covered whole.
            let covered = state
                .turns
                .iter()
                .take_while(|turn| turn.outcome.is_some() && turn.seqs.last <= *covers_through_seq)
                .count();
            let covered_steps = match state.turns.get(covered) {
                Some(turn) if turn.seqs.opened <= *covers_through_seq => {
                    let steps = turn
                        .seqs
                        .steps
                        .iter()
                        .take_while(|seq| **seq <= *covers_through_seq)
                        .count();
                    // A running turn's steps so far end an exchange: its
                    // reply would have ended it.
                    let ends_exchange = matches!(
                        turn.steps.get(steps.wrapping_sub(1)),
                        Some(
                            TurnStep::ToolResult { .. }
                                | TurnStep::ToolFeedback { .. }
                                | TurnStep::Steered { .. }
                        )
                    );
                    let at_boundary = turn
                        .exchange_starts()
                        .get(1..)
                        .is_some_and(|starts| starts.contains(&steps))
                        // Where a cut fell when a steer went with the step
                        // before it: a log written then still reads.
                        || turn.calls_tools(steps)
                        || (turn.outcome.is_none() && steps == turn.steps.len() && ends_exchange);
                    if steps == 0 || !at_boundary {
                        return Err("a checkpoint covers part of a turn".to_string());
                    }
                    steps
                }
                _ => 0,
            };
            if covered == 0 && covered_steps == 0 {
                return Err("a checkpoint covers no turn".to_string());
            }
            // Never less than the one in force covered of the same turn.
            if let Some(in_force) = &state.checkpoint {
                if covered == in_force.covered_turns && covered_steps < in_force.covered_steps {
                    return Err("a checkpoint covers less than the one in force".to_string());
                }
            }
            let count = state
                .checkpoint
                .as_ref()
                .map_or(0, |checkpoint| checkpoint.count)
                + 1;
            state.checkpoint = Some(Checkpoint {
                covers_through_seq: *covers_through_seq,
                covered_turns: covered,
                covered_steps,
                compacted: compacted.clone(),
                count,
            });
        }
    }

    state.updated_at_ms = envelope.timestamp_ms;
    state.last_event_seq = envelope.seq;
    Ok(())
}

/// The turn currently open, or why there is not one, with `seq` recorded as
/// its last frame.
///
/// A turn a checkpoint covers takes nothing more: the checkpoint is what the
/// model reads for it, and a step the compacted form does not hold would be a
/// step no request ever carries.
fn open_turn<'a>(
    state: &'a mut DurableState,
    what: &str,
    seq: u64,
) -> Result<&'a mut HistoryTurn, String> {
    let covered = state
        .checkpoint
        .as_ref()
        .map_or(0, |checkpoint| checkpoint.covered_turns);
    let count = state.turns.len();
    let Some(turn) = state.turns.last_mut() else {
        return Err(format!("{what} appears before any user message"));
    };
    if turn.outcome.is_some() {
        return Err(format!("{what} appears after its turn already concluded"));
    }
    if count <= covered {
        return Err(format!("{what} appears in a turn a checkpoint covers"));
    }
    turn.seqs.last = seq;
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
    lock_exclusive_within(file, path, id, Wait::Grace)
}

/// How long a resume waits for a lock someone else holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Wait {
    /// [`LOCK_RETRY_BUDGET`]: long enough for a child that is about to let go.
    Grace,
    /// Not at all: one attempt, and `Busy` if it is held.
    None,
}

#[cfg(unix)]
fn lock_exclusive_within(
    file: &File,
    path: &Path,
    id: &SessionId,
    wait: Wait,
) -> Result<(), SessionError> {
    use rustix::fs::{flock, FlockOperation};

    let attempt = || match flock(file, FlockOperation::NonBlockingLockExclusive) {
        Ok(()) => AttemptOutcome::Acquired,
        // `EAGAIN` and `EWOULDBLOCK` are the same value on every platform
        // xfx builds for, so matching one covers both.
        Err(rustix::io::Errno::WOULDBLOCK) => AttemptOutcome::WouldBlock,
        Err(err) => AttemptOutcome::Failed(io::Error::from(err)),
    };
    let outcome = match wait {
        Wait::Grace => acquire_with_grace(attempt, std::thread::sleep, Instant::now),
        Wait::None => acquire_within(Duration::ZERO, attempt, std::thread::sleep, Instant::now),
    };
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
    attempt: impl FnMut() -> AttemptOutcome,
    sleep: impl FnMut(Duration),
    now: impl FnMut() -> Instant,
) -> Result<(), AcquireFailure> {
    acquire_within(LOCK_RETRY_BUDGET, attempt, sleep, now)
}

/// [`acquire_with_grace`] over a budget of the caller's: zero is one attempt
/// and no pause, since the deadline has passed the first time it is read.
#[cfg(unix)]
fn acquire_within(
    budget: Duration,
    mut attempt: impl FnMut() -> AttemptOutcome,
    mut sleep: impl FnMut(Duration),
    mut now: impl FnMut() -> Instant,
) -> Result<(), AcquireFailure> {
    let deadline = now() + budget;
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

#[cfg(not(unix))]
fn lock_exclusive_within(
    _file: &File,
    _path: &Path,
    _id: &SessionId,
    _wait: Wait,
) -> Result<(), SessionError> {
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
    use crate::gateway::protocol::{ContentPart, Role};
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
            // An amendment is a step of a turn like any other: there is no
            // approval to answer before a turn has been asked for.
            SessionEvent::ToolFeedback {
                call_id: "c".to_string(),
                text: "say it differently".to_string(),
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
    fn an_amendment_stays_inside_the_turn_whose_approval_it_answered() {
        // Reduced as a step, not as a turn. Were it reduced the way a
        // `UserMessage` is, this log would end with two turns and the second
        // one would hold a result whose call is in the first.
        let state = reduce(vec![
            started(),
            SessionEvent::UserMessage {
                text: "draft two files".to_string(),
            },
            SessionEvent::AssistantMessage {
                text: "drafting".to_string(),
                tool_calls: vec![
                    RecordedToolCall {
                        id: "c1".to_string(),
                        name: "write_file".to_string(),
                        input: serde_json::json!({ "path": "a.md" }),
                    },
                    RecordedToolCall {
                        id: "c2".to_string(),
                        name: "write_file".to_string(),
                        input: serde_json::json!({ "path": "b.md" }),
                    },
                ],
                raw_content: Vec::new(),
                responses_state: Vec::new(),
                wire: None,
            },
            SessionEvent::ToolResult {
                call_id: "c1".to_string(),
                tool: "write_file".to_string(),
                ok: true,
                output: "wrote a.md".to_string(),
            },
            SessionEvent::ToolResult {
                call_id: "c2".to_string(),
                tool: "write_file".to_string(),
                ok: true,
                output: "wrote b.md".to_string(),
            },
            SessionEvent::ToolFeedback {
                call_id: "c1".to_string(),
                text: "prefer bullet points".to_string(),
            },
        ])
        .expect("an amendment inside an open turn is accepted");

        assert_eq!(state.turns.len(), 1, "{:?}", state.turns);
        let steps = &state.turns[0].steps;
        assert_eq!(steps.len(), 4, "{steps:?}");
        assert!(
            matches!(
                &steps[3],
                TurnStep::ToolFeedback { call_id, text }
                    if call_id == "c1" && text == "prefer bullet points"
            ),
            "{steps:?}"
        );

        // And it replays behind both results, in the group they belong to.
        let replay = state.history_messages(crate::provider::Wire::VercelGateway);
        assert_eq!(
            replay
                .messages
                .iter()
                .map(|message| message.role)
                .collect::<Vec<_>>(),
            [
                Role::User,
                Role::Assistant,
                Role::Tool,
                Role::Tool,
                Role::User
            ]
        );
        assert_eq!(replay.messages[4].text(), "prefer bullet points");
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
                turn_output_tokens: None,
                requests: Vec::new(),
                unaccounted: false,
                lines_added: 0,
                lines_removed: 0,
                settled: Vec::new(),
                rejected: Vec::new(),
            },
            SessionEvent::UsageRecorded {
                input_tokens: None,
                output_tokens: Some(1),
                turn_output_tokens: None,
                requests: Vec::new(),
                unaccounted: false,
                lines_added: 0,
                lines_removed: 0,
                settled: Vec::new(),
                rejected: Vec::new(),
            },
        ])
        .expect("reduces");
        assert_eq!(state.total_input_tokens, 3);
        assert_eq!(state.total_output_tokens, 5);
    }

    /// The runtime's later answer about a pending request takes its place
    /// under the same generation id (`applyGeneration`,
    /// `session_usage.zig:1080-1126`): the Gateway's time, model, tokens and
    /// cost, and the request's own duration. A rejection leaves it missing.
    /// Neither touches a request that is not pending, or the meter's totals,
    /// and neither is a frame from before request records.
    #[test]
    fn a_later_bill_settles_its_pending_request_and_a_rejection_leaves_it_missing() {
        let pending = |id: &str| crate::session::RequestUsage {
            at_ms: 1_000,
            duration_ms: 700,
            model: "vendor/model".to_string(),
            input_tokens: 12,
            output_tokens: 3,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            reasoning_tokens: None,
            cost: None,
            generation_id: Some(id.to_string()),
            origin: Some("http://127.0.0.1:4100".to_string()),
        };
        let first = "gen_01JABCDEFGHJKMNPQRSTVWXYZ1";
        let second = "gen_01JABCDEFGHJKMNPQRSTVWXYZ2";
        let bill = crate::session::RequestUsage {
            at_ms: 900,
            duration_ms: 0,
            model: "anthropic/claude-opus-5".to_string(),
            input_tokens: 1340,
            output_tokens: 250,
            cache_read_tokens: 300,
            cache_write_tokens: 40,
            reasoning_tokens: Some(50),
            cost: Some(0.0123),
            generation_id: Some(first.to_string()),
            origin: None,
        };
        let frame = |requests, settled, rejected| SessionEvent::UsageRecorded {
            input_tokens: None,
            output_tokens: None,
            turn_output_tokens: None,
            requests,
            unaccounted: false,
            lines_added: 0,
            lines_removed: 0,
            settled,
            rejected,
        };
        let state = reduce(vec![
            started(),
            frame(
                vec![pending(first), pending(second)],
                Vec::new(),
                Vec::new(),
            ),
            frame(Vec::new(), vec![bill.clone()], vec![second.to_string()]),
            // A second answer about the same generation changes nothing.
            frame(
                Vec::new(),
                vec![crate::session::RequestUsage {
                    cost: Some(9.0),
                    ..bill.clone()
                }],
                Vec::new(),
            ),
        ])
        .expect("reduces");
        let requests = &state.usage.requests;
        assert_eq!(
            requests[0],
            crate::session::RequestUsage {
                duration_ms: 700,
                ..bill
            }
        );
        assert!(!requests[1].pending());
        assert_eq!(requests[1].cost, None);
        assert_eq!(state.usage.pending().count(), 0);
        assert!(state.usage.legacy_at_ms.is_empty());
        assert_eq!(
            (state.total_input_tokens, state.total_output_tokens),
            (0, 0)
        );
    }

    #[test]
    fn a_reset_withdraws_every_grant_before_it_and_none_after_it() {
        let grant = |tool: &str, target: &str| SessionEvent::PermissionGrantRecorded {
            tool: tool.to_string(),
            target: target.to_string(),
        };
        let state = reduce(vec![
            started(),
            grant("edit_file", "/w/a.txt"),
            grant("terminal", "ls in /w"),
            SessionEvent::PermissionGrantsCleared,
            grant("edit_file", "/w/a.txt"),
        ])
        .expect("reduces");
        assert_eq!(state.grants, vec![Grant::new("edit_file", "/w/a.txt")]);

        let cleared = reduce(vec![
            started(),
            grant("edit_file", "/w/a.txt"),
            SessionEvent::PermissionGrantsCleared,
        ])
        .expect("reduces");
        assert!(cleared.grants.is_empty());
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

    /// The roles and the text of each message, for a case about the shape.
    fn shape(messages: &[Message]) -> Vec<(Role, String)> {
        messages
            .iter()
            .map(|message| {
                let text = message
                    .content
                    .iter()
                    .map(|part| match part {
                        ContentPart::Text { text } => text.clone(),
                        ContentPart::ToolCall(call) => format!("call {}", call.id),
                        ContentPart::ToolResult {
                            call_id,
                            output,
                            status,
                            ..
                        } => match status {
                            ResultStatus::Denied => format!("result {call_id} {output} denied"),
                            _ => format!(
                                "result {call_id} {output} error={}",
                                *status != ResultStatus::Success
                            ),
                        },
                        ContentPart::RawBlocks(_) => "raw".to_string(),
                        ContentPart::Image { media_type, .. } => {
                            format!("image {}", media_type.label())
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" | ");
                (message.role, text)
            })
            .collect()
    }

    fn interrupted() -> SessionEvent {
        SessionEvent::TurnConcluded {
            outcome: TurnConclusion::Interrupted {
                reason: "the turn was cancelled".to_string(),
            },
        }
    }

    fn assistant(text: &str, calls: &[(&str, &str)]) -> SessionEvent {
        SessionEvent::AssistantMessage {
            text: text.to_string(),
            tool_calls: calls
                .iter()
                .map(|(id, name)| RecordedToolCall {
                    id: id.to_string(),
                    name: name.to_string(),
                    input: serde_json::json!({}),
                })
                .collect(),
            raw_content: Vec::new(),
            responses_state: Vec::new(),
            wire: None,
        }
    }

    #[test]
    fn history_answers_a_call_its_interrupted_turn_never_finished_as_aborted() {
        // Claim changed (was `history_drops_a_tool_call_that_never_got_its_
        // result`): fx's next request after a command the user stopped
        // (`fx-interrupt-40x120-bodies/006-completion.json`) carries the call,
        // its result as `error-text` `aborted by user` (`session.zig:64`,
        // `:2725-2737`) and then the `<turn_aborted>` message (`:52-56`).
        let state = reduce(vec![
            started(),
            SessionEvent::UserMessage {
                text: "go".to_string(),
            },
            assistant("reading", &[("c1", "read_file")]),
            interrupted(),
        ])
        .expect("reduces");
        let replay = state.history_messages(Wire::AnthropicMessages);
        assert_eq!(
            shape(&replay.messages),
            vec![
                (Role::User, "go".to_string()),
                (Role::Assistant, "reading | call c1".to_string()),
                (
                    Role::Tool,
                    "result c1 aborted by user error=true".to_string()
                ),
                (Role::User, TURN_ABORTED.to_string()),
            ]
        );
        assert_eq!(
            TURN_ABORTED,
            "<turn_aborted>\nThe previous turn ended before completion. Any tools or commands may \
             have partially executed. Do not continue this request unless the user explicitly asks \
             to continue.\n</turn_aborted>"
        );
    }

    #[test]
    fn history_closes_an_interrupted_answer_with_upstreams_sentence() {
        // `fx-interrupt-40x120-bodies/003-completion.json`: the partial
        // answer, a blank line, `The previous response ended before
        // completion.` (`session.zig:63`, `:3067-3069`), then `<turn_aborted>`.
        let state = reduce(vec![
            started(),
            SessionEvent::UserMessage {
                text: "stream please".to_string(),
            },
            assistant("MARKER-HEAD word0 ", &[]),
            interrupted(),
            SessionEvent::UserMessage {
                text: "next".to_string(),
            },
            assistant("After interrupt answer.", &[]),
            SessionEvent::TurnConcluded {
                outcome: TurnConclusion::Final {
                    finish_reason: "stop".to_string(),
                    steps: 1,
                },
            },
        ])
        .expect("reduces");
        let replay = state.history_messages(Wire::VercelGateway);
        assert_eq!(
            shape(&replay.messages),
            vec![
                (Role::User, "stream please".to_string()),
                (
                    Role::Assistant,
                    "MARKER-HEAD word0 \n\nThe previous response ended before completion."
                        .to_string()
                ),
                (Role::User, TURN_ABORTED.to_string()),
                (Role::User, "next".to_string()),
                (Role::Assistant, "After interrupt answer.".to_string()),
            ]
        );
    }

    #[test]
    fn history_closes_an_interrupted_turn_that_said_nothing_with_the_sentence_alone() {
        // `formatInterruptedAssistantClosedContent` (`session.zig:3057-3065`):
        // no partial answer and no finished tool -- the sentence by itself.
        let state = reduce(vec![
            started(),
            SessionEvent::UserMessage {
                text: "go".to_string(),
            },
            interrupted(),
        ])
        .expect("reduces");
        let replay = state.history_messages(Wire::VercelGateway);
        assert_eq!(
            shape(&replay.messages),
            vec![
                (Role::User, "go".to_string()),
                (
                    Role::Assistant,
                    "The previous response ended before completion.".to_string()
                ),
                (Role::User, TURN_ABORTED.to_string()),
            ]
        );
    }

    #[test]
    fn history_says_which_tools_finished_before_the_interruption() {
        // `formatInterruptedAssistantToolContent` and
        // `formatCompletedToolSummary` (`session.zig:3020-3055`): tools that
        // finished are named, after whatever partial answer followed them.
        let state = reduce(vec![
            started(),
            SessionEvent::UserMessage {
                text: "go".to_string(),
            },
            assistant("", &[("c1", "read_file"), ("c2", "glob_files")]),
            SessionEvent::ToolResult {
                call_id: "c1".to_string(),
                tool: "read_file".to_string(),
                ok: true,
                output: "bytes".to_string(),
            },
            SessionEvent::ToolResult {
                call_id: "c2".to_string(),
                tool: "glob_files".to_string(),
                ok: true,
                output: "a.txt".to_string(),
            },
            assistant("so far", &[]),
            interrupted(),
        ])
        .expect("reduces");
        let replay = state.history_messages(Wire::VercelGateway);
        let shown = shape(&replay.messages);
        assert_eq!(
            shown[4],
            (
                Role::Assistant,
                "so far\n\nInterrupted by user after completing 2 tool calls: read_file, glob_files."
                    .to_string()
            ),
            "{shown:#?}"
        );
        assert_eq!(shown[5], (Role::User, TURN_ABORTED.to_string()));
        assert_eq!(shown.len(), 6, "{shown:#?}");
    }

    fn result(call_id: &str, tool: &str, ok: bool, output: &str) -> SessionEvent {
        SessionEvent::ToolResult {
            call_id: call_id.to_string(),
            tool: tool.to_string(),
            ok,
            output: output.to_string(),
        }
    }

    #[test]
    fn history_answers_a_question_the_user_cancelled_as_the_call_its_turn_stopped_under() {
        // A question cancelled with Ctrl-C or Escape answers `aborted by user`
        // as a failure (`crate::tools::question`), and upstream keeps that
        // call on the interrupted turn (`session.zig:2701-2736`): the call,
        // its answer as `error-text`, then `<turn_aborted>` -- no closing
        // assistant message, and the question is not a tool that completed.
        let state = reduce(vec![
            started(),
            SessionEvent::UserMessage {
                text: "ask me".to_string(),
            },
            assistant("", &[("q1", "ask_user_question")]),
            result("q1", "ask_user_question", false, ABORTED_TOOL_OUTPUT),
            interrupted(),
        ])
        .expect("reduces");
        let replay = state.history_messages(Wire::VercelGateway);
        assert_eq!(
            shape(&replay.messages),
            vec![
                (Role::User, "ask me".to_string()),
                (Role::Assistant, "call q1".to_string()),
                (
                    Role::Tool,
                    "result q1 aborted by user error=true".to_string()
                ),
                (Role::User, TURN_ABORTED.to_string()),
            ]
        );
    }

    #[test]
    fn history_answers_a_command_the_user_stopped_as_the_call_its_turn_stopped_under() {
        // A command stopped under its turn answers `aborted by user`
        // (`crate::tools::terminal`), with xfx's note after it when its group
        // could not be emptied; either way it is the call the turn stopped
        // under, not a tool that completed (`fx-interrupt-next-cmd-40x120-
        // bodies/003-completion.json`).
        for output in [
            ABORTED_TOOL_OUTPUT.to_string(),
            format!("{ABORTED_TOOL_OUTPUT}\n... [the command's process group was still there]\n"),
        ] {
            let state = reduce(vec![
                started(),
                SessionEvent::UserMessage {
                    text: "run cmd".to_string(),
                },
                assistant("", &[("c1", "terminal")]),
                result("c1", "terminal", false, &output),
                interrupted(),
            ])
            .expect("reduces");
            let shown = shape(&state.history_messages(Wire::VercelGateway).messages);
            assert_eq!(shown.len(), 4, "{shown:#?}");
            assert_eq!(shown[1], (Role::Assistant, "call c1".to_string()));
            assert_eq!(shown[3], (Role::User, TURN_ABORTED.to_string()));
        }
        // A tool's own failure that merely mentions the words is not one.
        assert!(!stopped_answer(false, "no such file: aborted by user"));
        assert!(!stopped_answer(true, ABORTED_TOOL_OUTPUT));
    }

    #[test]
    fn history_names_the_finished_tools_on_the_call_its_turn_stopped_under() {
        // `formatInterruptedAssistantToolContent` (`session.zig:3034-3055`):
        // when the turn stopped under a call, the tools that finished before
        // it are named after the text of the message that made that call.
        let state = reduce(vec![
            started(),
            SessionEvent::UserMessage {
                text: "go".to_string(),
            },
            assistant("", &[("c1", "read_file")]),
            result("c1", "read_file", true, "bytes"),
            assistant("checking", &[("c2", "terminal")]),
            interrupted(),
        ])
        .expect("reduces");
        let replay = state.history_messages(Wire::VercelGateway);
        assert_eq!(
            shape(&replay.messages),
            vec![
                (Role::User, "go".to_string()),
                (Role::Assistant, "call c1".to_string()),
                (Role::Tool, "result c1 bytes error=false".to_string()),
                (
                    Role::Assistant,
                    "checking\n\nInterrupted by user after completing 1 tool call: read_file. \
                     | call c2"
                        .to_string()
                ),
                (
                    Role::Tool,
                    "result c2 aborted by user error=true".to_string()
                ),
                (Role::User, TURN_ABORTED.to_string()),
            ]
        );
    }

    #[test]
    fn history_replays_a_failed_result_as_a_failure_and_as_a_call_that_finished() {
        // Upstream sends every result whose status is a failure as
        // `error-text` (`vercel_protocol.zig:742-747`); a call that failed on
        // its own still returned, so it is among the completed ones.
        let state = reduce(vec![
            started(),
            SessionEvent::UserMessage {
                text: "go".to_string(),
            },
            assistant("", &[("c1", "read_file")]),
            result("c1", "read_file", false, "no such file"),
            interrupted(),
        ])
        .expect("reduces");
        let replay = state.history_messages(Wire::VercelGateway);
        assert_eq!(
            shape(&replay.messages),
            vec![
                (Role::User, "go".to_string()),
                (Role::Assistant, "call c1".to_string()),
                (Role::Tool, "result c1 no such file error=true".to_string()),
                (
                    Role::Assistant,
                    "Interrupted by user after completing 1 tool call: read_file.".to_string()
                ),
                (Role::User, TURN_ABORTED.to_string()),
            ]
        );
    }

    #[test]
    fn the_aborted_answer_is_the_questions_own_words() {
        assert_eq!(ABORTED_TOOL_OUTPUT, "aborted by user");
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
    fn a_title_the_user_chose_outranks_the_first_prompt_and_the_last_one_wins() {
        let asked = vec![
            started(),
            SessionEvent::UserMessage {
                text: "first question alpha".to_string(),
            },
            SessionEvent::TurnConcluded {
                outcome: TurnConclusion::Final {
                    finish_reason: "stop".to_string(),
                    steps: 1,
                },
            },
        ];
        let derived = reduce(asked.clone()).expect("a valid log");
        assert_eq!(derived.title().as_deref(), Some("first question alpha"));

        let mut renamed = asked.clone();
        renamed.push(SessionEvent::SessionRenamed {
            title: "My Title".to_string(),
        });
        assert_eq!(
            reduce(renamed.clone())
                .expect("a valid log")
                .title()
                .as_deref(),
            Some("My Title")
        );
        renamed.push(SessionEvent::SessionRenamed {
            title: "Second name".to_string(),
        });
        assert_eq!(
            reduce(renamed).expect("a valid log").title().as_deref(),
            Some("Second name")
        );

        // A session renamed before anything was asked in it has a title and
        // no turn -- upstream keeps exactly that one on exit.
        let empty = reduce(vec![
            started(),
            SessionEvent::SessionRenamed {
                title: "Before asking".to_string(),
            },
        ])
        .expect("a valid log");
        assert_eq!(empty.title().as_deref(), Some("Before asking"));
        assert!(empty.turns.is_empty());

        // An explicit title is not clipped at the derived title's 80 bytes.
        let wide = "w".repeat(MAX_RENAMED_TITLE_BYTES);
        let state = reduce(vec![
            started(),
            SessionEvent::SessionRenamed {
                title: wide.clone(),
            },
        ])
        .expect("a valid log");
        assert_eq!(state.title(), Some(wide));
    }

    #[test]
    fn a_rename_the_validation_would_refuse_is_not_a_log_xfx_wrote() {
        for title in ["", "  padded ", "line\nbreak", &"x".repeat(241)] {
            let refused = reduce(vec![
                started(),
                SessionEvent::SessionRenamed {
                    title: title.to_string(),
                },
            ]);
            assert!(refused.is_err(), "{title:?} was replayed");
        }
    }

    #[test]
    fn a_title_is_trimmed_and_refused_as_upstreams_is() {
        // `validateSessionTitle`, `app_session_runtime.zig:3790-3799`.
        assert_eq!(validate_title("   My Title  "), Ok("My Title"));
        assert_eq!(validate_title("\t\r\nMy Title\n"), Ok("My Title"));
        assert_eq!(validate_title(""), Err(TitleError::Empty));
        assert_eq!(validate_title(" \t\r\n "), Err(TitleError::Empty));
        assert_eq!(
            validate_title(&"x".repeat(240)),
            Ok("x".repeat(240).as_str())
        );
        assert_eq!(validate_title(&"x".repeat(241)), Err(TitleError::TooLong));
        // Bytes, not characters: 81 three-byte characters are 243 bytes.
        assert_eq!(
            validate_title(&"\u{ac00}".repeat(81)),
            Err(TitleError::TooLong)
        );
        assert_eq!(validate_title("a\tb"), Err(TitleError::NotPrintable));
        assert_eq!(
            validate_title("a\u{1b}]2;x\u{7}"),
            Err(TitleError::NotPrintable)
        );
        assert_eq!(validate_title("a\u{7f}"), Err(TitleError::NotPrintable));
        // Upstream checks the length first.
        assert_eq!(
            validate_title(&format!("{}\u{1b}", "x".repeat(241))),
            Err(TitleError::TooLong)
        );
        assert_eq!(
            validate_title("caf\u{e9} \u{1f600}"),
            Ok("caf\u{e9} \u{1f600}")
        );
    }

    #[test]
    fn a_turn_remembers_when_it_was_asked_when_it_ended_and_what_it_wrote() {
        // `frame` stamps each event at 1000 + its sequence number.
        let state = reduce(vec![
            started(),
            SessionEvent::UserMessage {
                text: "q".to_string(),
            },
            SessionEvent::UsageRecorded {
                input_tokens: Some(6),
                output_tokens: Some(40),
                turn_output_tokens: None,
                requests: Vec::new(),
                unaccounted: false,
                lines_added: 0,
                lines_removed: 0,
                settled: Vec::new(),
                rejected: Vec::new(),
            },
            SessionEvent::TurnConcluded {
                outcome: TurnConclusion::Final {
                    finish_reason: "stop".to_string(),
                    steps: 1,
                },
            },
            SessionEvent::UserMessage {
                text: "r".to_string(),
            },
            SessionEvent::TurnConcluded {
                outcome: TurnConclusion::Interrupted {
                    reason: "the turn was cancelled".to_string(),
                },
            },
            SessionEvent::UserMessage {
                text: "s".to_string(),
            },
        ])
        .expect("a valid log");
        assert_eq!(
            state.turns[0].timing,
            TurnTiming {
                started_at_ms: 1_002,
                concluded_at_ms: Some(1_004),
                output_tokens: Some(40),
            }
        );
        assert_eq!(
            state.turns[1].timing,
            TurnTiming {
                started_at_ms: 1_005,
                concluded_at_ms: Some(1_006),
                output_tokens: None,
            }
        );
        assert_eq!(state.turns[2].timing.concluded_at_ms, None);
        assert_eq!(state.total_output_tokens, 40);
    }

    #[test]
    fn a_turn_replays_the_out_its_summary_said_and_an_older_frame_its_answering_step() {
        // A turn of two requests, 40 each, said `↓80` under it: the frame
        // keeps that beside the answering step's 40, and the replay reads the
        // 80. The session's own total is still the answering steps', as the
        // meter has always counted it.
        let usage = |output: u64, turn: Option<u64>| SessionEvent::UsageRecorded {
            input_tokens: Some(1300),
            output_tokens: Some(output),
            turn_output_tokens: turn,
            requests: Vec::new(),
            unaccounted: false,
            lines_added: 0,
            lines_removed: 0,
            settled: Vec::new(),
            rejected: Vec::new(),
        };
        let concluded = || SessionEvent::TurnConcluded {
            outcome: TurnConclusion::Final {
                finish_reason: "stop".to_string(),
                steps: 2,
            },
        };
        let asked = || SessionEvent::UserMessage {
            text: "q".to_string(),
        };
        let state = reduce(vec![
            started(),
            asked(),
            usage(40, Some(80)),
            concluded(),
            asked(),
            usage(40, None),
            concluded(),
            asked(),
            usage(40, Some(0)),
            concluded(),
        ])
        .expect("a valid log");
        let outs: Vec<Option<u64>> = state
            .turns
            .iter()
            .map(|turn| turn.timing.output_tokens)
            .collect();
        assert_eq!(outs, [Some(80), Some(40), Some(0)]);
        assert_eq!(state.total_output_tokens, 120);
    }

    #[test]
    fn a_renamed_session_publishes_its_title_and_a_resume_reads_it_back() {
        let root = tempfile::TempDir::new().expect("a profile root");
        // A directory the store creates, so it is private; the temporary
        // root around it is not.
        let store = SessionStore::open(&root.path().join("profile")).expect("a store");
        let workspace = root.path().join("work");
        std::fs::create_dir(&workspace).expect("a workspace");
        let id = SessionId::generate();
        let mut session = store
            .create(
                id.clone(),
                NewSession {
                    origin_workspace_root: workspace.clone(),
                    workspace_root: workspace.clone(),
                    model: "m".to_string(),
                    permission_mode: PermissionMode::Auto,
                },
            )
            .expect("a session");
        store
            .append(
                &mut session,
                SessionEvent::SessionRenamed {
                    title: "My Title".to_string(),
                },
            )
            .expect("appended");
        store.publish(&mut session).expect("published");
        assert_eq!(
            session
                .manifest()
                .and_then(|manifest| manifest.title.as_deref()),
            Some("My Title")
        );
        drop(session);

        // Listed under it with no turn in it, and resumed with it.
        let listed = store
            .list(&ListFilter::new(ListScope::AllWorkspaces))
            .expect("a listing");
        assert_eq!(listed.sessions.len(), 1);
        assert_eq!(listed.sessions[0].title.as_deref(), Some("My Title"));
        assert_eq!(listed.sessions[0].history_turns, 0);
        let resumed = store
            .resume_without_waiting(&Selector::Id(id.clone()), &workspace)
            .expect("resumed");
        assert_eq!(resumed.session.state().title().as_deref(), Some("My Title"));

        // Held now, so a second resume that does not wait is refused at once
        // as the session another process has open.
        match store.resume_without_waiting(&Selector::Id(id.clone()), &workspace) {
            Err(SessionError::Busy { id: busy }) => assert_eq!(busy, id.as_str()),
            other => panic!("a held session was not refused as busy: {other:?}"),
        }
    }

    /// A recorder on a fresh session in a private profile under `root`, and
    /// the workspace it was opened in.
    fn recorder_in(root: &Path) -> (SessionStore, SessionRecorder, SessionId, PathBuf) {
        let store = SessionStore::open(&root.join("profile")).expect("a store");
        let workspace = root.join("work");
        std::fs::create_dir_all(&workspace).expect("a workspace");
        let id = SessionId::generate();
        let session = store
            .create(
                id.clone(),
                NewSession {
                    origin_workspace_root: workspace.clone(),
                    workspace_root: workspace.clone(),
                    model: "m".to_string(),
                    permission_mode: PermissionMode::Auto,
                },
            )
            .expect("a session");
        let recorder = SessionRecorder::new(store.clone(), session);
        (store, recorder, id, workspace)
    }

    fn shot(id: u32) -> crate::image::Image {
        crate::image::Image::new(
            id,
            PathBuf::from("/w/shot.png"),
            crate::image::MediaType::Png,
            Arc::from(crate::image::fixture::SHOT_PNG),
        )
    }

    fn images_of(message: &Message) -> usize {
        message
            .content
            .iter()
            .filter(|part| matches!(part, crate::gateway::protocol::ContentPart::Image { .. }))
            .count()
    }

    #[test]
    fn a_turns_images_are_saved_beside_the_log_and_sent_again_after_a_resume() {
        let root = tempfile::TempDir::new().expect("a root");
        let (store, mut recorder, id, workspace) = recorder_in(root.path());
        recorder.commit(SessionEvent::UserMessage {
            text: "[Image #3][Image #4] what is in this image?".to_string(),
        });
        recorder.attach_images(&[shot(3), shot(4)]);
        recorder.commit(SessionEvent::TurnConcluded {
            outcome: TurnConclusion::Final {
                finish_reason: "stop".to_string(),
                steps: 1,
            },
        });
        assert_eq!(recorder.failure(), None);

        // The live state sends them without reading them back.
        let live = recorder.state().history_messages(Wire::VercelGateway);
        assert_eq!(images_of(&live.messages[0]), 2);
        assert!(live.notices.is_empty(), "{:?}", live.notices);

        // Each copy is owner-only, in an owner-only directory, under the name
        // the digest gives it, and byte for byte what was attached.
        let folder = store.session_dir(&id).join(IMAGES_DIR);
        let name = shot(3).snapshot_name();
        let copy = folder.join(&name);
        assert_eq!(
            std::fs::read(&copy).expect("a copy"),
            crate::image::fixture::SHOT_PNG
        );
        {
            use std::os::unix::fs::PermissionsExt;
            let mode =
                |path: &Path| std::fs::metadata(path).expect("mode").permissions().mode() & 0o777;
            assert_eq!(mode(&folder), 0o700);
            assert_eq!(mode(&copy), 0o600);
        }
        // The log names them by reference: no bytes in it.
        let log = std::fs::read_to_string(store.session_dir(&id).join(EVENTS_FILE)).expect("log");
        assert!(log.contains("\"kind\":\"images_attached\""), "{log}");
        assert!(
            !log.contains(crate::image::fixture::SHOT_PNG_BASE64),
            "{log}"
        );
        drop(recorder);

        let resumed = store
            .resume(&Selector::Id(id.clone()), &workspace)
            .expect("resumed");
        let state = resumed.session.state();
        assert_eq!(state.turns[0].images.len(), 2);
        assert_eq!(state.turns[0].images[0].recorded.id, 3);
        let replay = state.history_messages(Wire::AnthropicMessages);
        assert_eq!(images_of(&replay.messages[0]), 2, "{:?}", replay.messages);
        assert!(replay.notices.is_empty(), "{:?}", replay.notices);
    }

    #[test]
    fn a_copy_changed_or_gone_since_it_was_saved_is_not_sent_and_the_user_is_told() {
        let root = tempfile::TempDir::new().expect("a root");
        let (store, mut recorder, id, workspace) = recorder_in(root.path());
        recorder.commit(SessionEvent::UserMessage {
            text: "[Image #1][Image #2] compare".to_string(),
        });
        recorder.attach_images(&[shot(1), shot(2)]);
        assert_eq!(recorder.failure(), None);
        drop(recorder);
        let folder = store.session_dir(&id).join(IMAGES_DIR);
        // One replaced with other bytes of the same length, one removed.
        let mut tampered = crate::image::fixture::SHOT_PNG.to_vec();
        tampered[40] ^= 0xff;
        std::fs::write(folder.join(shot(1).snapshot_name()), &tampered).expect("tamper");
        std::fs::remove_file(folder.join(shot(2).snapshot_name())).expect("remove");

        let resumed = store
            .resume(&Selector::Id(id.clone()), &workspace)
            .expect("a session whose images are gone still resumes");
        let replay = resumed
            .session
            .state()
            .history_messages(Wire::VercelGateway);
        assert_eq!(images_of(&replay.messages[0]), 0);
        assert_eq!(replay.notices.len(), 2, "{:?}", replay.notices);
        assert!(
            replay.notices[0].contains("image #1")
                && replay.notices[0].contains("not the file that was saved"),
            "{:?}",
            replay.notices
        );
        assert!(
            replay.notices[1].contains("image #2") && replay.notices[1].contains("missing"),
            "{:?}",
            replay.notices
        );
    }

    #[test]
    fn a_saved_copy_that_is_a_symlink_is_not_followed() {
        let root = tempfile::TempDir::new().expect("a root");
        let (store, mut recorder, id, workspace) = recorder_in(root.path());
        recorder.commit(SessionEvent::UserMessage {
            text: "[Image #1]".to_string(),
        });
        recorder.attach_images(&[shot(1)]);
        drop(recorder);
        let copy = store
            .session_dir(&id)
            .join(IMAGES_DIR)
            .join(shot(1).snapshot_name());
        let elsewhere = root.path().join("elsewhere.png");
        std::fs::write(&elsewhere, crate::image::fixture::SHOT_PNG).expect("a decoy");
        std::fs::remove_file(&copy).expect("remove");
        std::os::unix::fs::symlink(&elsewhere, &copy).expect("a symlink");
        let resumed = store
            .resume(&Selector::Id(id.clone()), &workspace)
            .expect("resumed");
        let replay = resumed
            .session
            .state()
            .history_messages(Wire::VercelGateway);
        assert_eq!(images_of(&replay.messages[0]), 0);
        assert_eq!(replay.notices.len(), 1, "{:?}", replay.notices);
    }

    #[test]
    fn images_belong_right_behind_the_message_that_names_them() {
        let record = |id: u32| RecordedImage {
            id,
            path: "/w/shot.png".to_string(),
            media_type: "image/png".to_string(),
            sha256: crate::image::digest(crate::image::fixture::SHOT_PNG),
            bytes: crate::image::fixture::SHOT_PNG.len() as u64,
        };
        let opened = |extra: Vec<SessionEvent>| {
            let mut events = vec![
                started(),
                SessionEvent::UserMessage {
                    text: "[Image #1]".to_string(),
                },
            ];
            events.extend(extra);
            reduce(events)
        };
        let state = opened(vec![SessionEvent::ImagesAttached {
            images: vec![record(1)],
        }])
        .expect("reduced");
        assert_eq!(state.turns[0].images.len(), 1);
        assert_eq!(state.turns[0].images[0].snapshot, Snapshot::Unread);
        // Before any message, twice, or after a step: not the order a turn
        // writes them in.
        assert!(reduce(vec![
            started(),
            SessionEvent::ImagesAttached {
                images: vec![record(1)]
            }
        ])
        .is_err());
        assert!(opened(vec![
            SessionEvent::ImagesAttached {
                images: vec![record(1)]
            },
            SessionEvent::ImagesAttached {
                images: vec![record(2)]
            },
        ])
        .is_err());
        assert!(opened(vec![
            SessionEvent::AssistantMessage {
                text: "hi".to_string(),
                tool_calls: Vec::new(),
                raw_content: Vec::new(),
                responses_state: Vec::new(),
                wire: None,
            },
            SessionEvent::ImagesAttached {
                images: vec![record(1)]
            },
        ])
        .is_err());
        // A type, a digest or a size xfx would not have written.
        let mut bmp = record(1);
        bmp.media_type = "image/bmp".to_string();
        let mut short = record(1);
        short.sha256 = "abc".to_string();
        let mut huge = record(1);
        huge.bytes = crate::image::MAX_IMAGE_BYTES + 1;
        for bad in [bmp, short, huge] {
            assert!(opened(vec![SessionEvent::ImagesAttached { images: vec![bad] }]).is_err());
        }
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
    #[cfg(unix)]
    #[test]
    fn a_budget_of_nothing_is_one_attempt_and_no_pause() {
        let start = Instant::now();
        let mut attempts = 0;
        let mut slept = Vec::new();
        let result = acquire_within(
            Duration::ZERO,
            || {
                attempts += 1;
                AttemptOutcome::WouldBlock
            },
            |pause| slept.push(pause),
            || start,
        );
        assert!(matches!(result, Err(AcquireFailure::HeldThroughGrace)));
        assert_eq!(attempts, 1);
        assert!(slept.is_empty(), "{slept:?}");
    }

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

    // -- context checkpoints ------------------------------------------------

    fn user(text: &str) -> SessionEvent {
        SessionEvent::UserMessage {
            text: text.to_string(),
        }
    }

    fn answer(text: &str) -> SessionEvent {
        SessionEvent::AssistantMessage {
            text: text.to_string(),
            tool_calls: Vec::new(),
            raw_content: Vec::new(),
            responses_state: Vec::new(),
            wire: None,
        }
    }

    fn concluded() -> SessionEvent {
        SessionEvent::TurnConcluded {
            outcome: TurnConclusion::Final {
                finish_reason: "stop".to_string(),
                steps: 1,
            },
        }
    }

    fn compacted_first() -> super::super::checkpoint::Compacted {
        use super::super::checkpoint::{Compacted, CompactedTurn};
        Compacted {
            entries: Vec::new(),
            turns: vec![CompactedTurn {
                number: 1,
                users: vec!["one".to_string()],
                work: String::new(),
                final_reply: "ONE".to_string(),
                first_tool: 0,
                last_tool: 0,
                tools: Vec::new(),
            }],
            open: None,
            turn_count: 1,
            tool_count: 0,
        }
    }

    /// Two answered turns: frames 2-4 and 5-7.
    fn two_turns() -> Vec<SessionEvent> {
        vec![
            started(),
            user("one"),
            answer("ONE"),
            concluded(),
            user("two"),
            answer("TWO"),
            concluded(),
        ]
    }

    #[test]
    fn a_checkpoint_replaces_the_turns_it_covers_in_the_next_request_only() {
        let mut events = two_turns();
        events.push(SessionEvent::ContextCheckpoint {
            covers_through_seq: 4,
            compacted: compacted_first(),
        });
        let state = reduce(events).expect("a valid log");
        // The log keeps both turns -- a resumed screen shows them.
        assert_eq!(state.turns.len(), 2);
        assert_eq!(state.uncovered_turns().len(), 1);
        let checkpoint = state.checkpoint.as_ref().expect("in force");
        assert_eq!(checkpoint.covered_turns, 1);
        // The request: the compacted form, then the turn after it as it was.
        let messages = state.history_messages(Wire::VercelGateway).messages;
        assert_eq!(
            messages,
            vec![
                Message::user(super::super::checkpoint::render(&compacted_first())),
                Message::user("two"),
                Message::assistant(Some("TWO"), Vec::new()),
            ]
        );
    }

    #[test]
    fn a_checkpoint_written_inside_a_turn_covers_the_turns_before_it_and_the_turn_goes_on() {
        // What a compaction between a turn's steps writes
        // (`crate::tui::worker`'s `TurnCompactor`): the second turn's prompt
        // (frame 5), a checkpoint covering the first turn, then the rest of
        // the second turn.
        let mut events = two_turns();
        events.truncate(5);
        events.push(SessionEvent::ContextCheckpoint {
            covers_through_seq: 4,
            compacted: compacted_first(),
        });
        let open = reduce(events.clone()).expect("a checkpoint inside a turn");
        // The turn in progress goes on without the turns before it.
        assert_eq!(
            open.history_messages_before_open(Wire::VercelGateway)
                .messages,
            vec![Message::user(super::super::checkpoint::render(
                &compacted_first()
            ))]
        );
        events.push(answer("TWO"));
        events.push(concluded());
        let state = reduce(events).expect("the turn's later frames are taken");
        assert_eq!(state.checkpoint.as_ref().map(|c| c.covered_turns), Some(1));
        assert_eq!(
            state.history_messages(Wire::VercelGateway).messages,
            vec![
                Message::user(super::super::checkpoint::render(&compacted_first())),
                Message::user("two"),
                Message::assistant(Some("TWO"), Vec::new()),
            ]
        );
    }

    #[test]
    fn a_checkpoint_that_covers_part_of_a_turn_or_none_is_refused() {
        // Frame 3 is inside the first turn, whose conclusion is frame 4.
        let mut part = two_turns();
        part.push(SessionEvent::ContextCheckpoint {
            covers_through_seq: 3,
            compacted: compacted_first(),
        });
        assert_eq!(
            reduce(part).err().as_deref(),
            Some("a checkpoint covers part of a turn")
        );
        let mut none = two_turns();
        none.push(SessionEvent::ContextCheckpoint {
            covers_through_seq: 1,
            compacted: compacted_first(),
        });
        assert_eq!(
            reduce(none).err().as_deref(),
            Some("a checkpoint covers no turn")
        );
    }

    #[test]
    fn a_checkpoint_never_covers_frames_past_the_log_or_behind_the_one_in_force() {
        let mut ahead = two_turns();
        ahead.push(SessionEvent::ContextCheckpoint {
            covers_through_seq: 8,
            compacted: compacted_first(),
        });
        assert!(reduce(ahead).is_err(), "covered itself");
        let mut behind = two_turns();
        behind.push(SessionEvent::ContextCheckpoint {
            covers_through_seq: 7,
            compacted: compacted_first(),
        });
        behind.push(SessionEvent::ContextCheckpoint {
            covers_through_seq: 4,
            compacted: compacted_first(),
        });
        assert!(reduce(behind).is_err(), "went back");
    }

    #[test]
    fn a_checkpoint_inside_a_concluded_turn_leaves_its_message_and_later_steps() {
        // Upstream's coverage may fall at an exchange inside a turn
        // (`appendContextCompaction`, `session_log.zig:343-360`; `split`,
        // `window.zig:286-290`): the turn's first message stays, then its
        // steps from the first exchange the checkpoint does not cover.
        let call = |id: &str| SessionEvent::AssistantMessage {
            text: String::new(),
            tool_calls: vec![RecordedToolCall {
                id: id.to_string(),
                name: "read_file".to_string(),
                input: serde_json::json!({"path": "a.txt"}),
            }],
            raw_content: Vec::new(),
            responses_state: Vec::new(),
            wire: None,
        };
        // Frames: 2 the prompt; 3-4 and 5-6 two reads; 7 the reply; 8 done.
        let turn = vec![
            started(),
            user("read twice"),
            call("r1"),
            result("r1", "read_file", true, "FIRST-READ"),
            call("r2"),
            result("r2", "read_file", true, "SECOND-READ"),
            answer("READ"),
            concluded(),
        ];
        let mut events = turn.clone();
        events.push(SessionEvent::ContextCheckpoint {
            covers_through_seq: 4,
            compacted: compacted_first(),
        });
        let state = reduce(events).expect("a cut at an exchange");
        let checkpoint = state.checkpoint.as_ref().expect("in force");
        assert_eq!((checkpoint.covered_turns, checkpoint.covered_steps), (0, 2));
        let messages = state.history_messages(Wire::VercelGateway).messages;
        assert_eq!(messages.len(), 5, "{messages:#?}");
        assert_eq!(messages[1], Message::user("read twice"));
        assert!(
            !messages
                .iter()
                .any(|message| format!("{message:?}").contains("FIRST-READ")),
            "the covered read came back: {messages:#?}"
        );
        assert_eq!(messages[4], Message::assistant(Some("READ"), Vec::new()));
        // Not between a call and its result, nor at the reply.
        for inside in [3, 6] {
            let mut events = turn.clone();
            events.push(SessionEvent::ContextCheckpoint {
                covers_through_seq: inside,
                compacted: compacted_first(),
            });
            assert_eq!(
                reduce(events).err().as_deref(),
                Some("a checkpoint covers part of a turn"),
                "frame {inside}"
            );
        }
    }

    #[test]
    fn a_steer_goes_with_the_tool_step_after_it_and_a_cut_falls_before_it() {
        // `selectRecentContext`, `window.zig:219-224`: a steer whose
        // `after_tool_step_count` is one goes with step two, so the exchange
        // boundary is right after step one's result.
        let call = |id: &str| SessionEvent::AssistantMessage {
            text: String::new(),
            tool_calls: vec![RecordedToolCall {
                id: id.to_string(),
                name: "read_file".to_string(),
                input: serde_json::json!({"path": "a.txt"}),
            }],
            raw_content: Vec::new(),
            responses_state: Vec::new(),
            wire: None,
        };
        let steered = |text: &str| SessionEvent::Steered {
            text: text.to_string(),
        };
        // Frames: 2 the prompt; 3-4 a read; 5 a steer; 6-7 a read; 8 a steer
        // after the last; 9 the reply; 10 done.
        let turn = vec![
            started(),
            user("read twice"),
            call("r1"),
            result("r1", "read_file", true, "FIRST-READ"),
            steered("also the second"),
            call("r2"),
            result("r2", "read_file", true, "SECOND-READ"),
            steered("and sum up"),
            answer("READ"),
            concluded(),
        ];
        let state = reduce(turn.clone()).expect("a valid log");
        let read = &state.turns[0];
        assert_eq!(read.exchange_starts(), [0, 2]);
        // The cut before the steer, and -- written when a steer went with the
        // step before it -- the one after it: both read.
        for (covers, steps) in [(4, 2), (5, 3)] {
            let mut events = turn.clone();
            events.push(SessionEvent::ContextCheckpoint {
                covers_through_seq: covers,
                compacted: compacted_first(),
            });
            let state = reduce(events).expect("a cut at an exchange");
            let checkpoint = state.checkpoint.as_ref().expect("in force");
            assert_eq!(checkpoint.covered_steps, steps, "frame {covers}");
        }
        // Not between the second read and the reply: the steer after the
        // last step goes with it.
        let mut events = turn.clone();
        events.push(SessionEvent::ContextCheckpoint {
            covers_through_seq: 7,
            compacted: compacted_first(),
        });
        assert_eq!(
            reduce(events).err().as_deref(),
            Some("a checkpoint covers part of a turn")
        );
    }

    #[test]
    fn a_compacted_turns_images_go_with_it_and_a_cut_turns_stay_with_its_message() {
        // Upstream sends a compacted turn as text (`turnsFrom` takes the
        // message's text, `compactor.zig:266`), and a turn cut inside of keeps
        // its first message whole, images and all (`contextHistoryRange`,
        // `history_range.zig:36`).
        use crate::gateway::protocol::ContentPart;
        let image = |id: u32| TurnImage {
            recorded: RecordedImage {
                id,
                path: format!("shot{id}.png"),
                media_type: "image/png".to_string(),
                sha256: "0".repeat(64),
                bytes: 3,
            },
            snapshot: Snapshot::Loaded(std::sync::Arc::from(&b"png"[..])),
        };
        let step = |calls: bool, text: &str| TurnStep::Assistant {
            text: text.to_string(),
            tool_calls: if calls {
                vec![RecordedToolCall {
                    id: "c1".to_string(),
                    name: "read_file".to_string(),
                    input: serde_json::json!({"path": "a"}),
                }]
            } else {
                Vec::new()
            },
            raw_content: Vec::new(),
            responses_state: Vec::new(),
            wire: None,
        };
        let turn = |user: &str, id: u32, seq: u64| HistoryTurn {
            user: user.to_string(),
            images: vec![image(id)],
            steps: vec![
                step(true, ""),
                TurnStep::ToolResult {
                    call_id: "c1".to_string(),
                    tool: "read_file".to_string(),
                    ok: true,
                    output: "x".to_string(),
                },
                step(false, "done"),
            ],
            outcome: Some(TurnConclusion::Final {
                finish_reason: "stop".to_string(),
                steps: 2,
            }),
            timing: TurnTiming::default(),
            seqs: TurnSeqs {
                opened: seq,
                last: seq + 4,
                steps: vec![seq + 1, seq + 2, seq + 3],
            },
        };
        let mut state = DurableState::empty_for_tests();
        state.turns = vec![
            turn("[Image #1] first", 1, 2),
            turn("[Image #2] second", 2, 7),
        ];
        // The first turn whole, and the second cut at its reply's step --
        // as a cut at an exchange of a longer turn would leave it.
        state.checkpoint = Some(Checkpoint {
            covers_through_seq: 9,
            covered_turns: 1,
            covered_steps: 2,
            compacted: compacted_first(),
            count: 1,
        });
        let messages = state.history_messages(Wire::VercelGateway).messages;
        let images: Vec<usize> = messages
            .iter()
            .map(|message| {
                message
                    .content
                    .iter()
                    .filter(|part| matches!(part, ContentPart::Image { .. }))
                    .count()
            })
            .collect();
        assert_eq!(images, [0, 1, 0], "{messages:#?}");
        assert_eq!(messages[1].text(), "[Image #2] second");
    }

    #[test]
    fn an_older_log_replays_as_it_did_and_a_part_in_progress_is_omitted_when_absent() {
        // The frame a whole-turn checkpoint writes has no `open`: written
        // and read back exactly as before the field existed.
        let mut events = two_turns();
        events.push(SessionEvent::ContextCheckpoint {
            covers_through_seq: 4,
            compacted: compacted_first(),
        });
        let line = serde_json::to_string(events.last().expect("the checkpoint")).unwrap();
        assert!(!line.contains("\"open\""), "{line}");
        let older = r#"{"kind":"context_checkpoint","covers_through_seq":4,"compacted":{"turns":[],"turn_count":1,"tool_count":0}}"#;
        let read: SessionEvent = serde_json::from_str(older).expect("an older frame");
        assert!(matches!(
            read,
            SessionEvent::ContextCheckpoint { ref compacted, .. } if compacted.open.is_none()
        ));
    }

    #[test]
    fn a_checkpoint_over_a_running_turns_reply_is_refused() {
        // A running turn may be covered through its last exchange, and goes
        // on; never through a reply, which would have ended it -- a request
        // would carry a step the compacted form never held.
        let events = vec![
            started(),
            user("one"),
            answer("ONE"),
            SessionEvent::ContextCheckpoint {
                covers_through_seq: 3,
                compacted: compacted_first(),
            },
            answer("late"),
        ];
        assert_eq!(
            reduce(events).err().as_deref(),
            Some("a checkpoint covers part of a turn")
        );
    }

    #[test]
    fn the_latest_checkpoint_is_the_one_in_force() {
        let mut events = two_turns();
        events.push(SessionEvent::ContextCheckpoint {
            covers_through_seq: 4,
            compacted: compacted_first(),
        });
        let mut both = compacted_first();
        both.turns.push(super::super::checkpoint::CompactedTurn {
            number: 2,
            users: vec!["two".to_string()],
            work: String::new(),
            final_reply: "TWO".to_string(),
            first_tool: 0,
            last_tool: 0,
            tools: Vec::new(),
        });
        both.turn_count = 2;
        events.push(SessionEvent::ContextCheckpoint {
            covers_through_seq: 7,
            compacted: both.clone(),
        });
        events.push(user("three"));
        let state = reduce(events).expect("a valid log");
        assert_eq!(state.checkpoint.as_ref().map(|c| c.covered_turns), Some(2));
        let messages = state.history_messages(Wire::VercelGateway).messages;
        assert_eq!(
            messages[0],
            Message::user(super::super::checkpoint::render(&both))
        );
        assert_eq!(messages[1], Message::user("three"));
        assert_eq!(messages.len(), 2);
    }

    #[test]
    fn a_checkpoint_frame_round_trips_under_its_kind() {
        let event = SessionEvent::ContextCheckpoint {
            covers_through_seq: 4,
            compacted: compacted_first(),
        };
        assert_eq!(event.kind(), "context_checkpoint");
        let line = frame(8, event.clone()).encode().expect("encodes");
        assert!(line.contains("\"kind\":\"context_checkpoint\""), "{line}");
        assert!(line.contains("\"covers_through_seq\":4"), "{line}");
        assert_eq!(
            EventEnvelope::decode(line.trim_end())
                .expect("decodes")
                .event,
            event
        );
    }
}
