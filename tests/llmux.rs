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

use std::path::PathBuf;

use serde_json::{json, Value};
use tempfile::TempDir;

use xfx::gateway::protocol::{
    Completion, CompletionRequest, FinishReason, Message, ToolCall, ToolChoice,
};
use xfx::gateway::sse::{SseError, MAX_EVENT_BYTES};
use xfx::gateway::{CancelToken, DeltaSink, Endpoint, Provider, ProviderError};
use xfx::llmux::protocol;
use xfx::llmux::sse::AnthropicReader;
use xfx::llmux::LlmuxProvider;

use support::fake_gateway::Reply;
use support::fake_llmux::{
    anthropic_answer, anthropic_error, anthropic_event, anthropic_stop, anthropic_text_block,
    anthropic_tool_answer, catalog, FakeLlmux,
};

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

// ---------------------------------------------------------------------------
// the provider, over one real loopback socket
// ---------------------------------------------------------------------------

fn provider_for(daemon: &FakeLlmux, cancel: CancelToken) -> LlmuxProvider {
    LlmuxProvider::new(
        Endpoint::checked(&daemon.url(), "llmux_url").expect("a loopback url"),
        cancel,
    )
    .expect("build the provider")
}

async fn stream_once(
    daemon: &FakeLlmux,
    request: &CompletionRequest,
) -> (Result<Completion, ProviderError>, Vec<String>) {
    let provider = provider_for(daemon, CancelToken::new());
    let mut deltas = Collected::default();
    let outcome = provider.stream(request, &mut deltas).await;
    (outcome, deltas.0)
}

#[tokio::test]
async fn a_completion_is_posted_to_the_messages_path_with_no_credential_header() {
    let daemon = FakeLlmux::start(vec![Reply::Sse(anthropic_answer(&["Hello, ", "world"]))]);
    let (completion, deltas) = stream_once(&daemon, &user_request("hi")).await;

    let completion = completion.expect("the daemon answered");
    assert_eq!(deltas, ["Hello, ", "world"]);
    assert_eq!(completion.text, "Hello, world");
    assert_eq!(completion.finish_reason, FinishReason::Stop);

    let request = daemon.only_message_request();
    assert_eq!(request.method, "POST");
    assert_eq!(request.path, "/v1/messages");
    assert_eq!(request.header("content-type"), Some("application/json"));
    assert_eq!(request.header("accept"), Some("text/event-stream"));
    assert_eq!(request.header("anthropic-version"), Some("2023-06-01"));
    assert_eq!(
        request.header("user-agent"),
        Some(&*format!("xfx/{}", env!("CARGO_PKG_VERSION"))),
        "xfx identifies itself as itself"
    );
    // The credential story of this backend is that there is no credential.
    // Anything here would be a secret xfx had no reason to hold.
    for header in ["authorization", "x-api-key", "proxy-authorization"] {
        assert_eq!(request.header(header), None, "`{header}` must not be sent");
    }
    assert_eq!(request.json()["model"], "fable");
    assert_eq!(request.json()["stream"], json!(true));
}

#[tokio::test]
async fn a_tool_round_arrives_as_a_completion_that_names_its_calls() {
    let daemon = FakeLlmux::start(vec![Reply::Sse(anthropic_tool_answer(
        "toolu_1",
        "read_file",
        "{\"path\":\"a.txt\"}",
    ))]);
    let completion = stream_once(&daemon, &user_request("read it"))
        .await
        .0
        .expect("the daemon answered");
    assert_eq!(completion.finish_reason, FinishReason::ToolCalls);
    assert_eq!(completion.tool_calls.len(), 1);
    assert_eq!(completion.tool_calls[0].name, "read_file");
    assert_eq!(completion.tool_calls[0].input, json!({ "path": "a.txt" }));
}

#[tokio::test]
async fn a_stream_split_across_chunks_decodes_the_same() {
    // Transport boundaries never line up with frame boundaries on a real
    // connection, and a fake that only ever wrote whole frames would never say so.
    let body = anthropic_answer(&["one", "two"]);
    let pieces: Vec<String> = body
        .as_bytes()
        .chunks(7)
        .map(|chunk| String::from_utf8_lossy(chunk).into_owned())
        .collect();
    let daemon = FakeLlmux::start(vec![Reply::SsePieces(pieces)]);
    let (completion, deltas) = stream_once(&daemon, &user_request("hi")).await;
    assert_eq!(deltas, ["one", "two"]);
    assert_eq!(completion.expect("the daemon answered").text, "onetwo");
}

#[tokio::test]
async fn a_stream_that_ends_without_a_stop_reason_is_not_a_short_answer() {
    // The body is complete as HTTP -- the daemon really did stop here -- and it
    // still never said why the model stopped. Text arrived; an answer did not.
    let daemon = FakeLlmux::start(vec![Reply::Sse(anthropic_text_block(0, &["half"]))]);
    let (outcome, deltas) = stream_once(&daemon, &user_request("hi")).await;
    assert_eq!(deltas, ["half"], "what did arrive still reached the reader");
    assert!(
        matches!(
            outcome,
            Err(ProviderError::Protocol(SseError::MissingFinish))
        ),
        "got {outcome:?}"
    );
}

#[tokio::test]
async fn a_connection_that_dies_mid_body_is_never_replayed() {
    // A chunked body cut off without its terminating chunk breaks the HTTP
    // framing itself, so this is a transport failure rather than a decode one --
    // and either way the request may already have been processed and paid for,
    // which is why it is not replayable.
    let daemon = FakeLlmux::start(vec![Reply::SseThenAbort(vec![anthropic_text_block(
        0,
        &["half"],
    )])]);
    let (outcome, deltas) = stream_once(&daemon, &user_request("hi")).await;
    assert_eq!(deltas, ["half"], "what did arrive still reached the reader");
    let Err(err) = outcome else {
        panic!("a body that stopped mid-delivery is not a completion");
    };
    assert!(
        matches!(err, ProviderError::Transport { .. }),
        "got {err:?}"
    );
    assert!(!err.is_replayable(), "delivery had already started");
}

#[tokio::test]
async fn an_error_frame_inside_a_two_hundred_fails_the_attempt_with_its_message() {
    let daemon = FakeLlmux::start(vec![Reply::Sse(anthropic_error("model is overloaded"))]);
    match stream_once(&daemon, &user_request("hi")).await.0 {
        Err(ProviderError::Protocol(SseError::ProviderFailure { detail })) => {
            assert_eq!(detail, "model is overloaded");
        }
        other => panic!("expected a provider failure, got {other:?}"),
    }
}

#[tokio::test]
async fn a_non_success_status_carries_the_bounded_body_and_its_replay_verdict() {
    let daemon = FakeLlmux::start(vec![Reply::retry_after(429, 2, "slow down")]);
    let outcome = stream_once(&daemon, &user_request("hi")).await.0;
    let Err(err) = outcome else {
        panic!("a 429 is not a completion");
    };
    assert!(err.is_replayable(), "an edge 429 delivered nothing");
    assert_eq!(err.retry_after(), Some(std::time::Duration::from_secs(2)));
    let message = err.to_string();
    assert!(message.contains("429"), "{message}");
    assert!(message.contains("slow down"), "{message}");

    let daemon = FakeLlmux::start(vec![Reply::Status(400, "bad request".to_string())]);
    let outcome = stream_once(&daemon, &user_request("hi")).await.0;
    let Err(err) = outcome else {
        panic!("a 400 is not a completion");
    };
    assert!(
        !err.is_replayable(),
        "a rejected request must not be resent"
    );
}

#[tokio::test]
async fn a_daemon_that_is_not_listening_is_a_replayable_connect_failure() {
    // A port nothing is bound to: the payload provably never left.
    let provider = LlmuxProvider::new(
        Endpoint::checked("http://127.0.0.1:1", "llmux_url").unwrap(),
        CancelToken::new(),
    )
    .expect("build the provider");
    let mut deltas = Collected::default();
    let err = provider
        .stream(&user_request("hi"), &mut deltas)
        .await
        .expect_err("nothing is listening");
    assert!(matches!(err, ProviderError::Connect { .. }), "got {err:?}");
    assert!(err.is_replayable());
}

#[tokio::test]
async fn cancelling_a_started_stream_ends_the_attempt() {
    // A stream that has begun answering and stopped is the only state a user
    // can actually interrupt.
    let daemon = FakeLlmux::start(vec![Reply::SseThenHang(vec![anthropic_text_block(
        0,
        &["thinking"],
    )])]);
    let cancel = CancelToken::new();
    let provider = provider_for(&daemon, cancel.clone());
    // Set from another OS thread, the way the real interrupt handler sets it:
    // xfx's runtime is single-threaded and the stream is what is blocking it.
    let signal = cancel.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(100));
        signal.cancel();
    });
    let mut deltas = Collected::default();
    let outcome = provider.stream(&user_request("hi"), &mut deltas).await;
    assert!(
        matches!(outcome, Err(ProviderError::Cancelled)),
        "got {outcome:?}"
    );
}

// ---------------------------------------------------------------------------
// `xfx ask` against the llmux backend
// ---------------------------------------------------------------------------

/// Environment variables that must never leak in from the developer's shell.
const CONTROLLED_VARS: &[&str] = &[
    "VERCEL_OIDC_TOKEN",
    "AI_GATEWAY_API_KEY",
    "XFX_MODEL",
    "XFX_PERMISSION_MODE",
    "XFX_MAX_AGENT_STEPS",
    "XFX_GATEWAY_URL",
    "LLMUX_CONFIG",
    "XDG_CONFIG_HOME",
];

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
        std::fs::create_dir_all(&home).expect("create home");
        std::fs::create_dir_all(&workspace).expect("create workspace");
        Self {
            home: home.canonicalize().expect("canonicalize home"),
            workspace: workspace.canonicalize().expect("canonicalize workspace"),
            _root: root,
        }
    }

    fn profile_dir(&self) -> PathBuf {
        self.home.join(".xfx")
    }

    fn settings_path(&self) -> PathBuf {
        self.profile_dir().join("settings.json")
    }

    fn write_user_settings(&self, body: &str) {
        let dir = self.profile_dir();
        std::fs::create_dir_all(&dir).expect("create profile dir");
        // 0700, the way xfx creates it: the session store refuses a profile home
        // that group or other can reach, so a fixture that left it 0755 would
        // fail every recorded turn for a reason that has nothing to do with the
        // backend under test.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))
                .expect("tighten the profile dir");
        }
        std::fs::write(self.settings_path(), body).expect("write user settings");
    }

    /// Points the profile at `daemon`, the way `setup` would.
    fn select_llmux(&self, daemon: &FakeLlmux) {
        self.write_user_settings(&format!(
            "{{\"backend\":\"llmux\",\"llmux_url\":{},\"model\":\"fable\"}}",
            serde_json::to_string(&daemon.url()).unwrap()
        ));
    }

    fn run(&self, args: &[&str], env: &[(&str, &str)]) -> Run {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_xfx"));
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
    fn of(output: std::process::Output) -> Self {
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
            .map(|event| event["kind"].as_str().expect("a kind").to_string())
            .collect()
    }

    /// Exactly one JSON document on stdout.
    fn json(&self) -> Value {
        assert_eq!(
            self.stdout.matches('\n').count(),
            1,
            "expected one document, got {:?}",
            self.stdout
        );
        serde_json::from_str(self.stdout.trim_end()).expect("stdout parses as JSON")
    }
}

#[test]
fn ask_streams_a_turn_through_the_llmux_backend_without_any_credential() {
    let daemon = FakeLlmux::start(vec![Reply::Sse(anthropic_answer(&["Hello, ", "world"]))]);
    let sandbox = Sandbox::new();
    sandbox.select_llmux(&daemon);

    // No `VERCEL_OIDC_TOKEN`, no `AI_GATEWAY_API_KEY`: the whole point is that
    // this backend needs neither, and the gateway's missing-auth refusal must
    // not fire on a turn that was never going to the gateway.
    let run = sandbox.run(&["ask", "--json", "--no-save", "hello"], &[]);
    assert_eq!(run.code, Some(0), "stderr={:?}", run.stderr);
    assert_eq!(run.kinds(), ["assistant_delta", "assistant_delta", "final"]);
    assert_eq!(run.events()[2]["output"], "Hello, world");

    let request = daemon.only_message_request();
    assert_eq!(request.path, "/v1/messages");
    assert_eq!(request.json()["model"], "fable");
    assert_eq!(request.header("authorization"), None);
}

#[test]
fn ask_advertises_the_registry_to_llmux_in_the_anthropic_envelope() {
    let daemon = FakeLlmux::start(vec![Reply::Sse(anthropic_answer(&["ok"]))]);
    let sandbox = Sandbox::new();
    sandbox.select_llmux(&daemon);
    assert_eq!(
        sandbox
            .run(&["ask", "--json", "--no-save", "hello"], &[])
            .code,
        Some(0)
    );

    let tools = daemon.only_message_request().json()["tools"].clone();
    let names: Vec<String> = tools
        .as_array()
        .expect("a tool array")
        .iter()
        .map(|tool| tool["name"].as_str().expect("a name").to_string())
        .collect();
    assert_eq!(names, xfx::tools::ADVERTISED_TOOLS);
    for tool in tools.as_array().unwrap() {
        assert!(tool.get("input_schema").is_some(), "{tool}");
        assert!(tool.get("inputSchema").is_none(), "{tool}");
    }
}

#[test]
fn a_llmux_turn_records_a_session_the_same_way_a_gateway_turn_does() {
    let daemon = FakeLlmux::start(vec![Reply::Sse(anthropic_answer(&["remembered"]))]);
    let sandbox = Sandbox::new();
    sandbox.select_llmux(&daemon);
    let asked = sandbox.run(&["ask", "--json", "hello"], &[]);
    assert_eq!(
        asked.code,
        Some(0),
        "stdout={:?} stderr={:?}",
        asked.stdout,
        asked.stderr
    );

    let listed = sandbox.run(&["sessions", "--json"], &[]);
    assert_eq!(listed.code, Some(0), "stderr={:?}", listed.stderr);
    assert_eq!(listed.json()["count"], 1);

    let detail = sandbox.run(&["session", "last", "--json"], &[]);
    assert_eq!(detail.json()["model"], "fable");
    assert_eq!(detail.json()["history_turns"], 1);
}

#[test]
fn a_llmux_backend_with_no_url_refuses_the_turn_and_names_the_setup_command() {
    let sandbox = Sandbox::new();
    sandbox.write_user_settings("{\"backend\":\"llmux\"}");
    let run = sandbox.run(&["ask", "--json", "--no-save", "hello"], &[]);
    assert_eq!(run.code, Some(1), "stdout={:?}", run.stdout);
    assert_eq!(run.kinds(), ["error"]);

    let message = run.events()[0]["message"]
        .as_str()
        .expect("an error message")
        .to_string();
    assert!(message.contains("xfx setup llmux"), "got {message}");
    // Pointing at the gateway's credentials would be advice for a backend the
    // operator deliberately configured away from.
    assert!(!message.contains("AI_GATEWAY_API_KEY"), "got {message}");
}

#[test]
fn a_llmux_backend_whose_url_was_refused_also_names_the_setup_command() {
    let sandbox = Sandbox::new();
    sandbox.write_user_settings("{\"backend\":\"llmux\",\"llmux_url\":\"http://example.com:80\"}");
    let run = sandbox.run(&["ask", "--json", "--no-save", "hello"], &[]);
    assert_eq!(run.code, Some(1), "stdout={:?}", run.stdout);
    let events = run.events();
    let message = events[0]["message"].as_str().expect("a message");
    assert!(message.contains("xfx setup llmux"), "got {message}");
}

#[test]
fn the_gateway_backend_is_unchanged_when_nothing_selects_llmux() {
    // The default path still demands a credential and still says so in the
    // gateway's own words.
    let sandbox = Sandbox::new();
    let run = sandbox.run(&["ask", "--json", "--no-save", "hello"], &[]);
    assert_eq!(run.code, Some(1));
    let events = run.events();
    let message = events[0]["message"].as_str().expect("a message");
    assert!(message.contains("AI_GATEWAY_API_KEY"), "got {message}");
    assert!(!message.contains("llmux"), "got {message}");
}

// ---------------------------------------------------------------------------
// `xfx setup llmux`
// ---------------------------------------------------------------------------

#[test]
fn setup_probes_an_explicit_url_and_records_it_in_the_profile() {
    let daemon = FakeLlmux::start(Vec::new()).with_catalog(catalog(&[
        ("claude-fable-5[1m]", &["fable"]),
        ("gpt-x", &[]),
    ]));
    let sandbox = Sandbox::new();
    let run = sandbox.run(&["setup", "llmux", "--url", &daemon.url(), "--json"], &[]);
    assert_eq!(run.code, Some(0), "stderr={:?}", run.stderr);

    let document = run.json();
    assert_eq!(document["kind"], "setup");
    assert_eq!(document["backend"], "llmux");
    assert_eq!(document["url"], daemon.url());
    assert_eq!(document["models"], 2, "the catalog size, not the catalog");
    assert_eq!(document["model"], "fable", "the first entry's first alias");
    assert_eq!(
        document["settings_path"],
        sandbox.settings_path().display().to_string()
    );

    // Both probes ran, and no completion was asked for: setup must not spend a
    // token to prove a daemon is there.
    assert_eq!(daemon.paths(), ["/", "/models"]);
    assert!(daemon.message_requests().is_empty());

    let settings: Value =
        serde_json::from_str(&std::fs::read_to_string(sandbox.settings_path()).unwrap()).unwrap();
    assert_eq!(settings["backend"], "llmux");
    assert_eq!(settings["llmux_url"], daemon.url());
    assert_eq!(settings["model"], "fable");
}

#[test]
fn setup_writes_a_profile_that_the_next_turn_actually_uses() {
    // The receipt that matters: the file setup wrote is the file the loader
    // reads, so a turn right afterwards goes to the daemon with no further
    // configuration and no credential.
    let daemon = FakeLlmux::start(vec![Reply::Sse(anthropic_answer(&["configured"]))]);
    let sandbox = Sandbox::new();
    assert_eq!(
        sandbox
            .run(&["setup", "llmux", "--url", &daemon.url()], &[])
            .code,
        Some(0)
    );

    let run = sandbox.run(&["ask", "--json", "--no-save", "hello"], &[]);
    assert_eq!(run.code, Some(0), "stderr={:?}", run.stderr);
    assert_eq!(run.events().last().unwrap()["output"], "configured");
    assert_eq!(daemon.only_message_request().json()["model"], "fable");
}

#[test]
fn setup_prints_the_daemon_the_catalog_size_the_model_and_the_file_it_wrote() {
    let daemon = FakeLlmux::start(Vec::new());
    let sandbox = Sandbox::new();
    let run = sandbox.run(&["setup", "llmux", "--url", &daemon.url()], &[]);
    assert_eq!(run.code, Some(0), "stderr={:?}", run.stderr);
    for expected in [
        format!("[setup] url={}", daemon.url()),
        "[setup] models=1".to_string(),
        "[setup] model=fable".to_string(),
        format!(
            "[setup] settings_path={}",
            sandbox.settings_path().display()
        ),
    ] {
        assert!(
            run.stdout.contains(&expected),
            "missing `{expected}` in {:?}",
            run.stdout
        );
    }
    assert_eq!(run.stderr, "", "a success writes no diagnostic");
}

#[test]
fn setup_keeps_a_configured_model_the_catalog_actually_has() {
    let daemon =
        FakeLlmux::start(Vec::new()).with_catalog(catalog(&[("a-id", &["a"]), ("b-id", &["b"])]));
    let sandbox = Sandbox::new();

    // Named by alias.
    sandbox.write_user_settings("{\"model\":\"b\"}");
    let run = sandbox.run(&["setup", "llmux", "--url", &daemon.url(), "--json"], &[]);
    assert_eq!(
        run.json()["model"],
        "b",
        "a configured model that exists is kept"
    );

    // Named by id.
    sandbox.write_user_settings("{\"model\":\"b-id\"}");
    let run = sandbox.run(&["setup", "llmux", "--url", &daemon.url(), "--json"], &[]);
    assert_eq!(run.json()["model"], "b-id");

    // Not in the catalog at all: the daemon's own first entry wins, and setup
    // says which of the two happened.
    sandbox.write_user_settings("{\"model\":\"vendor/not-here\"}");
    let run = sandbox.run(&["setup", "llmux", "--url", &daemon.url(), "--json"], &[]);
    assert_eq!(run.json()["model"], "a");
    assert!(
        run.json()["model_reason"]
            .as_str()
            .is_some_and(|reason| !reason.is_empty()),
        "setup must say why it chose: {:?}",
        run.json()
    );
}

#[test]
fn setup_prefers_an_id_when_a_catalog_entry_has_no_alias() {
    let daemon = FakeLlmux::start(Vec::new()).with_catalog(catalog(&[("only-an-id", &[])]));
    let sandbox = Sandbox::new();
    let run = sandbox.run(&["setup", "llmux", "--url", &daemon.url(), "--json"], &[]);
    assert_eq!(run.json()["model"], "only-an-id");
}

#[test]
fn setup_merges_into_existing_settings_without_touching_an_unrelated_key() {
    let daemon = FakeLlmux::start(Vec::new());
    let sandbox = Sandbox::new();
    sandbox.write_user_settings(
        "{\"permission_mode\":\"ask\",\"max_agent_steps\":7,\
         \"workspaces\":{\"/somewhere\":{\"model\":\"kept\"}}}",
    );
    assert_eq!(
        sandbox
            .run(&["setup", "llmux", "--url", &daemon.url()], &[])
            .code,
        Some(0)
    );

    let settings: Value =
        serde_json::from_str(&std::fs::read_to_string(sandbox.settings_path()).unwrap()).unwrap();
    assert_eq!(settings["permission_mode"], "ask");
    assert_eq!(settings["max_agent_steps"], 7);
    assert_eq!(settings["workspaces"]["/somewhere"]["model"], "kept");
    assert_eq!(settings["backend"], "llmux");
}

#[test]
fn setup_refuses_to_clobber_settings_it_cannot_read() {
    // The file is the operator's. xfx does not get to replace bytes it failed to
    // understand, because "could not parse" and "is not worth keeping" are not
    // the same claim.
    let daemon = FakeLlmux::start(Vec::new());
    let sandbox = Sandbox::new();
    sandbox.write_user_settings("{ this is not json");
    let run = sandbox.run(&["setup", "llmux", "--url", &daemon.url()], &[]);
    assert_eq!(run.code, Some(1), "stdout={:?}", run.stdout);
    assert!(
        run.stderr
            .contains(&sandbox.settings_path().display().to_string()),
        "the refusal must name the file: {:?}",
        run.stderr
    );
    assert_eq!(
        std::fs::read_to_string(sandbox.settings_path()).unwrap(),
        "{ this is not json",
        "the operator's bytes are untouched"
    );
}

#[cfg(unix)]
#[test]
fn setup_creates_a_private_profile_home_and_a_private_settings_file() {
    use std::os::unix::fs::PermissionsExt;

    let daemon = FakeLlmux::start(Vec::new());
    let sandbox = Sandbox::new();
    assert!(!sandbox.profile_dir().exists(), "nothing exists yet");
    assert_eq!(
        sandbox
            .run(&["setup", "llmux", "--url", &daemon.url()], &[])
            .code,
        Some(0)
    );

    let dir_mode = std::fs::metadata(sandbox.profile_dir())
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(dir_mode, 0o700, "the profile home is owner-only");
    let file_mode = std::fs::metadata(sandbox.settings_path())
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(file_mode, 0o600, "settings are owner-only");

    // Nothing staged is left behind by a write that completed.
    let leftovers: Vec<String> = std::fs::read_dir(sandbox.profile_dir())
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name != "settings.json")
        .collect();
    assert!(leftovers.is_empty(), "got {leftovers:?}");
}

#[test]
fn setup_refuses_a_url_the_endpoint_rule_would_not_accept() {
    let sandbox = Sandbox::new();
    for url in [
        "http://example.com/",
        "http://127.0.0.1",
        "ftp://127.0.0.1:3456",
        "not a url",
    ] {
        let run = sandbox.run(&["setup", "llmux", "--url", url], &[]);
        assert_eq!(run.code, Some(1), "`{url}` must be refused");
        assert!(
            !sandbox.settings_path().exists(),
            "`{url}` must not be recorded"
        );
    }
}

#[test]
fn setup_refuses_a_server_that_does_not_identify_itself_as_the_daemon() {
    // Any HTTP server on loopback can answer 200. Only llmux answers `llmux`,
    // and recording a URL that is something else would point every later turn at
    // whatever happened to be listening on that port.
    let daemon = FakeLlmux::start(Vec::new()).with_root_body(200, "nginx");
    let sandbox = Sandbox::new();
    let run = sandbox.run(&["setup", "llmux", "--url", &daemon.url()], &[]);
    assert_eq!(run.code, Some(1), "stdout={:?}", run.stdout);
    assert!(!sandbox.settings_path().exists());
    assert!(run.stderr.contains("llmux"), "got {:?}", run.stderr);
}

#[test]
fn setup_refuses_a_daemon_whose_catalog_is_unusable() {
    let sandbox = Sandbox::new();
    for daemon in [
        FakeLlmux::start(Vec::new()).with_catalog_response(500, "boom"),
        FakeLlmux::start(Vec::new()).with_catalog_response(200, "not json"),
        FakeLlmux::start(Vec::new()).with_catalog(json!({ "models": [] })),
        FakeLlmux::start(Vec::new()).with_catalog(json!({ "nothing": true })),
    ] {
        let run = sandbox.run(&["setup", "llmux", "--url", &daemon.url()], &[]);
        assert_eq!(run.code, Some(1), "stdout={:?}", run.stdout);
        assert!(!sandbox.settings_path().exists());
    }
}

// Discovery with no `--url` is deliberately *not* exercised here. Its first
// candidate is `http://127.0.0.1:3456`, which on a developer's machine is a real
// llmux daemon: a test of it would reach live infrastructure, and its outcome
// would depend on whether that daemon happened to be running. The rule itself --
// the default port first, then the port llmux's own configuration names, never a
// scan and never anything off this machine -- is a pure function proven in
// `src/llmux/setup.rs`, and the reading of `proxy.port` out of each documented
// config location is proven there too.

#[test]
fn setup_never_copies_a_field_of_the_llmux_configuration_but_the_port() {
    // llmux's configuration file holds OAuth tokens and admin keys beside the
    // port. `--url` is used so the probe never reaches the real daemon; what is
    // under test is that the config is read for one `u16` and nothing else
    // reaches an output or the settings file.
    let daemon = FakeLlmux::start(Vec::new());
    let sandbox = Sandbox::new();
    let xdg = sandbox.home.join("xdg");
    std::fs::create_dir_all(&xdg).unwrap();
    std::fs::write(
        xdg.join("llmux.json"),
        json!({
            "proxy": { "port": daemon.port() },
            "accounts": [{ "oauth_token": "must-not-be-read" }],
        })
        .to_string(),
    )
    .unwrap();

    let run = sandbox.run(
        &["setup", "llmux", "--url", &daemon.url(), "--json"],
        &[("XDG_CONFIG_HOME", xdg.to_str().unwrap())],
    );
    assert_eq!(run.code, Some(0), "stderr={:?}", run.stderr);
    assert_eq!(run.json()["url"], daemon.url());
    assert!(!run.stdout.contains("must-not-be-read"), "{:?}", run.stdout);
    assert!(!run.stderr.contains("must-not-be-read"), "{:?}", run.stderr);

    let settings = std::fs::read_to_string(sandbox.settings_path()).unwrap();
    assert!(
        !settings.contains("must-not-be-read"),
        "no llmux config field may be persisted: {settings}"
    );
}

#[test]
fn a_failed_setup_still_emits_exactly_one_json_document() {
    let sandbox = Sandbox::new();
    let run = sandbox.run(
        &["setup", "llmux", "--url", "http://example.com/", "--json"],
        &[],
    );
    assert_eq!(run.code, Some(1));
    let document = run.json();
    assert_eq!(document["kind"], "error");
    assert!(
        document["message"].as_str().is_some_and(|m| !m.is_empty()),
        "got {document}"
    );
    assert_eq!(run.stderr, "", "a --json failure keeps stderr clean");
}
