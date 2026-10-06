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
    fn a_body_with_line_breaks_is_one_row() {
        let error = status(500, "line one\nline two\r\n\u{1b}[31mred");
        let said = diagnostic(&error);
        assert!(
            !said.contains('\n') && !said.contains('\r') && !said.contains('\u{1b}'),
            "{said:?}"
        );
    }
}
