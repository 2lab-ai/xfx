//! What compactions did, kept for `/trace` whether or not anyone asked for a
//! trace.
//!
//! Upstream keeps the newest 64 compaction decisions and failures in a
//! bounded ring for the life of the process, and `/trace` writes them in a
//! section of their own, `## Context Compaction`, with the failed ones also
//! under `## Problems` (`vercel-labs/fx@c1db9191
//! src/core/compactor/trace.zig`; `writeCompactionSummary`,
//! `src/core/app/app_commands.zig:2724-2775`; e2e
//! `tui-compaction-activity.test.ts:506-549`). The events and their details
//! are upstream's, word for word, wherever xfx takes the same step; a detail
//! holds counters, model names and enum names, never a prompt or a tool's
//! output -- the one provider error text it can carry is masked and cut
//! before it gets here ([`provider_detail`]).
//!
//! One ring for the process, as upstream's: a compaction runs on the
//! runtime's thread and `/trace` reads the ring from the screen's.

use std::collections::VecDeque;
use std::sync::Mutex;

/// How many events the ring keeps (`ring_capacity`, `trace.zig:44`).
pub const CAPACITY: usize = 64;

/// The most bytes of one event's detail (`max_detail_bytes`, `trace.zig:45`).
const MAX_DETAIL_BYTES: usize = 512;

/// The most bytes of a provider's error text an event quotes, after masking
/// (`debug_trace.preview(…, 240)`, `text_completion.zig:139-141`).
const PROVIDER_DETAIL_BYTES: usize = 240;

/// What happened (`Kind`, `trace.zig:47-67`), named as upstream names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Log,
    ProviderStart,
    ProviderCompleted,
    SummaryTransportFailed,
    SummaryIncomplete,
    SummaryToolCallRejected,
    SummaryTruncated,
    SourceCheckpointed,
    TransactionFailed,
    CredentialUnauthorized,
    Committed,
    Decision,
    OverflowWithoutCompaction,
    NoCompactableContext,
    RetentionExhausted,
    RetentionForcedZero,
    Installed,
    OverflowRecoveryIncomplete,
    ProviderOverflowRecovery,
}

impl Kind {
    /// Upstream's name for it (`@tagName`).
    pub fn name(self) -> &'static str {
        match self {
            Self::Log => "log",
            Self::ProviderStart => "provider_start",
            Self::ProviderCompleted => "provider_completed",
            Self::SummaryTransportFailed => "summary_transport_failed",
            Self::SummaryIncomplete => "summary_incomplete",
            Self::SummaryToolCallRejected => "summary_tool_call_rejected",
            Self::SummaryTruncated => "summary_truncated",
            Self::SourceCheckpointed => "source_checkpointed",
            Self::TransactionFailed => "transaction_failed",
            Self::CredentialUnauthorized => "credential_unauthorized",
            Self::Committed => "committed",
            Self::Decision => "decision",
            Self::OverflowWithoutCompaction => "overflow_without_compaction",
            Self::NoCompactableContext => "no_compactable_context",
            Self::RetentionExhausted => "retention_exhausted",
            Self::RetentionForcedZero => "retention_forced_zero",
            Self::Installed => "installed",
            Self::OverflowRecoveryIncomplete => "overflow_recovery_incomplete",
            Self::ProviderOverflowRecovery => "provider_overflow_recovery",
        }
    }
}

/// Which compaction it is (`compaction_activity.Origin`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// `/compact`.
    Manual,
    /// A request reached the automatic compaction point.
    Automatic,
    /// The provider refused a request as too large.
    ProviderOverflow,
}

impl Origin {
    pub fn name(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Automatic => "automatic",
            Self::ProviderOverflow => "provider_overflow",
        }
    }
}

/// Where an event happened: the turn and the model step, `0` for neither
/// (`debug_trace.TraceContext`). A compaction between turns has no turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Context {
    pub turn: u64,
    pub step: u64,
}

/// One event (`Event`, `trace.zig:69-88`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    /// Its place among every event this process recorded, from 1.
    pub sequence: u64,
    pub at_ms: i64,
    pub context: Context,
    pub failed: bool,
    pub kind: Kind,
    pub detail: String,
    /// The detail was longer than an event keeps.
    pub truncated: bool,
}

/// The newest [`CAPACITY`] events, oldest first (`Ring`, `trace.zig:90-115`).
#[derive(Debug, Default)]
pub struct Ring {
    events: VecDeque<Event>,
    total: u64,
}

impl Ring {
    pub const fn new() -> Self {
        Self {
            events: VecDeque::new(),
            total: 0,
        }
    }

    pub fn append(&mut self, mut event: Event) {
        self.total = self.total.saturating_add(1);
        event.sequence = self.total;
        if self.events.len() == CAPACITY {
            self.events.pop_front();
        }
        self.events.push_back(event);
    }

    pub fn snapshot(&self) -> Vec<Event> {
        self.events.iter().cloned().collect()
    }
}

static RING: Mutex<Ring> = Mutex::new(Ring::new());

static TURN_IDS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The next turn id of this process, from 1: one per prompt run and per
/// `/compact` admitted, as upstream numbers its work (`nextTurnId`,
/// `debug_trace.zig`; `worker_runtime.zig:989`).
pub fn next_turn_id() -> u64 {
    TURN_IDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1
}

/// An event made of `detail`, cut to what an event keeps.
fn event(context: Context, kind: Kind, failed: bool, detail: String) -> Event {
    let mut detail = detail;
    let truncated = detail.len() > MAX_DETAIL_BYTES;
    if truncated {
        let mut end = MAX_DETAIL_BYTES;
        while !detail.is_char_boundary(end) {
            end -= 1;
        }
        detail.truncate(end);
    }
    Event {
        sequence: 0,
        at_ms: crate::session::event::system_now_ms(),
        context,
        failed,
        kind,
        detail,
        truncated,
    }
}

fn record(event: Event) {
    // A poisoned ring is a ring a panic left mid-append, and every slot it
    // holds is a whole event: keep using it.
    let mut ring = RING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    ring.append(event);
}

/// A decision (`info`, `trace.zig:15-19`).
pub fn info(context: Context, kind: Kind, detail: String) {
    record(event(context, kind, false, detail));
}

/// A failure, which `/trace` also lists under its problems (`failure`,
/// `trace.zig:21-26`).
pub fn failure(context: Context, kind: Kind, detail: String) {
    record(event(context, kind, true, detail));
}

/// A note with no turn to it (`log`, `trace.zig:35-39`).
pub fn log(failed: bool, detail: String) {
    record(event(Context::default(), Kind::Log, failed, detail));
}

/// Every event the ring holds, oldest first.
pub fn snapshot() -> Vec<Event> {
    RING.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .snapshot()
}

/// A provider's error text as an event may quote it: secrets masked, control
/// bytes neutralized, at most [`PROVIDER_DETAIL_BYTES`] (the capture site in
/// `text_completion.zig:133-142`).
pub fn provider_detail(text: &str) -> String {
    let masked = crate::trace::mask_secrets(text);
    let mut out = String::new();
    for ch in masked.chars() {
        let ch = if ch.is_control() { ' ' } else { ch };
        if out.len() + ch.len_utf8() > PROVIDER_DETAIL_BYTES {
            break;
        }
        out.push(ch);
    }
    out
}

/// An optional count as upstream's `{any}` prints one.
pub fn any(value: Option<impl std::fmt::Display>) -> String {
    value.map_or_else(|| "null".to_string(), |value| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(n: i64) -> Event {
        Event {
            sequence: 0,
            at_ms: n,
            context: Context::default(),
            failed: false,
            kind: Kind::Log,
            detail: String::new(),
            truncated: false,
        }
    }

    #[test]
    fn the_ring_keeps_the_newest_events_in_order() {
        // `trace.zig:164-178`.
        let mut ring = Ring::new();
        for index in 0..CAPACITY as i64 + 3 {
            ring.append(at(index));
        }
        let events = ring.snapshot();
        assert_eq!(events.len(), CAPACITY);
        assert_eq!(events[0].sequence, 4);
        assert_eq!(events[0].at_ms, 3);
        assert_eq!(events[CAPACITY - 1].sequence, CAPACITY as u64 + 3);
    }

    #[test]
    fn a_detail_is_cut_to_what_an_event_keeps() {
        // `trace.zig:180-196`.
        let long = event(
            Context::default(),
            Kind::RetentionExhausted,
            true,
            "x".repeat(600),
        );
        assert!(long.truncated);
        assert_eq!(long.detail.len(), MAX_DETAIL_BYTES);
        let short = event(
            Context { turn: 7, step: 3 },
            Kind::Decision,
            false,
            "automatic_threshold tokens=279466/280000".to_string(),
        );
        assert!(!short.truncated);
        assert_eq!(short.kind.name(), "decision");
    }

    #[test]
    fn a_providers_error_text_is_masked_flattened_and_cut() {
        let detail = provider_detail("bad key sk-abcdef0123456789\nsecond line");
        assert!(!detail.contains("abcdef0123456789"), "{detail}");
        assert!(detail.contains("[redacted]"), "{detail}");
        assert!(!detail.contains('\n'));
        assert!(provider_detail(&"y".repeat(400)).len() <= PROVIDER_DETAIL_BYTES);
    }
}
