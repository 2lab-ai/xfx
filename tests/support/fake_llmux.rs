//! A local stand-in for a llmux daemon, and the Anthropic frames it sends.
//!
//! Two things live here. The frame builders below render the exact
//! `event: <name>\ndata: <json>\n\n` shape a real daemon writes, so a decoder
//! test reads like the wire it is about. The server is a sibling of
//! `fake_gateway`: it binds an ephemeral loopback port, records every request
//! verbatim, and answers by *path* rather than in script order, because
//! `xfx setup llmux` makes two probe requests before it makes any other kind.
//!
//! Nothing here holds or expects a credential. That is the point of the backend
//! under test: a keyless loopback request is what llmux accepts, so a fake that
//! demanded a key would be testing something xfx must never send.

use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// Anthropic SSE body construction
// ---------------------------------------------------------------------------

/// One named SSE frame, in the shape the live daemon writes it.
pub fn anthropic_event(name: &str, payload: Value) -> String {
    format!("event: {name}\ndata: {payload}\n\n")
}

/// The `message_start` frame, which is where the input token count arrives.
pub fn anthropic_start(model: &str, input_tokens: u64) -> String {
    anthropic_event(
        "message_start",
        json!({
            "type": "message_start",
            "message": {
                "id": "msg_fake",
                "type": "message",
                "role": "assistant",
                "model": model,
                "content": [],
                "stop_reason": null,
                "usage": { "input_tokens": input_tokens, "output_tokens": 1 },
            },
        }),
    )
}

/// The `message_delta` frame that names the stop reason and the output tokens.
///
/// This is the frame that decides whether a stream was an answer: `message_stop`
/// says the stream ended, and only this one says why the model stopped.
pub fn anthropic_stop(reason: &str, input_tokens: u64, output_tokens: u64) -> String {
    anthropic_event(
        "message_delta",
        json!({
            "type": "message_delta",
            "delta": { "stop_reason": reason, "stop_sequence": null },
            "usage": { "input_tokens": input_tokens, "output_tokens": output_tokens },
        }),
    )
}

/// One text block: start, one delta per fragment, stop.
pub fn anthropic_text_block(index: u64, fragments: &[&str]) -> String {
    let mut out = anthropic_event(
        "content_block_start",
        json!({
            "type": "content_block_start",
            "index": index,
            "content_block": { "type": "text", "text": "" },
        }),
    );
    for fragment in fragments {
        out.push_str(&anthropic_event(
            "content_block_delta",
            json!({
                "type": "content_block_delta",
                "index": index,
                "delta": { "type": "text_delta", "text": fragment },
            }),
        ));
    }
    out.push_str(&anthropic_event(
        "content_block_stop",
        json!({ "type": "content_block_stop", "index": index }),
    ));
    out
}

/// One `tool_use` block whose input arrives as JSON fragments.
pub fn anthropic_tool_block(index: u64, id: &str, name: &str, fragments: &[&str]) -> String {
    let mut out = anthropic_event(
        "content_block_start",
        json!({
            "type": "content_block_start",
            "index": index,
            "content_block": { "type": "tool_use", "id": id, "name": name },
        }),
    );
    for fragment in fragments {
        out.push_str(&anthropic_event(
            "content_block_delta",
            json!({
                "type": "content_block_delta",
                "index": index,
                "delta": { "type": "input_json_delta", "partial_json": fragment },
            }),
        ));
    }
    out.push_str(&anthropic_event(
        "content_block_stop",
        json!({ "type": "content_block_stop", "index": index }),
    ));
    out
}

/// A complete content-only answer, from `message_start` to `message_stop`.
pub fn anthropic_answer(fragments: &[&str]) -> String {
    let mut out = anthropic_start("claude-fake", 3);
    out.push_str(&anthropic_text_block(0, fragments));
    out.push_str(&anthropic_stop("end_turn", 3, 5));
    out.push_str(&anthropic_event(
        "message_stop",
        json!({ "type": "message_stop" }),
    ));
    out
}

/// A complete answer whose only content is one tool call.
pub fn anthropic_tool_answer(id: &str, name: &str, input: &str) -> String {
    let mut out = anthropic_start("claude-fake", 3);
    out.push_str(&anthropic_tool_block(0, id, name, &[input]));
    out.push_str(&anthropic_stop("tool_use", 3, 5));
    out.push_str(&anthropic_event(
        "message_stop",
        json!({ "type": "message_stop" }),
    ));
    out
}

/// The `error` frame a daemon sends inside a 200 response.
pub fn anthropic_error(message: &str) -> String {
    anthropic_event(
        "error",
        json!({
            "type": "error",
            "error": { "type": "api_error", "message": message },
        }),
    )
}
