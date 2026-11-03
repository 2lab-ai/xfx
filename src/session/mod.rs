//! Durable sessions: what a turn leaves behind, and how a later turn picks it up.
//!
//! A session is an append-only log of typed events under
//! `~/.xfx/sessions/<id>/events.jsonl`, plus an atomically replaced manifest
//! that publishes an exact byte boundary and digest of that log. The log is the
//! truth; the manifest and every listing built from it are projections that can
//! be rebuilt and are never preferred over the log when the two disagree.
//!
//! - [`event`] owns the wire format of one frame and the closed set of things a
//!   session may remember. No variant of [`SessionEvent`] holds xfx's own
//!   Gateway credential; [`SessionEvent::ToolResult`] does hold what a tool
//!   read, verbatim, which is where a user's own secret can end up -- [`event`]
//!   says what that means and README's "Safety, in plain terms" says what to do
//!   about it.
//! - [`checkpoint`] owns the saved form of a context compaction and the text
//!   the model reads in place of the turns it covers.
//! - [`store`] owns the directory layout, the append/publish protocol, the
//!   replay that rebuilds state, and the read-only projections `sessions` and
//!   `session` render.
//!
//! The crash contract is the reason this module exists at all: between an
//! `fsync`ed append and the manifest replacement that publishes it, the new
//! bytes are durable and not yet true. Readers stop at the published boundary,
//! so whatever a crash left after it -- a whole valid event, half a line, or
//! garbage -- is invisible; a writer truncates it, because the next append must
//! own the offset it starts at.

pub mod checkpoint;
pub mod event;
pub mod prompt_history;
pub mod store;

pub use event::{
    new_identifier, CancellationOrigin, EventEnvelope, FrameError, LineCounts, RecordedChange,
    RecordedImage, RecordedToolCall, RequestUsage, SessionEvent, TerminalReason, TitleSource,
    ToolEnding, TurnConclusion, EVENT_SCHEMA_VERSION, MAX_EVENT_FRAME_BYTES,
};
pub use store::{
    turn_messages, validate_title, workspace_key, Checkpoint, Clock, DurableState, HistoryTurn,
    ListFilter, ListScope, NewSession, RecordedUsage, Resumed, Selector, SessionDetail,
    SessionError, SessionId, SessionList, SessionManifest, SessionRecorder, SessionStore,
    SessionSummary, Snapshot, TitleError, TurnImage, TurnSeqs, TurnStep, TurnTiming, UsageScan,
    WritableSession, DEFAULT_LIST_LIMIT, EVENTS_FILE, IMAGES_DIR, MANIFEST_FILE,
    MANIFEST_SCHEMA_VERSION, MAX_LIST_LIMIT, MAX_RENAMED_TITLE_BYTES, SESSIONS_DIR_NAME,
    STAGE_SUFFIX, STORAGE_FORMAT,
};
