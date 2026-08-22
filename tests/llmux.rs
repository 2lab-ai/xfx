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

use xfx::gateway::protocol::{CompletionRequest, Message, ToolCall, ToolChoice};
use xfx::llmux::protocol;

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
