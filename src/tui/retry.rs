//! The row a failed request leaves on the band while the turn waits to try
//! again.
//!
//! The turn retries a request that provably delivered nothing, a bounded
//! number of times, after a backoff (`crate::agent::machine`'s `step` and
//! `retry_delay`). That policy is the turn's and nothing here changes it. What
//! this module is for is that the wait is **said**: upstream replaces the
//! activity row with a recovery label for as long as it lasts
//! (`RouteRecoveryStatus.recoveryLabel`, `types.zig:530-614`, `c1db919`) --
//! `⚠ Provider unavailable · HTTP 500 · server_error: upstream exploded ·
//! retrying request in 1s`, the seconds counting down to the next attempt and
//! the `in Ns` dropped once it is under way (`format_retry_label`,
//! `worker_status.zig:201-219`; `fx-retry-40x120-r0.6` and `-r1.3`). A turn
//! that went quiet because its provider is failing looks exactly like a turn
//! that went quiet because its provider is slow, unless it says so.
//!
//! Three pieces, each upstream's: a **cause** in a few words, a
//! **diagnostic** -- what the server said, shaped the way upstream shapes an
//! HTTP failure for this row (`formatHttpRecoveryDiagnostic`,
//! `gateway_error_format.zig:73-111`) -- and the **action**, which for every
//! failure this turn replays is `retrying request`.
//!
//! Each retry is also **recorded** on the full transcript, as upstream records
//! every recovery it admits (`recordRouteRecoveryNotice`,
//! `app_worker_runtime.zig:92-133`): the row is gone the moment the attempt
//! gets through, and the record is what is left of it ([`record`]).
//!
//! What is *not* here is upstream's budget. Upstream keeps a turn alive through
//! transient failure for as long as fifteen minutes
//! (`model_response_recovery.zig:7-9`); xfx's turn spends at most three
//! attempts on a step (`gateway::DEFAULT_MAX_ATTEMPTS`) and then fails with the
//! reason in the document. The row reports the retries xfx makes, not the ones
//! upstream would have.

use std::time::{Duration, Instant};

use crate::gateway::sse::SseError;
use crate::gateway::ProviderError;

use super::notice::Notice;

/// What the row says the turn is doing about it (`types.zig:547`).
const ACTION: &str = "retrying request";

/// The most of a server's own words the row quotes
/// (`RouteRecoveryStatus.label_max_bytes`, `types.zig:422`).
const DIAGNOSTIC_BYTES: usize = 512;

/// A retry the turn is waiting on: why, what the server said, and when the
/// next attempt goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Standing {
    pub(crate) cause: &'static str,
    pub(crate) diagnostic: String,
    pub(crate) deadline: Instant,
}

impl Standing {
    /// The row's text at `now` (`format_retry_label`): the whole seconds left
    /// before the next attempt, rounded **up** -- a wait of 250 ms is `in 1s`,
    /// not `in 0s` -- and no count at all once the deadline has passed and the
    /// attempt is under way.
    pub(crate) fn label(&self, now: Instant) -> String {
        let remaining = self.deadline.saturating_duration_since(now);
        label(self.cause, &self.diagnostic, seconds_up(remaining))
    }
}

/// How long the row says a retried request got through
/// (`recovered_status_visible_ms`, `worker_status.zig:5`).
pub(crate) const RECOVERED_VISIBLE: Duration = Duration::from_millis(1_500);

/// The most rows a recovery's row takes (`max_static_status_activity_rows`,
/// `render_input.zig:406`).
const MAX_ROWS: usize = 3;

/// What the activity row says about the turn's recovery, while it says
/// anything: upstream's worker status (`worker_status.zig:7-53`), which
/// stands in for the turn's own row for as long as it is set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Row {
    /// A retry is decided: the countdown to the next attempt, then -- once it
    /// is under way, through the whole of a retried answer arriving -- the
    /// same words without the count (`pending_auto_retry_status`,
    /// `orchestrator.zig:7826-7836`; `fx-interrupt-recovered-40x120-slow-2.8`).
    Retrying(Standing),
    /// The retried request got through: `✓ recovered · attempt N` until
    /// `until` (`format_recovered_label`, `worker_status.zig:221-235`), or
    /// until the turn ends (`clear_recovered_route`, `runtime.zig:6471`).
    Recovered { attempt: u32, until: Instant },
}

impl Row {
    /// The row's text at `now`, or `None` once a recovery has had its moment.
    pub(crate) fn label(&self, now: Instant) -> Option<String> {
        match self {
            Self::Retrying(standing) => Some(standing.label(now)),
            // Expired at the deadline itself (`is_expired`: `now >= expires`).
            Self::Recovered { attempt, until } => (now < *until).then(|| recovered_label(*attempt)),
        }
    }

    /// Whether a label too long for its rows keeps its end rather than its
    /// start: a warning's tail is the action and its count, which is what
    /// the row is for (`preserve_trailing_context`, `shimmer_runtime.zig:
    /// 202-205`).
    pub(crate) fn keeps_tail(&self) -> bool {
        matches!(self, Self::Retrying(_))
    }
}

/// `✓ recovered · attempt N` (`format_recovered_label`,
/// `worker_status.zig:221-235`).
pub(crate) fn recovered_label(attempt: u32) -> String {
    if attempt == 0 {
        return "\u{2713} recovered".to_string();
    }
    format!("\u{2713} recovered \u{b7} attempt {attempt}")
}

/// `label` on as many rows of `cols` cells as it needs, up to three, as
/// upstream lays out a status row (`wrappedStaticActivityLabel`,
/// `shimmer_runtime.zig:89-149`).
///
/// The first row keeps the label's leading mark and the space after it (`⚠ `,
/// `statusPrefixEnd`); each row after it is indented by that width. Rows break
/// at the last space that fits, or inside a word that does not. A label still
/// too long for the third row ends it with `...`: in front of the label's
/// tail when `keep_tail`, after its head otherwise. A label that fits is one
/// row, untouched.
pub(crate) fn wrapped(label: &str, cols: u16, keep_tail: bool) -> Vec<String> {
    let cols = usize::from(cols).max(1);
    if usize::from(super::wrap::width(label)) <= cols {
        return vec![label.to_string()];
    }
    let prefix_end = status_prefix_end(label);
    let prefix = &label[..prefix_end];
    let prefix_width = usize::from(super::wrap::width(prefix));
    let indent = prefix_width.max(1);
    let mut remaining = &label[prefix_end..];
    let mut rows: Vec<String> = Vec::new();
    while !remaining.is_empty() && rows.len() < MAX_ROWS {
        let first = rows.is_empty();
        let lead = if first {
            prefix.to_string()
        } else {
            " ".repeat(indent)
        };
        let lead_width = if first { prefix_width } else { indent };
        let available = cols.saturating_sub(lead_width).max(1);
        let last = rows.len() + 1 == MAX_ROWS;
        if last && usize::from(super::wrap::width(remaining)) > available {
            let marker = match available {
                1 => ".",
                2 => "..",
                _ => "...",
            };
            let content = available.saturating_sub(marker.len());
            rows.push(if keep_tail {
                format!("{lead}{marker}{}", suffix_by_width(remaining, content))
            } else {
                let mut chunk = wrap_cut(remaining, content);
                if chunk.is_empty() && content > 0 {
                    chunk = super::wrap::first_cluster(remaining).unwrap_or("");
                }
                format!("{lead}{chunk}{marker}")
            });
            remaining = "";
        } else {
            let mut chunk = wrap_cut(remaining, available);
            if chunk.is_empty() {
                chunk = super::wrap::first_cluster(remaining).unwrap_or(remaining);
            }
            rows.push(format!("{lead}{chunk}"));
            remaining = remaining[chunk.len()..].trim_start_matches([' ', '\t']);
        }
    }
    rows
}

/// Where a status label's leading mark ends, the space after it included, or
/// zero when it has none (`statusPrefixEnd`, `display_width.zig`).
fn status_prefix_end(label: &str) -> usize {
    let Some(first) = super::wrap::first_cluster(label) else {
        return 0;
    };
    if first.len() >= label.len() {
        return 0;
    }
    if label.as_bytes()[first.len()] == b' ' {
        first.len() + 1
    } else {
        0
    }
}

/// The longest start of `text` that is at most `width` cells.
fn prefix_by_width(text: &str, width: usize) -> &str {
    let mut used = 0usize;
    let mut end = 0usize;
    while let Some(cluster) = super::wrap::first_cluster(&text[end..]) {
        let cells = usize::from(super::wrap::width(cluster));
        if used + cells > width {
            break;
        }
        used += cells;
        end += cluster.len();
    }
    &text[..end]
}

/// The longest end of `text` that is at most `width` cells.
fn suffix_by_width(text: &str, width: usize) -> &str {
    let mut starts = Vec::new();
    let mut at = 0usize;
    while let Some(cluster) = super::wrap::first_cluster(&text[at..]) {
        starts.push((at, usize::from(super::wrap::width(cluster))));
        at += cluster.len();
    }
    let mut used = 0usize;
    let mut start = text.len();
    for (index, cells) in starts.into_iter().rev() {
        if used + cells > width {
            break;
        }
        used += cells;
        start = index;
    }
    &text[start..]
}

/// The start of `text` that fits `width` cells, broken at its last space when
/// there is one past the start (`wrapCutIgnoringAnsi`, `display_width.zig`).
fn wrap_cut(text: &str, width: usize) -> &str {
    let prefix = prefix_by_width(text, width);
    if prefix.len() == text.len() {
        return prefix;
    }
    match prefix.rfind([' ', '\t']) {
        Some(space) if space > 0 => &text[..space],
        _ => prefix,
    }
}

/// What kind of failure `error` is, in upstream's words for its causes
/// (`recoveryLabel`, `types.zig:531-543`).
///
/// Only the three kinds the turn replays are told apart: a status the server
/// answered with, a connection that never opened, and a failure the provider
/// reported inside its own stream. Anything else is never retried, so it never
/// reaches this row.
pub(crate) fn cause(error: &ProviderError) -> &'static str {
    match error {
        ProviderError::Status { status: 429, .. } => "Rate limited",
        // `connectivity_lost`: the network path refused, so nothing was sent
        // (`model_response_recovery.zig:12-14`).
        ProviderError::Connect { .. } => "Connection lost",
        _ => "Provider unavailable",
    }
}

/// Upstream's name for the same cause, as its recovery record writes it
/// (`@tagName(cause)`, `ModelRecoveryCause`): the three [`cause`] tells apart.
fn cause_name(error: &ProviderError) -> &'static str {
    match error {
        ProviderError::Status { status: 429, .. } => "rate_limited",
        ProviderError::Connect { .. } => "connectivity_lost",
        _ => "provider_unavailable",
    }
}

/// One retry the turn decided on, kept for the full transcript's records of
/// it (`writeRouteRecoveryBody`, `app_worker_runtime.zig:115-133`).
///
/// Upstream writes a `! recovery:` warning **twice** for each retry -- when
/// the retry is decided, about the attempt that failed, and when the next
/// attempt starts, about that one (`pending_auto_retry_status`,
/// `orchestrator.zig:7826-7836`, pushed as its stream opens, `:5033-5036`) --
/// with the same cause, delay and diagnostic, and then a `✓ recovery:` record
/// when an attempt gets through ([`recovered`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Retried {
    cause: &'static str,
    cause_name: &'static str,
    diagnostic: String,
    /// The delay in whole seconds **rounded down**, as upstream's status is
    /// built (`decision.delay_ns / std.time.ns_per_s`, `orchestrator.zig:
    /// 5017`), so a wait under a second has no `in Ns` and no `retry delay:`
    /// -- unlike the row, whose count is of the time left and rounds up.
    seconds: u64,
}

impl Retried {
    pub(crate) fn of(error: &ProviderError, delay: Duration) -> Self {
        Self {
            cause: cause(error),
            cause_name: cause_name(error),
            diagnostic: diagnostic(error),
            seconds: delay.as_secs(),
        }
    }

    /// The record for `attempt` of at most `limit` --
    ///
    /// `⚠ Provider unavailable · HTTP 500 · … · retrying request in 2s`
    /// `kind: auto_retry · cause: provider_unavailable · attempt: 1/3 · retry
    /// delay: 2s · diagnostic: HTTP 500 · …`
    ///
    /// -- the label, then the facts it cannot carry. The budget is xfx's own,
    /// three attempts rather than upstream's ten.
    pub(crate) fn record(&self, attempt: u32, limit: u32) -> Notice {
        let mut body = label(self.cause, &self.diagnostic, self.seconds);
        body.push_str(&format!(
            "\nkind: auto_retry \u{b7} cause: {}",
            self.cause_name
        ));
        if limit > 0 {
            body.push_str(&format!(" \u{b7} attempt: {attempt}/{limit}"));
        }
        if self.seconds > 0 {
            body.push_str(&format!(" \u{b7} retry delay: {}s", self.seconds));
        }
        if !self.diagnostic.is_empty() {
            body.push_str(&format!(" \u{b7} diagnostic: {}", self.diagnostic));
        }
        // `routeRecoveryNoticeTone`: a retry is a warning (`:107-113`).
        Notice::warning("recovery", body)
    }
}

/// The record that a retried request got through on `attempt`
/// (`pushAutoRecoveredStatus`, `orchestrator.zig:5040-5048`; its label
/// `types.zig:478-482`): a success, with no cause, delay or diagnostic.
pub(crate) fn recovered(attempt: u32, limit: u32) -> Notice {
    let mut body =
        format!("\u{2713} recovered \u{b7} succeeded on attempt {attempt}\nkind: auto_recovered");
    if limit > 0 {
        body.push_str(&format!(" \u{b7} attempt: {attempt}/{limit}"));
    }
    Notice::new(super::notice::Tone::Success, "recovery", body)
}

/// What the server said, as upstream puts it on this row.
///
/// For a status: `HTTP 500`, then -- when the body is the Gateway's error
/// shape -- the provider, then the error's code or type and its message after a
/// colon (`HTTP 500 · server_error: upstream exploded`), and otherwise the body
/// after a colon (`HTTP 500: unexpected request`). For a connection that never
/// opened, upstream's short name for what the socket said
/// (`ModelFailureDiagnostic.humanText`, `types.zig:395-418`) rather than the
/// HTTP client's sentence, which names the URL. For an in-band failure, the
/// provider's own detail.
///
/// One line, bounded: a server's body may be four kilobytes of anything, and
/// this is a row.
pub(crate) fn diagnostic(error: &ProviderError) -> String {
    let text = match error {
        ProviderError::Status { status, body, .. } => http(*status, body),
        ProviderError::Connect { detail, .. } => connection(detail).to_string(),
        ProviderError::Protocol(SseError::ProviderFailure { detail, .. }) => detail.clone(),
        other => other.to_string(),
    };
    crate::output::safe_one_line(&text, DIAGNOSTIC_BYTES)
}

/// The row's text: `⚠ {cause} · {diagnostic} · retrying request in {N}s`, the
/// diagnostic left out when there is none and the count when it is zero
/// (`recoveryLabel`, `types.zig:571-601`).
pub(crate) fn label(cause: &str, diagnostic: &str, seconds: u64) -> String {
    let mut text = format!("\u{26a0} {cause}");
    if !diagnostic.is_empty() {
        text.push_str(" \u{b7} ");
        text.push_str(diagnostic);
    }
    text.push_str(" \u{b7} ");
    text.push_str(ACTION);
    if seconds > 0 {
        text.push_str(&format!(" in {seconds}s"));
    }
    text
}

/// Whole seconds in `remaining`, rounded up (`worker_status.zig:210-212`).
fn seconds_up(remaining: Duration) -> u64 {
    let nanos = remaining.as_nanos();
    if nanos == 0 {
        return 0;
    }
    u64::try_from((nanos - 1) / 1_000_000_000 + 1).unwrap_or(u64::MAX)
}

/// `formatHttpDiagnostic` with no title (`gateway_error_format.zig:77-145`).
fn http(status: u16, body: &str) -> String {
    if body.is_empty() {
        return format!("HTTP {status}");
    }
    let parsed: Option<serde_json::Value> = serde_json::from_str(body).ok();
    let Some(error) = parsed
        .as_ref()
        .and_then(|root| root.get("error"))
        .and_then(serde_json::Value::as_object)
    else {
        return format!("HTTP {status}: {body}");
    };
    let field = |object: &serde_json::Map<String, serde_json::Value>, name: &str| {
        object
            .get(name)
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
    };
    let param = error.get("param").and_then(serde_json::Value::as_object);
    let code = field(error, "code")
        .or_else(|| field(error, "type"))
        .or_else(|| param.and_then(|param| field(param, "name")));
    let message =
        field(error, "message").or_else(|| param.and_then(|param| field(param, "message")));
    let mut text = format!("HTTP {status}");
    if let Some(provider) = field(error, "provider") {
        text.push_str(&format!(" \u{b7} Provider: {provider}"));
    }
    match (code, message) {
        (Some(code), Some(message)) if code != message => {
            text.push_str(&format!(" \u{b7} {code}: {message}"));
        }
        (Some(code), _) => text.push_str(&format!(" \u{b7} {code}")),
        (None, Some(message)) => text.push_str(&format!(" \u{b7} {message}")),
        (None, None) => {}
    }
    text
}

/// What a connection that never opened is called on this row: upstream's
/// short names (`types.zig:397-413`), read off the client's sentence, and a
/// plain `connection failed` when the sentence names none of them.
fn connection(detail: &str) -> &'static str {
    let detail = detail.to_ascii_lowercase();
    for (needle, name) in [
        ("refused", "connection refused"),
        ("reset", "connection reset"),
        ("timed out", "connection timed out"),
        ("timeout", "timed out"),
        ("dns", "cannot resolve host"),
        ("resolve", "cannot resolve host"),
        ("unreachable", "network unreachable"),
    ] {
        if detail.contains(needle) {
            return name;
        }
    }
    "connection failed"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(status: u16, body: &str) -> ProviderError {
        ProviderError::Status {
            subject: crate::gateway::GATEWAY_SUBJECT,
            status,
            body: body.to_string(),
            retryable: true,
            retry_after: None,
        }
    }

    #[test]
    fn a_gateway_error_body_is_said_as_its_code_and_its_message() {
        // `fx-retry-40x120-r0.6`, the fake gateway's 500 with
        // `{"error":{"message":"upstream exploded","type":"server_error"}}`.
        let error = status(
            500,
            r#"{"error":{"message":"upstream exploded","type":"server_error"}}"#,
        );
        assert_eq!(cause(&error), "Provider unavailable");
        assert_eq!(
            diagnostic(&error),
            "HTTP 500 \u{b7} server_error: upstream exploded"
        );
        assert_eq!(
            label(cause(&error), &diagnostic(&error), 1),
            "\u{26a0} Provider unavailable \u{b7} HTTP 500 \u{b7} server_error: upstream exploded \u{b7} retrying request in 1s"
        );
    }

    #[test]
    fn a_body_that_is_not_the_error_shape_is_quoted_after_the_status() {
        // `fx-retry-40x120-f1`: the fake gateway's plain-text 500.
        let error = status(500, "unexpected request");
        assert_eq!(diagnostic(&error), "HTTP 500: unexpected request");
        assert_eq!(
            label(cause(&error), &diagnostic(&error), 2),
            "\u{26a0} Provider unavailable \u{b7} HTTP 500: unexpected request \u{b7} retrying request in 2s"
        );
        assert_eq!(diagnostic(&status(503, "")), "HTTP 503");
    }

    #[test]
    fn the_provider_and_a_code_equal_to_its_message_are_said_once_each() {
        let error = status(
            502,
            r#"{"error":{"code":"overloaded","message":"overloaded","provider":"anthropic"}}"#,
        );
        assert_eq!(
            diagnostic(&error),
            "HTTP 502 \u{b7} Provider: anthropic \u{b7} overloaded"
        );
        assert_eq!(
            diagnostic(&status(500, r#"{"error":{"message":"only words"}}"#)),
            "HTTP 500 \u{b7} only words"
        );
    }

    #[test]
    fn a_rate_limit_and_a_connection_that_never_opened_are_named_as_upstream_names_them() {
        assert_eq!(cause(&status(429, "")), "Rate limited");
        let refused = ProviderError::Connect {
            subject: crate::gateway::GATEWAY_SUBJECT,
            detail: "error sending request for url (http://127.0.0.1:9/): Connection refused (os error 61)"
                .to_string(),
        };
        assert_eq!(cause(&refused), "Connection lost");
        assert_eq!(diagnostic(&refused), "connection refused");
        let failure = ProviderError::Protocol(SseError::ProviderFailure {
            detail: "overloaded_error: Overloaded".to_string(),
            retryable: true,
        });
        assert_eq!(cause(&failure), "Provider unavailable");
        assert_eq!(diagnostic(&failure), "overloaded_error: Overloaded");
    }

    #[test]
    fn the_count_rounds_up_and_is_dropped_once_the_attempt_is_under_way() {
        // `fx-retry-40x120-r0.6` (`in 1s`) then `-r1.3` (no count).
        let now = Instant::now();
        let standing = Standing {
            cause: "Provider unavailable",
            diagnostic: "HTTP 500".to_string(),
            deadline: now + Duration::from_millis(250),
        };
        assert!(standing.label(now).ends_with("retrying request in 1s"));
        assert!(standing
            .label(now + Duration::from_millis(249))
            .ends_with("retrying request in 1s"));
        assert!(standing
            .label(now + Duration::from_millis(250))
            .ends_with("\u{b7} retrying request"));
        let longer = Standing {
            deadline: now + Duration::from_millis(1001),
            ..standing
        };
        assert!(longer.label(now).ends_with("in 2s"));
        assert!(longer
            .label(now + Duration::from_millis(1))
            .ends_with("in 1s"));
    }

    #[test]
    fn each_retry_is_recorded_as_upstreams_recovery_record() {
        // `writeRouteRecoveryBody` (`app_worker_runtime.zig:115-133`): the
        // label as it read when the retry was decided, then `kind`, `cause`,
        // `attempt`, `retry delay` and `diagnostic`.
        let error = status(
            500,
            r#"{"error":{"message":"upstream exploded","type":"server_error"}}"#,
        );
        let record = Retried::of(&error, Duration::from_secs(2)).record(1, 3);
        assert_eq!(record.tone, super::super::notice::Tone::Warning);
        assert_eq!(record.topic, "recovery");
        assert_eq!(
            record.body,
            "\u{26a0} Provider unavailable \u{b7} HTTP 500 \u{b7} server_error: upstream exploded \
             \u{b7} retrying request in 2s\n\
             kind: auto_retry \u{b7} cause: provider_unavailable \u{b7} attempt: 1/3 \u{b7} \
             retry delay: 2s \u{b7} diagnostic: HTTP 500 \u{b7} server_error: upstream exploded"
        );
    }

    #[test]
    fn a_wait_under_a_second_is_recorded_without_a_delay_as_upstream_rounds_it_down() {
        // xfx's first backoff is 250 ms; upstream's status takes the delay in
        // whole seconds, rounded down (`orchestrator.zig:5017`).
        let record = Retried::of(&status(429, ""), Duration::from_millis(250)).record(2, 3);
        assert_eq!(
            record.body,
            "\u{26a0} Rate limited \u{b7} HTTP 429 \u{b7} retrying request\n\
             kind: auto_retry \u{b7} cause: rate_limited \u{b7} attempt: 2/3 \u{b7} \
             diagnostic: HTTP 429"
        );
        let refused = ProviderError::Connect {
            subject: crate::gateway::GATEWAY_SUBJECT,
            detail: "Connection refused (os error 61)".to_string(),
        };
        assert!(record_cause(&refused).contains("cause: connectivity_lost"));
    }

    #[test]
    fn the_next_attempt_and_the_recovery_are_recorded_as_upstream_records_them() {
        // `fx-interrupt-recovery-80x120-full`: `attempt: 2/10` again as
        // attempt 2 starts, with the failed one's cause, delay and
        // diagnostic; then `✓ recovery: ✓ recovered · succeeded on attempt 3`
        // and `kind: auto_recovered · attempt: 3/10`.
        let retried = Retried::of(&status(500, ""), Duration::from_secs(1));
        assert_eq!(
            retried.record(2, 3).body,
            "\u{26a0} Provider unavailable \u{b7} HTTP 500 \u{b7} retrying request in 1s\n\
             kind: auto_retry \u{b7} cause: provider_unavailable \u{b7} attempt: 2/3 \u{b7} \
             retry delay: 1s \u{b7} diagnostic: HTTP 500"
        );
        let done = recovered(3, 3);
        assert_eq!(done.tone, super::super::notice::Tone::Success);
        assert_eq!(done.topic, "recovery");
        assert_eq!(
            done.body,
            "\u{2713} recovered \u{b7} succeeded on attempt 3\nkind: auto_recovered \u{b7} attempt: 3/3"
        );
    }

    fn record_cause(error: &ProviderError) -> String {
        Retried::of(error, Duration::ZERO).record(1, 3).body
    }

    #[test]
    fn a_label_too_wide_wraps_under_its_mark_as_upstreams_does() {
        // `fx-interrupt-retry-24x80-r0.15`: the label broken at the last space
        // that fits 80 columns, the rest under it two columns in.
        let label = "\u{26a0} Provider unavailable \u{b7} HTTP 500 \u{b7} server_error: upstream \
                     exploded \u{b7} retrying request in 1s";
        assert_eq!(
            wrapped(label, 80, true),
            [
                "\u{26a0} Provider unavailable \u{b7} HTTP 500 \u{b7} server_error: upstream exploded \u{b7} retrying",
                "  request in 1s",
            ]
        );
        assert_eq!(wrapped(label, 120, true), [label]);
    }

    #[test]
    fn a_label_longer_than_three_rows_keeps_its_tail_or_its_head() {
        let label = format!(
            "\u{26a0} {} \u{b7} retrying request in 9s",
            "word ".repeat(30)
        );
        let rows = wrapped(&label, 30, true);
        assert_eq!(rows.len(), 3, "{rows:#?}");
        assert!(rows[0].starts_with("\u{26a0} word"), "{rows:#?}");
        assert!(rows[1].starts_with("  word"), "{rows:#?}");
        assert!(rows[2].starts_with("  ..."), "{rows:#?}");
        assert!(rows[2].ends_with("retrying request in 9s"), "{rows:#?}");
        for row in &rows {
            assert!(usize::from(super::super::wrap::width(row)) <= 30, "{row:?}");
        }
        let head = wrapped(&label, 30, false);
        assert!(head[2].ends_with("..."), "{head:#?}");
    }

    #[test]
    fn a_recovery_is_said_for_a_moment_and_then_not_at_all() {
        // `worker_status.zig:5` and its test: shown at 1 000 ms + 1 499, gone
        // at + 1 500.
        let now = Instant::now();
        let row = Row::Recovered {
            attempt: 2,
            until: now + RECOVERED_VISIBLE,
        };
        assert_eq!(
            row.label(now).as_deref(),
            Some("\u{2713} recovered \u{b7} attempt 2")
        );
        assert!(row
            .label(now + RECOVERED_VISIBLE - Duration::from_millis(1))
            .is_some());
        assert_eq!(row.label(now + RECOVERED_VISIBLE), None);
        assert!(!row.keeps_tail());
        assert_eq!(recovered_label(0), "\u{2713} recovered");
    }

    #[test]
    fn a_body_with_line_breaks_is_one_row() {
        let error = status(500, "line one\nline two\r\n\u{1b}[31mred");
        let said = diagnostic(&error);
        assert!(
            !said.contains('\n') && !said.contains('\r') && !said.contains('\u{1b}'),
            "{said:?}"
        );
    }
}
