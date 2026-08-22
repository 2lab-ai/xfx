//! The Vercel AI Gateway wire contract.
//!
//! This module owns the exact bytes xfx sends and the typed shape of what it
//! gets back. It performs no I/O, so every wire question can be answered by a
//! test that never opens a socket.
//!
//! The request is `{"prompt":[...],"tools":[...],"toolChoice":{"type":...}}`,
//! in that order, matching upstream's writer
//! (`vercel-labs/fx@580a0c5d src/core/gateway/gateway_json.zig:333-363`). Roles
//! are not interchangeable: `system` carries a bare string while every other
//! role carries typed content parts
//! (`src/core/gateway/gateway_json.zig:541-655`).
//!
//! Serialization is written by hand rather than derived. The wire shape is an
//! external contract with another implementation, so it must be readable next
//! to the upstream lines it mirrors, and it must not drift when an internal
//! field is renamed.

use std::fmt;

use serde::ser::{SerializeMap, SerializeSeq, Serializer};
use serde::Serialize;
use serde_json::Value;

/// Who a message is from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

impl Role {
    /// The wire label (`src/core/gateway/gateway_json.zig:20-27`).
    pub fn label(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::Tool => "tool",
        }
    }
}

/// A model request to run one tool.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCall {
    /// The provider's correlation identifier. A tool result must repeat it.
    pub id: String,
    pub name: String,
    /// The arguments, as JSON. Kept as a `Value` because the schema belongs to
    /// the tool registry, not to the transport.
    pub input: Value,
}

/// One typed piece of a message's content.
#[derive(Debug, Clone, PartialEq)]
pub enum ContentPart {
    Text {
        text: String,
    },
    ToolCall(ToolCall),
    ToolResult {
        call_id: String,
        tool: String,
        output: String,
    },
}

impl ContentPart {
    /// Whether `role` is allowed to carry this part.
    ///
    /// A tool call on a user message or a tool result on an assistant message is
    /// a client bug that the Gateway would reject; catching it here costs
    /// nothing and catching it there costs a round trip.
    fn allowed_on(&self, role: Role) -> bool {
        match self {
            Self::Text { .. } => matches!(role, Role::System | Role::User | Role::Assistant),
            Self::ToolCall(_) => role == Role::Assistant,
            Self::ToolResult { .. } => role == Role::Tool,
        }
    }

    fn kind(&self) -> &'static str {
        match self {
            Self::Text { .. } => "text",
            Self::ToolCall(_) => "tool-call",
            Self::ToolResult { .. } => "tool-result",
        }
    }
}

/// One message in the prompt.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub role: Role,
    pub content: Vec<ContentPart>,
}

impl Message {
    pub fn system(text: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: vec![ContentPart::Text { text: text.into() }],
        }
    }

    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: vec![ContentPart::Text { text: text.into() }],
        }
    }

    /// An assistant turn: optional text followed by its tool calls, in order.
    pub fn assistant(text: Option<&str>, tool_calls: Vec<ToolCall>) -> Self {
        let mut content = Vec::with_capacity(tool_calls.len() + 1);
        if let Some(text) = text {
            if !text.is_empty() {
                content.push(ContentPart::Text {
                    text: text.to_string(),
                });
            }
        }
        content.extend(tool_calls.into_iter().map(ContentPart::ToolCall));
        Self {
            role: Role::Assistant,
            content,
        }
    }

    /// The result of one tool call, correlated by `call_id`.
    pub fn tool_result(
        call_id: impl Into<String>,
        tool: impl Into<String>,
        output: impl Into<String>,
    ) -> Self {
        Self {
            role: Role::Tool,
            content: vec![ContentPart::ToolResult {
                call_id: call_id.into(),
                tool: tool.into(),
                output: output.into(),
            }],
        }
    }

    /// The concatenated text parts, ignoring calls and results.
    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(|part| match part {
                ContentPart::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect()
    }
}

/// How the model is allowed to use the advertised tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolChoice {
    /// The model decides.
    Auto,
    /// The model may not call a tool.
    None,
    /// The model must call a tool.
    Required,
}

impl ToolChoice {
    /// The wire label (`src/core/shared/types.zig` `ToolChoice.label`, written
    /// into `{"type":<label>}` at `gateway_json.zig:361-363`).
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::None => "none",
            Self::Required => "required",
        }
    }
}

/// Everything one Gateway completion request carries.
#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub model: String,
    /// The prompt, oldest first.
    pub messages: Vec<Message>,
    /// The advertised tool schemas, verbatim.
    ///
    /// The list is closed: it is exactly what the tool registry produced, and a
    /// release with no registry advertises an empty list rather than a
    /// placeholder. The element type is opaque JSON because the schema belongs
    /// to the registry that owns each tool.
    pub tools: Vec<Value>,
    pub tool_choice: ToolChoice,
}

/// A request that must not be sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolError {
    /// There is nothing to ask.
    EmptyPrompt,
    /// Two tool calls in one prompt claim the same identifier, so a result
    /// could not be correlated (`gateway_json.zig:512-523`).
    DuplicateToolCallId { call_id: String },
    /// A tool result names a call that no earlier assistant message made
    /// (`gateway_json.zig:497-523`).
    UnmatchedToolResult { call_id: String },
    /// A content part appeared on a role that cannot carry it.
    MisplacedPart {
        role: &'static str,
        part: &'static str,
    },
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyPrompt => write!(f, "the request has no prompt"),
            Self::DuplicateToolCallId { call_id } => {
                write!(f, "tool call id `{call_id}` appears more than once")
            }
            Self::UnmatchedToolResult { call_id } => write!(
                f,
                "tool result `{call_id}` has no matching assistant tool call"
            ),
            Self::MisplacedPart { role, part } => {
                write!(f, "a `{part}` part cannot appear on a `{role}` message")
            }
        }
    }
}

impl std::error::Error for ProtocolError {}

impl CompletionRequest {
    /// Validates the prompt and renders the exact request body.
    ///
    /// Validation happens here, in the one place that produces bytes, so an
    /// invalid prompt cannot reach the network by taking a different path.
    pub fn body(&self) -> Result<String, ProtocolError> {
        self.validate()?;
        Ok(serde_json::to_string(&WireRequest(self))
            .expect("a validated request is always serializable"))
    }

    /// Checks the prompt without rendering it.
    ///
    /// Visible to the crate because a second wire ([`crate::llmux::protocol`])
    /// renders the same prompt differently and must reject exactly the same
    /// requests: an orphan tool result and a duplicate call id are client bugs
    /// on either wire, and two validators would be two things to keep in step.
    pub(crate) fn validate(&self) -> Result<(), ProtocolError> {
        if self.messages.is_empty() {
            return Err(ProtocolError::EmptyPrompt);
        }

        let mut announced: Vec<&str> = Vec::new();
        for message in &self.messages {
            for part in &message.content {
                if !part.allowed_on(message.role) {
                    return Err(ProtocolError::MisplacedPart {
                        role: message.role.label(),
                        part: part.kind(),
                    });
                }
                match part {
                    ContentPart::ToolCall(call) => {
                        if announced.contains(&call.id.as_str()) {
                            return Err(ProtocolError::DuplicateToolCallId {
                                call_id: call.id.clone(),
                            });
                        }
                        announced.push(&call.id);
                    }
                    ContentPart::ToolResult { call_id, .. } => {
                        if !announced.contains(&call_id.as_str()) {
                            return Err(ProtocolError::UnmatchedToolResult {
                                call_id: call_id.clone(),
                            });
                        }
                    }
                    ContentPart::Text { .. } => {}
                }
            }
        }
        Ok(())
    }
}

/// The request body, written in upstream's key order.
struct WireRequest<'a>(&'a CompletionRequest);

impl Serialize for WireRequest<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(3))?;
        map.serialize_entry("prompt", &WireMessages(&self.0.messages))?;
        map.serialize_entry("tools", &self.0.tools)?;
        map.serialize_entry("toolChoice", &ToolChoiceBody(self.0.tool_choice))?;
        map.end()
    }
}

struct WireMessages<'a>(&'a [Message]);

impl Serialize for WireMessages<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_seq(Some(self.0.len()))?;
        for message in self.0 {
            seq.serialize_element(&WireMessage(message))?;
        }
        seq.end()
    }
}

struct ToolChoiceBody(ToolChoice);

impl Serialize for ToolChoiceBody {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(1))?;
        map.serialize_entry("type", self.0.label())?;
        map.end()
    }
}

struct WireMessage<'a>(&'a Message);

impl Serialize for WireMessage<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(2))?;
        map.serialize_entry("role", self.0.role.label())?;
        // A system message is a bare string; every other role is a typed part
        // array (`gateway_json.zig:552-651`).
        if self.0.role == Role::System {
            map.serialize_entry("content", &self.0.text())?;
        } else {
            map.serialize_entry("content", &WireParts(&self.0.content))?;
        }
        map.end()
    }
}

struct WireParts<'a>(&'a [ContentPart]);

impl Serialize for WireParts<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_seq(None)?;
        for part in self.0 {
            match part {
                // An empty text part is omitted rather than written as `""`,
                // matching `gateway_json.zig:564-571`.
                ContentPart::Text { text } if text.is_empty() => {}
                ContentPart::Text { text } => seq.serialize_element(&TextPart {
                    r#type: "text",
                    text,
                })?,
                ContentPart::ToolCall(call) => seq.serialize_element(&ToolCallPart {
                    r#type: "tool-call",
                    tool_call_id: &call.id,
                    tool_name: &call.name,
                    input: &call.input,
                })?,
                ContentPart::ToolResult {
                    call_id,
                    tool,
                    output,
                } => seq.serialize_element(&ToolResultPart {
                    r#type: "tool-result",
                    tool_call_id: call_id,
                    tool_name: tool,
                    output: TextOutput {
                        r#type: "text",
                        value: output,
                    },
                })?,
            }
        }
        seq.end()
    }
}

#[derive(Serialize)]
struct TextPart<'a> {
    r#type: &'static str,
    text: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ToolCallPart<'a> {
    r#type: &'static str,
    tool_call_id: &'a str,
    tool_name: &'a str,
    input: &'a Value,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ToolResultPart<'a> {
    r#type: &'static str,
    tool_call_id: &'a str,
    tool_name: &'a str,
    output: TextOutput<'a>,
}

#[derive(Serialize)]
struct TextOutput<'a> {
    r#type: &'static str,
    value: &'a str,
}

/// Why the provider stopped generating.
///
/// The unified vocabulary is upstream's
/// (`vercel-labs/fx@580a0c5d src/core/shared/types.zig:919-953`). An unknown
/// value is not mapped onto `Other`: guessing would let a future terminal state
/// be reported as a normal completion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinishReason {
    Stop,
    Length,
    ContentFilter,
    ToolCalls,
    ProviderError,
    Other,
}

impl FinishReason {
    pub fn parse_unified(raw: &str) -> Option<Self> {
        match raw {
            "stop" => Some(Self::Stop),
            "length" => Some(Self::Length),
            "content-filter" => Some(Self::ContentFilter),
            "tool-calls" => Some(Self::ToolCalls),
            "error" => Some(Self::ProviderError),
            "other" => Some(Self::Other),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Stop => "stop",
            Self::Length => "length",
            Self::ContentFilter => "content-filter",
            Self::ToolCalls => "tool-calls",
            Self::ProviderError => "error",
            Self::Other => "other",
        }
    }
}

/// Token counts reported by the finish event.
///
/// Absent is `None` rather than `0`: "the provider did not say" and "the
/// provider said zero" are different facts, and only one of them is a bug.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

/// One completed model response.
#[derive(Debug, Clone, PartialEq)]
pub struct Completion {
    /// Every text delta, concatenated in arrival order.
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
    pub finish_reason: FinishReason,
    pub usage: Usage,
    /// The provider's own failure description, when it sent one.
    pub provider_detail: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn role_labels_are_the_wire_names() {
        assert_eq!(Role::System.label(), "system");
        assert_eq!(Role::User.label(), "user");
        assert_eq!(Role::Assistant.label(), "assistant");
        assert_eq!(Role::Tool.label(), "tool");
    }

    #[test]
    fn finish_reason_labels_round_trip() {
        for reason in [
            FinishReason::Stop,
            FinishReason::Length,
            FinishReason::ContentFilter,
            FinishReason::ToolCalls,
            FinishReason::ProviderError,
            FinishReason::Other,
        ] {
            assert_eq!(FinishReason::parse_unified(reason.label()), Some(reason));
        }
        // The legacy snake_case spellings are not the unified vocabulary
        // (`src/core/shared/types.zig:937-941` keeps them in a separate parser).
        assert_eq!(FinishReason::parse_unified("tool_calls"), None);
        assert_eq!(FinishReason::parse_unified("content_filter"), None);
        assert_eq!(FinishReason::parse_unified(""), None);
    }

    #[test]
    fn an_assistant_constructor_drops_empty_text() {
        let message = Message::assistant(Some(""), Vec::new());
        assert!(message.content.is_empty());
        assert_eq!(message.text(), "");
    }

    #[test]
    fn a_part_is_allowed_only_on_the_role_that_can_carry_it() {
        let text = ContentPart::Text {
            text: "x".to_string(),
        };
        let call = ContentPart::ToolCall(ToolCall {
            id: "c".to_string(),
            name: "t".to_string(),
            input: json!({}),
        });
        let result = ContentPart::ToolResult {
            call_id: "c".to_string(),
            tool: "t".to_string(),
            output: "o".to_string(),
        };
        assert!(text.allowed_on(Role::System) && text.allowed_on(Role::User));
        assert!(!text.allowed_on(Role::Tool));
        assert!(call.allowed_on(Role::Assistant) && !call.allowed_on(Role::User));
        assert!(result.allowed_on(Role::Tool) && !result.allowed_on(Role::Assistant));
    }

    #[test]
    fn a_tool_result_may_correlate_to_a_call_from_an_earlier_step() {
        let request = CompletionRequest {
            model: "m".to_string(),
            messages: vec![
                Message::user("go"),
                Message::assistant(
                    None,
                    vec![
                        ToolCall {
                            id: "a".to_string(),
                            name: "t".to_string(),
                            input: json!({}),
                        },
                        ToolCall {
                            id: "b".to_string(),
                            name: "t".to_string(),
                            input: json!({}),
                        },
                    ],
                ),
                Message::tool_result("b", "t", "second"),
                Message::tool_result("a", "t", "first"),
            ],
            tools: Vec::new(),
            tool_choice: ToolChoice::Auto,
        };
        assert!(request.body().is_ok(), "results may arrive out of order");
    }

    #[test]
    fn advertised_tool_schemas_are_written_verbatim() {
        let request = CompletionRequest {
            model: "m".to_string(),
            messages: vec![Message::user("go")],
            tools: vec![json!({ "type": "function", "name": "read_file" })],
            tool_choice: ToolChoice::Auto,
        };
        let parsed: Value = serde_json::from_str(&request.body().unwrap()).unwrap();
        assert_eq!(
            parsed["tools"],
            json!([{ "type": "function", "name": "read_file" }])
        );
        assert_eq!(parsed["toolChoice"], json!({ "type": "auto" }));
    }
}
