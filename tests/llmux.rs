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

use xfx::config::{Backend, Environment, RuntimeConfig};
use xfx::gateway::protocol::{
    Completion, CompletionRequest, FinishReason, Message, ToolCall, ToolChoice,
};
use xfx::gateway::sse::{SseError, MAX_EVENT_BYTES};
use xfx::gateway::{CancelToken, DeltaSink, Endpoint, Provider, ProviderError};
use xfx::llmux::sse::AnthropicReader;
use xfx::llmux::LlmuxProvider;
use xfx::llmux::{protocol, setup};

use support::fake_gateway::Reply;

/// Runs one async library call to completion.
///
/// The setup entry points are `async` because they open sockets; a test that
/// only wants the answer does not need a `#[tokio::test]` around everything else
/// it is asserting.
fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a test runtime")
        .block_on(future)
}
use support::fake_llmux::{
    anthropic_answer, anthropic_error, anthropic_event, anthropic_stop, anthropic_text_block,
    anthropic_tool_answer, anthropic_tool_block, catalog, FakeLlmux,
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
fn a_request_disables_thinking_so_the_decoder_never_has_to_drop_any() {
    // The decoder drops thinking blocks, which is only safe while no response
    // can contain one. Today that holds because xfx never asks for extended
    // thinking -- an absence, not a guarantee. Declaring it makes the decoder's
    // behaviour a consequence of the request rather than a bet on a default.
    let parsed = body_of(&user_request("hi"));
    assert_eq!(parsed["thinking"], json!({ "type": "disabled" }));
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
fn a_tool_without_a_usable_schema_is_refused_before_the_socket_opens() {
    // The rename reverse-engineers the registry's envelope. If that envelope is
    // ever renamed, emitting the tool without `input_schema` would leave the
    // model to invent arguments for a tool that runs on the operator's machine.
    // Silent corruption; a refusal naming the tool is the only honest outcome.
    for tool in [
        json!({ "name": "read_file", "description": "d" }),
        json!({ "name": "read_file", "description": "d", "inputSchema": "not an object" }),
        json!({ "description": "d", "inputSchema": {} }),
        json!({ "name": "", "description": "d", "inputSchema": {} }),
        json!("an opaque tool"),
    ] {
        let request = CompletionRequest {
            model: "fable".to_string(),
            messages: vec![Message::user("hi")],
            tools: vec![tool.clone()],
            tool_choice: ToolChoice::Auto,
        };
        let err = protocol::body(&request).expect_err("`{tool}` is not advertisable");
        assert!(err.to_string().contains("tool"), "for {tool}: {err}");
    }

    // The registry's real advertisement still passes, which is what makes the
    // check a guard rather than a wall.
    let request = CompletionRequest {
        model: "fable".to_string(),
        messages: vec![Message::user("hi")],
        tools: xfx::tools::Registry::builtin().advertisement(),
        tool_choice: ToolChoice::Auto,
    };
    assert!(protocol::body(&request).is_ok());
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
fn a_prompt_that_renders_to_no_usable_messages_is_refused_before_the_socket() {
    use xfx::gateway::protocol::{ContentPart, ProtocolError, Role};

    // System messages leave `messages` entirely and empty-rendering ones are
    // dropped, so a prompt that is only those produces `"messages":[]` -- which
    // Anthropic answers with a 400 xfx would have to explain. And a history
    // beginning with an assistant turn is the other shape Anthropic refuses.
    let only_system = CompletionRequest {
        model: "fable".to_string(),
        messages: vec![Message::system("be terse")],
        tools: Vec::new(),
        tool_choice: ToolChoice::None,
    };
    assert!(
        matches!(
            protocol::body(&only_system),
            Err(ProtocolError::EmptyPrompt)
        ),
        "got {:?}",
        protocol::body(&only_system)
    );

    let all_empty = CompletionRequest {
        model: "fable".to_string(),
        messages: vec![Message {
            role: Role::User,
            content: vec![ContentPart::Text {
                text: String::new(),
            }],
        }],
        tools: Vec::new(),
        tool_choice: ToolChoice::None,
    };
    assert!(matches!(
        protocol::body(&all_empty),
        Err(ProtocolError::EmptyPrompt)
    ));

    let assistant_first = CompletionRequest {
        model: "fable".to_string(),
        messages: vec![
            Message::system("be terse"),
            Message::assistant(Some("I was here first"), Vec::new()),
            Message::user("hi"),
        ],
        tools: Vec::new(),
        tool_choice: ToolChoice::None,
    };
    let err = protocol::body(&assistant_first).expect_err("anthropic requires a user turn first");
    assert!(err.to_string().contains("user"), "got {err}");
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
        anthropic_text_block(0, &["hi"]),
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
fn a_text_delta_on_a_block_that_is_not_text_never_reaches_the_answer() {
    // `Block::Text` and `Block::Opaque` were tracked and never consulted: any
    // text_delta appended to the answer whatever index it named. A daemon that
    // streamed reasoning as `text_delta` on a thinking block would have had it
    // silently spliced into the assistant's reply.
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
                "delta": { "type": "text_delta", "text": "private reasoning" },
            })
        ),
        anthropic_text_block(1, &["the answer"]),
        anthropic_stop("end_turn", 1, 1),
    );
    let (completion, deltas) = decode(&body);
    assert_eq!(
        deltas,
        ["the answer"],
        "only the text block reaches the sink"
    );
    assert_eq!(completion.expect("finish").text, "the answer");
}

#[test]
fn a_text_delta_for_a_block_nobody_opened_is_dropped() {
    let body = format!(
        "{}{}",
        anthropic_event(
            "content_block_delta",
            json!({
                "type": "content_block_delta",
                "index": 7,
                "delta": { "type": "text_delta", "text": "from nowhere" },
            })
        ),
        anthropic_stop("end_turn", 1, 1),
    );
    let (completion, deltas) = decode(&body);
    assert!(deltas.is_empty(), "got {deltas:?}");
    assert_eq!(completion.expect("finish").text, "");
}

#[tokio::test]
async fn a_transient_in_band_error_is_replayable_and_a_client_error_is_not() {
    // Anthropic delivers `overloaded_error` and `rate_limit_error` as HTTP 200
    // plus an `error` frame. Treating every in-band error as unreplayable gave
    // llmux zero retries for the most common transient failure, while the same
    // upstream condition arriving as a 429 on the Gateway path got three.
    for (kind, replayable) in [
        ("overloaded_error", true),
        ("rate_limit_error", true),
        ("api_error", true),
        ("invalid_request_error", false),
        ("authentication_error", false),
        ("permission_error", false),
        ("not_found_error", false),
        ("something_new", false),
    ] {
        let daemon = FakeLlmux::start(vec![Reply::Sse(anthropic_event(
            "error",
            json!({
                "type": "error",
                "error": { "type": kind, "message": "upstream said so" },
            }),
        ))]);
        let err = stream_once(&daemon, &user_request("hi"))
            .await
            .0
            .expect_err("an error frame is not a completion");
        assert_eq!(err.is_replayable(), replayable, "for `{kind}`: {err}");
        assert!(
            err.to_string().contains("upstream said so"),
            "for `{kind}`: {err}"
        );
    }
}

#[test]
fn every_stop_reason_maps_to_a_reason_this_version_knows() {
    for (raw, expected) in [
        ("end_turn", FinishReason::Stop),
        ("stop_sequence", FinishReason::Stop),
        ("max_tokens", FinishReason::Length),
        ("refusal", FinishReason::ContentFilter),
    ] {
        let completion = decode(&anthropic_stop(raw, 1, 1)).0.expect("finish");
        assert_eq!(completion.finish_reason, expected, "for `{raw}`");
    }
    // `tool_use` needs a tool to have been called: the reason and the calls are
    // one claim, and a stream that makes half of it is refused.
    let body = format!(
        "{}{}",
        anthropic_tool_block(0, "toolu_1", "read_file", &["{}"]),
        anthropic_stop("tool_use", 1, 1),
    );
    let completion = decode(&body).0.expect("finish");
    assert_eq!(completion.finish_reason, FinishReason::ToolCalls);
    assert_eq!(completion.tool_calls.len(), 1);
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
        Err(SseError::ProviderFailure { detail, .. }) => {
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
        "data: {}\n\ndata: {}\n\ndata: {}\n\n",
        json!({
            "type": "content_block_start",
            "index": 0,
            "content_block": { "type": "text", "text": "" },
        }),
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
fn a_tool_block_that_cannot_be_closed_is_an_error_not_an_empty_tool_round() {
    // The stop frame's index is a string, so the open `tool_use` block is never
    // correlated. Dropping it silently produced `ToolCalls` with an empty call
    // list -- a completion claiming the model asked for a tool while naming
    // none, which the turn then has to interpret. That is a guess, and this
    // decoder does not guess.
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
            "content_block_stop",
            json!({ "type": "content_block_stop", "index": "0" })
        ),
        anthropic_stop("tool_use", 1, 1),
    );
    match decode(&body).0 {
        Err(SseError::InvalidToolCall { detail }) => {
            assert!(
                detail.contains("toolu_1"),
                "the block must be named: {detail}"
            );
        }
        other => panic!("expected an unusable tool call, got {other:?}"),
    }
}

#[test]
fn a_tool_use_stop_reason_with_no_calls_at_all_is_an_error() {
    // Same lie by a different route: the daemon says it stopped to call a tool
    // and never opened one.
    match decode(&anthropic_stop("tool_use", 1, 1)).0 {
        Err(SseError::InvalidToolCall { .. }) => {}
        other => panic!("expected an unusable tool call, got {other:?}"),
    }
}

#[test]
fn an_error_frame_fails_the_attempt_whatever_arrives_after_it() {
    // A daemon that reports an error and then closes cleanly used to produce a
    // successful completion carrying a truncated answer, because the failure was
    // only consulted when no stop reason had arrived. The frame itself is the
    // failure, so ordering cannot change the verdict.
    let body = format!(
        "{}{}{}{}",
        anthropic_text_block(0, &["half an ans"]),
        anthropic_error("upstream refused"),
        anthropic_stop("end_turn", 1, 1),
        anthropic_event("message_stop", json!({ "type": "message_stop" })),
    );
    match decode(&body).0 {
        Err(SseError::ProviderFailure { detail, .. }) => assert_eq!(detail, "upstream refused"),
        other => panic!("expected a provider failure, got {other:?}"),
    }
}

#[test]
fn input_tokens_include_the_cache_reads_the_prompt_was_billed_for() {
    // A cached prompt reports most of its input under the cache counters. Adding
    // only `input_tokens` reported a fraction of what the turn actually cost,
    // which is the number a session totals up.
    let body = format!(
        "{}{}",
        anthropic_event(
            "message_start",
            json!({
                "type": "message_start",
                "message": {
                    "usage": {
                        "input_tokens": 10,
                        "cache_read_input_tokens": 4000,
                        "cache_creation_input_tokens": 25,
                    },
                },
            })
        ),
        anthropic_stop("end_turn", 10, 7),
    );
    let completion = decode(&body).0.expect("finish");
    assert_eq!(completion.usage.input_tokens, Some(4035));
    assert_eq!(completion.usage.output_tokens, Some(7));
}

#[test]
fn a_completion_that_never_stops_growing_is_bounded() {
    // The module promises boundedness, and the per-event ceiling only bounds one
    // frame: a stream of well-formed small frames could accumulate without limit.
    let frame = anthropic_event(
        "content_block_delta",
        json!({
            "type": "content_block_delta",
            "index": 0,
            "delta": { "type": "text_delta", "text": "x".repeat(64 * 1024) },
        }),
    );
    let mut reader = AnthropicReader::new();
    let mut deltas = Collected::default();
    let opened = anthropic_event(
        "content_block_start",
        json!({
            "type": "content_block_start",
            "index": 0,
            "content_block": { "type": "text", "text": "" },
        }),
    );
    reader
        .push(opened.as_bytes(), &mut deltas)
        .expect("open the block the flood names");
    let mut pushes = 0usize;
    let outcome = loop {
        pushes += 1;
        assert!(pushes < 1024, "the ceiling was never reached");
        if let Err(err) = reader.push(frame.as_bytes(), &mut deltas) {
            break err;
        }
    };
    assert!(
        matches!(outcome, SseError::CompletionTooLarge { .. }),
        "got {outcome:?}"
    );
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
        Err(ProviderError::Protocol(SseError::ProviderFailure { detail, .. })) => {
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
async fn a_llmux_failure_names_the_daemon_rather_than_the_gateway() {
    // Every runtime failure rendered in the Gateway's vocabulary, so a stopped
    // daemon printed "cannot reach the Gateway: Connection refused" and sent the
    // operator to look at Vercel.
    let provider = LlmuxProvider::new(
        xfx::llmux::endpoint("http://127.0.0.1:1", xfx::llmux::URL_KEY).unwrap(),
        CancelToken::new(),
    )
    .expect("build the provider");
    let mut deltas = Collected::default();
    let message = provider
        .stream(&user_request("hi"), &mut deltas)
        .await
        .expect_err("nothing is listening")
        .to_string();
    assert!(message.contains("llmux"), "got {message}");
    assert!(!message.contains("Gateway"), "got {message}");

    // A non-2xx from the daemon says so too.
    let daemon = FakeLlmux::start(vec![Reply::Status(503, "down".to_string())]);
    let message = stream_once(&daemon, &user_request("hi"))
        .await
        .0
        .expect_err("a 503 is not a completion")
        .to_string();
    assert!(message.contains("llmux"), "got {message}");
    assert!(!message.contains("Gateway"), "got {message}");
}

#[tokio::test]
async fn a_redirect_from_the_daemon_never_replays_the_prompt_elsewhere() {
    // The endpoint policy validates the URL xfx *names*. A process holding the
    // configured port can still answer 307 with a `Location` pointing anywhere,
    // and a client that follows redirects replays the POST body -- the whole
    // prompt and the project context -- to it, keyless. The policy cannot see
    // that; only refusing to follow redirects can.
    let elsewhere = FakeLlmux::start(vec![Reply::Sse(anthropic_answer(&["stolen"]))]);
    let daemon = FakeLlmux::start(vec![Reply::Redirect {
        status: 307,
        location: format!("{}/v1/messages", elsewhere.url()),
    }]);

    let outcome = stream_once(&daemon, &user_request("a secret prompt"))
        .await
        .0;
    let Err(err) = outcome else {
        panic!("a redirect is not a completion");
    };
    assert!(
        matches!(err, ProviderError::Status { status: 307, .. }),
        "a 3xx must surface as a status error, got {err:?}"
    );
    assert!(
        elsewhere.requests().is_empty(),
        "the prompt was replayed off-machine: {:?}",
        elsewhere.requests()
    );
}

#[test]
fn setup_does_not_follow_a_redirect_off_the_machine() {
    let elsewhere = FakeLlmux::start(Vec::new());
    let daemon = FakeLlmux::start(Vec::new()).with_root_body(200, "llmux");
    // The catalog probe is what gets redirected here.
    let daemon = daemon.with_catalog_redirect(307, &format!("{}/models", elsewhere.url()));
    let sandbox = Sandbox::new();

    let run = sandbox.run(&["setup", "llmux", "--url", &daemon.url()], &[]);
    assert_eq!(run.code, Some(1), "stdout={:?}", run.stdout);
    assert!(
        elsewhere.requests().is_empty(),
        "the probe followed a redirect off-machine: {:?}",
        elsewhere.requests()
    );
    assert!(!sandbox.settings_path().exists());
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
    "HTTP_PROXY",
    "http_proxy",
    "HTTPS_PROXY",
    "https_proxy",
    "ALL_PROXY",
    "all_proxy",
    "NO_PROXY",
    "no_proxy",
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

    /// A `RuntimeConfig` for this sandbox, loaded the way the binary loads one.
    fn config(&self, vars: &[(&str, &str)]) -> RuntimeConfig {
        RuntimeConfig::load_with(&self.environment(vars), &self.workspace).expect("load config")
    }

    /// The injected environment, carrying only what a test named.
    fn environment(&self, vars: &[(&str, &str)]) -> Environment {
        let map = vars
            .iter()
            .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
            .collect();
        Environment::new(Some(self.home.clone()), map)
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
fn setup_decides_the_model_from_the_file_it_is_writing_not_from_an_env_override() {
    // The keep-or-replace decision used to read the fully layered model, so an
    // `XFX_MODEL` in the shell was persisted into the profile -- destroying the
    // profile's own value -- and the write was then a no-op for that shell,
    // because the env outranks it. Reported, of course, as "kept".
    let daemon =
        FakeLlmux::start(Vec::new()).with_catalog(catalog(&[("a-id", &["a"]), ("b-id", &["b"])]));
    let sandbox = Sandbox::new();
    sandbox.write_user_settings("{\"model\":\"b\"}");

    let run = sandbox.run(
        &["setup", "llmux", "--url", &daemon.url(), "--json"],
        &[("XFX_MODEL", "a")],
    );
    assert_eq!(run.code, Some(0), "stderr={:?}", run.stderr);
    assert_eq!(
        run.json()["model"],
        "b",
        "the profile's own model is what setup is deciding about"
    );

    let settings: Value =
        serde_json::from_str(&std::fs::read_to_string(sandbox.settings_path()).unwrap()).unwrap();
    assert_eq!(
        settings["model"], "b",
        "the env value must not be persisted"
    );

    // And the operator is told that what they just configured is not what the
    // next turn in this shell will use.
    assert!(
        run.stderr.contains("XFX_MODEL"),
        "the override must be named: {:?}",
        run.stderr
    );
    assert_eq!(run.json()["overridden_by"], "XFX_MODEL");
}

#[test]
fn setup_warns_when_a_workspace_entry_will_outrank_what_it_just_wrote() {
    // An exact-workspace entry pinning this directory to the gateway silently
    // outranks the profile setup just wrote, so the receipt would be a lie.
    let daemon = FakeLlmux::start(Vec::new());
    let sandbox = Sandbox::new();
    let workspace_key = sandbox.workspace.to_str().unwrap().to_string();
    sandbox.write_user_settings(&format!(
        "{{\"workspaces\":{{{}:{{\"backend\":\"gateway\"}}}}}}",
        serde_json::to_string(&workspace_key).unwrap()
    ));

    let run = sandbox.run(&["setup", "llmux", "--url", &daemon.url(), "--json"], &[]);
    assert_eq!(run.code, Some(0), "stderr={:?}", run.stderr);
    assert!(
        run.stderr.contains("workspace"),
        "the overriding layer must be named: {:?}",
        run.stderr
    );
    assert!(
        run.json()["overridden_by"]
            .as_str()
            .is_some_and(|s| !s.is_empty()),
        "got {}",
        run.json()
    );
}

#[test]
fn setup_says_nothing_about_overrides_when_there_are_none() {
    let daemon = FakeLlmux::start(Vec::new());
    let sandbox = Sandbox::new();
    let run = sandbox.run(&["setup", "llmux", "--url", &daemon.url(), "--json"], &[]);
    assert_eq!(run.code, Some(0));
    assert_eq!(run.stderr, "");
    assert!(run.json().get("overridden_by").is_none(), "{}", run.json());
}

#[test]
fn setup_prefers_an_id_when_a_catalog_entry_has_no_alias() {
    let daemon = FakeLlmux::start(Vec::new()).with_catalog(catalog(&[("only-an-id", &[])]));
    let sandbox = Sandbox::new();
    let run = sandbox.run(&["setup", "llmux", "--url", &daemon.url(), "--json"], &[]);
    assert_eq!(run.json()["model"], "only-an-id");
}

#[test]
fn every_key_setup_writes_is_a_key_the_loader_reads_back() {
    // Nothing else binds the two sides. `setup` writes three keys by name and
    // the config loader reads three keys by name, in different files, and a
    // rename on either side would leave a setup that reports success and a
    // machine that ignores it. This is the seam that goes red for that.
    let daemon = FakeLlmux::start(Vec::new()).with_catalog(catalog(&[("only-here", &["short"])]));
    let sandbox = Sandbox::new();
    let run = sandbox.run(&["setup", "llmux", "--url", &daemon.url(), "--json"], &[]);
    assert_eq!(run.code, Some(0), "stderr={:?}", run.stderr);
    let report = run.json();

    // Loaded from the same HOME, through the real loader, with nothing else set.
    let config = sandbox.config(&[]);
    assert_eq!(
        config.backend,
        Backend::Llmux,
        "`backend` did not round-trip"
    );
    assert_eq!(
        config.llmux_url.as_deref(),
        Some(daemon.url().as_str()),
        "`llmux_url` did not round-trip"
    );
    assert_eq!(config.model, "short", "`model` did not round-trip");

    // And what the loader resolved is exactly what setup reported.
    assert_eq!(report["url"], config.llmux_url.clone().unwrap());
    assert_eq!(report["model"], config.model);
    assert_eq!(report["backend"], config.backend.label());
    assert!(config.diagnostics.is_empty(), "{:?}", config.diagnostics);
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
fn setup_refuses_a_remote_daemon_before_it_opens_a_socket() {
    // The keyless story is "the request never leaves the machine". An https
    // collector would receive the prompt and the project context with no
    // credential while `status` went on saying `llmux-keyless-loopback`, so a
    // remote host is refused rather than trusted to TLS -- and refused before
    // any network I/O, so naming one cannot even be used as a probe.
    let sandbox = Sandbox::new();
    for url in [
        "https://collector.example.com",
        "https://collector.example.com:443",
        "http://198.51.100.7:3456",
    ] {
        let run = sandbox.run(&["setup", "llmux", "--url", url, "--json"], &[]);
        assert_eq!(run.code, Some(1), "`{url}` must be refused");
        let message = run.json()["message"]
            .as_str()
            .expect("a message")
            .to_string();
        assert!(message.contains("remote"), "`{url}`: {message}");
        assert!(!sandbox.settings_path().exists(), "`{url}` was recorded");
    }
}

#[test]
fn setup_refuses_a_base_url_that_already_carries_a_path() {
    // `http://127.0.0.1:3456/v1` plus the provider's own `/v1/messages` is a
    // path llmux does not match, so it forwards the request upstream keyless and
    // the operator sees an unexplained 401.
    let sandbox = Sandbox::new();
    for url in [
        "http://127.0.0.1:3456/v1",
        "http://127.0.0.1:3456/v1/messages",
    ] {
        let run = sandbox.run(&["setup", "llmux", "--url", url], &[]);
        assert_eq!(run.code, Some(1), "`{url}` must be refused");
        assert!(!sandbox.settings_path().exists());
    }
}

#[test]
fn a_remote_or_pathed_llmux_url_in_the_profile_is_refused_and_the_turn_stops() {
    for url in [
        "https://collector.example.com",
        "http://198.51.100.7:3456",
        "http://127.0.0.1:3456/v1",
    ] {
        let sandbox = Sandbox::new();
        sandbox.write_user_settings(&format!(
            "{{\"backend\":\"llmux\",\"llmux_url\":{}}}",
            serde_json::to_string(url).unwrap()
        ));
        // The value never becomes an endpoint...
        let run = sandbox.run(&["status", "--json"], &[]);
        assert_eq!(run.code, Some(0), "status still describes the machine");
        assert!(
            run.json().get("backend_url").is_none(),
            "`{url}` must not be reported as an endpoint: {}",
            run.json()
        );
        // ...and no turn is sent to it.
        let run = sandbox.run(&["ask", "--json", "--no-save", "hello"], &[]);
        assert_eq!(run.code, Some(1), "`{url}` must refuse the turn");
        let events = run.events();
        let message = events[0]["message"].as_str().expect("a message");
        assert!(message.contains("xfx setup llmux"), "`{url}`: {message}");
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
fn setup_does_not_buffer_an_unbounded_probe_body() {
    // Whatever is on that port decides how many bytes it sends. A probe that
    // materialized the whole body before clipping would let a hostile or merely
    // broken local server choose how much memory `xfx setup` uses.
    let huge = "x".repeat(4 * 1024 * 1024);
    let daemon = FakeLlmux::start(Vec::new()).with_root_body(200, &huge);
    let sandbox = Sandbox::new();
    let run = sandbox.run(&["setup", "llmux", "--url", &daemon.url()], &[]);
    assert_eq!(run.code, Some(1), "stdout={:?}", run.stdout);
    // The quoted body is clipped, so the refusal is readable rather than four
    // megabytes of `x`.
    assert!(
        run.stderr.len() < 4096,
        "the refusal quoted {} bytes",
        run.stderr.len()
    );
    assert!(!sandbox.settings_path().exists());
}

#[test]
fn a_proxy_in_the_environment_never_applies_to_a_loopback_daemon() {
    // The keyless story is that the request stays on the machine. A corporate
    // `HTTP_PROXY` would both break discovery and route the prompt through a
    // third party, so neither llmux client may honour one.
    let daemon = FakeLlmux::start(vec![Reply::Sse(anthropic_answer(&["direct"]))]);
    let sandbox = Sandbox::new();
    // A proxy pointing at a port nothing is listening on: if it were honoured,
    // every request below would fail to connect.
    let dead_proxy = "http://127.0.0.1:1";
    let proxied = [
        ("HTTP_PROXY", dead_proxy),
        ("http_proxy", dead_proxy),
        ("ALL_PROXY", dead_proxy),
        ("all_proxy", dead_proxy),
    ];

    let setup = sandbox.run(
        &["setup", "llmux", "--url", &daemon.url(), "--json"],
        &proxied,
    );
    assert_eq!(setup.code, Some(0), "stderr={:?}", setup.stderr);
    assert_eq!(setup.json()["url"], daemon.url());

    let ask = sandbox.run(&["ask", "--json", "--no-save", "hello"], &proxied);
    assert_eq!(ask.code, Some(0), "stderr={:?}", ask.stderr);
    assert_eq!(ask.events().last().unwrap()["output"], "direct");
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
fn setup_reads_the_llmux_configuration_for_its_port_and_for_nothing_else() {
    // llmux's configuration file holds OAuth tokens and admin keys beside the
    // port. This drives discovery through that file for real -- no `--url` -- so
    // the read actually happens, and then asserts that nothing but the port
    // reached an output or the settings file.
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
    let config = sandbox.config(&[("XDG_CONFIG_HOME", xdg.to_str().unwrap())]);
    let env = sandbox.environment(&[("XDG_CONFIG_HOME", xdg.to_str().unwrap())]);

    // The candidate list really is built from that file...
    let candidates = setup::candidates(&config, &env);
    assert!(
        candidates.contains(&daemon.url()),
        "the configured port must be a candidate: {candidates:?}"
    );
    // ...and nothing but the port came out of it.
    let rendered = format!("{candidates:?}");
    assert!(!rendered.contains("must-not-be-read"), "{rendered}");

    // Discovery is then driven over exactly the candidate that file produced.
    // The default `127.0.0.1:3456` is deliberately left out of this list: on a
    // developer's machine that is a live llmux daemon, and a test that probed it
    // would be reaching real infrastructure.
    let from_config: Vec<String> = candidates
        .into_iter()
        .filter(|candidate| candidate != "http://127.0.0.1:3456")
        .collect();
    assert_eq!(from_config, vec![daemon.url()]);
    let (url, catalog) =
        block_on(setup::discover_in(from_config)).expect("the configured port answers as llmux");
    assert_eq!(url, daemon.url());
    assert_eq!(catalog.len(), 1);
    assert_eq!(daemon.paths(), ["/", "/models"]);
}

#[test]
fn a_failed_discovery_says_what_actually_answered_on_each_port() {
    // "no llmux daemon answered" is true and useless when something *did*
    // answer and was not llmux: the operator has a port conflict, and the
    // message they were given sends them to start a daemon that is running.
    let impostor = FakeLlmux::start(Vec::new()).with_root_body(200, "nginx");
    let message = block_on(setup::discover_in(vec![impostor.url()]))
        .expect_err("an impostor is not a daemon")
        .to_string();
    assert!(message.contains("nginx"), "got {message}");
    assert!(message.contains(&impostor.url()), "got {message}");
}

#[test]
fn discovery_falls_through_a_dead_candidate_to_a_live_one() {
    // The fallthrough itself, hermetically: the first candidate is a port that
    // was bound and released, so nothing is listening on it.
    let dead = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        listener.local_addr().expect("addr").port()
    };
    let daemon = FakeLlmux::start(Vec::new());
    let (url, _) = block_on(setup::discover_in(vec![
        format!("http://127.0.0.1:{dead}"),
        daemon.url(),
    ]))
    .expect("the second candidate answers");
    assert_eq!(url, daemon.url());
}

#[test]
fn setup_looks_first_at_the_daemon_a_previous_setup_recorded() {
    // Re-running setup on a machine whose daemon is not on the default port used
    // to ignore the url every turn was already using and probe 3456 first.
    let daemon = FakeLlmux::start(Vec::new());
    let sandbox = Sandbox::new();
    sandbox.select_llmux(&daemon);
    let candidates = setup::candidates(&sandbox.config(&[]), &sandbox.environment(&[]));
    assert_eq!(
        candidates.first().map(String::as_str),
        Some(daemon.url().as_str()),
        "the url every turn already uses is the first place to look: {candidates:?}"
    );
    // The default is still in the list, behind it, so a daemon that moved back
    // is still found.
    assert!(
        candidates.contains(&"http://127.0.0.1:3456".to_string()),
        "{candidates:?}"
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

// ---------------------------------------------------------------------------
// what status and doctor say about the backend
// ---------------------------------------------------------------------------

#[test]
fn status_names_the_backend_and_the_gateway_stays_the_default() {
    let sandbox = Sandbox::new();
    let document = sandbox.run(&["status", "--json"], &[]).json();
    assert_eq!(document["backend"], "gateway");
    assert!(
        document.get("backend_url").is_none(),
        "the gateway has no configured url to report: {document}"
    );
    assert_eq!(document["auth"], "missing");
    assert!(document.get("auth_help").is_some());

    let text = sandbox.run(&["status"], &[]).stdout;
    assert!(text.contains("[status] backend=gateway"), "{text}");
    // Immediately after the model, which is the fact it qualifies.
    let lines: Vec<&str> = text.lines().collect();
    let model = lines
        .iter()
        .position(|l| l.starts_with("[status] model="))
        .unwrap();
    assert_eq!(lines[model + 1], "[status] backend=gateway", "{text}");
}

#[test]
fn status_reports_a_llmux_backend_as_keyless_rather_than_unauthenticated() {
    let daemon = FakeLlmux::start(Vec::new());
    let sandbox = Sandbox::new();
    sandbox.select_llmux(&daemon);

    let document = sandbox.run(&["status", "--json"], &[]).json();
    assert_eq!(document["backend"], "llmux");
    assert_eq!(document["backend_url"], daemon.url());
    // Not "missing": nothing is missing. Telling an llmux user to set a Vercel
    // token would be advice for a backend they configured away from.
    assert_eq!(document["auth"], "llmux-keyless-loopback");
    assert_eq!(document["auth_refreshable"], json!(false));
    assert!(
        document.get("auth_help").is_none(),
        "there is nothing to fix: {document}"
    );

    let text = sandbox.run(&["status"], &[]).stdout;
    let lines: Vec<&str> = text.lines().collect();
    let model = lines
        .iter()
        .position(|l| l.starts_with("[status] model="))
        .unwrap();
    assert_eq!(lines[model + 1], "[status] backend=llmux", "{text}");
    assert_eq!(
        lines[model + 2],
        format!("[status] backend_url={}", daemon.url()),
        "{text}"
    );
    assert!(!text.contains("auth_help"), "{text}");

    // status reads no network: the daemon was never contacted.
    assert!(daemon.requests().is_empty(), "{:?}", daemon.paths());
}

#[test]
fn status_does_not_describe_an_unrunnable_machine_as_a_healthy_gateway() {
    // `backend_rejected` left the snapshot reading the defaulted `Backend`, so
    // status printed a perfectly ordinary gateway machine -- credential advice
    // and all -- while every `ask` refused.
    let sandbox = Sandbox::new();
    sandbox.write_user_settings("{\"backend\":\"anthropc\"}");
    let run = sandbox.run(&["status", "--json"], &[]);
    assert_eq!(run.code, Some(0), "status must still render");

    let document = run.json();
    assert_eq!(document["backend"], "rejected");
    assert_eq!(document["backend_rejected"], "anthropc");
    let help = document["auth_help"].as_str().unwrap_or_default();
    assert!(
        help.contains("backend"),
        "the help must name the setting: {document}"
    );
    assert!(
        !help.contains("AI_GATEWAY_API_KEY"),
        "no credential advice for a backend nobody chose: {document}"
    );

    let text = sandbox.run(&["status"], &[]).stdout;
    assert!(text.contains("[status] backend=rejected"), "{text}");
    assert!(
        text.contains("[status] backend_rejected=anthropc"),
        "{text}"
    );
}

#[test]
fn status_carries_the_refusal_when_llmux_has_no_endpoint() {
    // The machine cannot run a turn, and status was the one surface that did
    // not say so.
    let sandbox = Sandbox::new();
    sandbox.write_user_settings("{\"backend\":\"llmux\"}");
    let document = sandbox.run(&["status", "--json"], &[]).json();
    assert_eq!(document["backend"], "llmux");
    assert!(document.get("backend_url").is_none(), "{document}");
    let help = document["auth_help"].as_str().unwrap_or_default();
    assert!(help.contains("xfx setup llmux"), "got {document}");
}

#[test]
fn doctor_reports_the_backend_and_adds_no_network_call() {
    let daemon = FakeLlmux::start(Vec::new());
    let sandbox = Sandbox::new();
    sandbox.select_llmux(&daemon);

    let document = sandbox.run(&["doctor", "--json"], &[]).json();
    assert_eq!(document["backend"], "llmux");
    assert_eq!(document["backend_url"], daemon.url());
    assert_eq!(document["auth"], "llmux-keyless-loopback");
    assert_eq!(
        document["fail_count"], 0,
        "a keyless backend is not a missing credential: {document}"
    );
    // `doctor` is the command that is always safe to run, so it stays offline.
    assert!(daemon.requests().is_empty(), "{:?}", daemon.paths());

    let text = sandbox.run(&["doctor"], &[]).stdout;
    assert!(text.contains("[doctor] backend=llmux"), "{text}");
    assert!(
        text.contains(&format!("[doctor] backend_url={}", daemon.url())),
        "{text}"
    );
}

#[test]
fn doctor_fails_when_the_llmux_backend_has_no_endpoint_and_names_the_fix() {
    let sandbox = Sandbox::new();
    sandbox.write_user_settings("{\"backend\":\"llmux\"}");
    let document = sandbox.run(&["doctor", "--json"], &[]).json();

    let checks = document["checks"].as_array().expect("checks");
    let backend = checks
        .iter()
        .find(|check| check["name"] == "backend")
        .unwrap_or_else(|| panic!("no backend check in {document}"));
    // Fail, not warn. Every turn on this machine refuses, so a doctor that
    // reported `fail=0` would be telling the operator their setup is fine while
    // `xfx ask` refuses one hundred percent of the time.
    assert_eq!(backend["status"], "fail");
    let detail = backend["detail"].as_str().expect("a detail");
    assert!(detail.contains("xfx setup llmux"), "got {detail}");
    assert!(
        document["fail_count"].as_u64().unwrap() >= 1,
        "got {document}"
    );

    // A configured backend is not a warning on its own.
    let daemon = FakeLlmux::start(Vec::new());
    sandbox.select_llmux(&daemon);
    let document = sandbox.run(&["doctor", "--json"], &[]).json();
    assert!(
        !document["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|check| check["name"] == "backend"),
        "a working backend needs no check of its own: {document}"
    );
}

#[test]
fn doctor_still_fails_a_gateway_backend_with_no_credential() {
    // The gateway path is untouched: a missing bearer token is still a failure
    // and still names the two variables that fix it.
    let sandbox = Sandbox::new();
    let document = sandbox.run(&["doctor", "--json"], &[]).json();
    assert_eq!(document["backend"], "gateway");
    assert_eq!(document["auth"], "missing");
    let auth = document["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|check| check["name"] == "auth")
        .expect("an auth check")
        .clone();
    assert_eq!(auth["status"], "fail");
    assert!(auth["detail"]
        .as_str()
        .unwrap()
        .contains("AI_GATEWAY_API_KEY"));
}

#[test]
fn a_resumed_llmux_turn_replays_its_history_back_through_the_anthropic_mapping() {
    // The deepest integration risk in this backend: history is stored in xfx's
    // own message shape, and every resume re-renders all of it -- so the merge
    // rule and the role mapping run over real recorded turns rather than over a
    // prompt a test built by hand.
    let daemon = FakeLlmux::start(vec![
        Reply::Sse(anthropic_answer(&["first answer"])),
        Reply::Sse(anthropic_answer(&["second answer"])),
    ]);
    let sandbox = Sandbox::new();
    sandbox.select_llmux(&daemon);

    let first = sandbox.run(&["ask", "--json", "first question"], &[]);
    assert_eq!(first.code, Some(0), "stderr={:?}", first.stderr);
    let second = sandbox.run(
        &["ask", "--json", "--resume", "last", "second question"],
        &[],
    );
    assert_eq!(second.code, Some(0), "stderr={:?}", second.stderr);

    let requests = daemon.message_requests();
    assert_eq!(requests.len(), 2);
    let messages = requests[1].json()["messages"].clone();
    let messages = messages.as_array().expect("a message array");

    // Alternation is what Anthropic requires, and it is what the merge rule
    // exists to guarantee once history is replayed.
    let roles: Vec<&str> = messages
        .iter()
        .map(|message| message["role"].as_str().expect("a role"))
        .collect();
    assert_eq!(roles, ["user", "assistant", "user"], "got {messages:?}");
    for pair in roles.windows(2) {
        assert_ne!(pair[0], pair[1], "two messages of one role in a row");
    }

    assert_eq!(messages[0]["content"][0]["text"], "first question");
    assert_eq!(messages[1]["content"][0]["text"], "first answer");
    assert_eq!(messages[2]["content"][0]["text"], "second question");
    // The system prompt is still the top-level field, never a replayed message.
    assert!(
        !roles.contains(&"system"),
        "a system message must not enter `messages`: {messages:?}"
    );
}
