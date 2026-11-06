//! What xfx itself has spent: the arithmetic behind `/usage`.
//!
//! Upstream's `/usage` reads a profile ledger of billed generations and adds
//! them up by window -- the last 30 days, 7 days, 24 hours, or the session in
//! front of the user (`vercel-labs/fx@c1db9191
//! src/core/session/usage_report.zig`). xfx keeps the same facts in the place
//! it already keeps everything a turn did: each turn's `usage_recorded` frame
//! carries one [`RequestUsage`] per model request ([`crate::session`]). This
//! module is the part that is neither the log nor the screen: which requests a
//! window holds, what they add up to, and how a number is written.
//!
//! # What counts
//!
//! A request counts when the provider said what it cost
//! ([`RequestUsage::cost`]) -- upstream's `GenerationFact`, which exists only
//! for a generation the Gateway billed (`session_usage.zig:110-175`). One
//! whose spend nobody said is not added anywhere: it marks its window
//! **incomplete**, which is what the dashboard's `Partial data` says. So does
//! a turn recorded before requests were (`legacy`), a turn whose request was
//! lost after it was sent, and a session the reader could not read.
//!
//! # Since when
//!
//! The rolling windows are only as complete as the record is old. Upstream
//! writes when its ledger began (`{"kind":"coverage","started_at_ms":…}`) and
//! calls a window that starts before that **partial**; with no ledger at all,
//! tracking `has not started`. xfx's record is the session logs, which say
//! nothing about when xfx began keeping requests in them, so the moment is kept
//! once, in the profile: [`COVERAGE_FILE`], written by the first turn that
//! records a request ([`start_tracking`]).

pub mod cache;
pub mod dashboard;
pub mod policy;
pub mod report;
pub mod view;

use std::collections::HashMap;
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;

use crate::session::{RecordedUsage, RequestUsage};

/// Where the profile keeps the moment `/usage` began counting, beside
/// `settings.json`.
pub const COVERAGE_FILE: &str = "usage.json";

/// The most of [`COVERAGE_FILE`] that is read: one small object.
const MAX_COVERAGE_BYTES: u64 = 1024;

/// One day, in milliseconds.
const DAY_MS: i64 = 24 * 60 * 60 * 1000;

/// The widest window, and so the oldest request any window can hold.
pub const LONGEST_WINDOW_MS: i64 = 30 * DAY_MS;

/// The most models one window lists (`max_models`, `usage_report.zig:8`).
const MAX_MODELS: usize = 128;

/// Which spend the dashboard is showing.
///
/// Declared in upstream's enum order -- session, then wider and wider windows
/// (`Scope`, `usage_report.zig:10-60`) -- which is the order Left and Right
/// walk, **not** the order the header prints them in ([`Scope::HEADER`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scope {
    Session,
    Hours24,
    Days7,
    Days30,
}

impl Scope {
    /// The header's order: the widest first (`composeUsageHeaderRow`,
    /// `compact_command_menu_presentation.zig:289-294`).
    pub const HEADER: [Scope; 4] = [Self::Days30, Self::Days7, Self::Hours24, Self::Session];

    /// What the header calls it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Session => "Session",
            Self::Hours24 => "24 hours",
            Self::Days7 => "7 days",
            Self::Days30 => "30 days",
        }
    }

    /// How far back a rolling window reaches; `None` for the session.
    pub fn duration_ms(self) -> Option<i64> {
        match self {
            Self::Session => None,
            Self::Hours24 => Some(DAY_MS),
            Self::Days7 => Some(7 * DAY_MS),
            Self::Days30 => Some(30 * DAY_MS),
        }
    }

    /// One step towards the session, stopping there (`Scope.previous`).
    pub fn previous(self) -> Self {
        match self {
            Self::Session | Self::Hours24 => Self::Session,
            Self::Days7 => Self::Hours24,
            Self::Days30 => Self::Days7,
        }
    }

    /// One step towards 30 days, stopping there (`Scope.next`).
    pub fn next(self) -> Self {
        match self {
            Self::Session => Self::Hours24,
            Self::Hours24 => Self::Days7,
            Self::Days7 | Self::Days30 => Self::Days30,
        }
    }

    /// One step along the header, wrapping: what Tab (`forward`) and Shift-Tab
    /// do (`cycleUsageMenuScope`, `app_input_runtime.zig:2714-2730`).
    pub fn cycled(self, forward: bool) -> Self {
        let at = Self::HEADER
            .iter()
            .position(|scope| *scope == self)
            .unwrap_or(0);
        let count = Self::HEADER.len();
        Self::HEADER[if forward {
            (at + 1) % count
        } else {
            (at + count - 1) % count
        }]
    }
}

/// How much of a rolling window the record reaches back over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Coverage {
    /// Nothing has been recorded yet, anywhere.
    NotStarted,
    /// Recording began inside the window, so its start is missing.
    Partial,
    /// Recording began before the window did.
    Full,
}

/// Whether what a window holds is all it should.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Completeness {
    /// Every request in it said what it cost.
    Complete,
    /// A request in it named its generation and not yet its bill, which is
    /// being asked for (`pending`, `usage_report.zig:90-95`).
    Pending,
    /// Something in it was spent without saying how much.
    Incomplete,
    /// The session's turns were recorded as totals only, before requests were:
    /// nothing in it can be counted (`legacy`, `usage_report.zig:90-95`).
    Legacy,
}

/// What a window adds up to (`Totals`, `usage_report.zig:178-187`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Totals {
    /// Input and output together; the cache and reasoning counts are inside
    /// them, never added to them.
    pub total_tokens: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    /// `None` once any request in it did not say.
    pub reasoning_tokens: Option<u64>,
    pub request_count: Option<u64>,
    pub total_cost: f64,
}

/// One model's share of a window.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelUsage {
    pub model: String,
    pub totals: Totals,
}

/// The session's own clock and edits (`SessionActivity`, `usage_report.zig:201-209`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SessionActivity {
    /// How long its requests took, added up.
    pub api_duration_ms: u64,
    /// How long since it began.
    pub wall_duration_ms: u64,
    pub lines_added: u64,
    pub lines_removed: u64,
}

/// One window, added up and ready to be drawn (`Snapshot`, `usage_report.zig:236-246`).
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub scope: Scope,
    pub coverage: Coverage,
    pub completeness: Completeness,
    /// `None` when there is nothing it could add up: tracking has not started,
    /// or the session predates request records.
    pub totals: Option<Totals>,
    /// Most tokens first, then most spend, then by name.
    pub models: Vec<ModelUsage>,
    /// The session's block, on the session scope only.
    pub activity: Option<SessionActivity>,
}

/// Every request the rolling windows can see, read from the session logs.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Ledger {
    /// When counting began: [`COVERAGE_FILE`], or the oldest request read when
    /// that file is missing; `None` when neither says anything.
    pub coverage_started_at_ms: Option<i64>,
    /// Every request that said what it cost.
    facts: Vec<RequestUsage>,
    /// When each thing happened that was spent without a cost: a request with
    /// none, a turn recorded as totals only, a request lost after it was sent,
    /// a session that could not be read.
    incidents: Vec<i64>,
    /// When each request was made whose bill is still being asked for
    /// ([`RequestUsage::pending`]).
    pending: Vec<i64>,
}

impl Ledger {
    /// The ledger the sessions add up to.
    ///
    /// `unreadable` is one entry per session that could not be read: when it
    /// was last written, where the manifest said, or `None` when not even that
    /// could be read -- spend that could be in any window.
    pub fn from_sessions<'a>(
        sessions: impl IntoIterator<Item = &'a RecordedUsage>,
        unreadable: &[Option<i64>],
        marker: Option<i64>,
    ) -> Self {
        let mut ledger = Self::default();
        let mut oldest: Option<i64> = None;
        for usage in sessions {
            for request in &usage.requests {
                oldest = Some(oldest.map_or(request.at_ms, |seen| seen.min(request.at_ms)));
                if request.cost.is_some() {
                    ledger.facts.push(request.clone());
                } else if request.pending() {
                    ledger.pending.push(request.at_ms);
                } else {
                    ledger.incidents.push(request.at_ms);
                }
            }
            ledger.incidents.extend(&usage.legacy_at_ms);
            ledger.incidents.extend(&usage.unaccounted_at_ms);
        }
        for session in unreadable {
            // A session nobody can date is spend that could be in any window:
            // it is placed in every one.
            ledger.incidents.push(session.unwrap_or(i64::MAX));
        }
        ledger.coverage_started_at_ms = match (marker, oldest) {
            (Some(marker), Some(oldest)) => Some(marker.min(oldest)),
            (marker, oldest) => marker.or(oldest),
        };
        ledger
    }

    /// One rolling window ending at `now_ms` (`buildRollingSnapshot`,
    /// `usage_report.zig:500-600`): closed at its start, open at its end.
    ///
    /// `scope` is a rolling one; asked for the session, it answers the widest
    /// window rather than inventing a session it was never given.
    pub fn snapshot(&self, scope: Scope, now_ms: i64) -> Snapshot {
        let duration = scope.duration_ms().unwrap_or(LONGEST_WINDOW_MS);
        let start = now_ms.saturating_sub(duration);
        let inside = |at: i64| at >= start && at < now_ms;
        // A coverage that begins in the future is a clock that moved, and says
        // nothing about this window.
        let started = self.coverage_started_at_ms.filter(|at| *at <= now_ms);
        let coverage = match started {
            None => Coverage::NotStarted,
            Some(at) if at <= start => Coverage::Full,
            Some(_) => Coverage::Partial,
        };
        // Missing spend outranks spend still being asked for
        // (`buildRollingSnapshot`, `usage_report.zig:530-539`).
        let mut completeness = if self
            .incidents
            .iter()
            .any(|at| *at == i64::MAX || inside(*at))
        {
            Completeness::Incomplete
        } else if self.pending.iter().any(|at| inside(*at)) {
            Completeness::Pending
        } else {
            Completeness::Complete
        };
        if coverage == Coverage::NotStarted {
            return Snapshot {
                scope,
                coverage,
                completeness,
                totals: None,
                models: Vec::new(),
                activity: None,
            };
        }
        let mut seen: HashMap<&str, &RequestUsage> = HashMap::new();
        let mut counted: Vec<&RequestUsage> = Vec::new();
        for fact in self.facts.iter().filter(|fact| inside(fact.at_ms)) {
            // The same generation twice is one generation, and two different
            // accounts of it mean neither can be trusted (`usage_report.zig:556-562`).
            if let Some(id) = fact.generation_id.as_deref() {
                if let Some(first) = seen.get(id) {
                    if *first != fact {
                        completeness = Completeness::Incomplete;
                    }
                    continue;
                }
                seen.insert(id, fact);
            }
            counted.push(fact);
        }
        let (totals, models) = add_up(&counted, Some(0), true);
        Snapshot {
            scope,
            coverage,
            completeness,
            totals: Some(totals),
            models,
            activity: None,
        }
    }
}

/// The session in front of the user (`buildSessionSnapshot`,
/// `usage_report.zig:380-470`), as of `now_ms`.
///
/// `started_at_ms` is when the session began, for the wall clock; `None` for a
/// session nothing has been asked in yet, whose clock has not started.
pub fn session_snapshot(
    usage: &RecordedUsage,
    started_at_ms: Option<i64>,
    now_ms: i64,
) -> Snapshot {
    let activity = SessionActivity {
        api_duration_ms: usage
            .requests
            .iter()
            .fold(0u64, |sum, request| sum.saturating_add(request.duration_ms)),
        wall_duration_ms: started_at_ms
            .map(|at| u64::try_from(now_ms.saturating_sub(at)).unwrap_or(0))
            .unwrap_or(0),
        lines_added: usage.lines_added,
        lines_removed: usage.lines_removed,
    };
    if !usage.legacy_at_ms.is_empty() {
        return Snapshot {
            scope: Scope::Session,
            coverage: Coverage::Full,
            completeness: Completeness::Legacy,
            totals: None,
            models: Vec::new(),
            activity: Some(activity),
        };
    }
    let missing = usage
        .requests
        .iter()
        .any(|request| request.cost.is_none() && !request.pending());
    let completeness = if missing || !usage.unaccounted_at_ms.is_empty() {
        Completeness::Incomplete
    } else if usage.pending().next().is_some() {
        Completeness::Pending
    } else {
        Completeness::Complete
    };
    let counted: Vec<&RequestUsage> = usage
        .requests
        .iter()
        .filter(|request| request.cost.is_some())
        .collect();
    // A session's reasoning starts at nought rather than at "not said"
    // (`Usage.initFresh`, `session_usage.zig:403-412`): the rolling windows
    // say `Reasoning unavailable` over no requests, and the session says `0`.
    let (totals, models) = add_up(&counted, Some(0), false);
    Snapshot {
        scope: Scope::Session,
        coverage: Coverage::Full,
        completeness,
        totals: Some(totals),
        models,
        activity: Some(activity),
    }
}

/// Adds `requests` up, overall and per model.
///
/// `rolling` is upstream's freeze rule for a window: over no requests at all
/// its reasoning is unknown rather than nought (`MutableTotals.freeze`,
/// `usage_report.zig:300-320`).
fn add_up(
    requests: &[&RequestUsage],
    reasoning_from: Option<u64>,
    rolling: bool,
) -> (Totals, Vec<ModelUsage>) {
    let mut whole = Sum::new(reasoning_from);
    let mut models: Vec<(String, Sum)> = Vec::new();
    for request in requests {
        whole.add(request);
        let known = models.len();
        match models.iter_mut().find(|(model, _)| *model == request.model) {
            Some((_, sum)) => sum.add(request),
            None if known < MAX_MODELS => {
                let mut sum = Sum::new(Some(0));
                sum.add(request);
                models.push((request.model.clone(), sum));
            }
            None => {}
        }
    }
    let mut models: Vec<ModelUsage> = models
        .into_iter()
        .map(|(model, sum)| ModelUsage {
            model,
            totals: sum.frozen(rolling),
        })
        .collect();
    models.sort_by(|left, right| {
        right
            .totals
            .total_tokens
            .cmp(&left.totals.total_tokens)
            .then_with(|| {
                right
                    .totals
                    .total_cost
                    .partial_cmp(&left.totals.total_cost)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| left.model.cmp(&right.model))
    });
    (whole.frozen(rolling), models)
}

/// A running total.
#[derive(Debug, Clone, Copy)]
struct Sum {
    input: u64,
    output: u64,
    cache_read: u64,
    cache_write: u64,
    reasoning: Option<u64>,
    requests: u64,
    cost: f64,
}

impl Sum {
    fn new(reasoning: Option<u64>) -> Self {
        Self {
            input: 0,
            output: 0,
            cache_read: 0,
            cache_write: 0,
            reasoning,
            requests: 0,
            cost: 0.0,
        }
    }

    fn add(&mut self, request: &RequestUsage) {
        self.input = self.input.saturating_add(request.input_tokens);
        self.output = self.output.saturating_add(request.output_tokens);
        self.cache_read = self.cache_read.saturating_add(request.cache_read_tokens);
        self.cache_write = self.cache_write.saturating_add(request.cache_write_tokens);
        self.reasoning = match (self.reasoning, request.reasoning_tokens) {
            (Some(sum), Some(more)) => Some(sum.saturating_add(more)),
            _ => None,
        };
        self.requests = self.requests.saturating_add(1);
        self.cost += request.cost.unwrap_or(0.0);
    }

    fn frozen(self, rolling: bool) -> Totals {
        Totals {
            total_tokens: self.input.saturating_add(self.output),
            input_tokens: self.input,
            output_tokens: self.output,
            cache_read_tokens: self.cache_read,
            cache_write_tokens: self.cache_write,
            reasoning_tokens: if rolling && self.requests == 0 {
                None
            } else {
                self.reasoning
            },
            request_count: Some(self.requests),
            total_cost: self.cost,
        }
    }
}

// ---------------------------------------------------------------------------
// when counting began
// ---------------------------------------------------------------------------

/// Notes that counting has begun at `at_ms`, if nothing has said so before.
///
/// Written once and never again: a second turn finds the file and leaves it,
/// which is what makes it the **first** moment. Created rather than replaced,
/// owner-only, and never through a link -- `create_new` refuses whatever is
/// already at the path. Best effort: a profile it cannot be written into costs
/// the dashboard its `Partial data` line, never a turn.
pub fn start_tracking(profile_dir: &Path, at_ms: i64) {
    let path = profile_dir.join(COVERAGE_FILE);
    if fs::symlink_metadata(&path).is_ok() {
        return;
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    if let Ok(mut file) = options.open(&path) {
        let _ = writeln!(file, "{{\"tracking_started_at_ms\":{at_ms}}}");
        let _ = file.sync_all();
    }
}

/// When counting began, as [`start_tracking`] wrote it, or `None` when it has
/// not -- or when what is there is not a small regular file that says so.
pub fn tracking_started(profile_dir: &Path) -> Option<i64> {
    let path = profile_dir.join(COVERAGE_FILE);
    let metadata = fs::symlink_metadata(&path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_COVERAGE_BYTES {
        return None;
    }
    let mut text = String::new();
    fs::File::open(&path)
        .ok()?
        .take(MAX_COVERAGE_BYTES)
        .read_to_string(&mut text)
        .ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    value.get("tracking_started_at_ms")?.as_i64()
}

/// Reads every session the widest window can see, and when counting began.
///
/// A session last written before the window began holds nothing it could
/// count, and is not opened. Off the thread that holds the terminal: this
/// replays logs. `Ok(None)` once `cancel` is set, which is looked at between
/// sessions ([`crate::session::SessionStore::usage_since`]).
pub fn read_ledger(
    profile_dir: &Path,
    now_ms: i64,
    cancel: &std::sync::atomic::AtomicBool,
) -> io::Result<Option<Ledger>> {
    let store = crate::session::SessionStore::read_only(profile_dir);
    let Some(scan) = store
        .usage_since(now_ms.saturating_sub(LONGEST_WINDOW_MS), cancel)
        .map_err(|err| io::Error::other(err.to_string()))?
    else {
        return Ok(None);
    };
    Ok(Some(Ledger::from_sessions(
        scan.sessions.iter(),
        &scan.unreadable,
        tracking_started(profile_dir),
    )))
}

// ---------------------------------------------------------------------------
// numbers, written the way upstream writes them
// ---------------------------------------------------------------------------

/// `value` with `places` decimals, rounded the way upstream's `{d:.N}` rounds:
/// on the **shortest** decimal that reads back as the same number, half away
/// from zero.
///
/// Not Rust's `{:.N}`, which rounds the exact binary value to even: the two
/// disagree on a tie, and a tie is what a price is made of -- upstream writes
/// `0.125` as `0.13` and `1.25` as `1.3`, where Rust writes `0.12` and `1.2`.
pub fn fixed(value: f64, places: usize) -> String {
    if !value.is_finite() {
        return "?".to_string();
    }
    let negative = value < 0.0;
    // `Display` is the shortest round-trip decimal, and never an exponent.
    let shortest = format!("{}", value.abs());
    let (whole, fraction) = shortest.split_once('.').unwrap_or((&shortest, ""));
    let mut digits: Vec<u8> = whole.bytes().chain(fraction.bytes()).collect();
    let mut point = whole.len();
    if fraction.len() > places {
        let keep = point + places;
        let round_up = digits[keep] >= b'5';
        digits.truncate(keep);
        if round_up {
            let mut at = digits.len();
            loop {
                if at == 0 {
                    digits.insert(0, b'1');
                    point += 1;
                    break;
                }
                at -= 1;
                if digits[at] == b'9' {
                    digits[at] = b'0';
                } else {
                    digits[at] += 1;
                    break;
                }
            }
        }
    } else {
        digits.extend(std::iter::repeat_n(b'0', places - fraction.len()));
    }
    let text = String::from_utf8(digits).unwrap_or_default();
    let (whole, fraction) = text.split_at(point);
    let sign = if negative { "-" } else { "" };
    if places == 0 {
        format!("{sign}{whole}")
    } else {
        format!("{sign}{whole}.{fraction}")
    }
}

/// A count in at most a few characters: `900`, `64K`, `67.9K`, `1.2M`
/// (`formatCompactUnsigned`, `compact_command_menu_presentation.zig:959-977`).
/// A round thousand has no decimal.
pub fn compact(value: u64) -> String {
    for (unit, suffix) in [(1_000_000_000u64, "B"), (1_000_000, "M"), (1_000, "K")] {
        if value < unit {
            continue;
        }
        if value.is_multiple_of(unit) {
            return format!("{}{suffix}", value / unit);
        }
        return format!("{}{suffix}", fixed(value as f64 / unit as f64, 1));
    }
    value.to_string()
}

/// A count with its thousands marked: `1,234` (`formatGroupedUnsigned`).
pub fn grouped(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// Dollars and cents: `$0.43` (`formatMoney`, `:997-999`).
pub fn money(value: f64) -> String {
    format!("${}", fixed(value, 2))
}

/// One part of a whole, in per cent to one place: `81.1%`
/// (`formatUsageShare`, `:1001-1007`). Nought of nought is `0.0%`.
pub fn share(part: u64, whole: u64) -> String {
    let share = if whole == 0 {
        0.0
    } else {
        part as f64 * 100.0 / whole as f64
    };
    format!("{}%", fixed(share, 1))
}

/// A duration the way the dashboard writes one: `17s`, `2m 5s`, `1h 0m 3s`
/// (`formatDuration`, `:1332-1344`).
pub fn duration(ms: u64) -> String {
    let seconds = ms / 1000;
    let (hours, minutes, seconds) = (seconds / 3600, (seconds % 3600) / 60, seconds % 60);
    if hours > 0 {
        format!("{hours}h {minutes}m {seconds}s")
    } else if minutes > 0 {
        format!("{minutes}m {seconds}s")
    } else {
        format!("{seconds}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 40 * DAY_MS;

    fn request(
        model: &str,
        at_ms: i64,
        input: u64,
        output: u64,
        cost: Option<f64>,
    ) -> RequestUsage {
        RequestUsage {
            at_ms,
            duration_ms: 100,
            model: model.to_string(),
            input_tokens: input,
            output_tokens: output,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            reasoning_tokens: Some(0),
            cost,
            generation_id: None,
            origin: None,
        }
    }

    fn usage(requests: Vec<RequestUsage>) -> RecordedUsage {
        RecordedUsage {
            requests,
            ..RecordedUsage::default()
        }
    }

    /// The two billed turns of the fx capture (`billed-turns.json`).
    fn billed_turns(at_ms: i64) -> RecordedUsage {
        usage(vec![
            RequestUsage {
                cache_read_tokens: 4000,
                generation_id: Some("gen_01JABCDEFGHJKMNPQRSTVWXYZ0".to_string()),
                origin: None,
                ..request("fake/model-a", at_ms, 12000, 800, Some(0.0125))
            },
            RequestUsage {
                cache_read_tokens: 30000,
                reasoning_tokens: Some(900),
                generation_id: Some("gen_01JABCDEFGHJKMNPQRSTVWXYZ1".to_string()),
                origin: None,
                ..request(
                    "anthropic/claude-opus-5",
                    at_ms + 1,
                    52000,
                    3100,
                    Some(0.421),
                )
            },
        ])
    }

    #[test]
    fn numbers_are_written_with_upstreams_rounding_not_rusts() {
        // The ties Rust's exact rounding and upstream's shortest-digit
        // rounding disagree on, measured against the Zig 0.16 build of fx.
        assert_eq!(fixed(0.125, 2), "0.13");
        assert_eq!(fixed(0.015, 2), "0.02");
        assert_eq!(fixed(1.005, 2), "1.01");
        assert_eq!(fixed(2.675, 2), "2.68");
        assert_eq!(fixed(1.25, 1), "1.3");
        assert_eq!(fixed(0.15, 1), "0.2");
        // And the ones they agree on, from the capture.
        assert_eq!(fixed(0.0125, 2), "0.01");
        assert_eq!(fixed(0.4335, 2), "0.43");
        assert_eq!(fixed(2.5, 2), "2.50");
        assert_eq!(fixed(0.0, 2), "0.00");
        assert_eq!(fixed(9.96, 1), "10.0");
        assert_eq!(fixed(99.99, 1), "100.0");
    }

    #[test]
    fn counts_are_compact_and_a_round_thousand_has_no_decimal() {
        assert_eq!(compact(0), "0");
        assert_eq!(compact(900), "900");
        assert_eq!(compact(64_000), "64K");
        assert_eq!(compact(67_900), "67.9K");
        assert_eq!(compact(3_900), "3.9K");
        assert_eq!(compact(1_250), "1.3K");
        assert_eq!(compact(2_000_000), "2M");
        assert_eq!(compact(1_500_000_000), "1.5B");
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(1_234_567), "1,234,567");
        assert_eq!(grouped(123), "123");
        assert_eq!(money(0.4335), "$0.43");
        assert_eq!(share(55_100, 67_900), "81.1%");
        assert_eq!(share(12_800, 67_900), "18.9%");
        assert_eq!(share(0, 0), "0.0%");
        assert_eq!(duration(17_400), "17s");
        assert_eq!(duration(125_000), "2m 5s");
        assert_eq!(duration(3_603_000), "1h 0m 3s");
    }

    #[test]
    fn tab_walks_the_header_and_the_arrows_walk_the_enum() {
        assert_eq!(Scope::Days30.cycled(true), Scope::Days7);
        assert_eq!(Scope::Session.cycled(true), Scope::Days30);
        assert_eq!(Scope::Days30.cycled(false), Scope::Session);
        // Left from 30 days lands on 7 days, which the header prints to its
        // right; Right from 30 days stays put (`fx-usage-40x120-left`).
        assert_eq!(Scope::Days30.previous(), Scope::Days7);
        assert_eq!(Scope::Days30.next(), Scope::Days30);
        assert_eq!(Scope::Session.previous(), Scope::Session);
    }

    #[test]
    fn nothing_recorded_anywhere_is_tracking_that_has_not_started() {
        let ledger = Ledger::from_sessions(std::iter::empty(), &[], None);
        let snapshot = ledger.snapshot(Scope::Days30, NOW);
        assert_eq!(snapshot.coverage, Coverage::NotStarted);
        assert_eq!(snapshot.totals, None);
    }

    #[test]
    fn the_capture_adds_up_to_its_own_numbers() {
        let sessions = [billed_turns(NOW - 60_000)];
        let ledger = Ledger::from_sessions(sessions.iter(), &[], Some(NOW - 120_000));
        let snapshot = ledger.snapshot(Scope::Days30, NOW);
        // Counting began a minute before: the window reaches back further.
        assert_eq!(snapshot.coverage, Coverage::Partial);
        assert_eq!(snapshot.completeness, Completeness::Complete);
        let totals = snapshot.totals.expect("totals");
        assert_eq!(compact(totals.total_tokens), "67.9K");
        assert_eq!(money(totals.total_cost), "$0.43");
        assert_eq!(totals.request_count, Some(2));
        assert_eq!(totals.reasoning_tokens, Some(900));
        assert_eq!(compact(totals.cache_read_tokens), "34K");
        let names: Vec<&str> = snapshot.models.iter().map(|m| m.model.as_str()).collect();
        assert_eq!(names, ["anthropic/claude-opus-5", "fake/model-a"]);
    }

    #[test]
    fn a_request_nobody_priced_is_missing_spend_and_counts_for_nothing() {
        let sessions = [usage(vec![request(
            "fake/model-a",
            NOW - 10,
            1200,
            40,
            None,
        )])];
        let ledger = Ledger::from_sessions(sessions.iter(), &[], Some(NOW - 20));
        let snapshot = ledger.snapshot(Scope::Hours24, NOW);
        assert_eq!(snapshot.completeness, Completeness::Incomplete);
        let totals = snapshot.totals.expect("totals");
        assert_eq!(totals.total_tokens, 0);
        assert_eq!(totals.request_count, Some(0));
        // Over no requests a window cannot say what reasoning came to.
        assert_eq!(totals.reasoning_tokens, None);
        assert!(snapshot.models.is_empty());
    }

    #[test]
    fn a_window_is_closed_at_its_start_and_open_at_its_end() {
        let start = NOW - DAY_MS;
        let sessions = [usage(vec![
            request("a/edge", start, 1, 1, Some(1.0)),
            request("a/before", start - 1, 1, 1, Some(1.0)),
            request("a/now", NOW, 1, 1, Some(1.0)),
        ])];
        let ledger = Ledger::from_sessions(sessions.iter(), &[], Some(0));
        let snapshot = ledger.snapshot(Scope::Hours24, NOW);
        assert_eq!(snapshot.coverage, Coverage::Full);
        let names: Vec<&str> = snapshot.models.iter().map(|m| m.model.as_str()).collect();
        assert_eq!(names, ["a/edge"]);
    }

    #[test]
    fn one_generation_seen_twice_is_counted_once_and_two_accounts_of_it_are_neither_trusted() {
        let mut twin = billed_turns(NOW - 10);
        twin.requests.truncate(1);
        let sessions = [twin.clone(), twin.clone()];
        let ledger = Ledger::from_sessions(sessions.iter(), &[], Some(0));
        let snapshot = ledger.snapshot(Scope::Days7, NOW);
        assert_eq!(snapshot.totals.expect("totals").request_count, Some(1));
        assert_eq!(snapshot.completeness, Completeness::Complete);

        let mut disagreeing = twin.clone();
        disagreeing.requests[0].cost = Some(9.0);
        let sessions = [twin, disagreeing];
        let ledger = Ledger::from_sessions(sessions.iter(), &[], Some(0));
        assert_eq!(
            ledger.snapshot(Scope::Days7, NOW).completeness,
            Completeness::Incomplete
        );
    }

    #[test]
    fn an_unreadable_session_leaves_every_window_partial() {
        let ledger = Ledger::from_sessions(std::iter::empty(), &[None], Some(0));
        for scope in [Scope::Hours24, Scope::Days7, Scope::Days30] {
            assert_eq!(
                ledger.snapshot(scope, NOW).completeness,
                Completeness::Incomplete
            );
        }
        // One last written long ago is only missing from the windows it is in.
        let ledger = Ledger::from_sessions(std::iter::empty(), &[Some(NOW - 10 * DAY_MS)], Some(0));
        assert_eq!(
            ledger.snapshot(Scope::Hours24, NOW).completeness,
            Completeness::Complete
        );
        assert_eq!(
            ledger.snapshot(Scope::Days30, NOW).completeness,
            Completeness::Incomplete
        );
    }

    #[test]
    fn without_the_coverage_file_counting_began_at_the_oldest_request() {
        let sessions = [usage(vec![request(
            "a/m",
            NOW - 2 * DAY_MS,
            1,
            1,
            Some(0.5),
        )])];
        let ledger = Ledger::from_sessions(sessions.iter(), &[], None);
        assert_eq!(ledger.coverage_started_at_ms, Some(NOW - 2 * DAY_MS));
        assert_eq!(
            ledger.snapshot(Scope::Hours24, NOW).coverage,
            Coverage::Full
        );
        assert_eq!(
            ledger.snapshot(Scope::Days7, NOW).coverage,
            Coverage::Partial
        );
    }

    #[test]
    fn the_session_starts_its_reasoning_at_nought_and_keeps_its_clock() {
        let fresh = session_snapshot(&RecordedUsage::default(), None, NOW);
        assert_eq!(fresh.completeness, Completeness::Complete);
        let totals = fresh.totals.expect("totals");
        assert_eq!(totals.reasoning_tokens, Some(0));
        assert_eq!(fresh.activity.expect("activity").wall_duration_ms, 0);

        let mut recorded = usage(vec![request("fake/model-a", NOW - 5, 1200, 40, None)]);
        recorded.lines_added = 3;
        recorded.lines_removed = 1;
        let snapshot = session_snapshot(&recorded, Some(NOW - 12_000), NOW);
        assert_eq!(snapshot.completeness, Completeness::Incomplete);
        assert_eq!(snapshot.totals.expect("totals").reasoning_tokens, Some(0));
        let activity = snapshot.activity.expect("activity");
        assert_eq!(activity.wall_duration_ms, 12_000);
        assert_eq!(activity.api_duration_ms, 100);
        assert_eq!((activity.lines_added, activity.lines_removed), (3, 1));
    }

    /// A request whose bill is being asked for makes its windows `pending`
    /// -- `Partial data` all the same -- and spend that is missing outranks it
    /// (`buildRollingSnapshot`, `usage_report.zig:530-539`).
    #[test]
    fn a_bill_still_being_asked_for_is_pending_and_missing_spend_outranks_it() {
        let pending = RequestUsage {
            generation_id: Some("gen_01JABCDEFGHJKMNPQRSTVWXYZ1".to_string()),
            origin: Some("http://127.0.0.1:4100".to_string()),
            ..request("a/m", NOW - 60_000, 10, 2, None)
        };
        let billed = request("a/m", NOW - 2 * DAY_MS, 1, 1, Some(0.5));
        let sessions = [usage(vec![billed.clone(), pending.clone()])];
        let ledger = Ledger::from_sessions(sessions.iter(), &[], Some(0));
        for scope in [Scope::Hours24, Scope::Days30] {
            let snapshot = ledger.snapshot(scope, NOW);
            assert_eq!(snapshot.completeness, Completeness::Pending, "{scope:?}");
            // Nothing of it is counted until its bill is known.
            assert_eq!(
                snapshot.totals.expect("totals").request_count,
                Some(u64::from(scope == Scope::Days30))
            );
        }
        assert_eq!(
            session_snapshot(&sessions[0], Some(0), NOW).completeness,
            Completeness::Pending
        );
        // Rejected, it is spend that is missing.
        let rejected = RequestUsage {
            origin: None,
            ..pending.clone()
        };
        let sessions = [usage(vec![billed, pending, rejected])];
        let ledger = Ledger::from_sessions(sessions.iter(), &[], Some(0));
        assert_eq!(
            ledger.snapshot(Scope::Hours24, NOW).completeness,
            Completeness::Incomplete
        );
        assert_eq!(
            session_snapshot(&sessions[0], Some(0), NOW).completeness,
            Completeness::Incomplete
        );
    }

    #[test]
    fn a_session_recorded_as_totals_only_counts_nothing_and_says_so() {
        let recorded = RecordedUsage {
            legacy_at_ms: vec![NOW - 5],
            ..RecordedUsage::default()
        };
        let snapshot = session_snapshot(&recorded, Some(NOW - 10), NOW);
        assert_eq!(snapshot.completeness, Completeness::Legacy);
        assert_eq!(snapshot.totals, None);
        assert!(snapshot.activity.is_some());
    }

    #[test]
    fn counting_begins_once_and_a_link_is_not_followed() {
        let home = tempfile::tempdir().expect("a home");
        assert_eq!(tracking_started(home.path()), None);
        start_tracking(home.path(), 1234);
        start_tracking(home.path(), 9999);
        assert_eq!(tracking_started(home.path()), Some(1234));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(home.path().join(COVERAGE_FILE))
                .expect("written")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);

            let other = tempfile::tempdir().expect("another home");
            let target = other.path().join("elsewhere.json");
            fs::write(&target, "{\"tracking_started_at_ms\":5}").expect("write");
            std::os::unix::fs::symlink(&target, other.path().join(COVERAGE_FILE)).expect("link");
            assert_eq!(tracking_started(other.path()), None);
            start_tracking(other.path(), 77);
            assert_eq!(
                fs::read_to_string(&target).expect("read"),
                "{\"tracking_started_at_ms\":5}"
            );
        }
    }
}
