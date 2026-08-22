//! The Anthropic Messages wire contract, as llmux speaks it.
//!
//! This module owns the exact bytes xfx sends to a llmux daemon. It performs no
//! I/O, so every wire question can be answered by a test that never opens a
//! socket -- the same split [`crate::gateway::protocol`] makes for the Gateway.
//!
//! The request is `{"model", "max_tokens", "stream", "thinking", "system"?,
//! "messages", "tools"?, "tool_choice"?}`, in that order. Serialization is written by hand
//! rather than derived, for the reason the Gateway writer gives: the shape is an
//! external contract with another implementation, so it has to be readable next
//! to the evidence it mirrors, and it must not drift when an internal field is
//! renamed.
//!
//! Two shapes of that contract are load-bearing and are not obvious from a
//! request that happens to work:
//!
//! 1. **Roles alternate.** Anthropic rejects two consecutive messages of the
//!    same role, and xfx's own history routinely produces them -- a tool result
//!    is a `tool` message that becomes a *user* message here, and the next
//!    prompt is another one. Consecutive wire messages of one role are therefore
//!    merged into a single message with several content blocks.
//! 2. **`tool_result` blocks lead the message that carries them.** When a merge
//!    puts tool results and ordinary prompt text in one user message, the
//!    results come first.
//!
//! Evidence for the round trip, including the `tool_use` / `input_json_delta` /
//! `stop_reason: "tool_use"` triple this file's decoder counterpart reads back,
//! is llmux's own end-to-end test (`2lab-ai/llmux@79f66748656b tests/e2e.rs:1117-1169`).

use serde::ser::{SerializeMap, SerializeSeq, Serializer};
use serde::Serialize;
use serde_json::{Map, Value};

use crate::gateway::protocol::{CompletionRequest, ContentPart, ProtocolError, Role, ToolChoice};

/// The output ceiling xfx asks for, in tokens.
///
/// Anthropic requires `max_tokens` on every request and llmux forwards the field
/// as written, so xfx has to name a number, and it is compiled in because
/// nothing in an invocation chooses one.
///
/// What is actually established about 8192 is narrower than "every Claude
/// model": it is within the output ceiling of the sixteen entries the live llmux
/// catalog served when this was measured. xfx does not read a model's ceiling
/// and cannot, because the catalog does not publish one -- so a model whose
/// ceiling is lower would answer 400, and that surfaces as a
/// [`crate::gateway::ProviderError::Status`] naming the daemon and quoting its
/// body rather than as anything silent.
///
/// A completion that *reaches* the ceiling is a different case and is not a
/// failure: it stops with `max_tokens`, which the decoder reports as
/// [`crate::gateway::protocol::FinishReason::Length`] rather than as a normal
/// stop. Truncation is visible, in other words, instead of silent.
pub const MAX_TOKENS: u32 = 8192;

/// Validates the prompt and renders the exact request body.
///
/// Validation is the Gateway's, unchanged and deliberately shared: an orphan
/// tool result or a duplicate call id is a client bug on either wire, and two
/// validators would be two things to keep in step.
pub fn body(request: &CompletionRequest) -> Result<String, ProtocolError> {
    request.validate()?;
    for tool in &request.tools {
        check_tool(tool)?;
    }
    // The mapping drops things -- system messages leave `messages` entirely and
    // an emptied message is dropped -- so a prompt the shared validator accepts
    // can still render to something Anthropic refuses. Checking the *rendered*
    // shape is the only way to catch that before it costs a round trip and
    // arrives as a 400 xfx would then have to explain.
    let messages = wire_messages(&request.messages);
    if messages.is_empty() {
        return Err(ProtocolError::EmptyPrompt);
    }
    if messages[0].role != "user" {
        return Err(ProtocolError::AssistantFirst);
    }
    Ok(serde_json::to_string(&WireRequest(request))
        .expect("a validated request is always serializable"))
}

/// Refuses an advertised tool this wire cannot render honestly.
///
/// The mapping below reverse-engineers the registry's envelope by key name. That
/// is fine while the two agree and silently corrupting when they stop: a renamed
/// `inputSchema` used to produce a tool with **no** `input_schema` at all, and a
/// model handed a schema-less tool invents arguments for something that runs on
/// the operator's machine. Refusing before the socket opens turns a future
/// rename into a failed request instead of a wrong one.
fn check_tool(tool: &Value) -> Result<(), ProtocolError> {
    let quoted = |tool: &Value| {
        tool.get("name")
            .and_then(Value::as_str)
            .map(|name| format!("`{name}`"))
            .unwrap_or_else(|| "with no name".to_string())
    };
    let Some(object) = tool.as_object() else {
        return Err(ProtocolError::UnusableTool {
            tool: "of an unreadable shape".to_string(),
            reason: "is not a JSON object, so its schema cannot be found",
        });
    };
    if !matches!(object.get("name"), Some(Value::String(name)) if !name.is_empty()) {
        return Err(ProtocolError::UnusableTool {
            tool: quoted(tool),
            reason: "has no name, so the provider could not correlate a call to it",
        });
    }
    if !matches!(schema_of(object), Some(Value::Object(_))) {
        return Err(ProtocolError::UnusableTool {
            tool: quoted(tool),
            reason: "carries no `inputSchema` object, and a tool advertised without \
                     one invites the model to invent its arguments",
        });
    }
    Ok(())
}

/// The registry's schema, under either spelling.
fn schema_of(tool: &Map<String, Value>) -> Option<&Value> {
    tool.get("inputSchema").or_else(|| tool.get("input_schema"))
}

/// The request body, in the key order above.
struct WireRequest<'a>(&'a CompletionRequest);

impl Serialize for WireRequest<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let system = system_text(&self.0.messages);
        let messages = wire_messages(&self.0.messages);
        // An empty tool list is omitted rather than written as `[]`: an empty
        // array beside a `tool_choice` asks the model to choose among nothing,
        // which is a request Anthropic can legitimately refuse.
        let tools = (!self.0.tools.is_empty()).then_some(&self.0.tools);

        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("model", &self.0.model)?;
        map.serialize_entry("max_tokens", &MAX_TOKENS)?;
        map.serialize_entry("stream", &true)?;
        map.serialize_entry("thinking", &ThinkingDisabled)?;
        if let Some(system) = &system {
            map.serialize_entry("system", system)?;
        }
        map.serialize_entry("messages", &WireMessages(&messages))?;
        if let Some(tools) = tools {
            map.serialize_entry("tools", &WireTools(tools))?;
            map.serialize_entry("tool_choice", &ToolChoiceBody(self.0.tool_choice))?;
        }
        map.end()
    }
}

/// The `system` field: every system message's text, oldest first.
///
/// A turn emits exactly one system message and it leads the prompt. Any others
/// are folded in here rather than dropped or left in `messages`, because
/// Anthropic has no `system` role in the message list at all -- a system message
/// left there would be rejected, and a dropped one would silently remove an
/// instruction the caller meant to send. `None` when there is nothing to say, so
/// the key is omitted rather than written as `""`.
fn system_text(messages: &[crate::gateway::protocol::Message]) -> Option<String> {
    let mut sections: Vec<String> = Vec::new();
    for message in messages {
        if message.role != Role::System {
            continue;
        }
        let text = message.text();
        if !text.is_empty() {
            sections.push(text);
        }
    }
    (!sections.is_empty()).then(|| sections.join("\n\n"))
}

/// One message on the Anthropic wire, after mapping and merging.
struct WireMessage {
    role: &'static str,
    /// The `tool_result` blocks, which lead the message.
    results: Vec<Value>,
    /// Everything else, in the order it was written.
    blocks: Vec<Value>,
}

impl WireMessage {
    fn is_empty(&self) -> bool {
        self.results.is_empty() && self.blocks.is_empty()
    }
}

/// Maps xfx's prompt onto Anthropic's message list.
///
/// Three steps, in this order, because each one depends on the last: map every
/// message to its blocks, drop the ones that have no blocks left, then merge the
/// consecutive same-role runs the drop may have created.
fn wire_messages(messages: &[crate::gateway::protocol::Message]) -> Vec<WireMessage> {
    let mut merged: Vec<WireMessage> = Vec::new();
    for message in messages {
        let mapped = match message.role {
            // System content is the top-level `system` field, never a message.
            Role::System => continue,
            Role::User | Role::Assistant | Role::Tool => map_message(message),
        };
        if mapped.is_empty() {
            continue;
        }
        match merged.last_mut() {
            Some(previous) if previous.role == mapped.role => {
                previous.results.extend(mapped.results);
                previous.blocks.extend(mapped.blocks);
            }
            _ => merged.push(mapped),
        }
    }
    merged
}

/// One xfx message's content blocks, before merging.
fn map_message(message: &crate::gateway::protocol::Message) -> WireMessage {
    // A tool result is answered by the *user* on this wire: Anthropic has no
    // `tool` role, and the result of a call the assistant made comes back as
    // user content.
    let role = match message.role {
        Role::Assistant => "assistant",
        _ => "user",
    };
    let mut wire = WireMessage {
        role,
        results: Vec::new(),
        blocks: Vec::new(),
    };
    for part in &message.content {
        match part {
            // An empty text part is omitted rather than written as `""`, which
            // is what the Gateway writer does and what Anthropic requires.
            ContentPart::Text { text } if text.is_empty() => {}
            ContentPart::Text { text } => wire.blocks.push(json_block([
                ("type", Value::from("text")),
                ("text", Value::from(text.as_str())),
            ])),
            ContentPart::ToolCall(call) => wire.blocks.push(json_block([
                ("type", Value::from("tool_use")),
                ("id", Value::from(call.id.as_str())),
                ("name", Value::from(call.name.as_str())),
                ("input", call.input.clone()),
            ])),
            ContentPart::ToolResult {
                call_id,
                tool: _,
                output,
            } => wire.results.push(json_block([
                ("type", Value::from("tool_result")),
                ("tool_use_id", Value::from(call_id.as_str())),
                ("content", Value::from(output.as_str())),
            ])),
        }
    }
    wire
}

fn json_block<const N: usize>(entries: [(&str, Value); N]) -> Value {
    let mut map = Map::with_capacity(N);
    for (key, value) in entries {
        map.insert(key.to_string(), value);
    }
    Value::Object(map)
}

struct WireMessages<'a>(&'a [WireMessage]);

impl Serialize for WireMessages<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_seq(Some(self.0.len()))?;
        for message in self.0 {
            seq.serialize_element(&WireMessageBody(message))?;
        }
        seq.end()
    }
}

struct WireMessageBody<'a>(&'a WireMessage);

impl Serialize for WireMessageBody<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(2))?;
        map.serialize_entry("role", self.0.role)?;
        map.serialize_entry("content", &ContentBlocks(self.0))?;
        map.end()
    }
}

/// The content array: results first, then everything else.
struct ContentBlocks<'a>(&'a WireMessage);

impl Serialize for ContentBlocks<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let message = self.0;
        let mut seq =
            serializer.serialize_seq(Some(message.results.len() + message.blocks.len()))?;
        for block in message.results.iter().chain(&message.blocks) {
            seq.serialize_element(block)?;
        }
        seq.end()
    }
}

/// The advertised tool list, renamed into Anthropic's envelope.
struct WireTools<'a>(&'a [Value]);

impl Serialize for WireTools<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_seq(Some(self.0.len()))?;
        for tool in self.0 {
            seq.serialize_element(&anthropic_tool(tool))?;
        }
        seq.end()
    }
}

/// One advertised tool as Anthropic spells it.
///
/// The registry's envelope is `{type, name, description, inputSchema}`
/// (`src/tools/spec.rs` `advertisement`); Anthropic's is
/// `{name, description, input_schema}`. This is a rename and a drop of the
/// Gateway's `function` marker, nothing more -- the schema itself belongs to the
/// registry that owns each tool and is copied through untouched.
///
/// Every tool reaching here has passed [`check_tool`], so the name and the
/// schema are known to be present: this function renames, it does not decide.
fn anthropic_tool(tool: &Value) -> Value {
    let Some(object) = tool.as_object() else {
        return tool.clone();
    };
    let mut mapped = Map::new();
    for key in ["name", "description"] {
        if let Some(value) = object.get(key) {
            mapped.insert(key.to_string(), value.clone());
        }
    }
    if let Some(schema) = schema_of(object) {
        mapped.insert("input_schema".to_string(), schema.clone());
    }
    Value::Object(mapped)
}

/// `{"type":"disabled"}`: xfx does not ask for extended thinking.
///
/// Declared rather than left to the default, because the decoder *drops*
/// thinking blocks and that is only sound while no response can contain one.
/// Relying on the default made the decoder's correctness a bet on a server-side
/// setting xfx does not control; saying it makes the drop a consequence of the
/// request. xfx has nowhere to render reasoning -- it streams the assistant's
/// answer and nothing else -- so asking for it would be paying for tokens that
/// are then discarded.
struct ThinkingDisabled;

impl Serialize for ThinkingDisabled {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(1))?;
        map.serialize_entry("type", "disabled")?;
        map.end()
    }
}

struct ToolChoiceBody(ToolChoice);

impl Serialize for ToolChoiceBody {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Anthropic spells "the model must call something" `any`, where the
        // Gateway spells it `required`; `auto` and `none` are shared spellings.
        let label = match self.0 {
            ToolChoice::Auto => "auto",
            ToolChoice::None => "none",
            ToolChoice::Required => "any",
        };
        let mut map = serializer.serialize_map(Some(1))?;
        map.serialize_entry("type", label)?;
        map.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_message_with_only_empty_text_contributes_nothing() {
        let message = crate::gateway::protocol::Message {
            role: Role::Assistant,
            content: vec![ContentPart::Text {
                text: String::new(),
            }],
        };
        assert!(map_message(&message).is_empty());
    }

    #[test]
    fn a_tool_already_spelling_input_schema_is_not_renamed_twice() {
        assert_eq!(
            anthropic_tool(&json!({ "name": "t", "input_schema": { "type": "object" } })),
            json!({ "name": "t", "input_schema": { "type": "object" } })
        );
        assert!(check_tool(&json!({ "name": "t", "input_schema": {} })).is_ok());
    }

    #[test]
    fn a_refusal_names_the_tool_it_could_not_advertise() {
        let err =
            check_tool(&json!({ "name": "read_file", "description": "d" })).expect_err("no schema");
        let message = err.to_string();
        assert!(message.contains("read_file"), "{message}");
        assert!(message.contains("inputSchema"), "{message}");
    }

    #[test]
    fn system_text_is_absent_rather_than_empty() {
        assert_eq!(
            system_text(&[crate::gateway::protocol::Message::system("")]),
            None
        );
        assert_eq!(
            system_text(&[crate::gateway::protocol::Message::user("hi")]),
            None
        );
    }
}
