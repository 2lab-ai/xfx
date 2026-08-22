//! The llmux backend: the exact bytes xfx sends to a local llmux daemon, what
//! it accepts back, and the `xfx setup llmux` command that points it there.
//!
//! Three layers are proven here, and each one is a product promise:
//!
//! 1. the Anthropic Messages request body a [`CompletionRequest`] becomes;
//! 2. what xfx accepts back, including every way an Anthropic stream can lie or
//!    stop; and
//! 3. what `xfx ask` and `xfx setup llmux` do against a daemon on loopback.
//!
//! Nothing here talks to a real llmux daemon and nothing here carries an llmux
//! credential: the whole point of the backend is that a loopback request needs
//! none. The fake in `support::fake_llmux` binds an ephemeral port and records
//! exactly what it was sent.

mod support;

use serde_json::{json, Value};

use xfx::gateway::protocol::{
    Completion, CompletionRequest, FinishReason, Message, ToolCall, ToolChoice,
};
use xfx::gateway::sse::{SseError, MAX_EVENT_BYTES};
use xfx::gateway::DeltaSink;
use xfx::llmux::protocol;
use xfx::llmux::sse::AnthropicReader;

use support::fake_llmux::{anthropic_event, anthropic_stop};

// ---------------------------------------------------------------------------
// request serialization
// ---------------------------------------------------------------------------

fn body_of(request: &CompletionRequest) -> Value {
    let body = protocol::body(request).expect("serialize");
    serde_json::from_str(&body).unwrap_or_else(|err| panic!("body is not JSON ({err}): {body}"))
}

fn user_request(prompt: &str) -> CompletionRequest {
    CompletionRequest {
        model: "fable".to_string(),
        messages: vec![Message::user(prompt)],
        tools: Vec::new(),
        tool_choice: ToolChoice::None,
    }
}

#[test]
fn a_request_carries_the_model_a_token_ceiling_and_a_stream_flag() {
    let body = protocol::body(&user_request("hi")).expect("serialize");
    // Anthropic requires `max_tokens`; the value is compiled in because nothing
    // in the invocation chooses it.
    assert!(
        body.starts_with("{\"model\":\"fable\",\"max_tokens\":"),
        "{body}"
    );

    let parsed: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(parsed["model"], "fable");
    assert_eq!(parsed["max_tokens"], json!(protocol::MAX_TOKENS));
    assert_eq!(parsed["stream"], json!(true));
    assert_eq!(
        parsed["messages"],
        json!([{ "role": "user", "content": [{ "type": "text", "text": "hi" }] }])
    );
    // A request with no tools carries neither key: an empty `tools` array with
    // a `tool_choice` beside it asks the model to choose among nothing.
    assert!(parsed.get("tools").is_none(), "{parsed}");
    assert!(parsed.get("tool_choice").is_none(), "{parsed}");
    assert!(parsed.get("system").is_none(), "{parsed}");
}

#[test]
fn system_messages_become_the_top_level_system_field_and_leave_the_prompt() {
    let request = CompletionRequest {
        model: "fable".to_string(),
        messages: vec![
            Message::system("be terse"),
            Message::system("cite files"),
            Message::user("hi"),
        ],
        tools: Vec::new(),
        tool_choice: ToolChoice::None,
    };
    let parsed = body_of(&request);
    assert_eq!(parsed["system"], "be terse\n\ncite files");
    assert_eq!(
        parsed["messages"],
        json!([{ "role": "user", "content": [{ "type": "text", "text": "hi" }] }]),
        "a system message never appears in `messages`"
    );
}

#[test]
fn consecutive_messages_of_one_role_are_merged_because_anthropic_alternates() {
    let request = CompletionRequest {
        model: "fable".to_string(),
        messages: vec![
            Message::user("first"),
            Message::user("second"),
            Message::assistant(Some("thinking"), Vec::new()),
            Message::assistant(Some("still"), Vec::new()),
        ],
        tools: Vec::new(),
        tool_choice: ToolChoice::None,
    };
    assert_eq!(
        body_of(&request)["messages"],
        json!([
            {
                "role": "user",
                "content": [
                    { "type": "text", "text": "first" },
                    { "type": "text", "text": "second" },
                ],
            },
            {
                "role": "assistant",
                "content": [
                    { "type": "text", "text": "thinking" },
                    { "type": "text", "text": "still" },
                ],
            },
        ])
    );
}

#[test]
fn a_tool_round_trip_becomes_tool_use_then_a_tool_result_on_a_user_message() {
    let request = CompletionRequest {
        model: "fable".to_string(),
        messages: vec![
            Message::user("read it"),
            Message::assistant(
                Some("looking"),
                vec![ToolCall {
                    id: "call_1".to_string(),
                    name: "read_file".to_string(),
                    input: json!({ "path": "a.txt" }),
                }],
            ),
            Message::tool_result("call_1", "read_file", "contents"),
        ],
        tools: Vec::new(),
        tool_choice: ToolChoice::None,
    };
    assert_eq!(
        body_of(&request)["messages"],
        json!([
            { "role": "user", "content": [{ "type": "text", "text": "read it" }] },
            {
                "role": "assistant",
                "content": [
                    { "type": "text", "text": "looking" },
                    {
                        "type": "tool_use",
                        "id": "call_1",
                        "name": "read_file",
                        "input": { "path": "a.txt" },
                    },
                ],
            },
            {
                "role": "user",
                "content": [{
                    "type": "tool_result",
                    "tool_use_id": "call_1",
                    "content": "contents",
                }],
            },
        ])
    );
}

#[test]
fn a_tool_result_leads_the_user_message_it_shares_with_typed_text() {
    // Anthropic requires every `tool_result` block to come first in the user
    // message that answers a tool round. The next prompt is an ordinary user
    // message, and the merge rule puts the two in one message.
    let request = CompletionRequest {
        model: "fable".to_string(),
        messages: vec![
            Message::user("go"),
            Message::assistant(
                None,
                vec![
                    ToolCall {
                        id: "a".to_string(),
                        name: "read_file".to_string(),
                        input: json!({}),
                    },
                    ToolCall {
                        id: "b".to_string(),
                        name: "read_file".to_string(),
                        input: json!({}),
                    },
                ],
            ),
            Message::user("and also this"),
            Message::tool_result("a", "read_file", "first"),
            Message::tool_result("b", "read_file", "second"),
        ],
        tools: Vec::new(),
        tool_choice: ToolChoice::None,
    };
    assert_eq!(
        body_of(&request)["messages"][2],
        json!({
            "role": "user",
            "content": [
                { "type": "tool_result", "tool_use_id": "a", "content": "first" },
                { "type": "tool_result", "tool_use_id": "b", "content": "second" },
                { "type": "text", "text": "and also this" },
            ],
        })
    );
}

#[test]
fn an_empty_text_part_is_omitted_and_never_becomes_an_empty_message() {
    // Anthropic refuses a message whose content array is empty, so a message
    // that has nothing left after the empty text is dropped rather than sent.
    let request = CompletionRequest {
        model: "fable".to_string(),
        messages: vec![
            Message::user("hi"),
            Message {
                role: xfx::gateway::protocol::Role::Assistant,
                content: vec![xfx::gateway::protocol::ContentPart::Text {
                    text: String::new(),
                }],
            },
            Message::user("still here"),
        ],
        tools: Vec::new(),
        tool_choice: ToolChoice::None,
    };
    assert_eq!(
        body_of(&request)["messages"],
        json!([{
            "role": "user",
            "content": [
                { "type": "text", "text": "hi" },
                { "type": "text", "text": "still here" },
            ],
        }]),
        "an emptied message is dropped, and the two around it then merge"
    );
}

#[test]
fn advertised_tools_are_renamed_into_the_anthropic_envelope() {
    // The registry's envelope is `{type, name, description, inputSchema}`; the
    // Anthropic one is `{name, description, input_schema}`. Nothing else moves.
    let advertisement = xfx::tools::Registry::builtin().advertisement()[0].clone();
    let request = CompletionRequest {
        model: "fable".to_string(),
        messages: vec![Message::user("hi")],
        tools: vec![advertisement.clone()],
        tool_choice: ToolChoice::Auto,
    };
    let parsed = body_of(&request);
    assert_eq!(
        parsed["tools"],
        json!([{
            "name": advertisement["name"],
            "description": advertisement["description"],
            "input_schema": advertisement["inputSchema"],
        }])
    );
    assert!(
        parsed["tools"][0].get("type").is_none(),
        "the Gateway's function envelope is not an Anthropic tool key"
    );
    assert!(
        parsed["tools"][0].get("inputSchema").is_none(),
        "the camelCase spelling must not survive the rename"
    );
}

#[test]
fn the_tool_choice_table_is_the_anthropic_vocabulary() {
    for (choice, expected) in [
        (ToolChoice::Auto, json!({ "type": "auto" })),
        (ToolChoice::Required, json!({ "type": "any" })),
        (ToolChoice::None, json!({ "type": "none" })),
    ] {
        let request = CompletionRequest {
            model: "fable".to_string(),
            messages: vec![Message::user("hi")],
            tools: vec![json!({ "name": "t", "description": "d", "inputSchema": {} })],
            tool_choice: choice,
        };
        assert_eq!(body_of(&request)["tool_choice"], expected, "{choice:?}");
    }
}

#[test]
fn an_invalid_prompt_is_refused_by_the_same_rules_as_the_gateway() {
    use xfx::gateway::protocol::ProtocolError;

    let orphan = CompletionRequest {
        model: "fable".to_string(),
        messages: vec![Message::tool_result("ghost", "read_file", "x")],
        tools: Vec::new(),
        tool_choice: ToolChoice::None,
    };
    assert!(matches!(
        protocol::body(&orphan),
        Err(ProtocolError::UnmatchedToolResult { .. })
    ));

    let empty = CompletionRequest {
        model: "fable".to_string(),
        messages: Vec::new(),
        tools: Vec::new(),
        tool_choice: ToolChoice::None,
    };
    assert!(matches!(
        protocol::body(&empty),
        Err(ProtocolError::EmptyPrompt)
    ));
}

// ---------------------------------------------------------------------------
// response decoding
// ---------------------------------------------------------------------------

/// The bytes a real llmux daemon on this machine answered a keyless loopback
/// `POST /v1/messages` with, captured on 2026-08-22 and stored verbatim.
///
/// It is a test vector rather than an illustration: the padding whitespace after
/// each JSON document, the `event:` names, and the `ping` frame are all in it
/// because the daemon really sends them, and a decoder written only against the
/// documented shape would be a decoder that has never seen the wire.
const LIVE_FIXTURE: &str = include_str!("support/llmux-live-minimal.sse");

#[derive(Default)]
struct Collected(Vec<String>);

impl DeltaSink for Collected {
    fn text_delta(&mut self, text: &str) -> std::io::Result<()> {
        self.0.push(text.to_string());
        Ok(())
    }
}

/// Decodes a whole body in one push, the way a small answer arrives.
fn decode(body: &str) -> (Result<Completion, SseError>, Vec<String>) {
    let mut deltas = Collected::default();
    let mut reader = AnthropicReader::new();
    match reader.push(body.as_bytes(), &mut deltas) {
        Ok(()) => (reader.finish(), deltas.0),
        Err(err) => (Err(err), deltas.0),
    }
}

#[test]
fn the_live_daemon_stream_decodes_into_one_completion() {
    let (completion, deltas) = decode(LIVE_FIXTURE);
    let completion = completion.expect("the live fixture is a complete answer");
    assert_eq!(deltas, ["OK"], "text arrives as it is decoded");
    assert_eq!(completion.text, "OK");
    assert_eq!(completion.finish_reason, FinishReason::Stop);
    assert_eq!(completion.usage.input_tokens, Some(10));
    assert_eq!(completion.usage.output_tokens, Some(4));
    assert!(completion.tool_calls.is_empty());
    assert_eq!(completion.provider_detail, None);
}

#[test]
fn the_live_daemon_stream_decodes_the_same_one_byte_at_a_time() {
    // Transport boundaries never line up with event boundaries on a real
    // stream, so the state has to live in the decoder rather than in the caller.
    let mut deltas = Collected::default();
    let mut reader = AnthropicReader::new();
    for byte in LIVE_FIXTURE.as_bytes() {
        reader.push(&[*byte], &mut deltas).expect("push one byte");
    }
    let completion = reader.finish().expect("finish");
    assert_eq!(deltas.0, ["OK"]);
    assert_eq!(completion.text, "OK");
    assert_eq!(completion.finish_reason, FinishReason::Stop);
    assert_eq!(completion.usage.output_tokens, Some(4));
}

#[test]
fn a_ping_and_an_unknown_event_carry_nothing_and_stop_nothing() {
    let body = format!(
        "{}{}{}{}",
        anthropic_event("ping", json!({ "type": "ping" })),
        anthropic_event("something_new", json!({ "type": "something_new", "x": 1 })),
        anthropic_event(
            "content_block_delta",
            json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": { "type": "text_delta", "text": "hi" },
            })
        ),
        anthropic_stop("end_turn", 1, 2),
    );
    let (completion, deltas) = decode(&body);
    assert_eq!(deltas, ["hi"]);
    assert_eq!(completion.expect("finish").text, "hi");
}

#[test]
fn a_tool_use_block_is_assembled_from_its_streamed_input_json() {
    let body = format!(
        "{}{}{}{}{}",
        anthropic_event(
            "content_block_start",
            json!({
                "type": "content_block_start",
                "index": 0,
                "content_block": { "type": "tool_use", "id": "toolu_1", "name": "read_file" },
            })
        ),
        anthropic_event(
            "content_block_delta",
            json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": { "type": "input_json_delta", "partial_json": "{\"path\":" },
            })
        ),
        anthropic_event(
            "content_block_delta",
            json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": { "type": "input_json_delta", "partial_json": "\"a.txt\"}" },
            })
        ),
        anthropic_event(
            "content_block_stop",
            json!({ "type": "content_block_stop", "index": 0 })
        ),
        anthropic_stop("tool_use", 7, 9),
    );
    let completion = decode(&body).0.expect("finish");
    assert_eq!(completion.finish_reason, FinishReason::ToolCalls);
    assert_eq!(completion.tool_calls.len(), 1);
    assert_eq!(completion.tool_calls[0].id, "toolu_1");
    assert_eq!(completion.tool_calls[0].name, "read_file");
    assert_eq!(completion.tool_calls[0].input, json!({ "path": "a.txt" }));
    assert_eq!(completion.usage.input_tokens, Some(7));
    assert_eq!(completion.usage.output_tokens, Some(9));
}

#[test]
fn a_tool_use_block_that_streamed_no_input_is_an_empty_object() {
    // Anthropic sends no `input_json_delta` at all for a tool whose schema has
    // no required fields, so "nothing arrived" means `{}` rather than a failure.
    let body = format!(
        "{}{}{}",
        anthropic_event(
            "content_block_start",
            json!({
                "type": "content_block_start",
                "index": 0,
                "content_block": { "type": "tool_use", "id": "toolu_1", "name": "list_files" },
            })
        ),
        anthropic_event(
            "content_block_stop",
            json!({ "type": "content_block_stop", "index": 0 })
        ),
        anthropic_stop("tool_use", 1, 1),
    );
    let completion = decode(&body).0.expect("finish");
    assert_eq!(completion.tool_calls[0].input, json!({}));
}

#[test]
fn an_unparsable_streamed_input_is_rejected_rather_than_guessed() {
    let body = format!(
        "{}{}{}",
        anthropic_event(
            "content_block_start",
            json!({
                "type": "content_block_start",
                "index": 0,
                "content_block": { "type": "tool_use", "id": "toolu_1", "name": "read_file" },
            })
        ),
        anthropic_event(
            "content_block_delta",
            json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": { "type": "input_json_delta", "partial_json": "{\"path\":" },
            })
        ),
        anthropic_event(
            "content_block_stop",
            json!({ "type": "content_block_stop", "index": 0 })
        ),
    );
    match decode(&body).0 {
        Err(SseError::InvalidToolCall { detail }) => {
            assert!(detail.contains("toolu_1"), "the id must be named: {detail}");
        }
        other => panic!("expected an invalid tool call, got {other:?}"),
    }
}

#[test]
fn two_tool_blocks_claiming_one_id_are_refused() {
    let mut body = String::new();
    for index in [0, 1] {
        body.push_str(&anthropic_event(
            "content_block_start",
            json!({
                "type": "content_block_start",
                "index": index,
                "content_block": { "type": "tool_use", "id": "toolu_1", "name": "read_file" },
            }),
        ));
        body.push_str(&anthropic_event(
            "content_block_stop",
            json!({ "type": "content_block_stop", "index": index }),
        ));
    }
    assert!(matches!(
        decode(&body).0,
        Err(SseError::DuplicateToolCallId { .. })
    ));
}

#[test]
fn a_thinking_block_is_tracked_and_its_deltas_reach_nobody() {
    let body = format!(
        "{}{}{}{}",
        anthropic_event(
            "content_block_start",
            json!({
                "type": "content_block_start",
                "index": 0,
                "content_block": { "type": "thinking", "thinking": "" },
            })
        ),
        anthropic_event(
            "content_block_delta",
            json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": { "type": "thinking_delta", "thinking": "hmm" },
            })
        ),
        anthropic_event(
            "content_block_stop",
            json!({ "type": "content_block_stop", "index": 0 })
        ),
        anthropic_stop("end_turn", 1, 1),
    );
    let (completion, deltas) = decode(&body);
    assert!(deltas.is_empty(), "reasoning is not the answer: {deltas:?}");
    assert_eq!(completion.expect("finish").text, "");
}

#[test]
fn every_stop_reason_maps_to_a_reason_this_version_knows() {
    for (raw, expected) in [
        ("end_turn", FinishReason::Stop),
        ("stop_sequence", FinishReason::Stop),
        ("max_tokens", FinishReason::Length),
        ("tool_use", FinishReason::ToolCalls),
        ("refusal", FinishReason::ContentFilter),
    ] {
        let completion = decode(&anthropic_stop(raw, 1, 1)).0.expect("finish");
        assert_eq!(completion.finish_reason, expected, "for `{raw}`");
    }
}

#[test]
fn an_unknown_stop_reason_is_an_error_rather_than_a_guess() {
    // Mapping an unrecognized terminal state onto a known one would report a
    // future refusal, pause, or quota stop as a normal completion.
    match decode(&anthropic_stop("pause_turn", 1, 1)).0 {
        Err(SseError::UnknownFinishReason { raw }) => assert_eq!(raw, "pause_turn"),
        other => panic!("expected an unknown finish reason, got {other:?}"),
    }
}

#[test]
fn a_stream_that_stops_without_saying_why_is_not_an_answer() {
    // Truncation is a failure, not a short answer. `message_stop` alone does not
    // say the model finished -- only `message_delta` carries a stop reason.
    let partial = anthropic_event(
        "content_block_delta",
        json!({
            "type": "content_block_delta",
            "index": 0,
            "delta": { "type": "text_delta", "text": "half an ans" },
        }),
    );
    assert!(matches!(decode(&partial).0, Err(SseError::MissingFinish)));

    let stopped = format!(
        "{partial}{}",
        anthropic_event("message_stop", json!({ "type": "message_stop" }))
    );
    assert!(matches!(decode(&stopped).0, Err(SseError::MissingFinish)));
}

#[test]
fn an_error_event_under_a_two_hundred_is_still_a_provider_failure() {
    // llmux reports an upstream failure as an SSE `error` frame inside a 200
    // response, so a transport that only looked at the status would call this a
    // successful empty answer.
    let body = anthropic_event(
        "error",
        json!({
            "type": "error",
            "error": { "type": "overloaded_error", "message": "upstream is overloaded" },
        }),
    );
    match decode(&body).0 {
        Err(SseError::ProviderFailure { detail }) => {
            assert_eq!(detail, "upstream is overloaded");
        }
        other => panic!("expected a provider failure, got {other:?}"),
    }
}

#[test]
fn a_frame_with_no_event_name_still_decodes() {
    // The `event:` line is optional in SSE and the payload carries the type, so
    // a bare `data:` frame has to decode identically.
    let body = format!(
        "data: {}\n\ndata: {}\n\n",
        json!({
            "type": "content_block_delta",
            "index": 0,
            "delta": { "type": "text_delta", "text": "bare" },
        }),
        json!({ "type": "message_delta", "delta": { "stop_reason": "end_turn" } }),
    );
    let (completion, deltas) = decode(&body);
    assert_eq!(deltas, ["bare"]);
    assert_eq!(
        completion.expect("finish").finish_reason,
        FinishReason::Stop
    );
}

#[test]
fn everything_after_the_stop_frame_is_trailer() {
    let body = format!(
        "{}{}{}",
        anthropic_stop("end_turn", 1, 1),
        anthropic_event("message_stop", json!({ "type": "message_stop" })),
        anthropic_event(
            "content_block_delta",
            json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": { "type": "text_delta", "text": "too late" },
            })
        ),
    );
    let (completion, deltas) = decode(&body);
    assert!(deltas.is_empty(), "got {deltas:?}");
    assert_eq!(completion.expect("finish").text, "");
}

#[test]
fn a_single_event_is_bounded() {
    let mut reader = AnthropicReader::new();
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
