//! Gateway protocol, SSE decoding, bounded turn, and binary-level `ask`.
//!
//! Three layers are proven here, and each one is a product promise:
//!
//! 1. the exact bytes xfx sends to the Vercel AI Gateway;
//! 2. what xfx accepts back, including every way a stream can lie or stop; and
//! 3. what `xfx ask` puts on stdout, header for header and event for event.
//!
//! Nothing here uses a real credential or a real endpoint. Upstream evidence is
//! pinned to `vercel-labs/fx@580a0c5da9386317251968c09c1cee69e763487a`.

mod support;

use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tempfile::TempDir;

use xfx::agent::{run_turn, TurnError, TurnRequest};
use xfx::gateway::protocol::{
    Completion, CompletionRequest, ContentPart, FinishReason, Message, ProtocolError, Role,
    ToolCall, ToolChoice,
};
use xfx::gateway::sse::{SseError, SseReader, MAX_EVENT_BYTES};
use xfx::gateway::{CancelToken, DeltaSink, Endpoint, EndpointError, Provider, ProviderError};
use xfx::output::{Event, RecordingSink};

use support::fake_gateway::{
    content_only, finish, finish_with_usage, sse_body, sse_body_without_done, text_delta,
    tool_call, FakeGateway, Reply,
};

/// Environment variables that must never leak in from the developer's shell.
const CONTROLLED_VARS: &[&str] = &[
    "VERCEL_OIDC_TOKEN",
    "AI_GATEWAY_API_KEY",
    "XFX_MODEL",
    "XFX_PERMISSION_MODE",
    "XFX_MAX_AGENT_STEPS",
    "XFX_GATEWAY_URL",
];

/// A test secret that must never appear on stdout or stderr.
const TEST_KEY: &str = "xfx-test-gateway-key-must-not-appear";

// ---------------------------------------------------------------------------
// request serialization
// ---------------------------------------------------------------------------

fn content_only_request(prompt: &str) -> CompletionRequest {
    CompletionRequest {
        model: "vendor/model".to_string(),
        messages: vec![Message::user(prompt)],
        tools: Vec::new(),
        tool_choice: ToolChoice::None,
    }
}

#[test]
fn a_request_is_prompt_then_closed_tools_then_tool_choice() {
    let body = content_only_request("hello").body().expect("serialize");

    // Key order matches upstream's writer, which emits `prompt`, `tools`, and
    // then `toolChoice` (`src/core/gateway/gateway_json.zig:333-363`).
    assert!(body.starts_with("{\"prompt\":["), "got {body}");
    let parsed: Value = serde_json::from_str(&body).expect("body is JSON");
    assert_eq!(
        parsed,
        json!({
            "prompt": [{ "role": "user", "content": [{ "type": "text", "text": "hello" }] }],
            "tools": [],
            "toolChoice": { "type": "none" },
        }),
        "got {body}"
    );
}

#[test]
fn a_request_writes_exactly_the_tool_list_it_was_given() {
    // The transport advertises what it is handed and nothing else, so a request
    // built with no tools carries none. What a *turn* advertises is the tool
    // registry's contract, proven in `tests/tool_loop.rs`.
    let parsed: Value =
        serde_json::from_str(&content_only_request("hi").body().expect("serialize")).unwrap();
    assert_eq!(parsed["tools"], json!([]));
    assert_eq!(parsed["toolChoice"], json!({ "type": "none" }));
}

#[test]
fn tool_choice_labels_match_the_gateway_vocabulary() {
    for (choice, label) in [
        (ToolChoice::Auto, "auto"),
        (ToolChoice::None, "none"),
        (ToolChoice::Required, "required"),
    ] {
        assert_eq!(choice.label(), label);
    }
}

#[test]
fn a_system_message_serializes_as_a_bare_string_not_a_part_array() {
    // Upstream writes system content as a string and every other role as a
    // typed part array (`src/core/gateway/gateway_json.zig:552-560`).
    let request = CompletionRequest {
        model: "vendor/model".to_string(),
        messages: vec![Message::system("be terse"), Message::user("hi")],
        tools: Vec::new(),
        tool_choice: ToolChoice::None,
    };
    let parsed: Value = serde_json::from_str(&request.body().expect("serialize")).unwrap();
    assert_eq!(
        parsed["prompt"][0],
        json!({ "role": "system", "content": "be terse" })
    );
    assert_eq!(
        parsed["prompt"][1],
        json!({ "role": "user", "content": [{ "type": "text", "text": "hi" }] })
    );
}

#[test]
fn an_assistant_tool_call_and_its_result_correlate_by_call_id() {
    // Upstream shape: `tool-call` carries `toolCallId`/`toolName`/`input`, and
    // the matching `tool-result` repeats both identifiers and wraps the output
    // in `{"type":"text","value":...}`
    // (`src/core/gateway/gateway_json.zig:617-649`).
    let request = CompletionRequest {
        model: "vendor/model".to_string(),
        messages: vec![
            Message::user("read it"),
            Message::assistant(
                Some("looking"),
                vec![ToolCall {
                    id: "call_1".to_string(),
                    name: "read_file".to_string(),
                    input: json!({ "path": "README.md" }),
                }],
            ),
            Message::tool_result("call_1", "read_file", "file body"),
        ],
        tools: Vec::new(),
        tool_choice: ToolChoice::None,
    };
    let parsed: Value = serde_json::from_str(&request.body().expect("serialize")).unwrap();

    assert_eq!(
        parsed["prompt"][1],
        json!({
            "role": "assistant",
            "content": [
                { "type": "text", "text": "looking" },
                {
                    "type": "tool-call",
                    "toolCallId": "call_1",
                    "toolName": "read_file",
                    "input": { "path": "README.md" },
                },
            ],
        })
    );
    assert_eq!(
        parsed["prompt"][2],
        json!({
            "role": "tool",
            "content": [{
                "type": "tool-result",
                "toolCallId": "call_1",
                "toolName": "read_file",
                "output": { "type": "text", "value": "file body" },
            }],
        })
    );
}

#[test]
fn an_assistant_message_without_text_writes_only_its_tool_calls() {
    let request = CompletionRequest {
        model: "vendor/model".to_string(),
        messages: vec![
            Message::user("go"),
            Message::assistant(
                None,
                vec![ToolCall {
                    id: "call_1".to_string(),
                    name: "list_files".to_string(),
                    input: json!({}),
                }],
            ),
            Message::tool_result("call_1", "list_files", "[]"),
        ],
        tools: Vec::new(),
        tool_choice: ToolChoice::None,
    };
    let parsed: Value = serde_json::from_str(&request.body().expect("serialize")).unwrap();
    let parts = parsed["prompt"][1]["content"].as_array().expect("parts");
    assert_eq!(parts.len(), 1, "an empty text part must not be written");
    assert_eq!(parts[0]["type"], "tool-call");
}

#[test]
fn a_tool_result_without_a_matching_call_is_rejected_before_any_bytes_are_sent() {
    // Upstream validates tool-message history before building the body
    // (`src/core/gateway/gateway_json.zig:285`, `:497-523`). An orphan result
    // is a client bug; sending it spends a model call to learn that.
    let request = CompletionRequest {
        model: "vendor/model".to_string(),
        messages: vec![
            Message::user("go"),
            Message::tool_result("call_missing", "read_file", "x"),
        ],
        tools: Vec::new(),
        tool_choice: ToolChoice::None,
    };
    assert!(matches!(
        request.body(),
        Err(ProtocolError::UnmatchedToolResult { .. })
    ));
}

#[test]
fn duplicate_tool_call_ids_are_rejected() {
    let duplicate = ToolCall {
        id: "call_1".to_string(),
        name: "read_file".to_string(),
        input: json!({}),
    };
    let request = CompletionRequest {
        model: "vendor/model".to_string(),
        messages: vec![
            Message::user("go"),
            Message::assistant(None, vec![duplicate.clone(), duplicate]),
        ],
        tools: Vec::new(),
        tool_choice: ToolChoice::None,
    };
    assert!(matches!(
        request.body(),
        Err(ProtocolError::DuplicateToolCallId { .. })
    ));
}

#[test]
fn a_request_without_a_prompt_is_rejected() {
    let request = CompletionRequest {
        model: "vendor/model".to_string(),
        messages: Vec::new(),
        tools: Vec::new(),
        tool_choice: ToolChoice::None,
    };
    assert!(matches!(request.body(), Err(ProtocolError::EmptyPrompt)));
}

#[test]
fn a_content_part_cannot_be_placed_on_a_role_that_cannot_carry_it() {
    let request = CompletionRequest {
        model: "vendor/model".to_string(),
        messages: vec![Message {
            role: Role::User,
            content: vec![ContentPart::ToolCall(ToolCall {
                id: "call_1".to_string(),
                name: "read_file".to_string(),
                input: json!({}),
            })],
        }],
        tools: Vec::new(),
        tool_choice: ToolChoice::None,
    };
    assert!(matches!(
        request.body(),
        Err(ProtocolError::MisplacedPart { .. })
    ));
}

#[test]
fn a_prompt_is_escaped_rather_than_interpolated() {
    let body = content_only_request("quote \" newline \n done")
        .body()
        .expect("serialize");
    assert_eq!(body.lines().count(), 1, "got {body}");
    let parsed: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        parsed["prompt"][0]["content"][0]["text"],
        "quote \" newline \n done"
    );
}

// ---------------------------------------------------------------------------
// SSE decoding
// ---------------------------------------------------------------------------

/// Records the assistant text fragments a decoder emitted, in order.
#[derive(Debug, Default)]
struct RecordingDeltas {
    fragments: Vec<String>,
}

impl DeltaSink for RecordingDeltas {
    fn text_delta(&mut self, text: &str) -> io::Result<()> {
        self.fragments.push(text.to_string());
        Ok(())
    }
}

/// Feeds `body` to a decoder one byte at a time.
///
/// Transport boundaries never line up with event boundaries on a real stream,
/// so the harshest available framing is the default here.
fn decode_byte_by_byte(body: &str) -> (Result<Completion, SseError>, RecordingDeltas) {
    let mut deltas = RecordingDeltas::default();
    let mut reader = SseReader::new();
    for byte in body.as_bytes() {
        if let Err(err) = reader.push(&[*byte], &mut deltas) {
            return (Err(err), deltas);
        }
    }
    (reader.finish(), deltas)
}

#[test]
fn text_deltas_arrive_in_order_when_the_stream_is_fed_one_byte_at_a_time() {
    let body = sse_body(&[
        text_delta("t1", "Hel"),
        text_delta("t1", "lo, "),
        text_delta("t1", "world"),
        finish("stop"),
    ]);
    let (completion, deltas) = decode_byte_by_byte(&body);
    let completion = completion.expect("a canonical finish completes the stream");
    assert_eq!(deltas.fragments, ["Hel", "lo, ", "world"]);
    assert_eq!(completion.text, "Hello, world");
    assert_eq!(completion.finish_reason, FinishReason::Stop);
    assert!(completion.tool_calls.is_empty());
}

#[test]
fn a_crlf_framed_stream_decodes_identically() {
    // Upstream accepts CRLF frames (`src/gateway/client.zig:3792-3794`).
    let body = "data: {\"type\":\"text-delta\",\"id\":\"t1\",\"delta\":\"answer\"}\r\n\r\n\
                data: {\"type\":\"finish\",\"finishReason\":{\"unified\":\"stop\"}}\r\n\r\n\
                data: [DONE]\r\n\r\n";
    let (completion, deltas) = decode_byte_by_byte(body);
    assert_eq!(deltas.fragments, ["answer"]);
    assert_eq!(completion.expect("finish").text, "answer");
}

#[test]
fn usage_and_finish_reason_are_extracted_from_the_finish_event() {
    let body = sse_body(&[text_delta("t1", "x"), finish_with_usage("length", 10, 5)]);
    let completion = decode_byte_by_byte(&body).0.expect("finish");
    assert_eq!(completion.finish_reason, FinishReason::Length);
    assert_eq!(completion.usage.input_tokens, Some(10));
    assert_eq!(completion.usage.output_tokens, Some(5));
}

#[test]
fn a_finish_event_without_usage_reports_no_token_counts() {
    let body = sse_body(&[finish("stop")]);
    let completion = decode_byte_by_byte(&body).0.expect("finish");
    assert_eq!(completion.usage.input_tokens, None);
    assert_eq!(completion.usage.output_tokens, None);
}

#[test]
fn every_canonical_finish_reason_is_understood() {
    // The unified vocabulary is `src/core/shared/types.zig:927-935`.
    for (raw, expected) in [
        ("stop", FinishReason::Stop),
        ("length", FinishReason::Length),
        ("content-filter", FinishReason::ContentFilter),
        ("tool-calls", FinishReason::ToolCalls),
        ("error", FinishReason::ProviderError),
        ("other", FinishReason::Other),
    ] {
        let body = sse_body(&[finish(raw)]);
        let completion = decode_byte_by_byte(&body)
            .0
            .unwrap_or_else(|err| panic!("`{raw}` must be a canonical finish reason, got {err}"));
        assert_eq!(completion.finish_reason, expected, "for `{raw}`");
        assert_eq!(expected.label(), raw);
    }
}

#[test]
fn an_unknown_finish_reason_is_rejected_rather_than_guessed() {
    // Upstream returns InvalidProviderFinishReason for an unknown unified value
    // (`src/gateway/client.zig:3210-3217`, `:3547-3548`).
    for raw in ["", "future-reason", "tool_calls"] {
        let body = sse_body(&[finish(raw)]);
        assert!(
            matches!(
                decode_byte_by_byte(&body).0,
                Err(SseError::UnknownFinishReason { .. })
            ),
            "`{raw}` must not be accepted as a finish reason"
        );
    }
}

#[test]
fn a_finish_event_with_a_malformed_reason_is_rejected() {
    for event in [
        json!({ "type": "finish" }),
        json!({ "type": "finish", "finishReason": "stop" }),
        json!({ "type": "finish", "finishReason": { "raw": "stop" } }),
        json!({ "type": "finish", "finishReason": { "unified": 7 } }),
    ] {
        let body = sse_body(std::slice::from_ref(&event));
        assert!(
            matches!(
                decode_byte_by_byte(&body).0,
                Err(SseError::InvalidFinishReason { .. })
            ),
            "{event} must be rejected"
        );
    }
}

#[test]
fn a_direct_tool_call_event_is_decoded_with_its_input() {
    let body = sse_body(&[
        tool_call("c1", "read_file", json!({ "path": "README.md" })),
        finish("tool-calls"),
    ]);
    let completion = decode_byte_by_byte(&body).0.expect("finish");
    assert_eq!(completion.finish_reason, FinishReason::ToolCalls);
    assert_eq!(completion.tool_calls.len(), 1);
    assert_eq!(completion.tool_calls[0].id, "c1");
    assert_eq!(completion.tool_calls[0].name, "read_file");
    assert_eq!(
        completion.tool_calls[0].input,
        json!({ "path": "README.md" })
    );
}

#[test]
fn a_tool_call_input_that_arrives_as_a_json_string_is_parsed() {
    // Upstream accepts an `input` that arrives as encoded JSON text
    // (`src/gateway/client.zig:3986-3988`).
    let body = sse_body(&[
        tool_call("c1", "read_file", json!("  {\"path\":\"README.md\"} \n")),
        finish("tool-calls"),
    ]);
    let completion = decode_byte_by_byte(&body).0.expect("finish");
    assert_eq!(
        completion.tool_calls[0].input,
        json!({ "path": "README.md" })
    );
}

#[test]
fn a_streamed_tool_call_input_is_assembled_from_its_deltas() {
    // Upstream assembles `tool-input-start` / `tool-input-delta` /
    // `tool-input-end` and correlates them to the final `tool-call` by id
    // (`src/gateway/client.zig:2923-2973`, `:4077-4081`).
    let body = sse_body(&[
        json!({ "type": "tool-input-start", "id": "c1", "toolName": "read_file" }),
        json!({ "type": "tool-input-delta", "id": "c1", "delta": "{\"path\":" }),
        json!({ "type": "tool-input-delta", "id": "c1", "delta": "\"README.md\"}" }),
        json!({ "type": "tool-input-end", "id": "c1" }),
        json!({ "type": "tool-call", "toolCallId": "c1" }),
        finish("tool-calls"),
    ]);
    let completion = decode_byte_by_byte(&body).0.expect("finish");
    assert_eq!(completion.tool_calls.len(), 1);
    assert_eq!(completion.tool_calls[0].name, "read_file");
    assert_eq!(
        completion.tool_calls[0].input,
        json!({ "path": "README.md" })
    );
}

#[test]
fn a_direct_input_outranks_a_streamed_one_for_the_same_call() {
    // Upstream prefers the final `tool-call` input over the streamed buffer
    // (`src/gateway/client.zig:4141-4145`).
    let body = sse_body(&[
        json!({ "type": "tool-input-start", "id": "c1", "toolName": "read_file" }),
        json!({ "type": "tool-input-delta", "id": "c1", "delta": "{\"path\":\"STALE\"}" }),
        tool_call("c1", "read_file", json!({ "path": "FINAL" })),
        finish("tool-calls"),
    ]);
    let completion = decode_byte_by_byte(&body).0.expect("finish");
    assert_eq!(completion.tool_calls[0].input, json!({ "path": "FINAL" }));
}

#[test]
fn a_duplicate_tool_call_id_in_one_stream_is_rejected() {
    let body = sse_body(&[
        tool_call("c1", "read_file", json!({})),
        tool_call("c1", "read_file", json!({})),
        finish("tool-calls"),
    ]);
    assert!(matches!(
        decode_byte_by_byte(&body).0,
        Err(SseError::DuplicateToolCallId { .. })
    ));
}

#[test]
fn a_tool_call_without_a_usable_identity_is_rejected() {
    for event in [
        json!({ "type": "tool-call", "toolName": "read_file", "input": {} }),
        json!({ "type": "tool-call", "toolCallId": "", "toolName": "read_file" }),
        json!({ "type": "tool-call", "toolCallId": 7, "toolName": "read_file" }),
    ] {
        let body = sse_body(&[event.clone(), finish("tool-calls")]);
        assert!(
            matches!(
                decode_byte_by_byte(&body).0,
                Err(SseError::InvalidToolCall { .. })
            ),
            "{event} must be rejected"
        );
    }
}

#[test]
fn an_error_event_is_reported_as_a_provider_failure() {
    // Upstream captures the provider failure detail from an `error` event
    // (`src/gateway/client.zig:2902-2903`).
    let body = sse_body_without_done(&[
        json!({ "type": "error", "error": { "message": "upstream exploded" } }),
        finish("error"),
    ]);
    let completion = decode_byte_by_byte(&body)
        .0
        .expect("a finish still arrived");
    assert_eq!(completion.finish_reason, FinishReason::ProviderError);
    let detail = completion.provider_detail.expect("provider detail");
    assert!(detail.contains("upstream exploded"), "got {detail}");
}

#[test]
fn an_error_event_without_a_finish_is_still_a_provider_failure() {
    let body = sse_body(&[json!({ "type": "error", "error": { "message": "boom" } })]);
    match decode_byte_by_byte(&body).0 {
        Err(SseError::ProviderFailure { detail }) => {
            assert!(detail.contains("boom"), "got {detail}")
        }
        other => panic!("expected a provider failure, got {other:?}"),
    }
}

#[test]
fn a_malformed_nonterminal_event_is_ignored_and_the_stream_still_completes() {
    // Upstream skips an unparsable data event and keeps reading
    // (`src/gateway/client.zig:2837-2841`).
    let body = "data: {not json at all\n\n\
                data: {\"type\":\"text-delta\",\"id\":\"t1\",\"delta\":\"kept\"}\n\n\
                data: 42\n\n\
                data: {\"type\":\"unknown-future-event\"}\n\n\
                data: {\"type\":\"text-delta\",\"id\":\"t1\"}\n\n\
                data: {\"type\":\"finish\",\"finishReason\":{\"unified\":\"stop\"}}\n\n\
                data: [DONE]\n\n";
    let (completion, deltas) = decode_byte_by_byte(body);
    assert_eq!(deltas.fragments, ["kept"]);
    assert_eq!(completion.expect("finish").text, "kept");
}

#[test]
fn comment_and_blank_lines_are_ignored() {
    let body = ": keep-alive\n\n\
                \n\
                data: {\"type\":\"text-delta\",\"id\":\"t1\",\"delta\":\"a\"}\n\n\
                : another comment\n\n\
                data: {\"type\":\"finish\",\"finishReason\":{\"unified\":\"stop\"}}\n\n";
    let (completion, deltas) = decode_byte_by_byte(body);
    assert_eq!(deltas.fragments, ["a"]);
    assert!(completion.is_ok());
}

#[test]
fn eof_without_a_canonical_finish_is_rejected() {
    // A truncated stream is not a completed answer, however much text arrived.
    let body = sse_body_without_done(&[text_delta("t1", "partial")]);
    let (completion, deltas) = decode_byte_by_byte(&body);
    assert_eq!(deltas.fragments, ["partial"]);
    assert!(matches!(completion, Err(SseError::MissingFinish)));
}

#[test]
fn done_without_a_canonical_finish_is_rejected() {
    // `[DONE]` alone does not prove completion
    // (`src/gateway/client.zig:3641`, design "Gateway data flow" step 4).
    let body = sse_body(&[text_delta("t1", "partial")]);
    assert!(matches!(
        decode_byte_by_byte(&body).0,
        Err(SseError::MissingFinish)
    ));
    assert!(matches!(
        decode_byte_by_byte("data: [DONE]\n\n").0,
        Err(SseError::MissingFinish)
    ));
}

#[test]
fn an_empty_stream_is_rejected() {
    assert!(matches!(
        decode_byte_by_byte("").0,
        Err(SseError::MissingFinish)
    ));
}

#[test]
fn events_after_a_canonical_finish_are_ignored() {
    // Upstream stops at the finish event and never appends the late delta
    // (`src/gateway/client.zig:3522-3528`, `:3237-3238`).
    let body = sse_body(&[
        text_delta("t1", "answer"),
        finish("stop"),
        text_delta("t2", "late"),
    ]);
    let (completion, deltas) = decode_byte_by_byte(&body);
    assert_eq!(
        deltas.fragments,
        ["answer"],
        "a late delta must not be shown"
    );
    assert_eq!(completion.expect("finish").text, "answer");
}

#[test]
fn a_completed_reader_reports_that_it_needs_no_more_bytes() {
    let mut deltas = RecordingDeltas::default();
    let mut reader = SseReader::new();
    reader
        .push(sse_body(&[finish("stop")]).as_bytes(), &mut deltas)
        .expect("push");
    assert!(reader.is_complete());

    let mut fresh = SseReader::new();
    fresh
        .push(
            b"data: {\"type\":\"text-delta\",\"id\":\"t\",\"delta\":\"x\"}\n\n",
            &mut deltas,
        )
        .expect("push");
    assert!(!fresh.is_complete());
}

#[test]
fn an_event_larger_than_the_ceiling_is_rejected_instead_of_buffered() {
    let mut deltas = RecordingDeltas::default();
    let mut reader = SseReader::new();
    reader.push(b"data: ", &mut deltas).expect("prefix");

    // Push a megabyte at a time so the test does not build a 32 MiB string.
    let block = vec![b'x'; 1024 * 1024];
    let mut pushed = "data: ".len();
    loop {
        match reader.push(&block, &mut deltas) {
            Ok(()) => pushed += block.len(),
            Err(SseError::EventTooLarge { limit }) => {
                assert_eq!(limit, MAX_EVENT_BYTES);
                assert!(
                    pushed <= MAX_EVENT_BYTES + block.len(),
                    "the decoder buffered {pushed} bytes past the {MAX_EVENT_BYTES} ceiling"
                );
                return;
            }
            Err(other) => panic!("expected an event-size rejection, got {other:?}"),
        }
        assert!(
            pushed < MAX_EVENT_BYTES + 2 * block.len(),
            "the decoder never enforced the {MAX_EVENT_BYTES} ceiling"
        );
    }
}

#[test]
fn a_cancelled_decode_stops_at_the_next_frame() {
    let cancel = CancelToken::new();
    let mut deltas = RecordingDeltas::default();
    let mut reader = SseReader::with_cancel(cancel.clone());

    reader
        .push(
            b"data: {\"type\":\"text-delta\",\"id\":\"t1\",\"delta\":\"a\"}\n\n",
            &mut deltas,
        )
        .expect("first frame");
    cancel.cancel();
    let err = reader
        .push(
            b"data: {\"type\":\"text-delta\",\"id\":\"t1\",\"delta\":\"b\"}\n\n",
            &mut deltas,
        )
        .expect_err("a cancelled decode must stop");
    assert!(matches!(err, SseError::Cancelled));
    assert_eq!(deltas.fragments, ["a"], "no text after cancellation");
}

// ---------------------------------------------------------------------------
// endpoint safety
// ---------------------------------------------------------------------------

#[test]
fn the_default_endpoint_is_the_upstream_gateway_over_https() {
    // `vercel-labs/fx@580a0c5d src/builtins/gateway.zig:41`.
    let endpoint = Endpoint::resolve(None).expect("the default endpoint always resolves");
    assert_eq!(
        endpoint.url(),
        "https://ai-gateway.vercel.sh/v3/ai/language-model"
    );
}

#[test]
fn a_loopback_http_override_is_accepted() {
    // Upstream trusts an HTTP override only for loopback
    // (`src/builtins/gateway.zig:759-765`, `src/gateway/client.zig:1787-1803`).
    for url in [
        "http://127.0.0.1:8080/v3/ai/language-model",
        "http://localhost:8080/v3/ai/language-model",
        "http://LOCALHOST:8080/v3/ai/language-model",
        "http://[::1]:8080/v3/ai/language-model",
    ] {
        let endpoint = Endpoint::resolve(Some(url))
            .unwrap_or_else(|err| panic!("`{url}` must be accepted, got {err}"));
        assert_eq!(endpoint.url(), url);
    }
}

#[test]
fn a_non_loopback_http_override_is_rejected() {
    for url in [
        "http://198.51.100.7:8080/v3/ai/language-model",
        "http://example.com/v3/ai/language-model",
        "http://127.0.0.1.example.com:8080/v3",
    ] {
        assert!(
            matches!(
                Endpoint::resolve(Some(url)),
                Err(EndpointError::NonLoopbackHttp { .. })
            ),
            "`{url}` carries a bearer token in cleartext and must be rejected"
        );
    }
}

#[test]
fn an_https_override_is_accepted_anywhere() {
    let url = "https://gateway.example.com/v3/ai/language-model";
    assert_eq!(Endpoint::resolve(Some(url)).expect("https").url(), url);
}

#[test]
fn an_override_that_embeds_credentials_or_uses_another_scheme_is_rejected() {
    assert!(matches!(
        Endpoint::resolve(Some("https://user:pass@gateway.example.com/v3")),
        Err(EndpointError::EmbeddedCredentials { .. })
    ));
    assert!(matches!(
        Endpoint::resolve(Some("ftp://gateway.example.com/v3")),
        Err(EndpointError::UnsupportedScheme { .. })
    ));
    assert!(matches!(
        Endpoint::resolve(Some("not a url")),
        Err(EndpointError::Malformed { .. })
    ));
    assert!(matches!(
        Endpoint::resolve(Some("")),
        Err(EndpointError::Malformed { .. })
    ));
}

#[test]
fn an_endpoint_rejection_names_the_url_and_the_rule() {
    let err = Endpoint::resolve(Some("http://example.com/v3")).expect_err("rejected");
    let message = err.to_string();
    assert!(message.contains("http://example.com/v3"), "got {message}");
    assert!(message.contains("loopback"), "got {message}");
}

// ---------------------------------------------------------------------------
// turn machine
// ---------------------------------------------------------------------------

/// A provider that replays scripted results and records what it was asked.
struct ScriptedProvider {
    results: std::cell::RefCell<std::collections::VecDeque<ScriptedResult>>,
    seen: std::cell::RefCell<Vec<CompletionRequest>>,
    cancel_on_attempt: std::cell::RefCell<Option<(usize, CancelToken)>>,
}

enum ScriptedResult {
    /// Emit these text fragments, then return this completion.
    Streamed(Vec<String>, Completion),
    /// Emit these fragments and then fail. Proves that a failure after partial
    /// delivery is not replayed even when the failure itself looks retryable.
    StreamedThenFailed(Vec<String>, ProviderError),
    Failed(ProviderError),
}

impl ScriptedProvider {
    fn new(results: Vec<ScriptedResult>) -> Self {
        Self {
            results: std::cell::RefCell::new(results.into()),
            seen: std::cell::RefCell::new(Vec::new()),
            cancel_on_attempt: std::cell::RefCell::new(None),
        }
    }

    /// Cancels `token` at the start of attempt `attempt`, so a test can cancel a
    /// turn from inside the transport rather than racing it from outside.
    fn cancelling_on_attempt(self, attempt: usize, token: CancelToken) -> Self {
        *self.cancel_on_attempt.borrow_mut() = Some((attempt, token));
        self
    }

    fn attempts(&self) -> usize {
        self.seen.borrow().len()
    }
}

#[async_trait::async_trait(?Send)]
impl Provider for ScriptedProvider {
    async fn stream(
        &self,
        request: &CompletionRequest,
        deltas: &mut dyn DeltaSink,
    ) -> Result<Completion, ProviderError> {
        self.seen.borrow_mut().push(request.clone());
        let attempt = self.seen.borrow().len();
        if let Some((at, token)) = self.cancel_on_attempt.borrow().as_ref() {
            if *at == attempt {
                token.cancel();
            }
        }
        let next = self.results.borrow_mut().pop_front();
        match next {
            Some(ScriptedResult::Streamed(fragments, completion)) => {
                for fragment in fragments {
                    deltas.text_delta(&fragment).map_err(ProviderError::Sink)?;
                }
                Ok(completion)
            }
            Some(ScriptedResult::StreamedThenFailed(fragments, err)) => {
                for fragment in fragments {
                    deltas.text_delta(&fragment).map_err(ProviderError::Sink)?;
                }
                Err(err)
            }
            Some(ScriptedResult::Failed(err)) => Err(err),
            None => panic!("the provider was called more times than the script allows"),
        }
    }
}

/// A retryable edge status, optionally carrying the server's own delay.
fn edge_status(retry_after: Option<Duration>) -> ProviderError {
    ProviderError::Status {
        subject: xfx::gateway::GATEWAY_SUBJECT,
        status: 503,
        body: "try later".to_string(),
        retryable: true,
        retry_after,
    }
}

fn stopped(text: &str) -> Completion {
    Completion {
        text: text.to_string(),
        tool_calls: Vec::new(),
        finish_reason: FinishReason::Stop,
        usage: Default::default(),
        provider_detail: None,
    }
}

fn turn(prompt: &str) -> TurnRequest {
    TurnRequest {
        model: "vendor/model".to_string(),
        prompt: prompt.to_string(),
        history: Vec::new(),
        max_steps: 4,
        max_attempts: 3,
        cancel: CancelToken::new(),
        // These tests are about the transport and the turn's terminal states.
        // No test here reaches a tool executor; the registry, its scope, and
        // the tool loop are exercised in `tests/tool_loop.rs`.
        tools: xfx::tools::ToolContext::new(
            xfx::workspace::AccessScope::primary_only(
                std::env::current_dir().expect("a current directory"),
            )
            .expect("a usable workspace root"),
        ),
    }
}

fn kinds(sink: &RecordingSink) -> Vec<&'static str> {
    sink.events()
        .iter()
        .map(|event| match event {
            Event::AssistantDelta { .. } => "assistant_delta",
            Event::ToolStart { .. } => "tool_start",
            Event::ToolResult { .. } => "tool_result",
            Event::Final { .. } => "final",
            Event::Error { .. } => "error",
        })
        .collect()
}

#[tokio::test]
async fn a_content_only_turn_emits_ordered_deltas_then_exactly_one_final() {
    let provider = ScriptedProvider::new(vec![ScriptedResult::Streamed(
        vec!["Hel".to_string(), "lo".to_string()],
        stopped("Hello"),
    )]);
    let mut sink = RecordingSink::new();
    let outcome = run_turn(turn("hi"), &provider, &mut sink)
        .await
        .expect("a stop finish completes the turn");

    assert_eq!(
        kinds(&sink),
        ["assistant_delta", "assistant_delta", "final"]
    );
    assert_eq!(outcome.output, "Hello");
    assert_eq!(outcome.steps, 1);
    assert_eq!(provider.attempts(), 1);
    match &sink.events()[2] {
        Event::Final { output } => assert_eq!(output, "Hello"),
        other => panic!("expected a final event, got {other:?}"),
    }
}

#[tokio::test]
async fn the_turn_sends_the_user_prompt_and_the_configured_model() {
    let provider =
        ScriptedProvider::new(vec![ScriptedResult::Streamed(Vec::new(), stopped("done"))]);
    let mut sink = RecordingSink::new();
    run_turn(turn("read the file"), &provider, &mut sink)
        .await
        .expect("turn");

    let seen = provider.seen.borrow();
    let request = seen.first().expect("one request");
    assert_eq!(request.model, "vendor/model");
    assert_eq!(request.messages.len(), 1);
    assert_eq!(request.messages[0].role, Role::User);
    // What the turn advertises is the registry's contract; see
    // `tests/tool_loop.rs`.
    assert_eq!(request.tool_choice, ToolChoice::Auto);
}

#[tokio::test]
async fn a_provider_failure_emits_exactly_one_error_and_no_final() {
    let provider = ScriptedProvider::new(vec![ScriptedResult::Failed(ProviderError::Status {
        subject: xfx::gateway::GATEWAY_SUBJECT,
        status: 401,
        body: "unauthorized".to_string(),
        retryable: false,
        retry_after: None,
    })]);
    let mut sink = RecordingSink::new();
    let err = run_turn(turn("hi"), &provider, &mut sink)
        .await
        .expect_err("a 401 fails the turn");

    assert!(matches!(
        err,
        TurnError::Provider(ProviderError::Status { status: 401, .. })
    ));
    assert_eq!(kinds(&sink), ["error"]);
    match &sink.events()[0] {
        Event::Error { message } => assert!(message.contains("401"), "got {message}"),
        other => panic!("expected an error event, got {other:?}"),
    }
}

#[tokio::test]
async fn a_failure_after_partial_delivery_still_finalizes_exactly_once() {
    let provider = ScriptedProvider::new(vec![ScriptedResult::Failed(ProviderError::Protocol(
        SseError::MissingFinish,
    ))]);
    let mut sink = RecordingSink::new();
    run_turn(turn("hi"), &provider, &mut sink)
        .await
        .expect_err("a stream without a finish is not a completed turn");
    assert_eq!(kinds(&sink), ["error"], "exactly one terminal event");
}

#[tokio::test]
async fn a_replayable_failure_is_retried_up_to_max_attempts() {
    let provider = ScriptedProvider::new(vec![
        ScriptedResult::Failed(ProviderError::Connect {
            subject: xfx::gateway::GATEWAY_SUBJECT,
            detail: "connection refused".to_string(),
        }),
        ScriptedResult::Streamed(vec!["ok".to_string()], stopped("ok")),
    ]);
    let mut sink = RecordingSink::new();
    let outcome = run_turn(turn("hi"), &provider, &mut sink)
        .await
        .expect("the second attempt succeeds");
    assert_eq!(provider.attempts(), 2);
    assert_eq!(outcome.output, "ok");
    assert_eq!(kinds(&sink), ["assistant_delta", "final"]);
}

#[tokio::test]
async fn a_replayable_failure_stops_at_max_attempts() {
    let failures = || {
        ScriptedResult::Failed(ProviderError::Connect {
            subject: xfx::gateway::GATEWAY_SUBJECT,
            detail: "connection refused".to_string(),
        })
    };
    let provider = ScriptedProvider::new(vec![failures(), failures(), failures()]);
    let mut request = turn("hi");
    request.max_attempts = 3;
    let mut sink = RecordingSink::new();
    run_turn(request, &provider, &mut sink)
        .await
        .expect_err("every attempt failed");
    assert_eq!(provider.attempts(), 3);
    assert_eq!(kinds(&sink), ["error"]);
}

#[tokio::test]
async fn a_retry_waits_for_the_server_requested_delay() {
    // The server named a delay; the turn obeys it instead of guessing.
    let provider = ScriptedProvider::new(vec![
        ScriptedResult::Failed(edge_status(Some(Duration::from_millis(400)))),
        ScriptedResult::Streamed(vec!["ok".to_string()], stopped("ok")),
    ]);
    let mut sink = RecordingSink::new();
    let started = Instant::now();
    run_turn(turn("hi"), &provider, &mut sink)
        .await
        .expect("the second attempt succeeds");
    let waited = started.elapsed();
    assert_eq!(provider.attempts(), 2);
    assert!(
        waited >= Duration::from_millis(350),
        "the turn replayed after only {waited:?}, ignoring Retry-After"
    );
    assert!(
        waited < Duration::from_secs(3),
        "the turn waited {waited:?}, far past what the server asked for"
    );
}

#[tokio::test]
async fn a_server_delay_beyond_the_cap_does_not_stall_the_turn() {
    // A server asking for ten minutes must not turn a foreground command into
    // something indistinguishable from a hang.
    let provider = ScriptedProvider::new(vec![
        ScriptedResult::Failed(edge_status(Some(Duration::from_secs(600)))),
        ScriptedResult::Streamed(vec!["ok".to_string()], stopped("ok")),
    ]);
    let mut sink = RecordingSink::new();
    let started = Instant::now();
    run_turn(turn("hi"), &provider, &mut sink)
        .await
        .expect("the second attempt succeeds");
    assert_eq!(provider.attempts(), 2);
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "the turn obeyed an unbounded server delay for {:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn a_retry_without_a_server_delay_backs_off_on_its_own() {
    let provider = ScriptedProvider::new(vec![
        ScriptedResult::Failed(edge_status(None)),
        ScriptedResult::Streamed(vec!["ok".to_string()], stopped("ok")),
    ]);
    let mut sink = RecordingSink::new();
    let started = Instant::now();
    run_turn(turn("hi"), &provider, &mut sink)
        .await
        .expect("the second attempt succeeds");
    let waited = started.elapsed();
    assert_eq!(provider.attempts(), 2);
    assert!(
        waited >= Duration::from_millis(200),
        "the turn replayed immediately, after only {waited:?}"
    );
}

#[tokio::test]
async fn cancelling_during_a_backoff_ends_the_turn_without_another_attempt() {
    let request = turn("hi");
    let cancel = request.cancel.clone();
    // The first attempt fails with a long server delay and cancels the turn on
    // its way out, so the turn is cancelled while it is waiting.
    let provider = ScriptedProvider::new(vec![
        ScriptedResult::Failed(edge_status(Some(Duration::from_secs(600)))),
        ScriptedResult::Streamed(vec!["never".to_string()], stopped("never")),
    ])
    .cancelling_on_attempt(1, cancel);

    let mut sink = RecordingSink::new();
    let started = Instant::now();
    let err = run_turn(request, &provider, &mut sink)
        .await
        .expect_err("a cancelled wait ends the turn");
    assert!(matches!(err, TurnError::Cancelled));
    assert_eq!(
        provider.attempts(),
        1,
        "a cancelled turn must not spend another attempt"
    );
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "cancellation did not interrupt the wait; it took {:?}",
        started.elapsed()
    );
    assert_eq!(kinds(&sink), ["error"]);
}

#[tokio::test]
async fn a_retryable_status_is_not_replayed_once_content_has_been_delivered() {
    // The failure is replayable in isolation, but an answer is already partly
    // in front of the user, so replaying it would answer one question twice.
    let provider = ScriptedProvider::new(vec![
        ScriptedResult::StreamedThenFailed(
            vec!["half an ans".to_string()],
            edge_status(Some(Duration::from_millis(1))),
        ),
        ScriptedResult::Streamed(vec!["replay".to_string()], stopped("replay")),
    ]);
    let mut sink = RecordingSink::new();
    run_turn(turn("hi"), &provider, &mut sink)
        .await
        .expect_err("delivered content is not replayed");
    assert_eq!(
        provider.attempts(),
        1,
        "an attempt that delivered content must not be replayed"
    );
    assert_eq!(kinds(&sink), ["assistant_delta", "error"]);
}

#[tokio::test]
async fn a_failure_that_may_have_been_delivered_is_never_replayed() {
    // The rule the design states under "Risks and controls": a retry after an
    // ambiguous delivery can duplicate model intent, so it does not happen.
    for err in [
        ProviderError::Transport {
            subject: xfx::gateway::GATEWAY_SUBJECT,
            detail: "connection reset".to_string(),
        },
        ProviderError::Protocol(SseError::MissingFinish),
    ] {
        let provider = ScriptedProvider::new(vec![ScriptedResult::Failed(err)]);
        let mut sink = RecordingSink::new();
        run_turn(turn("hi"), &provider, &mut sink)
            .await
            .expect_err("fails");
        assert_eq!(
            provider.attempts(),
            1,
            "an ambiguous delivery must not be replayed"
        );
    }
}

#[tokio::test]
async fn a_call_for_a_tool_that_is_not_advertised_is_rejected_rather_than_simulated() {
    // `delete_file` is `deferred` in `docs/parity.md`, so the turn never offered
    // it and will not act as though it had.
    let completion = Completion {
        text: String::new(),
        tool_calls: vec![ToolCall {
            id: "c1".to_string(),
            name: "delete_file".to_string(),
            input: json!({}),
        }],
        finish_reason: FinishReason::ToolCalls,
        usage: Default::default(),
        provider_detail: None,
    };
    let provider = ScriptedProvider::new(vec![ScriptedResult::Streamed(Vec::new(), completion)]);
    let mut sink = RecordingSink::new();
    let err = run_turn(turn("hi"), &provider, &mut sink)
        .await
        .expect_err("an unadvertised tool call cannot be executed");
    assert!(matches!(err, TurnError::ToolCallUnsupported { .. }));
    assert_eq!(kinds(&sink), ["error"]);
    match &sink.events()[0] {
        Event::Error { message } => assert!(message.contains("delete_file"), "got {message}"),
        other => panic!("expected an error event, got {other:?}"),
    }
}

#[tokio::test]
async fn a_provider_error_finish_fails_the_turn_with_its_detail() {
    let completion = Completion {
        text: "half".to_string(),
        tool_calls: Vec::new(),
        finish_reason: FinishReason::ProviderError,
        usage: Default::default(),
        provider_detail: Some("model overloaded".to_string()),
    };
    let provider = ScriptedProvider::new(vec![ScriptedResult::Streamed(
        vec!["half".to_string()],
        completion,
    )]);
    let mut sink = RecordingSink::new();
    run_turn(turn("hi"), &provider, &mut sink)
        .await
        .expect_err("an error finish is not a completed turn");
    assert_eq!(kinds(&sink), ["assistant_delta", "error"]);
    match &sink.events()[1] {
        Event::Error { message } => assert!(message.contains("model overloaded"), "got {message}"),
        other => panic!("expected an error event, got {other:?}"),
    }
}

#[tokio::test]
async fn a_tool_call_finish_that_names_no_tool_is_rejected() {
    let completion = Completion {
        text: String::new(),
        tool_calls: Vec::new(),
        finish_reason: FinishReason::ToolCalls,
        usage: Default::default(),
        provider_detail: None,
    };
    let provider = ScriptedProvider::new(vec![ScriptedResult::Streamed(Vec::new(), completion)]);
    let mut sink = RecordingSink::new();
    let err = run_turn(turn("hi"), &provider, &mut sink)
        .await
        .expect_err("a tool-calls finish with no calls is not a completed turn");
    assert!(matches!(err, TurnError::EmptyToolCallFinish));
    assert_eq!(kinds(&sink), ["error"]);
}

#[tokio::test]
async fn a_machine_refuses_to_run_a_second_time() {
    let provider = ScriptedProvider::new(vec![ScriptedResult::Streamed(
        vec!["once".to_string()],
        stopped("once"),
    )]);
    let mut machine = xfx::agent::TurnMachine::new(turn("hi"));
    let mut sink = RecordingSink::new();
    machine.run(&provider, &mut sink).await.expect("first run");
    let err = machine
        .run(&provider, &mut sink)
        .await
        .expect_err("a finalized turn cannot run again");
    assert!(matches!(err, TurnError::AlreadyFinalized));
    assert_eq!(
        kinds(&sink),
        ["assistant_delta", "final"],
        "a second run must not add a second terminal event"
    );
    assert_eq!(provider.attempts(), 1);
}

#[tokio::test]
async fn a_cancelled_turn_fails_before_it_calls_the_provider() {
    let provider = ScriptedProvider::new(Vec::new());
    let request = turn("hi");
    request.cancel.cancel();
    let mut sink = RecordingSink::new();
    let err = run_turn(request, &provider, &mut sink)
        .await
        .expect_err("a cancelled turn does not start");
    assert!(matches!(err, TurnError::Cancelled));
    assert_eq!(provider.attempts(), 0);
    assert_eq!(kinds(&sink), ["error"]);
}

#[test]
fn the_step_bound_treats_zero_as_unbounded_and_binds_above_it() {
    // `0` means unbounded, matching the configured semantics
    // (`vercel-labs/fx@580a0c5d src/core/config/agent_steps.zig:3-31`).
    assert!(xfx::agent::allows_step(0, 0));
    assert!(xfx::agent::allows_step(0, 10_000));
    assert!(xfx::agent::allows_step(1, 0));
    assert!(!xfx::agent::allows_step(1, 1));
    assert!(xfx::agent::allows_step(3, 2));
    assert!(!xfx::agent::allows_step(3, 3));
}

#[tokio::test]
async fn an_unbounded_step_limit_still_runs_a_content_only_turn() {
    let provider =
        ScriptedProvider::new(vec![ScriptedResult::Streamed(Vec::new(), stopped("done"))]);
    let mut request = turn("hi");
    request.max_steps = 0;
    let mut sink = RecordingSink::new();
    let outcome = run_turn(request, &provider, &mut sink)
        .await
        .expect("an unbounded limit still runs a content-only turn");
    assert_eq!(outcome.steps, 1);
    assert_eq!(provider.attempts(), 1);
}

#[tokio::test]
async fn a_turn_with_no_attempt_budget_never_calls_the_provider() {
    let provider = ScriptedProvider::new(Vec::new());
    let mut request = turn("hi");
    request.max_attempts = 0;
    let mut sink = RecordingSink::new();
    let err = run_turn(request, &provider, &mut sink)
        .await
        .expect_err("a zero attempt budget cannot call the provider");
    assert!(matches!(err, TurnError::AttemptLimit { .. }));
    assert_eq!(provider.attempts(), 0);
    assert_eq!(kinds(&sink), ["error"]);
}

// ---------------------------------------------------------------------------
// binary acceptance: `xfx ask`
// ---------------------------------------------------------------------------

struct Sandbox {
    _root: TempDir,
    home: PathBuf,
    workspace: PathBuf,
}

impl Sandbox {
    fn new() -> Self {
        let root = TempDir::new().expect("create sandbox root");
        let home = root.path().join("home");
        let workspace = root.path().join("workspace");
        fs::create_dir_all(&home).expect("create home");
        fs::create_dir_all(&workspace).expect("create workspace");
        Self {
            home: home.canonicalize().expect("canonicalize home"),
            workspace: workspace.canonicalize().expect("canonicalize workspace"),
            _root: root,
        }
    }

    fn run(&self, args: &[&str], env: &[(&str, &str)]) -> Run {
        let mut command = Command::new(env!("CARGO_BIN_EXE_xfx"));
        command.current_dir(&self.workspace);
        command.env("HOME", &self.home);
        for key in CONTROLLED_VARS {
            command.env_remove(key);
        }
        for (key, value) in env {
            command.env(key, value);
        }
        command.args(args);
        Run::of(command.output().expect("spawn xfx"))
    }
}

struct Run {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

impl Run {
    fn of(output: Output) -> Self {
        Self {
            code: output.status.code(),
            stdout: String::from_utf8(output.stdout).expect("stdout is utf-8"),
            stderr: String::from_utf8(output.stderr).expect("stderr is utf-8"),
        }
    }

    /// The stdout JSONL stream, one parsed object per line.
    fn events(&self) -> Vec<Value> {
        assert!(
            self.stdout.is_empty() || self.stdout.ends_with('\n'),
            "JSONL must be newline terminated, got {:?}",
            self.stdout
        );
        self.stdout
            .lines()
            .map(|line| {
                serde_json::from_str(line)
                    .unwrap_or_else(|err| panic!("`{line}` is not JSON ({err})"))
            })
            .collect()
    }

    fn kinds(&self) -> Vec<String> {
        self.events()
            .iter()
            .map(|event| {
                event["kind"]
                    .as_str()
                    .expect("every event has a kind")
                    .to_string()
            })
            .collect()
    }

    /// The message of the single `error` event on stdout.
    fn error_message(&self) -> String {
        let events = self.events();
        assert_eq!(
            events.len(),
            1,
            "expected exactly one event, got {events:?}"
        );
        events[0]["message"]
            .as_str()
            .expect("an error event carries a message")
            .to_string()
    }

    fn assert_no_secret(&self) {
        assert!(!self.stdout.contains(TEST_KEY), "the key leaked on stdout");
        assert!(!self.stderr.contains(TEST_KEY), "the key leaked on stderr");
    }
}

#[test]
fn ask_json_streams_deltas_then_exactly_one_final() {
    let gateway = FakeGateway::start(vec![Reply::Sse(content_only(&["Hello, ", "world"]))]);
    let sandbox = Sandbox::new();
    let run = sandbox.run(
        &["ask", "--json", "--no-save", "hello"],
        &[
            ("AI_GATEWAY_API_KEY", TEST_KEY),
            ("XFX_GATEWAY_URL", &gateway.chat_url()),
        ],
    );

    assert_eq!(run.code, Some(0), "stderr={:?}", run.stderr);
    run.assert_no_secret();
    assert_eq!(
        run.kinds(),
        ["assistant_delta", "assistant_delta", "final"],
        "stdout={:?}",
        run.stdout
    );
    let events = run.events();
    assert_eq!(events[0]["text"], "Hello, ");
    assert_eq!(events[1]["text"], "world");
    assert_eq!(events[2]["output"], "Hello, world");
    assert_eq!(
        events.iter().filter(|e| e["kind"] == "final").count(),
        1,
        "exactly one final event"
    );
}

#[test]
fn ask_sends_the_bearer_credential_and_the_xfx_source_headers() {
    let gateway = FakeGateway::start(vec![Reply::Sse(content_only(&["ok"]))]);
    let sandbox = Sandbox::new();
    let run = sandbox.run(
        &["ask", "--json", "--no-save", "hello"],
        &[
            ("AI_GATEWAY_API_KEY", TEST_KEY),
            ("XFX_GATEWAY_URL", &gateway.chat_url()),
        ],
    );
    assert_eq!(run.code, Some(0), "stderr={:?}", run.stderr);

    let request = gateway.only_request();
    assert_eq!(request.method, "POST");
    assert_eq!(request.path, "/v3/ai/language-model");
    assert_eq!(
        request.header("authorization"),
        Some(&*format!("Bearer {TEST_KEY}"))
    );
    assert_eq!(request.header("content-type"), Some("application/json"));
    assert_eq!(request.header("accept"), Some("text/event-stream"));
    // xfx identifies itself as xfx. Claiming to be `fx` would be an
    // impersonation of the upstream product it is a port of.
    assert_eq!(
        request.header("http-referer"),
        Some("https://github.com/2lab-ai/xfx")
    );
    assert_eq!(request.header("x-title"), Some("xfx"));
    assert_eq!(
        request.header("ai-language-model-id"),
        Some(xfx::config::DEFAULT_MODEL)
    );
    assert_eq!(request.header("ai-language-model-streaming"), Some("true"));
    assert_eq!(request.header("ai-gateway-protocol-version"), Some("0.0.1"));
    assert_eq!(
        request.header("ai-language-model-specification-version"),
        Some("4")
    );
}

#[test]
fn the_oidc_token_is_the_bearer_when_both_credentials_are_present() {
    let gateway = FakeGateway::start(vec![Reply::Sse(content_only(&["ok"]))]);
    let sandbox = Sandbox::new();
    let run = sandbox.run(
        &["ask", "--json", "--no-save", "hello"],
        &[
            ("VERCEL_OIDC_TOKEN", "oidc-value"),
            ("AI_GATEWAY_API_KEY", TEST_KEY),
            ("XFX_GATEWAY_URL", &gateway.chat_url()),
        ],
    );
    assert_eq!(run.code, Some(0), "stderr={:?}", run.stderr);
    assert_eq!(
        gateway.only_request().header("authorization"),
        Some("Bearer oidc-value")
    );
}

#[test]
fn ask_sends_exactly_the_documented_request_body() {
    let gateway = FakeGateway::start(vec![Reply::Sse(content_only(&["ok"]))]);
    let sandbox = Sandbox::new();
    let run = sandbox.run(
        &["ask", "--json", "--no-save", "explain", "this", "code"],
        &[
            ("AI_GATEWAY_API_KEY", TEST_KEY),
            ("XFX_GATEWAY_URL", &gateway.chat_url()),
        ],
    );
    assert_eq!(run.code, Some(0), "stderr={:?}", run.stderr);

    let request = gateway.only_request();
    assert!(
        request.body.starts_with("{\"prompt\":["),
        "got {}",
        request.body
    );
    let body = request.json();
    assert_eq!(
        body["prompt"],
        json!([{
            "role": "user",
            "content": [{ "type": "text", "text": "explain this code" }],
        }])
    );
    // The advertised schemas are the tool registry's contract, asserted in
    // `tests/tool_loop.rs`; here the point is that `tools` is what the registry
    // produced and that the model is allowed to use it.
    assert_eq!(body["toolChoice"], json!({ "type": "auto" }));
    assert_eq!(
        body["tools"],
        Value::Array(xfx::tools::Registry::builtin().advertisement())
    );
    assert_eq!(
        body.as_object().expect("an object").keys().len(),
        3,
        "the body carries exactly prompt, tools, and toolChoice: {}",
        request.body
    );
}

#[test]
fn ask_streams_plain_text_for_a_human() {
    let gateway = FakeGateway::start(vec![Reply::Sse(content_only(&["Hello, ", "world"]))]);
    let sandbox = Sandbox::new();
    let run = sandbox.run(
        &["ask", "--no-save", "hello"],
        &[
            ("AI_GATEWAY_API_KEY", TEST_KEY),
            ("XFX_GATEWAY_URL", &gateway.chat_url()),
        ],
    );
    assert_eq!(run.code, Some(0), "stderr={:?}", run.stderr);
    assert_eq!(run.stdout, "Hello, world\n");
    assert_eq!(run.stderr, "");
}

#[test]
fn an_sse_event_split_across_transport_writes_still_decodes() {
    let body = content_only(&["one ", "two"]);
    let pieces: Vec<String> = body
        .as_bytes()
        .chunks(7)
        .map(|chunk| String::from_utf8(chunk.to_vec()).expect("ascii fixture"))
        .collect();
    let gateway = FakeGateway::start(vec![Reply::SsePieces(pieces)]);
    let sandbox = Sandbox::new();
    let run = sandbox.run(
        &["ask", "--json", "--no-save", "hello"],
        &[
            ("AI_GATEWAY_API_KEY", TEST_KEY),
            ("XFX_GATEWAY_URL", &gateway.chat_url()),
        ],
    );
    assert_eq!(run.code, Some(0), "stderr={:?}", run.stderr);
    assert_eq!(run.kinds(), ["assistant_delta", "assistant_delta", "final"]);
    assert_eq!(run.events()[2]["output"], "one two");
}

#[test]
fn a_loopback_http_override_named_localhost_is_accepted() {
    let gateway = FakeGateway::start(vec![Reply::Sse(content_only(&["ok"]))]);
    let sandbox = Sandbox::new();
    let run = sandbox.run(
        &["ask", "--json", "--no-save", "hello"],
        &[
            ("AI_GATEWAY_API_KEY", TEST_KEY),
            ("XFX_GATEWAY_URL", &gateway.localhost_chat_url()),
        ],
    );
    assert_eq!(run.code, Some(0), "stderr={:?}", run.stderr);
    assert_eq!(gateway.request_count(), 1);
}

#[test]
fn a_non_loopback_http_override_fails_before_a_credential_is_sent() {
    let gateway = FakeGateway::start(vec![Reply::Sse(content_only(&["ok"]))]);
    // TEST-NET-3 (RFC 5737): unroutable, so a regression that ignored the rule
    // would fail with a connect error rather than this diagnostic.
    let override_url = format!("http://203.0.113.7:{}/v3/ai/language-model", gateway.port());
    let sandbox = Sandbox::new();
    let run = sandbox.run(
        &["ask", "--json", "--no-save", "hello"],
        &[
            ("AI_GATEWAY_API_KEY", TEST_KEY),
            ("XFX_GATEWAY_URL", &override_url),
        ],
    );

    assert_eq!(run.code, Some(1), "stdout={:?}", run.stdout);
    run.assert_no_secret();
    assert_eq!(
        gateway.request_count(),
        0,
        "no request may leave the process for a rejected endpoint"
    );
    assert_eq!(run.kinds(), ["error"], "stdout={:?}", run.stdout);
    let message = run.error_message();
    assert!(message.contains("loopback"), "got {message}");
    assert!(message.contains("203.0.113.7"), "got {message}");
}

#[test]
fn ask_without_a_credential_reports_the_missing_auth_help_and_sends_nothing() {
    let gateway = FakeGateway::start(vec![Reply::Sse(content_only(&["ok"]))]);
    let sandbox = Sandbox::new();
    let run = sandbox.run(
        &["ask", "--json", "--no-save", "hello"],
        &[("XFX_GATEWAY_URL", &gateway.chat_url())],
    );
    assert_eq!(run.code, Some(1), "stdout={:?}", run.stdout);
    assert_eq!(gateway.request_count(), 0);
    assert_eq!(run.kinds(), ["error"]);
    assert_eq!(
        run.events()[0]["message"],
        xfx::output::MISSING_AUTH_HELP,
        "the missing-credential message must be the one status and doctor use"
    );
}

#[test]
fn ask_reports_a_gateway_error_status_as_one_error_event() {
    let gateway = FakeGateway::start(vec![Reply::Status(
        401,
        "{\"error\":\"bad key\"}".to_string(),
    )]);
    let sandbox = Sandbox::new();
    let run = sandbox.run(
        &["ask", "--json", "--no-save", "hello"],
        &[
            ("AI_GATEWAY_API_KEY", TEST_KEY),
            ("XFX_GATEWAY_URL", &gateway.chat_url()),
        ],
    );
    assert_eq!(run.code, Some(1), "stdout={:?}", run.stdout);
    run.assert_no_secret();
    assert_eq!(run.kinds(), ["error"]);
    assert_eq!(gateway.request_count(), 1, "401 is not retryable");
    let message = run.error_message();
    assert!(message.contains("401"), "got {message}");
}

#[test]
fn a_retryable_status_is_retried_before_any_answer_was_delivered() {
    let gateway = FakeGateway::start(vec![
        Reply::Status(503, "{\"error\":\"try later\"}".to_string()),
        Reply::Sse(content_only(&["recovered"])),
    ]);
    let sandbox = Sandbox::new();
    let run = sandbox.run(
        &["ask", "--json", "--no-save", "hello"],
        &[
            ("AI_GATEWAY_API_KEY", TEST_KEY),
            ("XFX_GATEWAY_URL", &gateway.chat_url()),
        ],
    );
    assert_eq!(run.code, Some(0), "stderr={:?}", run.stderr);
    assert_eq!(gateway.request_count(), 2);
    assert_eq!(run.kinds(), ["assistant_delta", "final"]);
    assert_eq!(run.events()[1]["output"], "recovered");
}

#[test]
fn a_retry_after_header_from_the_gateway_is_honored_before_the_next_request() {
    // End to end through the real transport: the header must survive the
    // response head, reach the turn, and delay the second request.
    let gateway = FakeGateway::start(vec![
        Reply::retry_after(503, 1, "{\"error\":\"try later\"}"),
        Reply::Sse(content_only(&["recovered"])),
    ]);
    let sandbox = Sandbox::new();
    let started = Instant::now();
    let run = sandbox.run(
        &["ask", "--json", "--no-save", "hello"],
        &[
            ("AI_GATEWAY_API_KEY", TEST_KEY),
            ("XFX_GATEWAY_URL", &gateway.chat_url()),
        ],
    );
    let waited = started.elapsed();

    assert_eq!(run.code, Some(0), "stderr={:?}", run.stderr);
    assert_eq!(gateway.request_count(), 2);
    assert_eq!(run.kinds(), ["assistant_delta", "final"]);
    assert!(
        waited >= Duration::from_millis(900),
        "the second request went out after only {waited:?}, ignoring `Retry-After: 1`"
    );
    assert!(
        waited < Duration::from_secs(15),
        "the run took {waited:?}, far past the one second the server asked for"
    );
}

#[test]
fn a_truncated_body_is_not_replayed_once_delivery_has_started() {
    // The Gateway sends part of an answer and then drops the connection. The
    // model has already produced output, so a second request would duplicate
    // model intent and bill for it twice.
    let gateway = FakeGateway::start(vec![
        Reply::SseThenAbort(vec![
            "data: {\"type\":\"text-delta\",\"id\":\"t1\",\"delta\":\"half an ans\"}\n\n"
                .to_string(),
        ]),
        Reply::Sse(content_only(&["a replay that must never happen"])),
    ]);
    let sandbox = Sandbox::new();
    let run = sandbox.run(
        &["ask", "--json", "--no-save", "hello"],
        &[
            ("AI_GATEWAY_API_KEY", TEST_KEY),
            ("XFX_GATEWAY_URL", &gateway.chat_url()),
        ],
    );

    assert_eq!(run.code, Some(1), "stdout={:?}", run.stdout);
    assert_eq!(
        gateway.request_count(),
        1,
        "an attempt whose body delivery started must not be replayed"
    );
    assert_eq!(run.kinds(), ["assistant_delta", "error"]);
    assert_eq!(run.events()[0]["text"], "half an ans");
}

#[test]
fn a_stream_that_ends_without_a_finish_event_fails_the_turn() {
    let gateway = FakeGateway::start(vec![Reply::Sse(sse_body_without_done(&[text_delta(
        "t1", "partial",
    )]))]);
    let sandbox = Sandbox::new();
    let run = sandbox.run(
        &["ask", "--json", "--no-save", "hello"],
        &[
            ("AI_GATEWAY_API_KEY", TEST_KEY),
            ("XFX_GATEWAY_URL", &gateway.chat_url()),
        ],
    );
    assert_eq!(run.code, Some(1), "stdout={:?}", run.stdout);
    assert_eq!(run.kinds(), ["assistant_delta", "error"]);
}

#[test]
fn a_done_marker_without_a_finish_event_fails_the_turn() {
    let gateway = FakeGateway::start(vec![Reply::Sse(sse_body(&[text_delta("t1", "partial")]))]);
    let sandbox = Sandbox::new();
    let run = sandbox.run(
        &["ask", "--json", "--no-save", "hello"],
        &[
            ("AI_GATEWAY_API_KEY", TEST_KEY),
            ("XFX_GATEWAY_URL", &gateway.chat_url()),
        ],
    );
    assert_eq!(run.code, Some(1), "stdout={:?}", run.stdout);
    assert_eq!(run.kinds(), ["assistant_delta", "error"]);
}

#[test]
fn ask_fails_when_the_model_asks_for_a_tool_that_is_not_advertised() {
    // `delete_file` is `deferred`; the binary must refuse rather than answer as
    // though a file had been deleted.
    let gateway = FakeGateway::start(vec![Reply::Sse(sse_body(&[
        tool_call("c1", "delete_file", json!({ "path": "x" })),
        finish("tool-calls"),
    ]))]);
    let sandbox = Sandbox::new();
    let run = sandbox.run(
        &["ask", "--json", "--no-save", "write it"],
        &[
            ("AI_GATEWAY_API_KEY", TEST_KEY),
            ("XFX_GATEWAY_URL", &gateway.chat_url()),
        ],
    );
    assert_eq!(run.code, Some(1), "stdout={:?}", run.stdout);
    assert_eq!(run.kinds(), ["error"]);
    let message = run.error_message();
    assert!(message.contains("delete_file"), "got {message}");
}

#[test]
fn ask_applies_the_configured_model() {
    let gateway = FakeGateway::start(vec![Reply::Sse(content_only(&["ok"]))]);
    let sandbox = Sandbox::new();
    let run = sandbox.run(
        &["ask", "--json", "--no-save", "hello"],
        &[
            ("AI_GATEWAY_API_KEY", TEST_KEY),
            ("XFX_MODEL", "vendor/chosen-model"),
            ("XFX_GATEWAY_URL", &gateway.chat_url()),
        ],
    );
    assert_eq!(run.code, Some(0), "stderr={:?}", run.stderr);
    assert_eq!(
        gateway.only_request().header("ai-language-model-id"),
        Some("vendor/chosen-model")
    );
}

#[test]
fn ask_rejects_an_empty_prompt() {
    let sandbox = Sandbox::new();
    for args in [
        vec!["ask"],
        vec!["ask", "--json"],
        vec!["ask", "--json", "   "],
    ] {
        let run = sandbox.run(&args, &[("AI_GATEWAY_API_KEY", TEST_KEY)]);
        assert_eq!(run.code, Some(1), "{args:?} must be rejected");
        assert_eq!(run.stdout, "", "{args:?} must not write to stdout");
        assert!(
            !run.stderr.is_empty(),
            "{args:?} must explain the rejection"
        );
    }
}

#[test]
fn ask_treats_a_leading_dash_prompt_after_a_separator_as_text() {
    let gateway = FakeGateway::start(vec![Reply::Sse(content_only(&["ok"]))]);
    let sandbox = Sandbox::new();
    let run = sandbox.run(
        &["ask", "--json", "--no-save", "--", "--not-a-flag"],
        &[
            ("AI_GATEWAY_API_KEY", TEST_KEY),
            ("XFX_GATEWAY_URL", &gateway.chat_url()),
        ],
    );
    assert_eq!(run.code, Some(0), "stderr={:?}", run.stderr);
    assert_eq!(
        gateway.only_request().json()["prompt"][0]["content"][0]["text"],
        "--not-a-flag"
    );
}

#[test]
fn ask_help_advertises_only_the_implemented_flags() {
    let sandbox = Sandbox::new();
    let run = sandbox.run(&["ask", "--help"], &[]);
    assert_eq!(run.code, Some(0));
    for flag in [
        "--json",
        "--no-save",
        "--auto",
        "--yolo",
        "--resume",
        "--resume-id",
    ] {
        assert!(run.stdout.contains(flag), "ask help must list {flag}");
    }
    for deferred in ["--quiet", "--acp"] {
        assert!(
            !run.stdout.contains(deferred),
            "ask help must not advertise the deferred flag {deferred}"
        );
    }
}

#[test]
fn the_no_save_flag_help_states_exactly_what_it_prevents() {
    // The flag is load-bearing now: the default records the turn and this one
    // records nothing at all, so the help states the flag's own guarantee. The
    // caveat it used to carry -- that the default was indistinguishable from it
    // -- would now be the false statement.
    let sandbox = Sandbox::new();
    for alias in ["--help", "-h"] {
        let run = sandbox.run(&["ask", alias], &[]);
        assert_eq!(run.code, Some(0), "ask {alias} must exit 0");
        assert!(
            run.stdout.contains("Do not record this turn in a session"),
            "ask {alias} must state what --no-save prevents, got {:?}",
            run.stdout
        );
        assert!(
            !run.stdout.contains("records none either way"),
            "ask {alias} must not still claim the default records nothing, got {:?}",
            run.stdout
        );
    }
}

#[test]
fn the_no_save_flag_and_the_log_it_opts_out_of_are_both_implemented() {
    let parity =
        fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/parity.md"))
            .expect("read parity.md");
    let row = parity
        .lines()
        .find(|line| line.starts_with("| `ask --no-save` | persistence |"))
        .expect("docs/parity.md has an `ask --no-save` persistence row");
    assert!(row.contains("| implemented |"), "got {row}");

    let session_row = parity
        .lines()
        .find(|line| line.starts_with("| session event log | persistence |"))
        .expect("docs/parity.md has a session event log row");
    assert!(
        session_row.contains("| implemented |"),
        "the flag only means something because the log exists, got {session_row}"
    );
}

#[test]
fn ask_is_advertised_in_the_command_inventory_and_the_parity_ledger() {
    assert!(
        xfx::cli::ADVERTISED_COMMANDS.contains(&"ask"),
        "ask must be in the advertised inventory"
    );
    let parity =
        fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/parity.md"))
            .expect("read parity.md");
    let row = parity
        .lines()
        .find(|line| line.starts_with("| `ask` | command |"))
        .expect("docs/parity.md has an `ask` command row");
    assert!(row.contains("| implemented |"), "got {row}");

    let gateway_row = parity
        .lines()
        .find(|line| line.starts_with("| `Vercel AI Gateway` | provider |"))
        .expect("docs/parity.md has a Gateway provider row");
    assert!(gateway_row.contains("| implemented |"), "got {gateway_row}");
}
