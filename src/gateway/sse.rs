//! A bounded decoder for the Gateway's `text/event-stream` response.
//!
//! The decoder is fed arbitrary byte chunks. Transport boundaries never line up
//! with event boundaries on a real stream, so the state lives here rather than
//! in the transport, and a one-byte-at-a-time feed must produce the same result
//! as a single write.
//!
//! Two rules make this decoder safe rather than merely convenient:
//!
//! 1. **A finish event is required.** Neither end-of-stream nor `[DONE]` proves
//!    the model completed; upstream terminates on both without a finish reason
//!    and records the difference (`vercel-labs/fx@580a0c5d
//!    src/gateway/client.zig:2817-2833`, `:3641`). xfx refuses to call a
//!    truncated stream an answer.
//! 2. **A single event is bounded.** A stream that never sends a newline would
//!    otherwise buffer without limit, so the accumulator is capped by
//!    [`MAX_EVENT_BYTES`], mirroring upstream's per-line ceiling
//!    (`src/gateway/client.zig:2660-2686`).
//!
//! A malformed nonterminal event is skipped rather than fatal
//! (`src/gateway/client.zig:2837-2841`); a malformed *terminal* event is fatal,
//! because that is the one event whose meaning cannot be guessed.

use std::fmt;
use std::io;

use serde_json::{Map, Value};

use super::{CancelToken, DeltaSink};
use crate::gateway::protocol::{
    Bill, Billing, Completion, Deferred, FinishReason, ToolCall, Usage,
};
use crate::provider::Wire;

/// The largest single SSE event xfx will buffer, in bytes.
pub const MAX_EVENT_BYTES: usize = 32 * 1024 * 1024;

/// The most a single completion may accumulate, in bytes.
///
/// [`MAX_EVENT_BYTES`] bounds one frame; this bounds the whole answer, and the
/// second does not follow from the first -- a stream of well-formed small frames
/// grows without limit. Counts the assistant text and every streamed tool
/// argument together, because those are the two things that grow with the
/// stream.
///
/// The same ceiling both decoders use, defined here because the error that
/// reports it is shared and a bound advertised by one enum but enforced by one
/// of its two users is not a bound.
pub const MAX_COMPLETION_BYTES: usize = 8 * 1024 * 1024;

/// The `data:` prefix, including the single separating space upstream requires
/// (`src/gateway/client.zig:2652-2653`).
const DATA_PREFIX: &str = "data: ";

/// Why a stream could not be turned into a completion.
#[derive(Debug)]
pub enum SseError {
    /// The turn was cancelled while the stream was being read.
    Cancelled,
    /// One event exceeded the buffering ceiling.
    EventTooLarge { limit: usize },
    /// The accumulated answer exceeded the ceiling on a whole completion.
    ///
    /// Distinct from [`Self::EventTooLarge`], which bounds one frame: a stream of
    /// well-formed small frames can grow without limit, and "bounded decoder" has
    /// to mean bounded over the stream rather than over each event of it.
    CompletionTooLarge { limit: usize },
    /// The stream ended, with or without `[DONE]`, and never sent a finish
    /// event. A partial answer is not a completed answer.
    MissingFinish,
    /// A finish event arrived whose reason could not be read at all.
    InvalidFinishReason { detail: String },
    /// A finish event named a reason this version does not know. Mapping it
    /// onto a known reason would report an unknown terminal state as normal.
    UnknownFinishReason { raw: String },
    /// A tool call arrived without a usable identity, name, or input.
    InvalidToolCall { detail: String },
    /// Two tool calls in one stream claimed the same identifier.
    DuplicateToolCallId { call_id: String },
    /// The provider reported its own failure and never finished.
    ProviderFailure {
        detail: String,
        /// Whether the condition is transient, so the turn may replay the
        /// attempt when nothing was delivered.
        ///
        /// Anthropic delivers `overloaded_error` and `rate_limit_error` inside
        /// an HTTP 200, where the Gateway would deliver the same upstream
        /// condition as a 429. Without this the two backends disagree about how
        /// many attempts the identical failure is worth.
        retryable: bool,
    },
    /// The consumer of the assistant text could not accept it.
    Sink(io::Error),
}

impl fmt::Display for SseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => write!(f, "the stream was cancelled"),
            Self::EventTooLarge { limit } => {
                write!(f, "a single stream event exceeded {limit} bytes")
            }
            Self::CompletionTooLarge { limit } => {
                write!(f, "the accumulated completion exceeded {limit} bytes")
            }
            Self::MissingFinish => write!(
                f,
                "the stream ended without a finish event, so the answer is incomplete"
            ),
            Self::InvalidFinishReason { detail } => {
                write!(f, "the finish event carried no readable reason: {detail}")
            }
            Self::UnknownFinishReason { raw } => {
                write!(f, "unknown finish reason `{raw}`")
            }
            Self::InvalidToolCall { detail } => write!(f, "unusable tool call: {detail}"),
            Self::DuplicateToolCallId { call_id } => {
                write!(f, "the stream reused tool call id `{call_id}`")
            }
            Self::ProviderFailure { detail, .. } => write!(f, "the provider failed: {detail}"),
            Self::Sink(err) => write!(f, "cannot write assistant output: {err}"),
        }
    }
}

impl std::error::Error for SseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Sink(err) => Some(err),
            _ => None,
        }
    }
}

/// A tool call whose input is being streamed in pieces.
#[derive(Debug)]
struct StreamedInput {
    id: String,
    name: String,
    arguments: String,
}

/// Decodes an SSE response into one [`Completion`].
#[derive(Debug)]
pub struct SseReader {
    /// The bytes of the current, not-yet-terminated line.
    line: Vec<u8>,
    text: String,
    tool_calls: Vec<ToolCall>,
    streamed: Vec<StreamedInput>,
    finish_reason: Option<FinishReason>,
    usage: Usage,
    provider_detail: Option<String>,
    /// What the response's `response-metadata` said it was billed at, or
    /// [`Stamp::Unreadable`] once two of them disagreed or one could not be
    /// read (`client.zig:3647-3670`).
    stamp: Stamp,
    /// The Gateway's generation id, from whichever event carried it first.
    generation_id: Option<String>,
    /// An event carried a generation id that was not one, or a second that
    /// disagreed with the first (`captureGenerationMetadata`,
    /// `client.zig:3428-3456`): the response is not counted.
    generation_invalid: bool,
    /// The finish event's billing, read once it has arrived.
    billing: Option<Billing>,
    /// Where the response came from, for a generation whose bill is asked
    /// for later ([`Deferred::origin`]); empty until [`Self::from_origin`].
    origin: String,
    complete: bool,
    /// Bytes counted against [`MAX_COMPLETION_BYTES`]: text plus tool arguments.
    accumulated: usize,
    cancel: CancelToken,
}

impl Default for SseReader {
    fn default() -> Self {
        Self::new()
    }
}

impl SseReader {
    pub fn new() -> Self {
        Self::with_cancel(CancelToken::new())
    }

    /// A reader that stops at the next frame boundary once `cancel` is set.
    pub fn with_cancel(cancel: CancelToken) -> Self {
        Self {
            line: Vec::new(),
            text: String::new(),
            tool_calls: Vec::new(),
            streamed: Vec::new(),
            finish_reason: None,
            usage: Usage::default(),
            provider_detail: None,
            stamp: Stamp::Unseen,
            generation_id: None,
            generation_invalid: false,
            billing: None,
            origin: String::new(),
            complete: false,
            accumulated: 0,
            cancel,
        }
    }

    /// The same reader, reading a response from `origin`.
    pub fn from_origin(mut self, origin: &str) -> Self {
        self.origin = origin.to_string();
        self
    }

    /// Charges `bytes` against the completion ceiling.
    fn charge(&mut self, bytes: usize) -> Result<(), SseError> {
        self.accumulated = self.accumulated.saturating_add(bytes);
        if self.accumulated > MAX_COMPLETION_BYTES {
            return Err(SseError::CompletionTooLarge {
                limit: MAX_COMPLETION_BYTES,
            });
        }
        Ok(())
    }

    /// Whether a canonical finish event has arrived.
    ///
    /// The transport uses this to stop reading: once the model has finished,
    /// remaining bytes are trailer, and waiting for them delays the answer.
    pub fn is_complete(&self) -> bool {
        self.complete
    }

    /// Consumes one transport chunk.
    ///
    /// Text deltas are handed to `deltas` as they are decoded, in arrival order,
    /// because a delta that arrives after the turn is useless to a reader.
    pub fn push(&mut self, chunk: &[u8], deltas: &mut dyn DeltaSink) -> Result<(), SseError> {
        if self.complete {
            // Everything after a canonical finish is trailer
            // (`src/gateway/client.zig:3237-3238`).
            return Ok(());
        }
        if self.cancel.is_cancelled() {
            return Err(SseError::Cancelled);
        }

        let mut rest = chunk;
        while let Some(position) = rest.iter().position(|byte| *byte == b'\n') {
            let (head, tail) = rest.split_at(position);
            self.buffer(head)?;
            let line = std::mem::take(&mut self.line);
            self.consume_line(&line, deltas)?;
            if self.complete {
                return Ok(());
            }
            if self.cancel.is_cancelled() {
                return Err(SseError::Cancelled);
            }
            rest = &tail[1..];
        }
        self.buffer(rest)
    }

    /// Ends the stream and produces the completion it proved.
    pub fn finish(mut self) -> Result<Completion, SseError> {
        // A final line without a trailing newline is still an event.
        if !self.complete && !self.line.is_empty() {
            let line = std::mem::take(&mut self.line);
            let mut discard = DiscardDeltas;
            self.consume_line(&line, &mut discard)?;
        }

        match self.finish_reason {
            Some(finish_reason) => Ok(Completion {
                text: self.text,
                tool_calls: self.tool_calls,
                finish_reason,
                usage: self.usage,
                provider_detail: self.provider_detail,
                // The Vercel wire has no replay contract and no signed blocks,
                // so there is nothing here to preserve.
                raw_content: Vec::new(),
                wire: Wire::VercelGateway,
                // Counted only when every piece upstream counts it by arrived
                // and agreed: the finish's billing and a generation id
                // nothing contradicted (`InvocationObservation.complete`,
                // `session_usage.zig:110-175`). The id without the billing
                // is a bill to ask for later; anything contradicted is one
                // nobody can ask for.
                billing: match (self.billing, self.generation_id) {
                    _ if self.generation_invalid => None,
                    (Some(billing), Some(id)) => Some(Bill::Exact(Billing {
                        generation_id: Some(id),
                        ..billing
                    })),
                    (None, Some(generation_id)) => Some(Bill::Deferred(Deferred {
                        generation_id,
                        origin: self.origin,
                    })),
                    (_, None) => None,
                },
            }),
            // The provider said why it failed, so report that rather than the
            // generic truncation.
            None => match self.provider_detail {
                Some(detail) => Err(SseError::ProviderFailure {
                    detail,
                    // The Vercel wire carries no error taxonomy this decoder
                    // could classify, and upstream records the detail and
                    // decides nothing (`client.zig:2902-2903`). An unclassified
                    // failure is not replayed.
                    retryable: false,
                }),
                None => Err(SseError::MissingFinish),
            },
        }
    }

    fn buffer(&mut self, bytes: &[u8]) -> Result<(), SseError> {
        if self.line.len() + bytes.len() > MAX_EVENT_BYTES {
            return Err(SseError::EventTooLarge {
                limit: MAX_EVENT_BYTES,
            });
        }
        self.line.extend_from_slice(bytes);
        Ok(())
    }

    fn consume_line(&mut self, line: &[u8], deltas: &mut dyn DeltaSink) -> Result<(), SseError> {
        let line = String::from_utf8_lossy(line);
        let line = line.trim_end_matches('\r');
        if line.is_empty() || line.starts_with(':') {
            return Ok(());
        }
        // Upstream accepts both spellings of the terminator
        // (`src/gateway/client.zig:2650-2656`). Neither one completes a turn.
        if line == "DONE" {
            return Ok(());
        }
        let Some(payload) = line.strip_prefix(DATA_PREFIX) else {
            return Ok(());
        };
        if payload == "[DONE]" {
            return Ok(());
        }

        // A nonterminal event that cannot be parsed is skipped, not fatal
        // (`src/gateway/client.zig:2837-2841`).
        let Ok(Value::Object(event)) = serde_json::from_str::<Value>(payload) else {
            return Ok(());
        };
        let Some(Value::String(event_type)) = event.get("type") else {
            return Ok(());
        };
        // Any event may carry the generation id, and every one that does must
        // agree with the first.
        self.capture_generation(&event);

        match event_type.as_str() {
            "response-metadata" => {
                self.on_response_metadata(&event);
                Ok(())
            }
            "text-delta" => self.on_text_delta(&event, deltas),
            "tool-input-start" => {
                self.on_tool_input_start(&event);
                Ok(())
            }
            "tool-input-delta" => self.on_tool_input_delta(&event, deltas),
            "reasoning-delta" => {
                on_reasoning_delta(&event, deltas);
                Ok(())
            }
            "tool-call" => self.on_tool_call(&event),
            "error" => {
                if let Some(detail) = failure_detail(&event) {
                    self.provider_detail = Some(detail);
                }
                Ok(())
            }
            "finish" => self.on_finish(&event),
            // `tool-input-end`, `text-start`, the rest of `reasoning-*`,
            // `start-step` and every future event carry nothing xfx acts on
            // yet.
            _ => Ok(()),
        }
    }

    fn on_text_delta(
        &mut self,
        event: &Map<String, Value>,
        deltas: &mut dyn DeltaSink,
    ) -> Result<(), SseError> {
        let Some(Value::String(delta)) = event.get("delta") else {
            return Ok(());
        };
        if delta.is_empty() {
            return Ok(());
        }
        self.charge(delta.len())?;
        self.text.push_str(delta);
        deltas.text_delta(delta).map_err(SseError::Sink)
    }

    fn on_tool_input_start(&mut self, event: &Map<String, Value>) {
        let Some(id) = string_field(event, "id") else {
            return;
        };
        if self.streamed.iter().any(|record| record.id == id) {
            return;
        }
        self.streamed.push(StreamedInput {
            id: id.to_string(),
            name: string_field(event, "toolName")
                .unwrap_or_default()
                .to_string(),
            arguments: String::new(),
        });
    }

    fn on_tool_input_delta(
        &mut self,
        event: &Map<String, Value>,
        deltas: &mut dyn DeltaSink,
    ) -> Result<(), SseError> {
        let Some(id) = string_field(event, "id") else {
            return Ok(());
        };
        let Some(Value::String(delta)) = event.get("delta") else {
            return Ok(());
        };
        self.charge(delta.len())?;
        // A delta for an unannounced id is dropped; correlating it would invent
        // a call the provider never opened. Nor is it counted: upstream counts
        // the arguments of a call it is assembling, and only those
        // (`vercel-labs/fx@c1db9191 src/gateway/client.zig:3746-3760`).
        if let Some(record) = self.streamed.iter_mut().find(|record| record.id == id) {
            record.arguments.push_str(delta);
            deltas.tool_input_delta(delta);
        }
        Ok(())
    }

    fn on_tool_call(&mut self, event: &Map<String, Value>) -> Result<(), SseError> {
        let Some(call_id) = string_field(event, "toolCallId") else {
            return Err(SseError::InvalidToolCall {
                detail: "the tool call has no `toolCallId`".to_string(),
            });
        };
        let call_id = call_id.to_string();
        if self.tool_calls.iter().any(|call| call.id == call_id) {
            return Err(SseError::DuplicateToolCallId { call_id });
        }

        let streamed = self.streamed.iter().find(|record| record.id == call_id);
        let name = match string_field(event, "toolName") {
            Some(name) => name.to_string(),
            None => match streamed.map(|record| record.name.as_str()) {
                Some(name) if !name.is_empty() => name.to_string(),
                _ => {
                    return Err(SseError::InvalidToolCall {
                        detail: format!("tool call `{call_id}` names no tool"),
                    })
                }
            },
        };

        // A direct input outranks a streamed one; the provider's last word wins
        // (`src/gateway/client.zig:4141-4145`).
        let input = match event.get("input") {
            Some(value) => normalize_input(value),
            None => match streamed.map(|record| record.arguments.as_str()) {
                Some(arguments) if !arguments.trim().is_empty() => serde_json::from_str(arguments)
                    .map_err(|err| SseError::InvalidToolCall {
                        detail: format!("tool call `{call_id}` streamed unusable input: {err}"),
                    })?,
                _ => Value::Object(Map::new()),
            },
        };

        self.tool_calls.push(ToolCall {
            id: call_id,
            name,
            input,
        });
        Ok(())
    }

    fn on_finish(&mut self, event: &Map<String, Value>) -> Result<(), SseError> {
        // `finishReason` is an object carrying a `unified` string
        // (`src/gateway/client.zig:2463-2473`).
        let Some(Value::Object(reason)) = event.get("finishReason") else {
            return Err(SseError::InvalidFinishReason {
                detail: "`finishReason` is missing or is not an object".to_string(),
            });
        };
        let Some(Value::String(unified)) = reason.get("unified") else {
            return Err(SseError::InvalidFinishReason {
                detail: "`finishReason.unified` is missing or is not a string".to_string(),
            });
        };
        let Some(finish_reason) = FinishReason::parse_unified(unified) else {
            return Err(SseError::UnknownFinishReason {
                raw: unified.clone(),
            });
        };

        self.usage = parse_usage(event);
        self.billing = match self.stamp {
            Stamp::Read(at_ms) => parse_billing(event, at_ms),
            Stamp::Unseen | Stamp::Unreadable => None,
        };
        if finish_reason == FinishReason::ProviderError {
            if let Some(detail) = failure_detail(event) {
                self.provider_detail = Some(detail);
            }
        }
        self.finish_reason = Some(finish_reason);
        self.complete = true;
        Ok(())
    }
}

impl SseReader {
    /// Reads the time a `response-metadata` event says the response was billed
    /// at. A second one that disagrees, or one that cannot be read, leaves the
    /// response with no time at all, and so with no billing.
    fn on_response_metadata(&mut self, event: &Map<String, Value>) {
        let Some(value) = event.get("timestamp") else {
            return;
        };
        let read = match value {
            Value::String(text) => parse_gateway_timestamp(text),
            _ => None,
        };
        self.stamp = match (self.stamp, read) {
            (Stamp::Unseen, Some(at_ms)) => Stamp::Read(at_ms),
            (Stamp::Read(seen), Some(at_ms)) if seen == at_ms => Stamp::Read(at_ms),
            _ => Stamp::Unreadable,
        };
    }

    /// Notes the generation id an event carries under
    /// `providerMetadata.gateway.generationId`, if it carries one.
    fn capture_generation(&mut self, event: &Map<String, Value>) {
        let Some(metadata) = event.get("providerMetadata") else {
            return;
        };
        let Some(gateway) = metadata
            .as_object()
            .and_then(|object| object.get("gateway"))
        else {
            if !metadata.is_object() {
                self.generation_invalid = true;
            }
            return;
        };
        let Some(gateway) = gateway.as_object() else {
            self.generation_invalid = true;
            return;
        };
        let Some(value) = gateway.get("generationId") else {
            return;
        };
        match value.as_str().filter(|id| is_generation_id(id)) {
            None => self.generation_invalid = true,
            Some(id) => match &self.generation_id {
                Some(first) if first != id => self.generation_invalid = true,
                Some(_) => {}
                None => self.generation_id = Some(id.to_string()),
            },
        }
    }
}

/// What the response's `response-metadata` events said about when it was
/// billed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stamp {
    /// None has said anything yet.
    Unseen,
    /// All that have said anything agreed on this, in milliseconds.
    Read(i64),
    /// One could not be read, or two disagreed.
    Unreadable,
}

/// The finish event's billing, or `None` when any piece upstream requires is
/// missing or out of shape (`parseSseBilling`, `client.zig:3313-3395`): both
/// token buckets and their totals, `providerMetadata.gateway.routing.canonicalSlug`
/// as printable ASCII, and `cost` as a decimal string that is finite and not
/// negative. A cache count larger than the input, or a reasoning count larger
/// than the output, is a bill that does not add up, and is not one.
fn parse_billing(event: &Map<String, Value>, at_ms: i64) -> Option<Billing> {
    let usage = event.get("usage")?.as_object()?;
    let input = usage.get("inputTokens")?.as_object()?;
    let output = usage.get("outputTokens")?.as_object()?;
    let gateway = event
        .get("providerMetadata")?
        .as_object()?
        .get("gateway")?
        .as_object()?;
    let model = gateway
        .get("routing")?
        .as_object()?
        .get("canonicalSlug")?
        .as_str()?;
    if model.is_empty()
        || model.len() > 1024
        || !model.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
    {
        return None;
    }
    let cost: f64 = gateway.get("cost")?.as_str()?.parse().ok()?;
    if !cost.is_finite() || cost < 0.0 {
        return None;
    }
    let input_tokens = count(input.get("total"))?;
    let output_tokens = count(output.get("total"))?;
    let cache_read_tokens = optional_count(input.get("cacheRead"))?.unwrap_or(0);
    let cache_write_tokens = optional_count(input.get("cacheWrite"))?.unwrap_or(0);
    // `null` is "not reported", which is a count of its own kind.
    let reasoning_tokens = match output.get("reasoning") {
        None | Some(Value::Null) => None,
        Some(value) => Some(count(Some(value))?),
    };
    if cache_read_tokens > input_tokens
        || cache_write_tokens > input_tokens
        || reasoning_tokens.is_some_and(|reasoning| reasoning > output_tokens)
    {
        return None;
    }
    Some(Billing {
        at_ms,
        model: model.to_string(),
        generation_id: None,
        cost,
        input_tokens,
        output_tokens,
        cache_read_tokens,
        cache_write_tokens,
        reasoning_tokens,
    })
}

/// A token count that must be there and must be a whole number.
fn count(value: Option<&Value>) -> Option<u64> {
    value?.as_u64()
}

/// A token count that may be absent (`Some(None)`) but, when present, must be
/// a whole number (`None`).
fn optional_count(value: Option<&Value>) -> Option<Option<u64>> {
    match value {
        None => Some(None),
        Some(value) => value.as_u64().map(Some),
    }
}

/// Whether `id` is a Gateway generation id: `gen_` and 26 characters of
/// Crockford's base 32 (`validGatewayGenerationId`, `types.zig:1837-1844`).
pub(crate) fn is_generation_id(id: &str) -> bool {
    id.len() == 30
        && id.starts_with("gen_")
        && id[4..]
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'A'..=b'H' | b'J'..=b'K' | b'M'..=b'N' | b'P'..=b'T' | b'V'..=b'Z'))
}

/// A Gateway timestamp -- `YYYY-MM-DDTHH:MM:SS[.fraction]Z`, UTC and nothing
/// looser -- in milliseconds since the epoch (`parseGatewayTimestamp`,
/// `types.zig:1736-1800`). A fraction is one to nine digits, of which the
/// first three are kept.
pub(crate) fn parse_gateway_timestamp(text: &str) -> Option<i64> {
    let bytes = text.as_bytes();
    if bytes.len() < 20
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
    {
        return None;
    }
    let digits = |range: std::ops::Range<usize>| -> Option<i64> {
        let part = &bytes[range];
        part.iter().all(u8::is_ascii_digit).then(|| {
            part.iter()
                .fold(0i64, |sum, digit| sum * 10 + i64::from(digit - b'0'))
        })
    };
    let (year, month, day) = (digits(0..4)?, digits(5..7)?, digits(8..10)?);
    let (hour, minute, second) = (digits(11..13)?, digits(14..16)?, digits(17..19)?);
    if year < 1970
        || !(1..=12).contains(&month)
        || day < 1
        || day > days_in_month(year, month)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }
    let mut cursor = 19;
    let mut fraction_ms = 0;
    if bytes.get(cursor) == Some(&b'.') {
        cursor += 1;
        let start = cursor;
        while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
            cursor += 1;
        }
        let length = cursor - start;
        if length == 0 || length > 9 {
            return None;
        }
        let kept = length.min(3);
        fraction_ms = digits(start..start + kept)? * [100, 10, 1][kept - 1];
    }
    if cursor + 1 != bytes.len() || bytes[cursor] != b'Z' {
        return None;
    }
    let days = days_from_civil(year, month, day);
    days.checked_mul(86_400)?
        .checked_add(hour * 3600 + minute * 60 + second)?
        .checked_mul(1000)?
        .checked_add(fraction_ms)
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Days from 1970-01-01 to a civil date (Howard Hinnant's algorithm, which is
/// the one upstream's `daysFromCivil` is).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let shifted_month = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// A sink for text that has nowhere left to go.
///
/// Only used for a trailing unterminated line at end of stream, which cannot
/// contain a delta a reader is still waiting for.
struct DiscardDeltas;

impl DeltaSink for DiscardDeltas {
    fn text_delta(&mut self, _text: &str) -> io::Result<()> {
        Ok(())
    }
}

/// A `reasoning-delta`'s text, told to `deltas` to be counted and kept nowhere:
/// xfx neither shows reasoning nor sends it back on this wire. A nonempty
/// string only, as upstream takes one (`src/gateway/client.zig:3705-3711`).
fn on_reasoning_delta(event: &Map<String, Value>, deltas: &mut dyn DeltaSink) {
    if let Some(Value::String(delta)) = event.get("delta") {
        if !delta.is_empty() {
            deltas.reasoning_delta(delta);
        }
    }
}

/// A nonempty string field.
fn string_field<'a>(event: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    match event.get(key) {
        Some(Value::String(value)) if !value.is_empty() => Some(value),
        _ => None,
    }
}

/// Interprets a `tool-call` input, which may arrive parsed or as JSON text
/// (`src/gateway/client.zig:3927-3988`).
fn normalize_input(value: &Value) -> Value {
    match value {
        Value::String(text) => serde_json::from_str(text).unwrap_or_else(|_| value.clone()),
        other => other.clone(),
    }
}

/// The provider's own description of a failure, if it gave one
/// (`src/gateway/client.zig:2902-2903`).
fn failure_detail(event: &Map<String, Value>) -> Option<String> {
    let error = event.get("error")?;
    match error {
        Value::String(text) if !text.is_empty() => Some(text.clone()),
        Value::Object(object) => match object.get("message") {
            Some(Value::String(message)) if !message.is_empty() => Some(message.clone()),
            _ => Some(error.to_string()),
        },
        Value::Null => None,
        other => Some(other.to_string()),
    }
}

/// Token totals, when the provider reported readable ones
/// (`src/gateway/client.zig:2475-2492`).
fn parse_usage(event: &Map<String, Value>) -> Usage {
    let Some(Value::Object(usage)) = event.get("usage") else {
        return Usage::default();
    };
    Usage {
        input_tokens: token_total(usage, "inputTokens"),
        output_tokens: token_total(usage, "outputTokens"),
    }
}

fn token_total(usage: &Map<String, Value>, key: &str) -> Option<u64> {
    let Some(Value::Object(bucket)) = usage.get(key) else {
        return None;
    };
    bucket.get("total")?.as_u64()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[derive(Default)]
    struct Collected(Vec<String>);

    impl DeltaSink for Collected {
        fn text_delta(&mut self, text: &str) -> io::Result<()> {
            self.0.push(text.to_string());
            Ok(())
        }
    }

    fn decode(body: &str) -> (Result<Completion, SseError>, Vec<String>) {
        let mut deltas = Collected::default();
        let mut reader = SseReader::new();
        match reader.push(body.as_bytes(), &mut deltas) {
            Ok(()) => (reader.finish(), deltas.0),
            Err(err) => (Err(err), deltas.0),
        }
    }

    /// A stream the way the Gateway bills one (`billed-turns.json`'s second
    /// turn): `response-metadata` first, then the finish with its usage and
    /// `providerMetadata.gateway`. `finish` is spliced into the finish event.
    fn billed(metadata: serde_json::Value, finish: serde_json::Value) -> String {
        let mut event = json!({
            "type": "finish",
            "finishReason": { "unified": "stop", "raw": "stop" },
            "usage": {
                "inputTokens": { "total": 52000, "cacheRead": 30000 },
                "outputTokens": { "total": 3100, "reasoning": 900 }
            },
            "providerMetadata": { "gateway": {
                "routing": { "canonicalSlug": "anthropic/claude-opus-5" },
                "cost": "0.4210",
                "generationId": "gen_01JABCDEFGHJKMNPQRSTVWXYZ1"
            } }
        });
        if let (Some(event), Some(changes)) = (event.as_object_mut(), finish.as_object()) {
            for (key, value) in changes {
                event.insert(key.clone(), value.clone());
            }
        }
        format!(
            "data: {metadata}\n\ndata: {{\"type\":\"text-delta\",\"id\":\"t\",\"delta\":\"x\"}}\n\ndata: {event}\n\n"
        )
    }

    fn stamped() -> serde_json::Value {
        json!({ "type": "response-metadata", "modelId": "anthropic/claude-opus-5", "timestamp": "2026-10-07T01:02:03.456Z" })
    }

    /// The bill a stream said all of, or a failed case.
    fn exact(completion: &Completion) -> Billing {
        match &completion.billing {
            Some(Bill::Exact(billing)) => billing.clone(),
            other => panic!("not billed: {other:?}"),
        }
    }

    /// What a generation whose bill was not in its stream is asked for by.
    fn deferred(id: &str, origin: &str) -> Option<Bill> {
        Some(Bill::Deferred(Deferred {
            generation_id: id.to_string(),
            origin: origin.to_string(),
        }))
    }

    const GENERATION: &str = "gen_01JABCDEFGHJKMNPQRSTVWXYZ1";

    #[test]
    fn a_billed_finish_is_read_whole() {
        let completion = decode(&billed(stamped(), json!({}))).0.expect("finish");
        let billing = exact(&completion);
        assert_eq!(billing.model, "anthropic/claude-opus-5");
        assert_eq!(billing.cost, 0.421);
        assert_eq!((billing.input_tokens, billing.output_tokens), (52000, 3100));
        assert_eq!(
            (billing.cache_read_tokens, billing.cache_write_tokens),
            (30000, 0)
        );
        assert_eq!(billing.reasoning_tokens, Some(900));
        assert_eq!(
            billing.generation_id.as_deref(),
            Some("gen_01JABCDEFGHJKMNPQRSTVWXYZ1")
        );
        assert_eq!(
            billing.at_ms,
            parse_gateway_timestamp("2026-10-07T01:02:03.456Z").unwrap()
        );
        // The meter's own totals are untouched by any of it.
        assert_eq!(completion.usage.input_tokens, Some(52000));
    }

    /// Upstream's `.deferred` (`gatewayUsageOutcome`, `gateway.zig:650-660`):
    /// a stream whose generation id arrived intact and whose bill did not is
    /// one whose bill is asked for later, by that id. Without an id, or with
    /// one that was contradicted, there is nothing to ask by.
    #[test]
    fn a_finish_missing_any_piece_upstream_counts_by_is_asked_for_by_its_id() {
        let no_id = json!({ "providerMetadata": { "gateway": {
            "routing": { "canonicalSlug": "a/b" }, "cost": "0.1" } } });
        let bad_id = json!({ "providerMetadata": { "gateway": {
            "routing": { "canonicalSlug": "a/b" }, "cost": "0.1", "generationId": "gen_lowercase" } } });
        let no_cost = json!({ "providerMetadata": { "gateway": {
            "routing": { "canonicalSlug": "a/b" }, "generationId": "gen_01JABCDEFGHJKMNPQRSTVWXYZ1" } } });
        let numeric_cost = json!({ "providerMetadata": { "gateway": {
            "routing": { "canonicalSlug": "a/b" }, "cost": 0.1, "generationId": "gen_01JABCDEFGHJKMNPQRSTVWXYZ1" } } });
        let negative_cost = json!({ "providerMetadata": { "gateway": {
            "routing": { "canonicalSlug": "a/b" }, "cost": "-1", "generationId": "gen_01JABCDEFGHJKMNPQRSTVWXYZ1" } } });
        let spaced_slug = json!({ "providerMetadata": { "gateway": {
            "routing": { "canonicalSlug": "a b" }, "cost": "0.1", "generationId": "gen_01JABCDEFGHJKMNPQRSTVWXYZ1" } } });
        let cache_over_input = json!({ "usage": {
            "inputTokens": { "total": 10, "cacheRead": 11 }, "outputTokens": { "total": 1 } } });
        let reasoning_over_output = json!({ "usage": {
            "inputTokens": { "total": 10 }, "outputTokens": { "total": 1, "reasoning": 2 } } });
        let no_output_total = json!({ "usage": {
            "inputTokens": { "total": 10 }, "outputTokens": {} } });
        for finish in [no_id, bad_id] {
            let completion = decode(&billed(stamped(), finish.clone()))
                .0
                .expect("finish");
            assert_eq!(completion.billing, None, "{finish}");
        }
        for finish in [
            no_cost,
            numeric_cost,
            negative_cost,
            spaced_slug,
            cache_over_input,
            reasoning_over_output,
            no_output_total,
        ] {
            let completion = decode(&billed(stamped(), finish.clone()))
                .0
                .expect("finish");
            assert_eq!(completion.billing, deferred(GENERATION, ""), "{finish}");
        }
    }

    #[test]
    fn a_deferred_bill_names_where_its_generation_was_made() {
        let finish = json!({ "providerMetadata": { "gateway": { "generationId": GENERATION } } });
        let mut reader = SseReader::new().from_origin("http://127.0.0.1:4100");
        reader
            .push(
                billed(stamped(), finish).as_bytes(),
                &mut Collected::default(),
            )
            .expect("decoded");
        assert_eq!(
            reader.finish().expect("finish").billing,
            deferred(GENERATION, "http://127.0.0.1:4100")
        );
    }

    /// A bill with no time it can be placed at is not one; its generation is
    /// still asked about, and the Gateway's answer carries its own time.
    #[test]
    fn a_bill_needs_one_readable_time() {
        let unbilled = deferred(GENERATION, "");
        // No `response-metadata` at all.
        let body = billed(json!({ "type": "text-start", "id": "t" }), json!({}));
        assert_eq!(decode(&body).0.expect("finish").billing, unbilled);
        // Two that disagree.
        let body = format!(
            "data: {}\n\n{}",
            json!({ "type": "response-metadata", "timestamp": "2026-10-07T00:00:00Z" }),
            billed(stamped(), json!({}))
        );
        assert_eq!(decode(&body).0.expect("finish").billing, unbilled);
        // One that is not a UTC timestamp.
        let body = billed(
            json!({ "type": "response-metadata", "timestamp": "2026-10-07 01:02:03" }),
            json!({}),
        );
        assert_eq!(decode(&body).0.expect("finish").billing, unbilled);
    }

    #[test]
    fn two_different_generation_ids_in_one_stream_are_not_billed() {
        let body = format!(
            "data: {}\n\n{}",
            json!({ "type": "text-start", "id": "t",
                "providerMetadata": { "gateway": { "generationId": "gen_01JABCDEFGHJKMNPQRSTVWXYZ9" } } }),
            billed(stamped(), json!({}))
        );
        assert_eq!(decode(&body).0.expect("finish").billing, None);
    }

    #[test]
    fn unreported_reasoning_is_not_nought() {
        let finish = json!({ "usage": {
            "inputTokens": { "total": 12000, "cacheRead": 4000 },
            "outputTokens": { "total": 800, "reasoning": null } } });
        let billing = exact(&decode(&billed(stamped(), finish)).0.expect("finish"));
        assert_eq!(billing.reasoning_tokens, None);
    }

    #[test]
    fn gateway_timestamps_are_utc_and_strict() {
        assert_eq!(
            parse_gateway_timestamp("2026-04-01T12:11:07Z"),
            Some(1_775_045_467_000)
        );
        assert_eq!(
            parse_gateway_timestamp("2026-04-01T12:11:07.5Z"),
            Some(1_775_045_467_500)
        );
        assert_eq!(
            parse_gateway_timestamp("2026-04-01T12:11:07.123456789Z"),
            Some(1_775_045_467_123)
        );
        assert_eq!(
            parse_gateway_timestamp("2024-02-29T00:00:00Z"),
            Some(1_709_164_800_000)
        );
        for bad in [
            "2026-04-01T12:11:07",
            "2026-04-01T12:11:07+00:00",
            "2026-02-30T00:00:00Z",
            "2026-04-01T24:00:00Z",
            "2026-04-01T12:11:07.Z",
            "2026-04-01T12:11:07.1234567890Z",
            "1969-12-31T23:59:59Z",
        ] {
            assert_eq!(parse_gateway_timestamp(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_trailing_event_without_a_newline_is_still_decoded() {
        let (completion, deltas) = decode(
            "data: {\"type\":\"text-delta\",\"id\":\"t\",\"delta\":\"x\"}\n\n\
             data: {\"type\":\"finish\",\"finishReason\":{\"unified\":\"stop\"}}",
        );
        assert_eq!(deltas, ["x"]);
        assert_eq!(
            completion.expect("finish").finish_reason,
            FinishReason::Stop
        );
    }

    #[test]
    fn a_data_line_without_the_required_space_is_ignored() {
        // Upstream requires `data: ` exactly (`client.zig:2652-2653`); accepting
        // a second spelling here would decode frames upstream drops.
        let (completion, _) = decode(
            "data:{\"type\":\"finish\",\"finishReason\":{\"unified\":\"stop\"}}\n\n\
             data: {\"type\":\"finish\",\"finishReason\":{\"unified\":\"length\"}}\n\n",
        );
        assert_eq!(
            completion.expect("finish").finish_reason,
            FinishReason::Length
        );
    }

    #[test]
    fn a_bare_done_line_does_not_complete_a_stream() {
        let (completion, _) = decode("DONE\n\n");
        assert!(matches!(completion, Err(SseError::MissingFinish)));
    }

    #[test]
    fn a_sink_failure_stops_the_decode() {
        struct Broken;
        impl DeltaSink for Broken {
            fn text_delta(&mut self, _text: &str) -> io::Result<()> {
                Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed"))
            }
        }
        let mut reader = SseReader::new();
        let err = reader
            .push(
                b"data: {\"type\":\"text-delta\",\"id\":\"t\",\"delta\":\"x\"}\n\n",
                &mut Broken,
            )
            .expect_err("a closed reader must stop the stream");
        assert!(matches!(err, SseError::Sink(_)));
    }

    #[test]
    fn a_negative_or_non_integer_token_total_is_reported_as_absent() {
        // Upstream rejects a negative or non-integer total rather than coercing
        // it (`client.zig:2485-2492`).
        let event = json!({
            "type": "finish",
            "finishReason": { "unified": "stop" },
            "usage": {
                "inputTokens": { "total": -1 },
                "outputTokens": { "total": "5" },
            },
        });
        let (completion, _) = decode(&format!("data: {event}\n\n"));
        let usage = completion.expect("finish").usage;
        assert_eq!(usage.input_tokens, None);
        assert_eq!(usage.output_tokens, None);
    }

    /// Every stream a decoder told its sink about, apart.
    #[derive(Default)]
    struct Streams {
        text: Vec<String>,
        reasoning: Vec<String>,
        tool_input: Vec<String>,
    }

    impl DeltaSink for Streams {
        fn text_delta(&mut self, text: &str) -> io::Result<()> {
            self.text.push(text.to_string());
            Ok(())
        }

        fn reasoning_delta(&mut self, text: &str) {
            self.reasoning.push(text.to_string());
        }

        fn tool_input_delta(&mut self, text: &str) {
            self.tool_input.push(text.to_string());
        }
    }

    #[test]
    fn reasoning_and_an_opened_calls_arguments_are_told_to_be_counted_and_nothing_else() {
        // What a request wrote that nobody is shown, for the estimate a turn's
        // summary falls back on (`crate::agent::tokens`): a `reasoning-delta`'s
        // text -- an empty one is not text (`client.zig:3705-3711`) -- and the
        // arguments of a call that was opened. A ghost id's are dropped here as
        // they are from the call (`client.zig:3746-3760`).
        let body = [
            json!({ "type": "reasoning-start", "id": "r" }),
            json!({ "type": "reasoning-delta", "id": "r", "delta": "" }),
            json!({ "type": "reasoning-delta", "id": "r", "delta": "let me look" }),
            json!({ "type": "reasoning-end", "id": "r" }),
            json!({ "type": "tool-input-delta", "id": "ghost", "delta": "{\"a\":1}" }),
            json!({ "type": "tool-input-start", "id": "c1", "toolName": "read_file" }),
            json!({ "type": "tool-input-delta", "id": "c1", "delta": "{\"path\":" }),
            json!({ "type": "tool-input-delta", "id": "c1", "delta": "\"a.md\"}" }),
            json!({ "type": "tool-call", "toolCallId": "c1", "toolName": "read_file" }),
            json!({ "type": "finish", "finishReason": { "unified": "tool-calls" } }),
        ]
        .iter()
        .map(|event| format!("data: {event}\n\n"))
        .collect::<String>();
        let mut streams = Streams::default();
        let mut reader = SseReader::new();
        reader.push(body.as_bytes(), &mut streams).expect("decodes");
        let completion = reader.finish().expect("finish");
        assert_eq!(completion.tool_calls[0].input, json!({ "path": "a.md" }));
        assert!(completion.text.is_empty() && streams.text.is_empty());
        assert_eq!(streams.reasoning, ["let me look"]);
        assert_eq!(streams.tool_input, ["{\"path\":", "\"a.md\"}"]);
    }

    #[test]
    fn a_streamed_input_delta_for_an_unopened_call_is_dropped() {
        let body = format!(
            "data: {}\n\ndata: {}\n\ndata: {}\n\n",
            json!({ "type": "tool-input-delta", "id": "ghost", "delta": "{\"a\":1}" }),
            json!({ "type": "tool-call", "toolCallId": "ghost", "toolName": "t" }),
            json!({ "type": "finish", "finishReason": { "unified": "tool-calls" } }),
        );
        let completion = decode(&body).0.expect("finish");
        assert_eq!(completion.tool_calls[0].input, json!({}));
    }

    #[test]
    fn an_unparsable_streamed_input_is_rejected_rather_than_guessed() {
        let body = format!(
            "data: {}\n\ndata: {}\n\ndata: {}\n\n",
            json!({ "type": "tool-input-start", "id": "c1", "toolName": "t" }),
            json!({ "type": "tool-input-delta", "id": "c1", "delta": "{\"path\":" }),
            json!({ "type": "tool-call", "toolCallId": "c1" }),
        );
        assert!(matches!(
            decode(&body).0,
            Err(SseError::InvalidToolCall { .. })
        ));
    }

    #[test]
    fn a_non_json_string_input_is_kept_as_a_string_value() {
        let body = format!(
            "data: {}\n\ndata: {}\n\n",
            json!({ "type": "tool-call", "toolCallId": "c1", "toolName": "t", "input": "not json" }),
            json!({ "type": "finish", "finishReason": { "unified": "tool-calls" } }),
        );
        let completion = decode(&body).0.expect("finish");
        assert_eq!(completion.tool_calls[0].input, json!("not json"));
    }

    #[test]
    fn a_provider_error_string_is_captured() {
        let body = format!(
            "data: {}\n\n",
            json!({ "type": "error", "error": "rate limited" })
        );
        match decode(&body).0 {
            Err(SseError::ProviderFailure { detail, .. }) => assert_eq!(detail, "rate limited"),
            other => panic!("expected a provider failure, got {other:?}"),
        }
    }

    #[test]
    fn the_ceiling_is_enforced_across_pushes_not_within_one() {
        let mut reader = SseReader::new();
        let mut deltas = Collected::default();
        let half = vec![b'x'; MAX_EVENT_BYTES / 2];
        reader.push(&half, &mut deltas).expect("half fits");
        reader
            .push(&half, &mut deltas)
            .expect("the rest fits exactly");
        assert!(matches!(
            reader.push(b"x", &mut deltas),
            Err(SseError::EventTooLarge { .. })
        ));
    }
}
