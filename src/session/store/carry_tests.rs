//! Every kind of message a conversation holds, sent to each wire after a
//! provider switch.
//!
//! The history is xfx's own shape -- [`Message`], provider-neutral -- and
//! each wire's encoder renders it ([`crate::gateway::protocol`],
//! [`crate::llmux::protocol`]), as upstream's `ChatMessage` is rendered by
//! each of its providers (`vercel-labs/fx@c1db9191
//! src/core/shared/types.zig:1530-1546`). These cases are the translation
//! table of a switch: a kind, its Gateway shape, its llmux shape, and the one
//! thing that does not translate -- replay state sealed by the other wire.

use std::sync::Arc;

use serde_json::{json, Value};

use super::{apply, DurableState, EventEnvelope, RecordedToolCall, SessionEvent, Wire};
use crate::gateway::protocol::{
    CompletionRequest, ContentPart, Message, ResultStatus, ToolCall, ToolChoice,
};
use crate::image::{MediaType, ToolImage};
use crate::session::event::{TurnConclusion, EVENT_SCHEMA_VERSION};

/// Eight bytes that stand for an image; neither encoder decodes them.
const PICTURE: &[u8] = b"\x89PNG-ish";

fn request(messages: Vec<Message>) -> CompletionRequest {
    CompletionRequest {
        model: "m".to_string(),
        messages,
        tools: Vec::new(),
        tool_choice: ToolChoice::Auto,
    }
}

fn gateway_body(messages: Vec<Message>) -> Value {
    serde_json::from_str(&request(messages).body().expect("the Gateway takes it")).expect("json")
}

fn llmux_body(messages: Vec<Message>) -> Value {
    serde_json::from_str(&crate::llmux::protocol::body(&request(messages)).expect("llmux takes it"))
        .expect("json")
}

fn call(id: &str, name: &str, input: Value) -> ToolCall {
    ToolCall {
        id: id.to_string(),
        name: name.to_string(),
        input,
    }
}

/// One of each: a system message, a prompt with an image, an answer that
/// calls three tools -- one under an id Anthropic does not take -- their
/// three endings (a result that returned an image, a refusal, a failure), a
/// steer, an Anthropic answer that thought before it spoke, and the prompt.
fn every_kind() -> Vec<Message> {
    let image = ContentPart::Image {
        id: 1,
        media_type: MediaType::Png,
        data: Arc::from(PICTURE),
        saved: None,
    };
    vec![
        Message::system("rules"),
        Message::user_with_images("look at [Image #1]", [image]),
        Message::assistant(
            Some("reading"),
            vec![
                call("functions.read:0", "read_file", json!({"path": "a.png"})),
                call("call_2", "write_file", json!({"path": "b"})),
                call("call_3", "terminal", json!({"command": "false"})),
            ],
        ),
        Message::tool_result_with_images(
            "functions.read:0",
            "read_file",
            "image attached",
            vec![ToolImage {
                media_type: MediaType::Png,
                data: Arc::from(PICTURE),
                path: "a.png".to_string(),
            }],
        ),
        Message::tool_ended(
            "call_2",
            "write_file",
            "write_file was not permitted: no",
            ResultStatus::Denied,
        ),
        Message::tool_error("call_3", "terminal", "exit 1"),
        Message::user("also this"),
        Message::assistant_raw(
            vec![
                json!({"type": "thinking", "thinking": "SEALED", "signature": "sig-1"}),
                json!({"type": "text", "text": "fine"}),
            ],
            Vec::new(),
        ),
        Message::user("next"),
    ]
}

#[test]
fn every_kind_of_message_goes_to_the_gateway_in_its_own_shape() {
    let body = gateway_body(every_kind());
    let prompt = body["prompt"].as_array().expect("a prompt");
    let picture = crate::image::base64(PICTURE);
    // The alias the call that Anthropic would refuse goes under -- on this
    // wire too, as upstream projects on every wire (`vercel_protocol.zig:331`).
    let alias = prompt[2]["content"][1]["toolCallId"]
        .as_str()
        .expect("an id")
        .to_string();
    assert!(
        crate::gateway::tool_call_ids::portable(&alias) && alias.starts_with("fx_"),
        "{alias}"
    );
    assert_eq!(
        prompt,
        &vec![
            json!({"role": "system", "content": "rules"}),
            json!({"role": "user", "content": [
                {"type": "text", "text": "look at [Image #1]"},
                {"type": "file", "mediaType": "image/png", "data": {"type": "data", "data": picture}},
            ]}),
            json!({"role": "assistant", "content": [
                {"type": "text", "text": "reading"},
                {"type": "tool-call", "toolCallId": alias, "toolName": "read_file", "input": {"path": "a.png"}},
                {"type": "tool-call", "toolCallId": "call_2", "toolName": "write_file", "input": {"path": "b"}},
                {"type": "tool-call", "toolCallId": "call_3", "toolName": "terminal", "input": {"command": "false"}},
            ]}),
            json!({"role": "tool", "content": [
                {"type": "tool-result", "toolCallId": alias, "toolName": "read_file",
                 "output": {"type": "content", "value": [{"type": "text", "text": "image attached"}]}},
            ]}),
            json!({"role": "tool", "content": [
                {"type": "tool-result", "toolCallId": "call_2", "toolName": "write_file",
                 "output": {"type": "execution-denied", "reason": "write_file was not permitted: no"}},
            ]}),
            json!({"role": "tool", "content": [
                {"type": "tool-result", "toolCallId": "call_3", "toolName": "terminal",
                 "output": {"type": "error-text", "value": "exit 1"}},
            ]}),
            // The results' image, in the user message after the run of them
            // (`write_tool_image_user_message`, `vercel_protocol.zig:679-713`).
            json!({"role": "user", "content": [
                {"type": "text", "text": crate::image::TOOL_IMAGES_FOLLOWUP},
                {"type": "file", "mediaType": "image/png", "data": {"type": "data", "data": picture}},
            ]}),
            json!({"role": "user", "content": [{"type": "text", "text": "also this"}]}),
            // Anthropic's blocks reduced to their text: the signed thinking
            // stays with the wire that sealed it.
            json!({"role": "assistant", "content": [{"type": "text", "text": "fine"}]}),
            json!({"role": "user", "content": [{"type": "text", "text": "next"}]}),
        ]
    );
}

#[test]
fn every_kind_of_message_goes_to_llmux_in_anthropics_shape() {
    let body = llmux_body(every_kind());
    let picture = crate::image::base64(PICTURE);
    let image = json!({"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": picture}});
    assert_eq!(body["system"], "rules");
    let messages = body["messages"].as_array().expect("messages");
    let alias = messages[1]["content"][1]["id"]
        .as_str()
        .expect("an id")
        .to_string();
    assert!(
        crate::gateway::tool_call_ids::portable(&alias) && alias.starts_with("fx_"),
        "{alias}"
    );
    assert_eq!(
        messages,
        &vec![
            json!({"role": "user", "content": [
                {"type": "text", "text": "look at [Image #1]"},
                image,
            ]}),
            json!({"role": "assistant", "content": [
                {"type": "text", "text": "reading"},
                {"type": "tool_use", "id": alias, "name": "read_file", "input": {"path": "a.png"}},
                {"type": "tool_use", "id": "call_2", "name": "write_file", "input": {"path": "b"}},
                {"type": "tool_use", "id": "call_3", "name": "terminal", "input": {"command": "false"}},
            ]}),
            // The three results lead one user message, the steer after them;
            // the result's image inside its own block; a refusal and a
            // failure both `is_error`, Anthropic's one flag for either.
            json!({"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": alias,
                 "content": [{"type": "text", "text": "image attached"}, image]},
                {"type": "tool_result", "tool_use_id": "call_2",
                 "content": "write_file was not permitted: no", "is_error": true},
                {"type": "tool_result", "tool_use_id": "call_3", "content": "exit 1", "is_error": true},
                {"type": "text", "text": "also this"},
            ]}),
            // Its own blocks, back as they arrived, signature and all.
            json!({"role": "assistant", "content": [
                {"type": "thinking", "thinking": "SEALED", "signature": "sig-1"},
                {"type": "text", "text": "fine"},
            ]}),
            json!({"role": "user", "content": [{"type": "text", "text": "next"}]}),
        ]
    );
}

fn reduce(events: Vec<SessionEvent>) -> DurableState {
    let mut state = DurableState::empty();
    for (index, event) in events.into_iter().enumerate() {
        let seq = index as u64 + 1;
        apply(
            &mut state,
            &EventEnvelope {
                schema_version: EVENT_SCHEMA_VERSION,
                log_generation: "a".repeat(32),
                seq,
                event_id: format!("{seq:032x}"),
                timestamp_ms: 1_000 + seq as i64,
                event,
            },
        )
        .expect("a valid log");
    }
    state
}

fn result(call_id: &str, tool: &str, ok: bool, output: &str) -> SessionEvent {
    SessionEvent::ToolResult {
        ending: None,
        lines: None,
        change: None,
        call_id: call_id.to_string(),
        tool: tool.to_string(),
        ok,
        output: output.to_string(),
    }
}

fn concluded() -> SessionEvent {
    SessionEvent::TurnConcluded {
        outcome: TurnConclusion::Final {
            finish_reason: "stop".to_string(),
            steps: 2,
        },
    }
}

/// A Gateway turn -- a call under an id Anthropic does not take, its result,
/// a steer, the answer -- then the switch, then a llmux turn whose first step
/// thought before it called a tool.
fn switched_session() -> DurableState {
    reduce(vec![
        SessionEvent::SessionStarted {
            id: "s1".to_string(),
            origin_workspace_root: "/w".to_string(),
            workspace_root: "/w".to_string(),
            model: "zai/glm-5.2".to_string(),
            permission_mode: "auto".to_string(),
            provider: Some("gateway".to_string()),
        },
        SessionEvent::UserMessage {
            text: "read it".to_string(),
        },
        SessionEvent::AssistantMessage {
            text: String::new(),
            tool_calls: vec![RecordedToolCall {
                id: "functions.read:0".to_string(),
                name: "read_file".to_string(),
                input: json!({"path": "notes.txt"}),
            }],
            raw_content: Vec::new(),
            responses_state: Vec::new(),
            wire: None,
        },
        result("functions.read:0", "read_file", true, "NOTES"),
        SessionEvent::Steered {
            text: "and be quick".to_string(),
        },
        SessionEvent::AssistantMessage {
            text: "GATEWAY-ANSWER".to_string(),
            tool_calls: Vec::new(),
            raw_content: Vec::new(),
            responses_state: Vec::new(),
            wire: None,
        },
        concluded(),
        SessionEvent::PreferencesChanged {
            model: Some("fable".to_string()),
            permission_mode: None,
            provider: Some("llmux".to_string()),
            effort: None,
            fast_mode: None,
            ultrafast_mode: None,
        },
        SessionEvent::UserMessage {
            text: "again".to_string(),
        },
        SessionEvent::AssistantMessage {
            text: String::new(),
            tool_calls: vec![RecordedToolCall {
                id: "toolu_1".to_string(),
                name: "read_file".to_string(),
                input: json!({"path": "notes.txt"}),
            }],
            raw_content: vec![
                json!({"type": "thinking", "thinking": "SEALED", "signature": "sig-1"}),
                json!({"type": "tool_use", "id": "toolu_1", "name": "read_file", "input": {"path": "notes.txt"}}),
            ],
            responses_state: Vec::new(),
            wire: Some(Wire::AnthropicMessages),
        },
        result("toolu_1", "read_file", true, "NOTES"),
        SessionEvent::AssistantMessage {
            text: "LLMUX-ANSWER".to_string(),
            tool_calls: Vec::new(),
            raw_content: vec![json!({"type": "text", "text": "LLMUX-ANSWER"})],
            responses_state: Vec::new(),
            wire: Some(Wire::AnthropicMessages),
        },
        concluded(),
    ])
}

#[test]
fn the_log_says_which_provider_the_conversation_went_on_with() {
    let state = switched_session();
    assert_eq!(state.provider.as_deref(), Some("llmux"));
    assert_eq!(state.model, "fable");
    assert_eq!(state.turns.len(), 2);
}

#[test]
fn a_switched_session_goes_to_llmux_with_its_own_blocks_and_the_gateways_turn_as_text_and_calls() {
    let replay = switched_session().history_messages(Wire::AnthropicMessages);
    assert!(replay.notices.is_empty(), "{:?}", replay.notices);
    let mut messages = replay.messages;
    messages.push(Message::user("third"));
    let body = llmux_body(messages);
    let messages = body["messages"].as_array().expect("messages");
    let alias = messages[1]["content"][0]["id"].as_str().expect("an id");
    assert!(alias.starts_with("fx_"), "{alias}");
    assert_eq!(
        messages,
        &vec![
            json!({"role": "user", "content": [{"type": "text", "text": "read it"}]}),
            json!({"role": "assistant", "content": [
                {"type": "tool_use", "id": alias, "name": "read_file", "input": {"path": "notes.txt"}},
            ]}),
            json!({"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": alias, "content": "NOTES"},
                {"type": "text", "text": "and be quick"},
            ]}),
            json!({"role": "assistant", "content": [{"type": "text", "text": "GATEWAY-ANSWER"}]}),
            json!({"role": "user", "content": [{"type": "text", "text": "again"}]}),
            json!({"role": "assistant", "content": [
                {"type": "thinking", "thinking": "SEALED", "signature": "sig-1"},
                {"type": "tool_use", "id": "toolu_1", "name": "read_file", "input": {"path": "notes.txt"}},
            ]}),
            json!({"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "toolu_1", "content": "NOTES"},
            ]}),
            json!({"role": "assistant", "content": [{"type": "text", "text": "LLMUX-ANSWER"}]}),
            json!({"role": "user", "content": [{"type": "text", "text": "third"}]}),
        ]
    );
}

#[test]
fn a_switched_session_goes_to_the_gateway_without_the_thinking_llmux_sealed() {
    // Upstream sends a step's replay state only to the provider that made it
    // and the rest of the step as it is (`projectProviderReplay`,
    // `types.zig:1548-1563`). xfx drops it the same way and, by its own rule,
    // says so (`.prd/04-providers.md` §Provenance): one line per step whose
    // state stayed behind.
    let replay = switched_session().history_messages(Wire::VercelGateway);
    assert_eq!(replay.notices.len(), 2, "{:?}", replay.notices);
    let mut messages = replay.messages;
    messages.push(Message::user("third"));
    let body = gateway_body(messages);
    let sent = body.to_string();
    assert!(
        !sent.contains("SEALED") && !sent.contains("sig-1"),
        "{sent}"
    );
    let prompt = body["prompt"].as_array().expect("a prompt");
    let alias = prompt[1]["content"][0]["toolCallId"]
        .as_str()
        .expect("an id");
    assert_eq!(
        prompt,
        &vec![
            json!({"role": "user", "content": [{"type": "text", "text": "read it"}]}),
            json!({"role": "assistant", "content": [
                {"type": "tool-call", "toolCallId": alias, "toolName": "read_file", "input": {"path": "notes.txt"}},
            ]}),
            json!({"role": "tool", "content": [
                {"type": "tool-result", "toolCallId": alias, "toolName": "read_file",
                 "output": {"type": "text", "value": "NOTES"}},
            ]}),
            json!({"role": "user", "content": [{"type": "text", "text": "and be quick"}]}),
            json!({"role": "assistant", "content": [{"type": "text", "text": "GATEWAY-ANSWER"}]}),
            json!({"role": "user", "content": [{"type": "text", "text": "again"}]}),
            json!({"role": "assistant", "content": [
                {"type": "tool-call", "toolCallId": "toolu_1", "toolName": "read_file", "input": {"path": "notes.txt"}},
            ]}),
            json!({"role": "tool", "content": [
                {"type": "tool-result", "toolCallId": "toolu_1", "toolName": "read_file",
                 "output": {"type": "text", "value": "NOTES"}},
            ]}),
            json!({"role": "assistant", "content": [{"type": "text", "text": "LLMUX-ANSWER"}]}),
            json!({"role": "user", "content": [{"type": "text", "text": "third"}]}),
        ]
    );
}

#[test]
fn a_compacted_conversation_carries_its_checkpoint_to_either_wire_as_a_user_message() {
    // Upstream's compaction summary is a user message of text (`window.zig`),
    // which both wires take as they take any prompt.
    let mut events = vec![
        SessionEvent::SessionStarted {
            id: "s1".to_string(),
            origin_workspace_root: "/w".to_string(),
            workspace_root: "/w".to_string(),
            model: "m".to_string(),
            permission_mode: "auto".to_string(),
            provider: Some("gateway".to_string()),
        },
        SessionEvent::UserMessage {
            text: "one".to_string(),
        },
        SessionEvent::AssistantMessage {
            text: "ONE".to_string(),
            tool_calls: Vec::new(),
            raw_content: Vec::new(),
            responses_state: Vec::new(),
            wire: None,
        },
        concluded(),
    ];
    let compacted = super::super::checkpoint::Compacted {
        entries: Vec::new(),
        turns: vec![super::super::checkpoint::CompactedTurn {
            number: 1,
            users: vec!["one".to_string()],
            work: String::new(),
            final_reply: "ONE".to_string(),
            first_tool: 0,
            last_tool: 0,
            tools: Vec::new(),
        }],
        open: None,
        turn_count: 1,
        tool_count: 0,
    };
    events.push(SessionEvent::ContextCheckpoint {
        covers_through_seq: 4,
        compacted: compacted.clone(),
    });
    let state = reduce(events);
    let summary = super::super::checkpoint::render(&compacted);
    for wire in [Wire::VercelGateway, Wire::AnthropicMessages] {
        let mut messages = state.history_messages(wire.clone()).messages;
        messages.push(Message::user("two"));
        let text = |body: &Value, at: &str| body[at][0]["content"][0]["text"].clone();
        assert_eq!(
            text(&gateway_body(messages.clone()), "prompt"),
            json!(summary),
            "{wire:?}"
        );
        assert_eq!(
            text(&llmux_body(messages), "messages"),
            json!(summary),
            "{wire:?}"
        );
    }
}
