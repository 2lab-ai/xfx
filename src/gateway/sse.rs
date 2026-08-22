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
use crate::gateway::protocol::{Completion, FinishReason, ToolCall, Usage};

/// The largest single SSE event xfx will buffer, in bytes.
pub const MAX_EVENT_BYTES: usize = 32 * 1024 * 1024;

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
    ProviderFailure { detail: String },
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
            Self::ProviderFailure { detail } => write!(f, "the provider failed: {detail}"),
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
    complete: bool,
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
            complete: false,
            cancel,
        }
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
            }),
            // The provider said why it failed, so report that rather than the
            // generic truncation.
            None => match self.provider_detail {
                Some(detail) => Err(SseError::ProviderFailure { detail }),
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

        match event_type.as_str() {
            "text-delta" => self.on_text_delta(&event, deltas),
            "tool-input-start" => {
                self.on_tool_input_start(&event);
                Ok(())
            }
            "tool-input-delta" => {
                self.on_tool_input_delta(&event);
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
            // `tool-input-end`, `text-start`, `reasoning-*`, `start-step` and
            // every future event carry nothing xfx acts on yet.
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

    fn on_tool_input_delta(&mut self, event: &Map<String, Value>) {
        let Some(id) = string_field(event, "id") else {
            return;
        };
        let Some(Value::String(delta)) = event.get("delta") else {
            return;
        };
        // A delta for an unannounced id is dropped; correlating it would invent
        // a call the provider never opened.
        if let Some(record) = self.streamed.iter_mut().find(|record| record.id == id) {
            record.arguments.push_str(delta);
        }
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
            Err(SseError::ProviderFailure { detail }) => assert_eq!(detail, "rate limited"),
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
