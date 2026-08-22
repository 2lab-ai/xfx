//! A bounded decoder for the Anthropic `text/event-stream` response.
//!
//! A sibling of [`crate::gateway::sse`], not a variant of it. The two streams
//! disagree about almost everything that matters: the Gateway sends one flat
//! `tool-call` event, Anthropic opens an indexed content block and streams its
//! arguments in as `partial_json`; the Gateway's `finish` carries the reason and
//! the totals together, Anthropic splits them across `message_delta` and
//! `message_stop`. Contorting one decoder to answer both would make every branch
//! in it conditional on a wire nobody reading it can see.
//!
//! What *is* shared is the discipline, deliberately, because these are the rules
//! that make a decoder safe rather than merely convenient:
//!
//! 1. **A stop reason is required.** End of stream does not prove the model
//!    finished, and neither does `message_stop`: only `message_delta` says why
//!    generation ended. A truncated stream is a failure, not a short answer.
//! 2. **Both the frame and the answer are bounded** -- one frame by
//!    [`MAX_EVENT_BYTES`], the whole accumulation by [`MAX_COMPLETION_BYTES`].
//!    The second is not implied by the first: a stream of well-formed small
//!    frames grows without limit.
//! 3. **An unknown terminal state is an error.** A future `stop_reason` mapped
//!    onto a known one would report a refusal, a pause, or a quota stop as a
//!    normal completion.
//! 4. **A tool round is one claim.** `stop_reason: "tool_use"` and the calls it
//!    refers to stand or fall together: a stream that says it stopped to call a
//!    tool and names none, or opens a `tool_use` block xfx cannot close, is
//!    refused rather than handed on as a completion with an empty call list.
//!
//! And one rule this stream needs that the Gateway's does not: an `error` frame
//! arrives inside an HTTP 200
//! (`2lab-ai/llmux@79f66748656b src/provider/responses.rs:569-583`),
//! so a transport that only read the status would call an upstream failure a
//! successful empty answer. It fails **at the frame**, not at the end, because
//! consulting it later made the verdict depend on what happened to arrive
//! afterwards.
//!
//! A malformed nonterminal event is skipped rather than fatal, matching the
//! Gateway decoder; a malformed terminal one is fatal, because that is the one
//! event whose meaning cannot be guessed.

use serde_json::{Map, Value};

use crate::gateway::protocol::{Completion, FinishReason, ToolCall, Usage};
use crate::gateway::sse::{SseError, MAX_COMPLETION_BYTES, MAX_EVENT_BYTES};
use crate::gateway::{CancelToken, DeltaSink};

/// The most content blocks a single message may have open or tracked at once.
///
/// A stream of `content_block_start` frames with distinct indexes grows the
/// block map without bound, and their ids and names are strings the daemon
/// chooses. Real messages use single digits; 64 is far past anything Anthropic
/// emits and far short of a number that costs a machine anything.
pub const MAX_OPEN_BLOCKS: usize = 64;

/// What an open content block is accumulating.
#[derive(Debug)]
enum Block {
    /// A text block. Its deltas are the answer.
    Text,
    /// A tool call whose arguments are arriving as JSON fragments.
    ToolUse {
        id: String,
        name: String,
        arguments: String,
    },
    /// A block this version does not act on -- `thinking`, `redacted_thinking`,
    /// or something newer. Tracked rather than ignored so that its deltas are
    /// dropped on purpose instead of being mistaken for another block's.
    ///
    /// Dropping thinking is sound because the request declares
    /// `thinking: {"type":"disabled"}` (`crate::llmux::protocol`), so a
    /// conforming daemon cannot send one at all; this arm is what keeps a
    /// *non*conforming one from splicing reasoning into the answer.
    Opaque,
}

/// One open content block and the index the stream addresses it by.
#[derive(Debug)]
struct OpenBlock {
    index: u64,
    block: Block,
}

/// Decodes an Anthropic SSE response into one [`Completion`].
#[derive(Debug)]
pub struct AnthropicReader {
    /// The bytes of the current, not-yet-terminated line.
    line: Vec<u8>,
    /// The name from the most recent `event:` line, if there was one.
    ///
    /// The payload carries its own `type` and that is what dispatch keys off,
    /// so this is only consulted for a frame whose data says nothing -- which is
    /// how an `error` frame can still be recognized.
    pending_event: Option<String>,
    text: String,
    blocks: Vec<OpenBlock>,
    tool_calls: Vec<ToolCall>,
    /// Bytes counted against [`MAX_COMPLETION_BYTES`]: text plus tool arguments.
    accumulated: usize,
    finish_reason: Option<FinishReason>,
    usage: Usage,
    complete: bool,
    cancel: CancelToken,
}

impl Default for AnthropicReader {
    fn default() -> Self {
        Self::new()
    }
}

impl AnthropicReader {
    pub fn new() -> Self {
        Self::with_cancel(CancelToken::new())
    }

    /// A reader that stops at the next frame boundary once `cancel` is set.
    pub fn with_cancel(cancel: CancelToken) -> Self {
        Self {
            line: Vec::new(),
            pending_event: None,
            text: String::new(),
            blocks: Vec::new(),
            tool_calls: Vec::new(),
            accumulated: 0,
            finish_reason: None,
            usage: Usage::default(),
            complete: false,
            cancel,
        }
    }

    /// Whether `message_stop` has arrived.
    ///
    /// The transport uses this to stop reading: once the message has ended,
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

        // An open `tool_use` block at end of stream was never closed, so its
        // arguments are half a JSON document xfx must not guess at.
        self.refuse_open_tool_block("the stream ended while it was still open")?;
        if self.finish_reason == Some(FinishReason::ToolCalls) && self.tool_calls.is_empty() {
            return Err(SseError::InvalidToolCall {
                detail: "the daemon stopped to call a tool and named none".to_string(),
            });
        }

        match self.finish_reason {
            Some(finish_reason) => Ok(Completion {
                text: self.text,
                tool_calls: self.tool_calls,
                finish_reason,
                usage: self.usage,
                // Never set on this wire: an `error` frame fails the decode
                // where it arrives, so a stream that reaches here reported no
                // failure to carry.
                provider_detail: None,
            }),
            None => Err(SseError::MissingFinish),
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
        if let Some(name) = line.strip_prefix("event:") {
            self.pending_event = Some(name.trim().to_string());
            return Ok(());
        }
        let Some(payload) = line.strip_prefix("data:") else {
            return Ok(());
        };
        // The SSE field grammar strips one optional leading space after the
        // colon, and llmux writes that space. The rest is trimmed as well: the
        // live daemon pads every frame with trailing spaces after the JSON
        // document, and while `serde_json` happens to tolerate that today, the
        // padding is not part of the value and this decoder should not depend on
        // a parser's tolerance to say so.
        let payload = payload.strip_prefix(' ').unwrap_or(payload).trim();
        let event_name = self.pending_event.take();

        // A nonterminal event that cannot be parsed is skipped, not fatal --
        // except that a frame the daemon *named* an error still fails the
        // stream, because that is the one name whose meaning cannot be guessed
        // from a body xfx could not read.
        let Ok(Value::Object(event)) = serde_json::from_str::<Value>(payload) else {
            if event_name.as_deref() == Some("error") {
                return Err(SseError::ProviderFailure {
                    detail: "the daemon reported an unreadable error".to_string(),
                    // Unreadable means unclassifiable, and replaying a failure
                    // that might have been a rejected request is the expensive
                    // direction to be wrong in.
                    retryable: false,
                });
            }
            return Ok(());
        };
        let event_type = match event.get("type") {
            Some(Value::String(kind)) => kind.as_str(),
            _ => event_name.as_deref().unwrap_or_default(),
        };

        match event_type {
            "message_start" => {
                self.on_message_start(&event);
                Ok(())
            }
            "content_block_start" => self.on_block_start(&event),
            "content_block_delta" => self.on_block_delta(&event, deltas),
            "content_block_stop" => self.on_block_stop(&event),
            "message_delta" => self.on_message_delta(&event),
            "message_stop" => {
                self.complete = true;
                Ok(())
            }
            // The frame *is* the failure. Recording it and deciding later made
            // the verdict depend on what arrived afterwards: a daemon that
            // reported an error and then closed cleanly produced a successful
            // completion carrying a truncated answer.
            "error" => Err(SseError::ProviderFailure {
                detail: failure_detail(&event)
                    .unwrap_or_else(|| "the daemon reported an error".to_string()),
                retryable: is_transient_error(&event),
            }),
            // `ping` and every future event carry nothing xfx acts on.
            _ => Ok(()),
        }
    }

    /// The input token count, which arrives before any content.
    fn on_message_start(&mut self, event: &Map<String, Value>) {
        let Some(Value::Object(message)) = event.get("message") else {
            return;
        };
        let Some(Value::Object(usage)) = message.get("usage") else {
            return;
        };
        if let Some(total) = input_total(usage) {
            self.usage.input_tokens = Some(total);
        }
    }

    fn on_block_start(&mut self, event: &Map<String, Value>) -> Result<(), SseError> {
        let Some(index) = event.get("index").and_then(Value::as_u64) else {
            return Ok(());
        };
        let kind = match event.get("content_block") {
            Some(Value::Object(block)) => block,
            _ => return Ok(()),
        };
        // A flood of distinct indexes would grow this map without bound, and the
        // ids and names inside are strings the daemon chooses.
        if self.blocks.len() >= MAX_OPEN_BLOCKS
            && !self.blocks.iter().any(|open| open.index == index)
        {
            return Err(SseError::CompletionTooLarge {
                limit: MAX_COMPLETION_BYTES,
            });
        }
        let block = match kind.get("type").and_then(Value::as_str) {
            Some("text") => Block::Text,
            Some("tool_use") => {
                // A tool call with no identity cannot be correlated to a result,
                // so it is tracked as opaque rather than invented.
                match (string_field(kind, "id"), string_field(kind, "name")) {
                    (Some(id), Some(name)) => {
                        self.charge(id.len() + name.len())?;
                        Block::ToolUse {
                            id: id.to_string(),
                            name: name.to_string(),
                            arguments: String::new(),
                        }
                    }
                    _ => Block::Opaque,
                }
            }
            _ => Block::Opaque,
        };
        // A reused index replaces the block it addresses: the stream owns the
        // numbering, and keeping a stale entry would misroute later deltas.
        self.blocks.retain(|open| open.index != index);
        self.blocks.push(OpenBlock { index, block });
        Ok(())
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

    fn on_block_delta(
        &mut self,
        event: &Map<String, Value>,
        deltas: &mut dyn DeltaSink,
    ) -> Result<(), SseError> {
        let Some(index) = event.get("index").and_then(Value::as_u64) else {
            return Ok(());
        };
        let Some(Value::Object(delta)) = event.get("delta") else {
            return Ok(());
        };
        match delta.get("type").and_then(Value::as_str) {
            Some("text_delta") => {
                let Some(text) = string_field(delta, "text") else {
                    return Ok(());
                };
                // Routed by index, not accepted on sight. `Block::Text` and
                // `Block::Opaque` were tracked and never consulted, so a
                // `text_delta` naming a thinking block -- or naming nothing --
                // was spliced into the assistant's reply. Dropping it is the
                // documented meaning of an opaque block: xfx tracks the index
                // precisely so that its deltas go nowhere. Dropping rather than
                // failing, because a nonconforming daemon sending reasoning
                // this way is a stream xfx can still answer from correctly,
                // and refusing the turn would be a harsher response to the
                // daemon's mistake than the operator's problem warrants.
                if !matches!(
                    self.blocks.iter().find(|open| open.index == index),
                    Some(OpenBlock {
                        block: Block::Text,
                        ..
                    })
                ) {
                    return Ok(());
                }
                self.charge(text.len())?;
                self.text.push_str(text);
                // Handed to the sink borrowed: this is the hot path of every
                // streamed answer, and a fresh allocation per token buys nothing.
                deltas.text_delta(text).map_err(SseError::Sink)
            }
            Some("input_json_delta") => {
                let Some(fragment) = string_field(delta, "partial_json") else {
                    return Ok(());
                };
                self.charge(fragment.len())?;
                // A fragment for an unopened block is dropped: correlating it
                // would invent a call the daemon never opened.
                if let Some(OpenBlock {
                    block: Block::ToolUse { arguments, .. },
                    ..
                }) = self.blocks.iter_mut().find(|open| open.index == index)
                {
                    arguments.push_str(fragment);
                }
                Ok(())
            }
            // `thinking_delta` and `signature_delta` are reasoning, not the
            // answer, and xfx does not render them.
            _ => Ok(()),
        }
    }

    fn on_block_stop(&mut self, event: &Map<String, Value>) -> Result<(), SseError> {
        let Some(index) = event.get("index").and_then(Value::as_u64) else {
            // A stop frame xfx cannot correlate is harmless while nothing is
            // being assembled, and a silent lie while a tool call is: the block
            // would be dropped, the daemon would still say it stopped to call a
            // tool, and the turn would be handed `ToolCalls` with no calls in it.
            return self.refuse_open_tool_block("a `content_block_stop` carried no usable `index`");
        };
        let Some(position) = self.blocks.iter().position(|open| open.index == index) else {
            return Ok(());
        };
        let closed = self.blocks.remove(position);
        let Block::ToolUse {
            id,
            name,
            arguments,
        } = closed.block
        else {
            return Ok(());
        };

        if self.tool_calls.iter().any(|call| call.id == id) {
            return Err(SseError::DuplicateToolCallId { call_id: id });
        }
        // A tool whose schema needs no arguments streams no fragments at all, so
        // "nothing arrived" is an empty object rather than a failure. Anything
        // that did arrive has to parse: guessing at half a JSON document would
        // run a tool with arguments the model did not send.
        let input = if arguments.trim().is_empty() {
            Value::Object(Map::new())
        } else {
            serde_json::from_str(&arguments).map_err(|err| SseError::InvalidToolCall {
                detail: format!("tool call `{id}` streamed unusable input: {err}"),
            })?
        };
        self.tool_calls.push(ToolCall { id, name, input });
        Ok(())
    }

    /// Fails the decode when a tool block is open and cannot be closed.
    ///
    /// Nothing to assemble means nothing to lie about, so this is a no-op unless
    /// a `tool_use` block is still open.
    fn refuse_open_tool_block(&self, reason: &str) -> Result<(), SseError> {
        let open = self.blocks.iter().find_map(|open| match &open.block {
            Block::ToolUse { id, .. } => Some(id.as_str()),
            _ => None,
        });
        match open {
            Some(id) => Err(SseError::InvalidToolCall {
                detail: format!("tool call `{id}` was never closed: {reason}"),
            }),
            None => Ok(()),
        }
    }

    fn on_message_delta(&mut self, event: &Map<String, Value>) -> Result<(), SseError> {
        if let Some(Value::Object(usage)) = event.get("usage") {
            if let Some(total) = token_total(usage, "output_tokens") {
                self.usage.output_tokens = Some(total);
            }
            // The input total belongs to `message_start`: the prompt was already
            // sent by then, and a `message_delta` that restates `input_tokens`
            // without the cache counters is the same prompt described with less
            // detail. Letting the last word win turned a cache-inclusive 4035
            // back into 10.
            if self.usage.input_tokens.is_none() {
                self.usage.input_tokens = input_total(usage);
            }
        }
        let Some(Value::Object(delta)) = event.get("delta") else {
            return Ok(());
        };
        match delta.get("stop_reason") {
            // Anthropic sends `"stop_reason": null` while generation continues,
            // which is not a terminal state and not a malformed one.
            None | Some(Value::Null) => Ok(()),
            Some(Value::String(raw)) => match parse_stop_reason(raw) {
                Some(reason) => {
                    self.finish_reason = Some(reason);
                    Ok(())
                }
                None => Err(SseError::UnknownFinishReason { raw: raw.clone() }),
            },
            Some(other) => Err(SseError::InvalidFinishReason {
                detail: format!("`delta.stop_reason` is not a string: {other}"),
            }),
        }
    }
}

/// Anthropic's stop vocabulary, mapped onto the unified one.
///
/// Closed on purpose, like [`FinishReason::parse_unified`]: an unrecognized
/// value is refused rather than folded into `Other`, because the values that do
/// not exist yet are exactly the ones that mean something unusual happened.
fn parse_stop_reason(raw: &str) -> Option<FinishReason> {
    match raw {
        "end_turn" | "stop_sequence" => Some(FinishReason::Stop),
        "max_tokens" => Some(FinishReason::Length),
        "tool_use" => Some(FinishReason::ToolCalls),
        "refusal" => Some(FinishReason::ContentFilter),
        _ => None,
    }
}

/// A sink for text that has nowhere left to go.
struct DiscardDeltas;

impl DeltaSink for DiscardDeltas {
    fn text_delta(&mut self, _text: &str) -> std::io::Result<()> {
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

/// Whether an `error` frame describes a condition worth another attempt.
///
/// Anthropic's transient family, mapped onto the same verdict the Gateway path
/// reaches when the identical upstream condition arrives as a 429 or a 5xx. A
/// type this version does not recognize is **not** replayable: an unknown error
/// might be a rejected request, and paying for it twice is the expensive
/// direction to be wrong in. The turn still refuses to replay anything once a
/// delta has been delivered, so this only widens what may be retried from a
/// clean start.
fn is_transient_error(event: &Map<String, Value>) -> bool {
    let kind = event
        .get("error")
        .and_then(Value::as_object)
        .and_then(|error| error.get("type"))
        .and_then(Value::as_str);
    matches!(
        kind,
        Some("overloaded_error" | "rate_limit_error" | "api_error")
    )
}

/// The daemon's own description of a failure, if it gave one.
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

/// One token total, when the provider reported a readable one.
///
/// A negative or non-integer value reads as absent rather than as zero, matching
/// the Gateway decoder: "the provider did not say" and "the provider said zero"
/// are different facts.
fn token_total(usage: &Map<String, Value>, key: &str) -> Option<u64> {
    usage.get(key)?.as_u64()
}

/// Everything the prompt was billed as input.
///
/// Anthropic splits the input across three counters, and a cached prompt reports
/// almost all of it under the cache ones: reading `input_tokens` alone reported
/// ten tokens for a turn that was billed four thousand, and that number is what
/// a session totals up and shows the operator. Absent counters contribute
/// nothing rather than making the whole total absent, and the addition
/// saturates, because a total that wrapped would be worse than one that is
/// merely large.
fn input_total(usage: &Map<String, Value>) -> Option<u64> {
    let counted: Vec<u64> = [
        "input_tokens",
        "cache_read_input_tokens",
        "cache_creation_input_tokens",
    ]
    .iter()
    .filter_map(|key| token_total(usage, key))
    .collect();
    if counted.is_empty() {
        return None;
    }
    Some(
        counted
            .iter()
            .fold(0u64, |sum, part| sum.saturating_add(*part)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn decode(body: &str) -> Result<Completion, SseError> {
        let mut reader = AnthropicReader::new();
        let mut discard = DiscardDeltas;
        reader.push(body.as_bytes(), &mut discard)?;
        reader.finish()
    }

    #[test]
    fn the_anthropic_stop_vocabulary_is_closed() {
        assert_eq!(parse_stop_reason("end_turn"), Some(FinishReason::Stop));
        assert_eq!(parse_stop_reason("stop_sequence"), Some(FinishReason::Stop));
        assert_eq!(parse_stop_reason("max_tokens"), Some(FinishReason::Length));
        assert_eq!(parse_stop_reason("tool_use"), Some(FinishReason::ToolCalls));
        assert_eq!(
            parse_stop_reason("refusal"),
            Some(FinishReason::ContentFilter)
        );
        // The Gateway's own spellings are a different vocabulary.
        assert_eq!(parse_stop_reason("tool-calls"), None);
        assert_eq!(parse_stop_reason("stop"), None);
        assert_eq!(parse_stop_reason(""), None);
    }

    #[test]
    fn a_negative_or_non_integer_token_total_reads_as_absent() {
        let body = format!(
            "data: {}\n\ndata: {}\n\n",
            json!({
                "type": "message_start",
                "message": { "usage": { "input_tokens": -1 } },
            }),
            json!({
                "type": "message_delta",
                "delta": { "stop_reason": "end_turn" },
                "usage": { "output_tokens": "5" },
            }),
        );
        let usage = decode(&body).expect("finish").usage;
        assert_eq!(usage.input_tokens, None);
        assert_eq!(usage.output_tokens, None);
    }

    #[test]
    fn a_stop_reason_that_is_not_a_string_is_refused_rather_than_ignored() {
        let body = format!(
            "data: {}\n\n",
            json!({ "type": "message_delta", "delta": { "stop_reason": 7 } })
        );
        assert!(matches!(
            decode(&body),
            Err(SseError::InvalidFinishReason { .. })
        ));
    }

    #[test]
    fn a_null_stop_reason_is_not_a_terminal_state() {
        let body = format!(
            "data: {}\n\n",
            json!({ "type": "message_delta", "delta": { "stop_reason": null } })
        );
        assert!(matches!(decode(&body), Err(SseError::MissingFinish)));
    }

    #[test]
    fn a_tool_block_with_no_identity_is_tracked_as_opaque_rather_than_invented() {
        let body = format!(
            "data: {}\n\ndata: {}\n\ndata: {}\n\n",
            json!({
                "type": "content_block_start",
                "index": 0,
                "content_block": { "type": "tool_use", "name": "read_file" },
            }),
            json!({ "type": "content_block_stop", "index": 0 }),
            json!({ "type": "message_delta", "delta": { "stop_reason": "end_turn" } }),
        );
        assert!(decode(&body).expect("finish").tool_calls.is_empty());
    }

    #[test]
    fn a_frame_the_daemon_named_an_error_fails_even_when_its_body_is_unreadable() {
        // An ordinary malformed frame is skipped; one the daemon *named* an
        // error is not, because that is the one name whose meaning cannot be
        // guessed from a body xfx could not read. It fails at the frame, so
        // nothing that arrives afterwards can turn it back into a success.
        let mut reader = AnthropicReader::new();
        let mut discard = DiscardDeltas;
        assert!(matches!(
            reader.push(b"event: error\ndata: not json\n\n", &mut discard),
            Err(SseError::ProviderFailure { .. })
        ));
    }

    #[test]
    fn a_sink_failure_stops_the_decode() {
        struct Broken;
        impl DeltaSink for Broken {
            fn text_delta(&mut self, _text: &str) -> std::io::Result<()> {
                Err(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "closed",
                ))
            }
        }
        let mut reader = AnthropicReader::new();
        // The block has to be opened first: a `text_delta` is routed by index,
        // so one naming no open text block never reaches a sink at all.
        let frames = format!(
            "data: {}\n\ndata: {}\n\n",
            json!({
                "type": "content_block_start",
                "index": 0,
                "content_block": { "type": "text", "text": "" },
            }),
            json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": { "type": "text_delta", "text": "x" },
            })
        );
        assert!(matches!(
            reader.push(frames.as_bytes(), &mut Broken),
            Err(SseError::Sink(_))
        ));
    }

    #[test]
    fn a_cancelled_reader_stops_at_the_next_frame_boundary() {
        let cancel = CancelToken::new();
        cancel.cancel();
        let mut reader = AnthropicReader::with_cancel(cancel);
        assert!(matches!(
            reader.push(b"data: {}\n\n", &mut DiscardDeltas),
            Err(SseError::Cancelled)
        ));
    }
}
