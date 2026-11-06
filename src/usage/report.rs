//! AI Gateway usage reports: `GET {origin}/v1/report`, read back as one row
//! per model.
//!
//! fx v0.0.16 stopped adding up a local ledger for `/usage` and asks the
//! provider instead (`vercel-labs/fx@481104fd src/usage/usage.zig:1-5`). For
//! an AI Gateway API key that is one authenticated `GET` per range of UTC days,
//! `/v1/report?start_date=…&end_date=…&group_by=model&api_key_id=self`
//! (`src/usage/report.zig:196-210`), whose `results[]` rows are summed per
//! model and sorted by cost (`:220-280`). This module is that client and that
//! parser, and nothing else: the cache, the fetch plan and the screen are built
//! on its types.
//!
//! # Who is asked
//!
//! Only an `AI_GATEWAY_API_KEY` credential, fx's `api_key` scope
//! (`src/core/app/usage_host.zig:105-111`). `VERCEL_OIDC_TOKEN` and llmux are
//! [`Unavailable::UnsupportedProvider`], fx's answer for every source it has
//! no reader for (`:72-80`, `:128`; the user chose fx's answer for both on
//! 2026-11-06). A provider setting xfx could not read is
//! [`Unavailable::ProviderRejected`]: it poisons every request that would carry
//! the Gateway key (`RuntimeConfig::provider_rejected`). No request is made for
//! any of them.
//!
//! # What the key is trusted to
//!
//! - **The configured Gateway origin, and nothing else.** A [`Question`] is
//!   built from an [`Endpoint`], which exists only for a URL the bearer rule
//!   accepted -- https anywhere, cleartext http only to a loopback address with
//!   a port (`crate::gateway::Endpoint::checked`) -- and the report is asked
//!   under that URL's origin, the host a turn already sends the same key to.
//! - **No redirect is followed, and no proxy is used over cleartext**: an
//!   environment proxy is honoured only for https, where it sees a TLS tunnel
//!   ([`crate::gateway::bearer_rules`]). A `3xx` is an answer that is not
//!   a report: `bad_response`, as fx's `.redirect_behavior = .unhandled` makes
//!   it (`src/gateway/client.zig:418`, `src/usage/usage.zig:889-896`).
//! - **The answer is bounded**: at most [`MAX_BODY_BYTES`], refused -- not
//!   truncated -- past that, and the whole request, connect to last byte,
//!   within [`DEADLINE`] (`client.zig:268-269`, `:299-308`, `:427-433`).
//! - **No failure quotes the other end.** fx's [`Failure`] is a kind with no
//!   text, and so is what this module reports: a [`Failed`]'s detail is a
//!   status code or a fixed phrase, never a byte of the response, so a server
//!   that echoes the key back has nowhere to put it. A report whose model name
//!   holds the key is refused rather than shown or cached.
//! - **The worker lets go when nobody waits**: dropping a [`Fetch`] cancels it,
//!   so a dashboard that is gone does not keep the key asking.

use std::collections::BTreeMap;
use std::fmt;
use std::io;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Duration;

use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use tokio_util::sync::CancellationToken;

use crate::config::{CredentialSource, RuntimeConfig};
use crate::gateway::{Endpoint, USER_AGENT};
use crate::provider::ProviderId;

/// The largest answer read from `/v1/report` (`max_body_bytes`,
/// `report.zig:9`; `usage_response_max_bytes`, `client.zig:268`).
pub const MAX_BODY_BYTES: usize = 1024 * 1024;

/// The most model rows one report may hold (`max_models`, `report.zig:11`).
pub const MAX_MODELS: usize = 256;

/// The longest model name, in bytes (`max_model_name_bytes`, `report.zig:12`).
pub const MAX_MODEL_NAME_BYTES: usize = 256;

/// How long one request may take, from connecting to its last byte
/// (`usage_request_timeout_ms`, `client.zig:269`).
pub const DEADLINE: Duration = Duration::from_secs(15);

/// Who the numbers belong to, for the heading (`usage_host.zig:110`). Names the
/// variable, never its value.
pub const API_KEY_LABEL: &str = "API key (AI_GATEWAY_API_KEY)";

/// The report filter for an API key: that key's own spend.
const API_KEY_FILTER: &str = "api_key_id=self";

/// One day, in milliseconds.
const DAY_MS: i64 = 24 * 60 * 60 * 1000;

/// What a failure says when the request never got an answer.
const UNREACHABLE: &str = "couldn't reach AI Gateway";
/// What a failure says about an answer past [`MAX_BODY_BYTES`].
const TOO_LARGE: &str = "AI Gateway's answer is over 1 MiB";
/// What a failure says about an answer that broke off.
const CUT_SHORT: &str = "AI Gateway's answer stopped partway";
/// What a failure says when the key cannot be written as a header value.
const UNSENDABLE_KEY: &str = "the key cannot be sent as a header";
/// What a failure says when no HTTP client or runtime could be made.
const NO_CLIENT: &str = "no HTTP client could be started";
/// What a failure says when the worker ended without an answer.
const WORKER_GONE: &str = "the usage worker ended without an answer";
/// What a failure says about a report that names the key.
const KEY_IN_REPORT: &str = "unexpected report: a model name holds the key";

/// What one model, or a whole period, adds up to (`Totals`,
/// `report.zig:33-102`).
///
/// Serialized as the usage cache writes it (`crate::usage::cache`): every
/// field named as fx names it, each defaulting to 0 when absent, and an
/// unknown field refused, as fx's `std.json` parse refuses one.
#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Totals {
    pub request_count: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub reasoning_tokens: u64,
    pub total_cost: f64,
    /// Requests that hit the prompt cache (`cache_hit_pct` times requests).
    /// Counts add exactly across reports, unlike percentages.
    pub cache_hit_requests: f64,
    /// Requests that failed: `http_failure_pct + malformed_response_pct`
    /// times requests.
    pub failed_requests: f64,
    /// `p90_ttft_ms` and `p90_tps` times the requests they describe. A p90
    /// cannot be combined exactly, so a merged value is the request-weighted
    /// mean of its parts (exact for a single report).
    pub ttft_weighted: f64,
    pub ttft_requests: u64,
    pub tps_weighted: f64,
    pub tps_requests: u64,
}

impl Totals {
    /// Input, output, cache read and cache write; reasoning is not added
    /// (`totalTokens`, `report.zig:55-57`).
    pub fn total_tokens(&self) -> u64 {
        self.input_tokens
            .saturating_add(self.output_tokens)
            .saturating_add(self.cache_read_tokens)
            .saturating_add(self.cache_write_tokens)
    }

    /// Adds `other` in (`add`, `report.zig:59-73`): counts saturate, money and
    /// weights add. A sum can stop being finite; [`Self::valid`] says so.
    pub fn add(&mut self, other: &Totals) {
        self.request_count = self.request_count.saturating_add(other.request_count);
        self.input_tokens = self.input_tokens.saturating_add(other.input_tokens);
        self.output_tokens = self.output_tokens.saturating_add(other.output_tokens);
        self.cache_read_tokens = self
            .cache_read_tokens
            .saturating_add(other.cache_read_tokens);
        self.cache_write_tokens = self
            .cache_write_tokens
            .saturating_add(other.cache_write_tokens);
        self.reasoning_tokens = self.reasoning_tokens.saturating_add(other.reasoning_tokens);
        self.total_cost += other.total_cost;
        self.cache_hit_requests += other.cache_hit_requests;
        self.failed_requests += other.failed_requests;
        self.ttft_weighted += other.ttft_weighted;
        self.ttft_requests = self.ttft_requests.saturating_add(other.ttft_requests);
        self.tps_weighted += other.tps_weighted;
        self.tps_requests = self.tps_requests.saturating_add(other.tps_requests);
    }

    /// Percent of requests that hit the cache, or `None` without requests.
    pub fn cache_hit_pct(&self) -> Option<f64> {
        percent_of(self.cache_hit_requests, self.request_count)
    }

    /// Percent of requests that failed, or `None` without requests.
    pub fn error_pct(&self) -> Option<f64> {
        percent_of(self.failed_requests, self.request_count)
    }

    pub fn p90_ttft_ms(&self) -> Option<f64> {
        (self.ttft_requests != 0).then(|| self.ttft_weighted / self.ttft_requests as f64)
    }

    pub fn p90_tps(&self) -> Option<f64> {
        (self.tps_requests != 0).then(|| self.tps_weighted / self.tps_requests as f64)
    }

    /// Whether every number is finite and not negative (`valid`,
    /// `report.zig:95-101`).
    pub fn valid(&self) -> bool {
        [
            self.total_cost,
            self.cache_hit_requests,
            self.failed_requests,
            self.ttft_weighted,
            self.tps_weighted,
        ]
        .iter()
        .all(|value| value.is_finite() && *value >= 0.0)
    }
}

/// `part` of `whole` as a percent, at most 100 (`percentOf`,
/// `report.zig:104-107`).
fn percent_of(part: f64, whole: u64) -> Option<f64> {
    (whole != 0).then(|| (part / whole as f64 * 100.0).min(100.0))
}

/// One model's row of a report (`ModelRow`, `report.zig:109-112`).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelRow {
    /// **The server's text, untrusted.** Bounded to 1-256 bytes of valid
    /// UTF-8 and refused when it holds the key, nothing more: an escaped ESC,
    /// CR/LF, C1, bidi and zero-width characters all survive decoding. A
    /// renderer spells it through `crate::mcp::terminal_safe` -- not
    /// `crate::tui::bridge::inert` alone, which keeps line breaks and bidi
    /// controls, and never through `Debug` -- and a cache encodes it.
    pub model: String,
    pub totals: Totals,
}

/// How often a budget starts over (`RefreshPeriod`, `report.zig:124`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefreshPeriod {
    Daily,
    Weekly,
    Monthly,
    None,
}

/// A spend limit and what was spent of it (`Budget`, `report.zig:126-130`).
/// Only an fx login's team has one, which xfx does not read yet; the usage
/// cache keeps the field so its schema is fx's.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Budget {
    pub limit: f64,
    pub spent: f64,
    pub period: RefreshPeriod,
}

/// The UTC day a moment falls on, as days since the Unix epoch (`utcDay`,
/// `report.zig:144-146`).
pub fn utc_day(ms: i64) -> i64 {
    ms.div_euclid(DAY_MS)
}

/// `YYYY-MM-DD` for a UTC day number; a day before the epoch is the epoch, as
/// fx clamps it (`formatDay`, `report.zig:184-194`).
pub fn format_day(day: i64) -> String {
    // Howard Hinnant's civil-from-days, the inverse of fx's `civilDay`
    // (`report.zig:133-142`).
    let z = day.max(0) + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day_of_month = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day_of_month:02}")
}

/// The `/v1/report` path for per-model usage over the UTC days `first..=last`
/// under `filter` (`reportPath`, `report.zig:196-210`).
fn path_with(filter: &str, first: i64, last: i64) -> String {
    format!(
        "/v1/report?start_date={}&end_date={}&group_by=model&{filter}",
        format_day(first),
        format_day(last)
    )
}

/// Why a report body was refused (`error.InvalidResponse`, `report.zig:220`).
/// The reason is this module's own words, never the body's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidResponse(&'static str);

impl InvalidResponse {
    pub fn reason(&self) -> &'static str {
        self.0
    }
}

impl fmt::Display for InvalidResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

/// Parses a `/v1/report?group_by=model` body (`parseModelRows`,
/// `report.zig:222-280`). Rows for the same model are summed; rows are sorted
/// by cost, highest first, then by name.
///
/// The body is read the way fx's `std.json.Value` parse reads it: a key twice
/// in any object is refused (`duplicate_field_behavior = .@"error"`), and an
/// integer past `i64` is not a number fx will use (`number_string`) -- see
/// [`Json`].
pub fn parse_model_rows(body: &[u8]) -> Result<Vec<ModelRow>, InvalidResponse> {
    if body.len() > MAX_BODY_BYTES {
        return Err(InvalidResponse("over 1 MiB"));
    }
    let root: Json =
        serde_json::from_slice(body).map_err(|_| InvalidResponse("not JSON fx would read"))?;
    let Json::Object(root) = root else {
        return Err(InvalidResponse("not an object"));
    };
    let Some(Json::Array(results)) = root.get("results") else {
        return Err(InvalidResponse("no results array"));
    };

    let mut rows: Vec<ModelRow> = Vec::new();
    for item in results {
        let Json::Object(item) = item else {
            return Err(InvalidResponse("a row that is not an object"));
        };
        let Some(Json::String(model)) = item.get("model") else {
            return Err(InvalidResponse("a row without a model name"));
        };
        if model.is_empty() || model.len() > MAX_MODEL_NAME_BYTES {
            return Err(InvalidResponse("a model name of 0 or over 256 bytes"));
        }
        let requests = count(item.get("request_count"))?;
        let requests_f = requests as f64;
        let hit_pct = optional_number(item.get("cache_hit_pct"))?;
        let failure_pct = optional_number(item.get("http_failure_pct"))?.unwrap_or(0.0)
            + optional_number(item.get("malformed_response_pct"))?.unwrap_or(0.0);
        let ttft = optional_number(item.get("p90_ttft_ms"))?;
        let tps = optional_number(item.get("p90_tps"))?;
        let total_cost = item
            .get("total_cost")
            .ok_or(InvalidResponse("a row without total_cost"))?;
        let totals = Totals {
            request_count: requests,
            input_tokens: count(item.get("input_tokens"))?,
            output_tokens: count(item.get("output_tokens"))?,
            cache_read_tokens: count(item.get("cached_input_tokens"))?,
            cache_write_tokens: count(item.get("cache_creation_input_tokens"))?,
            reasoning_tokens: count(item.get("reasoning_tokens"))?,
            total_cost: money(total_cost)?,
            cache_hit_requests: hit_pct.unwrap_or(0.0) / 100.0 * requests_f,
            failed_requests: failure_pct / 100.0 * requests_f,
            ttft_weighted: ttft.map_or(0.0, |value| value * requests_f),
            ttft_requests: if ttft.is_some() { requests } else { 0 },
            tps_weighted: tps.map_or(0.0, |value| value * requests_f),
            tps_requests: if tps.is_some() { requests } else { 0 },
        };
        // Each number was finite, but a product or a sum can still overflow.
        if !totals.valid() {
            return Err(InvalidResponse("a row whose numbers overflow"));
        }
        if let Some(row) = rows.iter_mut().find(|row| row.model == *model) {
            row.totals.add(&totals);
            if !row.totals.valid() {
                return Err(InvalidResponse("a model whose sum overflows"));
            }
            continue;
        }
        if rows.len() == MAX_MODELS {
            return Err(InvalidResponse("more than 256 models"));
        }
        rows.push(ModelRow {
            model: model.clone(),
            totals,
        });
    }
    // Every cost is finite here, so the comparison is total (`costDescending`,
    // `report.zig:315-318`).
    rows.sort_by(|a, b| {
        b.totals
            .total_cost
            .partial_cmp(&a.totals.total_cost)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.model.cmp(&b.model))
    });
    Ok(rows)
}

/// A token or request count: absent or `null` is 0 (`count`,
/// `report.zig:320-328`).
fn count(value: Option<&Json>) -> Result<u64, InvalidResponse> {
    const BAD: InvalidResponse =
        InvalidResponse("a count that is negative, too large or not a number");
    match value {
        None | Some(Json::Null) => Ok(0),
        Some(Json::Integer(integer)) => u64::try_from(*integer).map_err(|_| BAD),
        // Truncated toward zero, as `@intFromFloat`.
        Some(Json::Float(float)) if float.is_finite() && *float >= 0.0 && *float <= 1e18 => {
            Ok(*float as u64)
        }
        Some(_) => Err(BAD),
    }
}

/// A non-negative finite number, or `None` when absent or `null`
/// (`optionalNumber`, `report.zig:330-335`).
fn optional_number(value: Option<&Json>) -> Result<Option<f64>, InvalidResponse> {
    match value {
        None | Some(Json::Null) => Ok(None),
        Some(value) => money(value).map(Some),
    }
}

/// A non-negative finite number (`money`, `report.zig:337-345`).
fn money(value: &Json) -> Result<f64, InvalidResponse> {
    let number = match value {
        Json::Integer(integer) => *integer as f64,
        Json::Float(float) => *float,
        _ => return Err(InvalidResponse("a number that is missing or not a number")),
    };
    if !number.is_finite() || number < 0.0 {
        return Err(InvalidResponse("a number that is negative or not finite"));
    }
    Ok(number)
}

/// A JSON value as fx's parser classifies it (`std.json.Value`,
/// `parseFromNumberSlice`, Zig 0.16.0 `lib/std/json/dynamic.zig:30-47`; fx
/// pins 0.17.0).
///
/// Two of its rules are not serde_json's own, so this type carries them:
///
/// - **A key twice in one object is an error**, anywhere in the body: the
///   `Value` parse runs under the default `duplicate_field_behavior =
///   .@"error"` (`dynamic.zig:146-157`, `static.zig:20-26`), where a
///   `serde_json::Map` keeps the last.
/// - **An integer past `i64` is text, not a number** (`number_string`,
///   `dynamic.zig:39-45`), so a count or a cost written as one is refused.
///
/// What serde_json does not say cannot be copied: an integer literal past
/// `u64` reaches a visitor as a float, so it is read as fx reads a float of
/// that size, and a literal past `f64` or nesting past serde_json's 128
/// levels refuses the whole body, where fx would refuse only a row field
/// that holds one (`docs/parity.md`).
#[derive(Debug, Clone, PartialEq)]
enum Json {
    Null,
    Integer(i64),
    Float(f64),
    /// A number fx keeps as its digits: no field reads one.
    NumberText,
    /// A boolean: no field reads one either.
    Bool,
    String(String),
    Array(Vec<Json>),
    Object(BTreeMap<String, Json>),
}

impl<'de> Deserialize<'de> for Json {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(JsonVisitor)
    }
}

struct JsonVisitor;

impl<'de> Visitor<'de> for JsonVisitor {
    type Value = Json;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON value")
    }

    fn visit_unit<E: de::Error>(self) -> Result<Json, E> {
        Ok(Json::Null)
    }

    fn visit_bool<E: de::Error>(self, _: bool) -> Result<Json, E> {
        Ok(Json::Bool)
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Json, E> {
        Ok(Json::Integer(value))
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Json, E> {
        Ok(i64::try_from(value).map_or(Json::NumberText, Json::Integer))
    }

    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Json, E> {
        Ok(if value.is_finite() {
            Json::Float(value)
        } else {
            Json::NumberText
        })
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Json, E> {
        Ok(Json::String(value.to_owned()))
    }

    fn visit_string<E: de::Error>(self, value: String) -> Result<Json, E> {
        Ok(Json::String(value))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Json, A::Error> {
        let mut items = Vec::new();
        while let Some(item) = seq.next_element()? {
            items.push(item);
        }
        Ok(Json::Array(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Json, A::Error> {
        let mut object = BTreeMap::new();
        while let Some(key) = map.next_key::<String>()? {
            let value = map.next_value()?;
            if object.insert(key, value).is_some() {
                return Err(de::Error::custom("a key twice in one object"));
            }
        }
        Ok(Json::Object(object))
    }
}

/// Why usage cannot be shown at all (`Unavailable`, `usage.zig:82-92`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Unavailable {
    /// Not signed in to AI Gateway.
    NoGatewaySignIn,
    /// Signed in to a provider whose usage cannot be shown yet.
    UnsupportedProvider,
    /// A settings layer named a provider xfx could not read. xfx's own state,
    /// with no fx twin: the turn refuses it too, and is told it the same way
    /// (`crate::output::rejected_provider_help`).
    ProviderRejected,
}

/// Why a fetch failed (`Failure`, `usage.zig:94-105`, less `cache`: fx's
/// dashboard runtime reports a store's `error.InvalidCache` as it
/// (`usage.zig:705-708`), so it arrives with that runtime (U3), from
/// `crate::usage::cache::StoreError::Invalid`). Cached numbers stay on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Failure {
    /// No answer: nothing reachable, too slow, or too large.
    Network,
    /// 401/403: the provider refused this sign-in.
    Refused,
    /// 402/404: the team cannot use usage reports.
    NotAvailable,
    RateLimited,
    Server,
    /// Any other status, a redirect included, or a body that is not a report.
    BadResponse,
}

/// The failure a non-`200` status maps to (`statusFailure`,
/// `usage.zig:889-896`).
pub fn status_failure(status: u16) -> Failure {
    match status {
        401 | 403 => Failure::Refused,
        402 | 404 => Failure::NotAvailable,
        429 => Failure::RateLimited,
        500.. => Failure::Server,
        _ => Failure::BadResponse,
    }
}

/// A failed fetch: its kind, and one line for a diagnostic.
///
/// The detail is built from a status code and this module's own phrases only,
/// never from the response, so it holds no secret a server could echo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failed {
    pub failure: Failure,
    pub detail: String,
}

impl Failed {
    fn network(detail: &str) -> Self {
        Self {
            failure: Failure::Network,
            detail: detail.to_string(),
        }
    }
}

/// What a fetch asks with: the configured Gateway origin and an
/// `AI_GATEWAY_API_KEY`. The key is never printed: `Debug` names its source.
#[derive(Debug, Clone)]
pub struct Question {
    origin: String,
    credential: crate::config::Credential,
}

impl Question {
    /// Who the numbers belong to, for the heading.
    pub fn label(&self) -> &'static str {
        API_KEY_LABEL
    }

    /// Whose usage cache entry the answers belong to: fx's `api_key` scope,
    /// this key as the subject, no team (`usage_host.zig:105-111`,
    /// `usage.zig:422-427`).
    ///
    /// The second reader of the key after the transport, and only to hash it:
    /// the cache module is handed the digest and never the key, so no code
    /// that writes the cache can write the key.
    pub fn identity(&self) -> super::cache::Identity {
        super::cache::Identity::of(super::cache::API_KEY_SCOPE, self.credential.secret(), "")
    }

    /// The report URL for `range`.
    fn url(&self, range: DayRange) -> String {
        format!(
            "{}{}",
            self.origin,
            path_with(API_KEY_FILTER, range.first, range.last)
        )
    }
}

/// What `/usage` would ask with `config`, sending to `endpoint` -- the
/// endpoint a turn is sent to -- or why it asks nobody.
///
/// Only an `AI_GATEWAY_API_KEY` is asked (`usage_host.zig:105-111`). A
/// `VERCEL_OIDC_TOKEN` resolves first when both are set (`config.rs`
/// `resolve_credential`), and is unsupported, as fx has it
/// (`usage_host.zig:78,128`); so is llmux. An unreadable provider setting is
/// refused before anything else: it left `provider` at the default, the
/// Gateway, and that default must not be what sends the key.
pub fn question(config: &RuntimeConfig, endpoint: &Endpoint) -> Result<Question, Unavailable> {
    if config.provider_rejected.is_some() {
        return Err(Unavailable::ProviderRejected);
    }
    match config.provider {
        ProviderId::Llmux => Err(Unavailable::UnsupportedProvider),
        ProviderId::Gateway => {
            let credential = config
                .credential
                .as_ref()
                .ok_or(Unavailable::NoGatewaySignIn)?;
            match credential.source() {
                CredentialSource::AiGatewayApiKey => Ok(Question {
                    origin: endpoint.origin().to_string(),
                    credential: credential.clone(),
                }),
                CredentialSource::VercelOidcToken | CredentialSource::LlmuxLoopback => {
                    Err(Unavailable::UnsupportedProvider)
                }
            }
        }
    }
}

/// The UTC days `first..=last`, as [`utc_day`] numbers them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DayRange {
    pub first: i64,
    pub last: i64,
}

/// How a fetch ended.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// One report per range asked, in the order asked.
    Fetched(Vec<Vec<ModelRow>>),
    /// The first range that failed, and why; nothing after it was asked.
    Failed(Failed),
    /// [`Fetch::cancel`] was called before the fetch ended.
    Cancelled,
}

/// What [`Fetch::start`] hands its spawner: the work of one fetch.
pub type Job = Box<dyn FnOnce() + Send + 'static>;

/// A fetch running on a worker thread of its own. Dropping it cancels it.
#[derive(Debug)]
pub struct Fetch {
    outcome: Option<Receiver<Outcome>>,
    cancel: CancellationToken,
}

impl Fetch {
    /// Starts reading one report per range in `ranges`, in order, on a thread
    /// `spawn` starts, and returns without waiting. The first failure ends the
    /// fetch.
    ///
    /// `spawn` runs the [`Job`] on a new thread. The full-screen shell's is a
    /// closure around `crate::tui::signals::spawn_unsignalled`, the way every
    /// thread of a session but the UI's is started.
    pub fn start(
        question: Question,
        ranges: Vec<DayRange>,
        spawn: impl FnOnce(Job) -> io::Result<()>,
    ) -> io::Result<Self> {
        let (done, outcome) = mpsc::channel();
        let cancel = CancellationToken::new();
        let cancelled = cancel.clone();
        spawn(Box::new(move || {
            // Nobody left to tell is a fetch nobody waits for.
            let _ = done.send(run(&question, &ranges, &cancelled));
        }))?;
        Ok(Self {
            outcome: Some(outcome),
            cancel,
        })
    }

    /// How the fetch ended, once, or `None` while it runs (and after).
    pub fn try_outcome(&mut self) -> Option<Outcome> {
        let outcome = match self.outcome.as_ref()?.try_recv() {
            Ok(outcome) => outcome,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => Outcome::Failed(Failed::network(WORKER_GONE)),
        };
        self.outcome = None;
        Some(outcome)
    }

    /// Stops the fetch: the request in flight is dropped and the outcome is
    /// [`Outcome::Cancelled`], unless it had already ended.
    pub fn cancel(&self) {
        self.cancel.cancel();
    }
}

/// A fetch nobody holds is a fetch nobody waits for: it stops, and its worker
/// lets go of the key, rather than asking for up to 15 s per range left.
impl Drop for Fetch {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

/// The worker's body: a runtime of its own for the length of the fetch.
fn run(question: &Question, ranges: &[DayRange], cancel: &CancellationToken) -> Outcome {
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return Outcome::Failed(Failed::network(NO_CLIENT));
    };
    let outcome = runtime.block_on(fetch_all(question, ranges, cancel));
    // A resolver still in `getaddrinfo` is not waited for.
    runtime.shutdown_background();
    outcome
}

async fn fetch_all(
    question: &Question,
    ranges: &[DayRange],
    cancel: &CancellationToken,
) -> Outcome {
    let Ok(client) = crate::gateway::bearer_rules(
        reqwest::Client::builder().user_agent(USER_AGENT),
        &question.origin,
    )
    .build() else {
        return Outcome::Failed(Failed::network(NO_CLIENT));
    };
    let mut reports = Vec::with_capacity(ranges.len());
    for range in ranges {
        let rows = tokio::select! {
            biased;
            () = cancel.cancelled() => return Outcome::Cancelled,
            rows = fetch_rows(&client, question, *range) => rows,
        };
        match rows {
            Ok(rows) => reports.push(rows),
            Err(failed) => return Outcome::Failed(failed),
        }
    }
    Outcome::Fetched(reports)
}

/// One report, within [`DEADLINE`] from connecting to its last byte
/// (`fetchUsageGet`, `client.zig:284-309`).
async fn fetch_rows(
    client: &reqwest::Client,
    question: &Question,
    range: DayRange,
) -> Result<Vec<ModelRow>, Failed> {
    let secret = question.credential.secret();
    let url = question.url(range);
    let (status, body) = tokio::time::timeout(DEADLINE, get(client, &url, secret))
        .await
        .map_err(|_| Failed::network(&no_answer()))??;
    if status != 200 {
        return Err(Failed {
            failure: status_failure(status),
            detail: format!("HTTP {status}"),
        });
    }
    let rows = parse_model_rows(&body).map_err(|invalid| Failed {
        failure: Failure::BadResponse,
        detail: format!("unexpected report: {invalid}"),
    })?;
    if !secret.is_empty() && rows.iter().any(|row| row.model.contains(secret)) {
        return Err(Failed {
            failure: Failure::BadResponse,
            detail: KEY_IN_REPORT.to_string(),
        });
    }
    Ok(rows)
}

/// What a failure says when nothing answered within [`DEADLINE`].
fn no_answer() -> String {
    format!("AI Gateway did not answer within {} s", DEADLINE.as_secs())
}

/// One authenticated `GET`: its status, and its body read in full before the
/// status is judged, as fx reads it (`client.zig:424-434`), so an answer
/// over the cap is a network failure whatever its status.
async fn get(client: &reqwest::Client, url: &str, secret: &str) -> Result<(u16, Vec<u8>), Failed> {
    let mut authorization = reqwest::header::HeaderValue::from_str(&format!("Bearer {secret}"))
        .map_err(|_| Failed::network(UNSENDABLE_KEY))?;
    // Keeps the credential out of reqwest's own diagnostics.
    authorization.set_sensitive(true);
    let response = client
        .get(url)
        .header(reqwest::header::AUTHORIZATION, authorization)
        .send()
        .await
        .map_err(|_| Failed::network(UNREACHABLE))?;
    let status = response.status().as_u16();
    Ok((status, read_capped(response, MAX_BODY_BYTES).await?))
}

/// The body, or a refusal once it is past `limit`: a declared length past it
/// is refused before a byte is read, and a body that grows past it is
/// dropped at the chunk that crosses.
///
/// Not `crate::gateway::read_bounded`, which keeps the first `limit` bytes
/// and stops: a report cut short is not a report, and fx refuses it
/// (`error.StreamTooLong => error.UsageResponseTooLarge`, `client.zig:430-431`).
async fn read_capped(mut response: reqwest::Response, limit: usize) -> Result<Vec<u8>, Failed> {
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err(Failed::network(TOO_LARGE));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| Failed::network(CUT_SHORT))?
    {
        if chunk.len() > limit - body.len() {
            return Err(Failed::network(TOO_LARGE));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_net::{json_answer, redirect, Listener};
    use std::io::Write;
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    const KEY: &str = "xfx-usage-report-test-key-must-not-appear";

    /// 2026-10-11, the day fx's own vectors are written for
    /// (`report.zig:354`).
    const OCT_11: i64 = 1_791_676_800_000 / DAY_MS;

    // -- fx's vectors -------------------------------------------------------

    #[test]
    fn the_report_path_is_fxs_for_an_api_key() {
        // `report.zig:352-362@481104fd`, the `api_key` vector. The member,
        // budget and Codex vectors (`:355-358,363-367`) are the member scope's
        // and the plans', which are not asked here.
        let day = utc_day(1_791_676_800_000);
        assert_eq!(day, OCT_11);
        assert_eq!(
            path_with(API_KEY_FILTER, day, day),
            "/v1/report?start_date=2026-10-11&end_date=2026-10-11&group_by=model&api_key_id=self"
        );
        assert_eq!(
            path_with(API_KEY_FILTER, day - 6, day - 1),
            "/v1/report?start_date=2026-10-05&end_date=2026-10-10&group_by=model&api_key_id=self"
        );
    }

    #[test]
    fn model_rows_parse_merge_duplicates_and_sort_by_cost() {
        // `report.zig:370-392@481104fd`.
        let body = br#"{"results":[
 {"model":"openai/gpt-5.6-luna","request_count":1484,"input_tokens":10,"output_tokens":20,
  "cached_input_tokens":3,"cache_creation_input_tokens":4,"reasoning_tokens":5,"total_cost":1.65,
  "market_cost":1.65,"cache_hit_pct":12.5,"p90_ttft_ms":null},
 {"model":"anthropic/claude-opus-5.5","request_count":2168,"total_cost":397.44},
 {"model":"openai/gpt-5.6-luna","request_count":1,"total_cost":0.35}
]}"#;
        let rows = parse_model_rows(body).expect("a report");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].model, "anthropic/claude-opus-5.5");
        let luna = rows[1].totals;
        assert_eq!(luna.request_count, 1485);
        assert!((luna.total_cost - 2.0).abs() < 1e-9);
        assert_eq!(luna.total_tokens(), 37);
        assert_eq!(luna.reasoning_tokens, 5);
        // 12.5% of 1484 requests hit the cache; the 1-request row reported none.
        assert!((luna.cache_hit_pct().unwrap() - 12.5 * 1484.0 / 1485.0).abs() < 1e-9);
        assert_eq!(luna.p90_ttft_ms(), None);
    }

    #[test]
    fn perf_percentages_merge_by_request_count_and_p90s_by_request_weight() {
        // `report.zig:394-412@481104fd`.
        let body = br#"{"results":[
 {"model":"m","request_count":300,"total_cost":3,"cache_hit_pct":90,"http_failure_pct":1,
  "malformed_response_pct":1,"p90_ttft_ms":2000,"p90_tps":50,"rate_limit_pct":7},
 {"model":"m","request_count":100,"total_cost":1,"cache_hit_pct":50,"http_failure_pct":0,
  "malformed_response_pct":null,"p90_ttft_ms":1000,"p90_tps":70}
]}"#;
        let rows = parse_model_rows(body).expect("a report");
        let t = rows[0].totals;
        assert!((t.cache_hit_pct().unwrap() - 80.0).abs() < 1e-9);
        assert!((t.error_pct().unwrap() - 1.5).abs() < 1e-9);
        assert!((t.p90_ttft_ms().unwrap() - 1750.0).abs() < 1e-9);
        assert!((t.p90_tps().unwrap() - 55.0).abs() < 1e-9);
        assert_eq!(Totals::default().cache_hit_pct(), None);
        assert!(parse_model_rows(
            br#"{"results":[{"model":"m","total_cost":1,"cache_hit_pct":-1}]}"#
        )
        .is_err());
    }

    #[test]
    fn malformed_reports_are_rejected_instead_of_shown() {
        // `report.zig:427-445@481104fd`, every vector.
        let bad: [&[u8]; 10] = [
            b"not json",
            b"{}",
            br#"{"results":{}}"#,
            br#"{"results":[{"request_count":1,"total_cost":1}]}"#,
            br#"{"results":[{"model":"m","request_count":1}]}"#,
            br#"{"results":[{"model":"m","total_cost":-1}]}"#,
            br#"{"results":[{"model":"m","total_cost":1,"input_tokens":-3}]}"#,
            br#"{"results":[{"model":"","total_cost":1}]}"#,
            // Finite numbers whose product or sum is not: the cache would
            // refuse them.
            br#"{"results":[{"model":"m","request_count":100000000000000000,"total_cost":1,"p90_ttft_ms":1e300}]}"#,
            br#"{"results":[{"model":"m","total_cost":1.7e308},{"model":"m","total_cost":1.7e308}]}"#,
        ];
        for body in bad {
            assert!(
                parse_model_rows(body).is_err(),
                "{}",
                String::from_utf8_lossy(body)
            );
        }
        assert_eq!(parse_model_rows(br#"{"results":[]}"#), Ok(Vec::new()));
    }

    #[test]
    fn a_status_maps_to_the_failure_fx_names() {
        // `statusFailure`, `usage.zig:889-896@481104fd`.
        for (status, failure) in [
            (401, Failure::Refused),
            (403, Failure::Refused),
            (402, Failure::NotAvailable),
            (404, Failure::NotAvailable),
            (429, Failure::RateLimited),
            (500, Failure::Server),
            (503, Failure::Server),
            (599, Failure::Server),
            (301, Failure::BadResponse),
            (307, Failure::BadResponse),
            (400, Failure::BadResponse),
            (418, Failure::BadResponse),
            (201, Failure::BadResponse),
        ] {
            assert_eq!(status_failure(status), failure, "{status}");
        }
    }

    // -- the parser beyond fx's vectors ------------------------------------

    fn row(fields: &str) -> Vec<u8> {
        format!(r#"{{"results":[{{"model":"m",{fields}}}]}}"#).into_bytes()
    }

    #[test]
    fn numbers_are_read_as_fxs_parser_reads_them() {
        // A count may be a float, truncated toward zero, up to 1e18
        // (`report.zig:325`).
        let rows = parse_model_rows(&row(
            r#""total_cost":1,"request_count":7.9,"input_tokens":1e18"#,
        ))
        .expect("a report");
        assert_eq!(rows[0].totals.request_count, 7);
        assert_eq!(rows[0].totals.input_tokens, 1_000_000_000_000_000_000);
        // An integer is any i64 (`dynamic.zig:39`).
        let rows = parse_model_rows(&row(r#""total_cost":1,"input_tokens":9223372036854775807"#))
            .expect("a report");
        assert_eq!(rows[0].totals.input_tokens, i64::MAX as u64);
        // `-0` is a float, and not negative (`Scanner.zig:1760`).
        let rows =
            parse_model_rows(&row(r#""total_cost":-0,"request_count":-0"#)).expect("a report");
        assert_eq!(rows[0].totals.request_count, 0);
        for refused in [
            // Past 1e18 as a float, past i64 as an integer: `number_string`.
            r#""total_cost":1,"input_tokens":1.5e18"#,
            r#""total_cost":1,"input_tokens":9223372036854775808"#,
            r#""total_cost":9223372036854775808"#,
            r#""total_cost":1,"cache_hit_pct":18446744073709551615"#,
            r#""total_cost":1,"request_count":-1"#,
            // Past f64: not a number at all.
            r#""total_cost":1e400"#,
            r#""total_cost":1,"p90_tps":-1e400"#,
            // Not numbers.
            r#""total_cost":"1""#,
            r#""total_cost":null"#,
            r#""total_cost":true"#,
            r#""total_cost":1,"request_count":"1""#,
            r#""total_cost":1,"p90_ttft_ms":[]"#,
            // A model name that is not a string.
            r#""total_cost":1,"model":7"#,
        ] {
            assert!(parse_model_rows(&row(refused)).is_err(), "{refused}");
        }
    }

    #[test]
    fn a_key_twice_in_any_object_refuses_the_report() {
        // fx's `Value` parse refuses a duplicate field
        // (`dynamic.zig:146-157`): a later value cannot hide a refused one.
        for body in [
            br#"{"results":[],"results":[]}"#.as_slice(),
            br#"{"results":[{"model":"m","total_cost":-1,"total_cost":1}]}"#,
            br#"{"results":[{"model":"m","total_cost":1,"extra":{"a":1,"a":2}}]}"#,
        ] {
            assert!(
                parse_model_rows(body).is_err(),
                "{}",
                String::from_utf8_lossy(body)
            );
        }
    }

    #[test]
    fn names_and_models_are_bounded_as_fx_bounds_them() {
        // 256 bytes, decoded: `é` is two.
        let name = "\\u00e9".repeat(128);
        let body = format!(r#"{{"results":[{{"model":"{name}","total_cost":1}}]}}"#);
        assert_eq!(
            parse_model_rows(body.as_bytes()).expect("a report")[0]
                .model
                .len(),
            256
        );
        let body = format!(r#"{{"results":[{{"model":"{name}x","total_cost":1}}]}}"#);
        assert!(parse_model_rows(body.as_bytes()).is_err());
        // Invalid UTF-8 inside a string (`Scanner.zig:665`).
        assert!(
            parse_model_rows(b"{\"results\":[{\"model\":\"\xff\",\"total_cost\":1}]}").is_err()
        );

        let models = |n: usize| {
            let rows: Vec<String> = (0..n)
                .map(|i| format!(r#"{{"model":"m{i}","total_cost":{i}}}"#))
                .collect();
            format!(r#"{{"results":[{}]}}"#, rows.join(","))
        };
        let rows = parse_model_rows(models(MAX_MODELS).as_bytes()).expect("256 models");
        assert_eq!(rows.len(), MAX_MODELS);
        assert_eq!(rows[0].model, "m255");
        // A repeat of a listed model is not a 257th.
        let mut repeat = models(MAX_MODELS);
        repeat.truncate(repeat.len() - 2);
        repeat.push_str(r#",{"model":"m0","total_cost":1}]}"#);
        assert_eq!(
            parse_model_rows(repeat.as_bytes()).expect("a repeat").len(),
            MAX_MODELS
        );
        assert!(parse_model_rows(models(MAX_MODELS + 1).as_bytes()).is_err());
    }

    #[test]
    fn equal_costs_sort_by_name_and_counts_saturate() {
        let rows = parse_model_rows(
            br#"{"results":[{"model":"b","total_cost":1},{"model":"a","total_cost":1},{"model":"c","total_cost":2}]}"#,
        )
        .expect("a report");
        let names: Vec<&str> = rows.iter().map(|row| row.model.as_str()).collect();
        assert_eq!(names, ["c", "a", "b"]);
        let mut big = Totals {
            input_tokens: u64::MAX,
            output_tokens: 1,
            ..Totals::default()
        };
        assert_eq!(big.total_tokens(), u64::MAX);
        big.add(&Totals {
            input_tokens: 5,
            ..Totals::default()
        });
        assert_eq!(big.input_tokens, u64::MAX);
    }

    #[test]
    fn a_body_over_the_cap_is_not_parsed() {
        let mut body = br#"{"results":[]}"#.to_vec();
        body.resize(MAX_BODY_BYTES, b' ');
        assert_eq!(parse_model_rows(&body), Ok(Vec::new()));
        body.push(b' ');
        assert_eq!(parse_model_rows(&body), Err(InvalidResponse("over 1 MiB")));
    }

    #[test]
    fn days_are_written_as_fx_writes_them() {
        assert_eq!(format_day(OCT_11), "2026-10-11");
        assert_eq!(format_day(0), "1970-01-01");
        assert_eq!(format_day(-5), "1970-01-01");
        assert_eq!(format_day(utc_day(1_835_395_200_000)), "2028-02-29");
        assert_eq!(format_day(utc_day(1_835_481_600_000)), "2028-03-01");
        assert_eq!(format_day(10_957), "2000-01-01");
        assert_eq!(utc_day(-1), -1);
        assert_eq!(utc_day(DAY_MS - 1), 0);
    }

    // -- who is asked -------------------------------------------------------

    fn config_with(vars: &[(&str, &str)]) -> RuntimeConfig {
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        RuntimeConfig::load_with(
            &crate::config::Environment::new(
                Some(home.path().to_path_buf()),
                vars.iter()
                    .map(|(key, value)| (key.to_string(), value.to_string()))
                    .collect::<BTreeMap<_, _>>(),
            ),
            workspace.path(),
        )
        .expect("a configuration")
    }

    fn default_endpoint() -> Endpoint {
        Endpoint::resolve(None).expect("the default endpoint")
    }

    #[test]
    fn only_an_ai_gateway_api_key_is_asked_for_usage() {
        // `usage_host.zig:72-80,105-111,128@481104fd`.
        let asked = question(
            &config_with(&[("AI_GATEWAY_API_KEY", KEY)]),
            &default_endpoint(),
        )
        .expect("a key is asked");
        assert_eq!(asked.label(), "API key (AI_GATEWAY_API_KEY)");
        assert_eq!(
            asked.url(DayRange { first: OCT_11, last: OCT_11 }),
            "https://ai-gateway.vercel.sh/v1/report?start_date=2026-10-11&end_date=2026-10-11&group_by=model&api_key_id=self"
        );
        let shown = format!("{asked:?}");
        assert!(!shown.contains(KEY), "{shown}");

        for (vars, why) in [
            (
                vec![("VERCEL_OIDC_TOKEN", KEY)],
                Unavailable::UnsupportedProvider,
            ),
            // The token resolves first, so the key is not used either.
            (
                vec![("VERCEL_OIDC_TOKEN", KEY), ("AI_GATEWAY_API_KEY", KEY)],
                Unavailable::UnsupportedProvider,
            ),
            (vec![], Unavailable::NoGatewaySignIn),
            (
                vec![("AI_GATEWAY_API_KEY", "  ")],
                Unavailable::NoGatewaySignIn,
            ),
        ] {
            let answer = question(&config_with(&vars), &default_endpoint());
            assert_eq!(answer.map(|_| ()), Err(why), "{vars:?}");
        }
        let mut llmux = config_with(&[("AI_GATEWAY_API_KEY", KEY)]);
        llmux.provider = ProviderId::Llmux;
        assert_eq!(
            question(&llmux, &default_endpoint()).map(|_| ()),
            Err(Unavailable::UnsupportedProvider)
        );
    }

    // -- the wire -----------------------------------------------------------

    const REPORT: &str = r#"{"results":[{"model":"anthropic/claude-opus-5.5","request_count":10,"input_tokens":5,"total_cost":2.5}]}"#;

    /// A question with [`KEY`] for the Gateway at `origin`.
    fn question_at(origin: &str) -> Question {
        let endpoint =
            Endpoint::resolve(Some(&format!("{origin}/v4/ai/language-model"))).expect("loopback");
        question(&config_with(&[("AI_GATEWAY_API_KEY", KEY)]), &endpoint).expect("a key")
    }

    fn on_a_thread(job: Job) -> io::Result<()> {
        std::thread::Builder::new()
            .name("xfx-usage-test".to_string())
            .spawn(job)
            .map(drop)
    }

    fn today() -> DayRange {
        DayRange {
            first: OCT_11,
            last: OCT_11,
        }
    }

    /// The fetch's outcome, or `None` when none came within `limit`.
    fn wait(fetch: &mut Fetch, limit: Duration) -> Option<Outcome> {
        let started = Instant::now();
        loop {
            if let Some(outcome) = fetch.try_outcome() {
                return Some(outcome);
            }
            if started.elapsed() >= limit {
                return None;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// Fetches `ranges` from `origin`, waiting a bounded time for the answer.
    fn fetch(origin: &str, ranges: Vec<DayRange>) -> Outcome {
        let mut fetch = Fetch::start(question_at(origin), ranges, on_a_thread).expect("a worker");
        wait(&mut fetch, Duration::from_secs(25)).expect("an outcome within 25 s")
    }

    /// Where [`keeping`] puts the worker's handle.
    type Slot = Arc<Mutex<Option<std::thread::JoinHandle<()>>>>;

    /// A spawner that keeps the worker's handle in `slot`, so a test can see
    /// the thread end.
    fn keeping(slot: &Slot) -> impl FnOnce(Job) -> io::Result<()> {
        let slot = Arc::clone(slot);
        move |job| {
            let handle = std::thread::Builder::new()
                .name("xfx-usage-test".to_string())
                .spawn(job)?;
            *slot.lock().expect("the slot") = Some(handle);
            Ok(())
        }
    }

    /// Whether the worker in `slot` ended, and was joined, within `limit`.
    fn exited_within(slot: &Slot, limit: Duration) -> bool {
        let handle = slot.lock().expect("the slot").take().expect("a worker");
        let started = Instant::now();
        while !handle.is_finished() {
            if started.elapsed() >= limit {
                return false;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        handle.join().is_ok()
    }

    /// A loopback server for one connection that sends a `200` head promising
    /// 100000 bytes, then one byte a second -- progress that never finishes --
    /// until a write fails (the client hung up), `stop` is set, or 30 s pass.
    /// Returns how many bytes it dripped and whether the client hung up.
    fn dripping(stop: &Arc<AtomicBool>) -> (String, std::thread::JoinHandle<(usize, bool)>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
        let origin = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let stop = Arc::clone(stop);
        let thread = std::thread::spawn(move || {
            let Some(mut connection) = accept_within(&listener, Duration::from_secs(20)) else {
                return (0, false);
            };
            let _ = connection.set_read_timeout(Some(Duration::from_secs(5)));
            let _ = connection.set_write_timeout(Some(Duration::from_secs(5)));
            crate::test_net::read_request(&mut connection);
            if connection
                .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 100000\r\n\r\n")
                .is_err()
            {
                return (0, true);
            }
            let started = Instant::now();
            let mut dripped = 0;
            let mut next = Instant::now();
            while !stop.load(Ordering::SeqCst) && started.elapsed() < Duration::from_secs(30) {
                if Instant::now() >= next {
                    if connection
                        .write_all(b" ")
                        .and_then(|()| connection.flush())
                        .is_err()
                    {
                        return (dripped, true);
                    }
                    dripped += 1;
                    next += Duration::from_secs(1);
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            (dripped, false)
        });
        (origin, thread)
    }

    fn failed(outcome: Outcome) -> Failed {
        match outcome {
            Outcome::Failed(failed) => failed,
            other => panic!("a failure, not {other:?}"),
        }
    }

    #[test]
    fn the_key_goes_to_the_configured_origin_with_fxs_query() {
        let gateway = Listener::answering(json_answer(REPORT));
        let outcome = fetch(
            &gateway.origin(),
            vec![
                today(),
                DayRange {
                    first: OCT_11 - 6,
                    last: OCT_11 - 1,
                },
            ],
        );
        let requests = gateway.finish();
        let Outcome::Fetched(reports) = outcome else {
            panic!("a fetch: {outcome:?}");
        };
        assert_eq!(reports.len(), 2);
        assert!((reports[0][0].totals.total_cost - 2.5).abs() < 1e-9);
        assert_eq!(requests.len(), 2, "{requests:?}");
        for (request, path) in requests.iter().zip([
            "/v1/report?start_date=2026-10-11&end_date=2026-10-11&group_by=model&api_key_id=self",
            "/v1/report?start_date=2026-10-05&end_date=2026-10-10&group_by=model&api_key_id=self",
        ]) {
            assert!(
                request.starts_with(&format!("GET {path} HTTP/1.1\r\n")),
                "{request}"
            );
            let head = request.to_ascii_lowercase();
            assert!(
                head.contains(&format!("\r\nauthorization: bearer {KEY}\r\n")),
                "{request}"
            );
            // `api_key` sends no team and no provider account
            // (`client.zig:370-373@481104fd`).
            for header in ["x-vercel-ai-gateway-team", "chatgpt-account-id", "x-userid"] {
                assert!(!head.contains(&format!("\r\n{header}:")), "{request}");
            }
        }
        // A cleartext Gateway anywhere but loopback is refused before a
        // question can be built for it.
        for url in [
            "http://example.com/v4/ai",
            "http://localhost.example:80/v4/ai",
            "ftp://127.0.0.1:1/v4",
        ] {
            assert!(Endpoint::resolve(Some(url)).is_err(), "{url}");
        }
    }

    /// A configuration loaded from `profile` as `~/.xfx/settings.json`.
    fn config_with_profile(profile: &str, vars: &[(&str, &str)]) -> RuntimeConfig {
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        std::fs::create_dir_all(home.path().join(".xfx")).expect("a profile dir");
        std::fs::write(home.path().join(".xfx").join("settings.json"), profile).expect("a profile");
        RuntimeConfig::load_with(
            &crate::config::Environment::new(
                Some(home.path().to_path_buf()),
                vars.iter()
                    .map(|(key, value)| (key.to_string(), value.to_string()))
                    .collect::<BTreeMap<_, _>>(),
            ),
            workspace.path(),
        )
        .expect("a configuration")
    }

    #[test]
    fn an_unreadable_provider_setting_sends_the_key_nowhere() {
        // `config.rs`: an unreadable `provider` is kept as `provider_rejected`
        // with `provider` at the default, the Gateway, and must poison every
        // request that would carry the Gateway credential.
        let config =
            config_with_profile(r#"{"provider":"gatewya"}"#, &[("AI_GATEWAY_API_KEY", KEY)]);
        assert!(config.provider_rejected.is_some());
        let gateway = Listener::answering(json_answer(REPORT));
        let endpoint =
            Endpoint::resolve(Some(&format!("{}/v4/ai/language-model", gateway.origin())))
                .expect("loopback");
        let answer = question(&config, &endpoint);
        if let Ok(asked) = &answer {
            let mut fetch =
                Fetch::start(asked.clone(), vec![today()], on_a_thread).expect("a worker");
            let _ = wait(&mut fetch, Duration::from_secs(25));
        }
        let requests = gateway.finish();
        assert!(
            answer.is_err() && requests.is_empty(),
            "question: {answer:?}; requests that reached the Gateway: {}",
            requests.len()
        );
        assert_eq!(answer.map(|_| ()), Err(Unavailable::ProviderRejected));
        // A readable `backend` beside it is no rescue, and an unreadable
        // `backend` poisons the same way.
        for profile in [
            r#"{"provider":"gatewya","backend":"gateway"}"#,
            r#"{"backend":"nonsense"}"#,
        ] {
            let config = config_with_profile(profile, &[("AI_GATEWAY_API_KEY", KEY)]);
            assert!(config.provider_rejected.is_some(), "{profile}");
            assert_eq!(
                question(&config, &default_endpoint()).map(|_| ()),
                Err(Unavailable::ProviderRejected),
                "{profile}"
            );
        }
    }

    #[test]
    fn the_report_takes_no_proxy_over_cleartext() {
        // Cleartext to loopback: a proxy from the environment would read the
        // key as sent (`crate::gateway::bearer_rules`).
        if let Some(origin) = crate::test_net::server_in_this_copy() {
            let outcome = fetch(&origin, vec![today()]);
            assert!(matches!(outcome, Outcome::Fetched(_)), "{outcome:?}");
            return;
        }
        crate::test_net::assert_no_proxy_is_used(
            "usage::report::tests::the_report_takes_no_proxy_over_cleartext",
            json_answer(REPORT),
        );
    }

    #[test]
    fn a_redirect_is_an_error_and_never_followed_with_the_key() {
        for status in [301u16, 302, 303, 307, 308] {
            let elsewhere = Listener::answering(json_answer(REPORT));
            let gateway = Listener::answering(redirect(
                status,
                &format!("{}/v1/report", elsewhere.origin()),
            ));
            let outcome = fetch(&gateway.origin(), vec![today()]);
            assert_eq!(gateway.finish().len(), 1);
            let followed = elsewhere.finish();
            assert!(followed.is_empty(), "a {status} was followed: {followed:?}");
            assert_eq!(
                failed(outcome),
                Failed {
                    failure: Failure::BadResponse,
                    detail: format!("HTTP {status}"),
                }
            );
        }
    }

    #[test]
    fn a_failed_range_ends_the_fetch() {
        let gateway = Listener::answering(
            "HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n".to_string(),
        );
        let outcome = fetch(&gateway.origin(), vec![today(), today(), today()]);
        assert_eq!(gateway.finish().len(), 1);
        assert_eq!(failed(outcome).failure, Failure::NotAvailable);
    }

    /// The next connection to `listener`, or `None` after `limit`, so a
    /// server thread never outlives its test by more than that.
    fn accept_within(listener: &TcpListener, limit: Duration) -> Option<std::net::TcpStream> {
        listener.set_nonblocking(true).ok()?;
        let started = Instant::now();
        while started.elapsed() < limit {
            if let Ok((connection, _)) = listener.accept() {
                connection.set_nonblocking(false).ok()?;
                return Some(connection);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        None
    }

    /// A chunked `200` whose one chunk is `body`.
    fn chunked_answer(body: &[u8]) -> Vec<u8> {
        let mut answer = format!(
            "HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\nconnection: close\r\n\r\n{:x}\r\n",
            body.len()
        )
        .into_bytes();
        answer.extend_from_slice(body);
        answer.extend_from_slice(b"\r\n0\r\n\r\n");
        answer
    }

    /// A loopback server for one connection: it reads the request, writes
    /// `head`, then keeps writing `chunk` until a write fails, `cap` bytes
    /// are out, or 20 s pass. Returns how much it wrote and whether a write
    /// failed: the client hung up.
    fn streaming(
        head: Vec<u8>,
        chunk: Vec<u8>,
        cap: usize,
    ) -> (String, std::thread::JoinHandle<(usize, bool)>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
        let origin = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let thread = std::thread::spawn(move || {
            let Some(mut connection) = accept_within(&listener, Duration::from_secs(20)) else {
                return (0, false);
            };
            let _ = connection.set_read_timeout(Some(Duration::from_secs(5)));
            let _ = connection.set_write_timeout(Some(Duration::from_secs(5)));
            crate::test_net::read_request(&mut connection);
            if connection.write_all(&head).is_err() {
                return (0, true);
            }
            let started = Instant::now();
            let mut written = 0;
            while written < cap && started.elapsed() < Duration::from_secs(20) {
                if connection.write_all(&chunk).is_err() {
                    return (written, true);
                }
                written += chunk.len();
            }
            (written, false)
        });
        (origin, thread)
    }

    #[test]
    fn an_answer_over_1_mib_is_refused_without_being_read_whole() {
        // One byte under, at, and over the cap, in one chunk each.
        let mut at_cap = REPORT.as_bytes().to_vec();
        at_cap.resize(MAX_BODY_BYTES, b' ');
        let gateway =
            Listener::answering(String::from_utf8(chunked_answer(&at_cap)).expect("ascii"));
        assert!(matches!(
            fetch(&gateway.origin(), vec![today()]),
            Outcome::Fetched(_)
        ));
        gateway.finish();
        at_cap.push(b' ');
        let gateway =
            Listener::answering(String::from_utf8(chunked_answer(&at_cap)).expect("ascii"));
        assert_eq!(
            failed(fetch(&gateway.origin(), vec![today()])),
            Failed::network(TOO_LARGE)
        );
        gateway.finish();

        // A declared length past the cap is refused at the head: the
        // server's few bytes are never awaited.
        let gateway = Listener::answering(format!(
            "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{{}}",
            MAX_BODY_BYTES + 1
        ));
        assert_eq!(
            failed(fetch(&gateway.origin(), vec![today()])),
            Failed::network(TOO_LARGE)
        );
        gateway.finish();

        // A body that never ends: the client hangs up past the cap, long
        // before the server's own bound of 64 MiB.
        let cap = 64 * MAX_BODY_BYTES;
        let mut chunk = format!("{:x}\r\n", 16 * 1024).into_bytes();
        chunk.extend(std::iter::repeat_n(b' ', 16 * 1024));
        chunk.extend_from_slice(b"\r\n");
        let (origin, server) = streaming(
            b"HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\n\r\n".to_vec(),
            chunk,
            cap,
        );
        assert_eq!(
            failed(fetch(&origin, vec![today()])),
            Failed::network(TOO_LARGE)
        );
        let (written, hung_up) = server.join().expect("the server");
        assert!(
            hung_up && written < cap,
            "wrote {written} bytes, hung up: {hung_up}"
        );
    }

    #[test]
    fn a_gateway_that_never_answers_fails_at_15_seconds() {
        // `usage_request_timeout_ms = 15_000`, from connecting to the last
        // byte (`client.zig:269,299-308@481104fd`). Three at once: one that
        // never reads the request (the handshake is the kernel's), one that
        // sends its head and stalls in the body, and one that keeps sending a
        // byte a second and never finishes -- which no idle or read timeout
        // would ever end.
        let silent = TcpListener::bind("127.0.0.1:0").expect("a port");
        let silent_origin = format!("http://127.0.0.1:{}", silent.local_addr().unwrap().port());
        let stop = Arc::new(AtomicBool::new(false));
        let stalled = TcpListener::bind("127.0.0.1:0").expect("a port");
        let stalled_origin = format!("http://127.0.0.1:{}", stalled.local_addr().unwrap().port());
        let holding = Arc::clone(&stop);
        let staller = std::thread::spawn(move || {
            let Some(mut connection) = accept_within(&stalled, Duration::from_secs(20)) else {
                return;
            };
            let _ = connection.set_read_timeout(Some(Duration::from_secs(5)));
            crate::test_net::read_request(&mut connection);
            let _ = connection
                .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 1000\r\n\r\n{\"results\":[");
            let started = Instant::now();
            while !holding.load(Ordering::SeqCst) && started.elapsed() < Duration::from_secs(30) {
                std::thread::sleep(Duration::from_millis(20));
            }
        });

        let (drip_origin, dripper) = dripping(&stop);

        let started = Instant::now();
        let mut fetches: Vec<Fetch> = [&silent_origin, &stalled_origin, &drip_origin]
            .into_iter()
            .map(|origin| {
                Fetch::start(question_at(origin), vec![today()], on_a_thread).expect("a worker")
            })
            .collect();
        for fetch in &mut fetches {
            let outcome = wait(fetch, Duration::from_secs(25)).expect("an outcome within 25 s");
            assert_eq!(failed(outcome), Failed::network(&no_answer()));
        }
        let elapsed = started.elapsed();
        stop.store(true, Ordering::SeqCst);
        staller.join().expect("the staller");
        let (dripped, _) = dripper.join().expect("the dripper");
        drop(silent);
        assert!(dripped >= 10, "the body kept coming: {dripped} bytes");
        assert!(
            elapsed >= DEADLINE && elapsed < DEADLINE + Duration::from_secs(5),
            "{elapsed:?}"
        );
    }

    #[test]
    fn a_fetch_runs_on_its_own_thread_and_can_be_cancelled_mid_body() {
        let stop = Arc::new(AtomicBool::new(false));
        let (origin, server) = dripping(&stop);
        let slot = Slot::default();
        let spawned = Arc::new(AtomicUsize::new(0));
        let counting = Arc::clone(&spawned);
        let spawn = keeping(&slot);
        let asked = question_at(&origin);
        let started = Instant::now();
        let mut fetch = Fetch::start(asked, vec![today()], move |job| {
            counting.fetch_add(1, Ordering::SeqCst);
            spawn(job)
        })
        .expect("a worker");
        assert_eq!(spawned.load(Ordering::SeqCst), 1);
        assert_eq!(
            fetch.try_outcome(),
            None,
            "start does not wait for the answer"
        );
        assert!(started.elapsed() < Duration::from_secs(1));
        // Into the body: the head and a byte or two have arrived.
        std::thread::sleep(Duration::from_millis(1500));
        fetch.cancel();
        assert_eq!(
            wait(&mut fetch, Duration::from_secs(5)),
            Some(Outcome::Cancelled)
        );
        assert!(started.elapsed() < Duration::from_secs(7));
        // Told once.
        assert_eq!(fetch.try_outcome(), None);
        // The thread ends, and with it the connection.
        assert!(
            exited_within(&slot, Duration::from_secs(5)),
            "the worker outlived its cancel"
        );
        let (dripped, hung_up) = server.join().expect("the server");
        assert!(
            dripped >= 1 && hung_up,
            "dripped {dripped}, hung up: {hung_up}"
        );
        stop.store(true, Ordering::SeqCst);

        // A spawner that cannot start a thread is an error, not a fetch.
        let refused = Fetch::start(question_at(&origin), vec![today()], |_| {
            Err(io::Error::other("no threads"))
        });
        assert!(refused.is_err());
    }

    #[test]
    fn a_dropped_fetch_stops_and_lets_go_of_the_key() {
        // Ranges left behind a dropped fetch would each ask again, for up to
        // 15 s apiece, with nobody to tell.
        let stop = Arc::new(AtomicBool::new(false));
        let (origin, server) = dripping(&stop);
        let slot = Slot::default();
        let fetch = Fetch::start(
            question_at(&origin),
            vec![today(), today(), today()],
            keeping(&slot),
        )
        .expect("a worker");
        std::thread::sleep(Duration::from_millis(1500));
        let dropped = Instant::now();
        drop(fetch);
        assert!(
            exited_within(&slot, Duration::from_secs(5)),
            "the worker outlived its fetch"
        );
        assert!(dropped.elapsed() < Duration::from_secs(5));
        let (_, hung_up) = server.join().expect("the server");
        stop.store(true, Ordering::SeqCst);
        assert!(hung_up, "the connection outlived the fetch");
    }

    #[test]
    fn the_key_never_appears_in_a_failure_even_when_echoed() {
        let escaped: String = KEY
            .chars()
            .map(|c| format!("\\u{:04x}", c as u32))
            .collect();
        let cases: Vec<(String, Failure)> = vec![
            (
                format!(
                    "HTTP/1.1 403 Forbidden\r\nwww-authenticate: Bearer {KEY}\r\ncontent-type: application/json\r\nconnection: close\r\n\r\n{{\"error\":{{\"message\":\"bad key {KEY}\\n{escaped}\"}}}}"
                ),
                Failure::Refused,
            ),
            (
                format!("HTTP/1.1 400 Bad Request\r\nconnection: close\r\n\r\nkey={KEY}\n{}", "x".repeat(4000)),
                Failure::BadResponse,
            ),
            (
                format!("HTTP/1.1 500 {KEY}\r\nconnection: close\r\n\r\n{KEY}"),
                Failure::Server,
            ),
            (json_answer(&format!("not json {KEY}")), Failure::BadResponse),
            (
                json_answer(&format!(
                    r#"{{"results":[{{"model":"{escaped}","total_cost":1}}]}}"#
                )),
                Failure::BadResponse,
            ),
            (
                json_answer(&format!(r#"{{"results":[{{"model":"x-{KEY}","total_cost":-1}}]}}"#)),
                Failure::BadResponse,
            ),
        ];
        for (answer, failure) in cases {
            let gateway = Listener::answering(answer.clone());
            let outcome = fetch(&gateway.origin(), vec![today()]);
            gateway.finish();
            let shown = format!("{outcome:?}");
            let failed = failed(outcome);
            assert_eq!(failed.failure, failure, "{answer}");
            for text in [&failed.detail, &shown] {
                assert!(!text.contains(KEY), "{text}");
                assert!(!text.contains('\n') && text.len() <= 128, "{text}");
            }
        }
    }
}
