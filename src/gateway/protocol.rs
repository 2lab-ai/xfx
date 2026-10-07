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

use crate::provider::Wire;

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
        /// Whether the call failed -- refused, broken, or stopped under its
        /// turn (`aborted by user`, `vercel-labs/fx@c1db9191
        /// src/core/session/session.zig:64`). Each wire has its own word for
        /// it: the Gateway's output type `error-text`, which upstream gives
        /// every result whose status is a failure
        /// (`src/gateway/vercel_protocol.zig:742-747`), and Anthropic's
        /// `is_error`.
        error: bool,
        /// The images the tool returned with its text -- `read_file` of an
        /// image file (`read_file.zig:228-278`) -- empty for almost every
        /// result. Each wire carries them its own way: the Gateway in a user
        /// message after the run of results (`vercel_protocol.zig:679-713`),
        /// Anthropic inside the `tool_result` block.
        images: Vec<crate::image::ToolImage>,
    },
    /// The provider's own content blocks, kept exactly as they arrived.
    ///
    /// Only an assistant message may carry these, and only the Anthropic wire
    /// emits them. They exist because that wire has a replay contract xfx cannot
    /// satisfy by paraphrase: a continuation must return the assistant's prior
    /// `thinking` blocks unchanged, with the signatures Anthropic verifies, and
    /// a signature cannot be reconstructed from the text it signed.
    ///
    /// The Gateway serializer derives its ordinary text-and-tool-call shape from
    /// these rather than refusing them, so a session recorded against one
    /// backend and continued on the other degrades instead of failing.
    RawBlocks(Vec<Value>),
    /// An image the user attached, as its bytes.
    ///
    /// Only a user message carries one, after the text whose `[Image #N]`
    /// names it, in the order the images were attached
    /// (`appendUserImageParts`, `vercel-labs/fx@c1db9191
    /// src/gateway/vercel_protocol.zig:778-817`). The bytes are the snapshot
    /// taken when it was attached ([`crate::image::Image::bytes`]), so every
    /// request that carries it -- this turn's and each later one that re-sends
    /// the history -- carries the same bytes.
    Image {
        /// The number the text's `[Image #N]` names it by.
        id: u32,
        media_type: crate::image::MediaType,
        data: std::sync::Arc<[u8]>,
        /// Where the session keeps its copy, when it keeps one: what the note
        /// a request that cannot carry it says in its place
        /// ([`crate::image::withhold`]). Never on the wire.
        saved: Option<std::sync::Arc<std::path::Path>>,
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
            Self::RawBlocks(_) => role == Role::Assistant,
            Self::Image { .. } => role == Role::User,
        }
    }

    fn kind(&self) -> &'static str {
        match self {
            Self::Text { .. } => "text",
            Self::ToolCall(_) => "tool-call",
            Self::ToolResult { .. } => "tool-result",
            Self::RawBlocks(_) => "raw-blocks",
            Self::Image { .. } => "file",
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

    /// A user message and the images it names, the text first and the images
    /// after it in the order they were attached -- upstream's order
    /// (`vercel_protocol.zig:778-817`). Each item is a [`ContentPart::Image`]
    /// ([`crate::image::Image::part`]).
    pub fn user_with_images(
        text: impl Into<String>,
        images: impl IntoIterator<Item = ContentPart>,
    ) -> Self {
        let mut message = Self::user(text);
        message.content.extend(images);
        message
    }

    /// Whether this message carries an image.
    pub fn has_image(&self) -> bool {
        self.content
            .iter()
            .any(|part| matches!(part, ContentPart::Image { .. }))
    }

    /// An assistant turn carrying the provider's own content blocks.
    ///
    /// Used when a completion brought `raw_content`: the blocks go back exactly
    /// as they arrived, because that is what the wire that produced them
    /// requires. `tool_calls` is kept alongside so prompt validation and the
    /// turn machine still see the calls without having to interpret the blocks.
    pub fn assistant_raw(blocks: Vec<Value>, tool_calls: Vec<ToolCall>) -> Self {
        let mut content = Vec::with_capacity(tool_calls.len() + 1);
        content.push(ContentPart::RawBlocks(blocks));
        content.extend(tool_calls.into_iter().map(ContentPart::ToolCall));
        Self {
            role: Role::Assistant,
            content,
        }
    }

    /// The provider's own blocks, when this message carries them.
    pub fn raw_blocks(&self) -> Option<&[Value]> {
        self.content.iter().find_map(|part| match part {
            ContentPart::RawBlocks(blocks) => Some(blocks.as_slice()),
            _ => None,
        })
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
                error: false,
                images: Vec::new(),
            }],
        }
    }

    /// A result that came back with images ([`ContentPart::ToolResult::images`]).
    pub fn tool_result_with_images(
        call_id: impl Into<String>,
        tool: impl Into<String>,
        output: impl Into<String>,
        images: Vec<crate::image::ToolImage>,
    ) -> Self {
        Self {
            role: Role::Tool,
            content: vec![ContentPart::ToolResult {
                call_id: call_id.into(),
                tool: tool.into(),
                output: output.into(),
                error: false,
                images,
            }],
        }
    }

    /// The answer to a call that failed, said as a failure on the wire
    /// ([`ContentPart::ToolResult::error`]).
    pub fn tool_error(
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
                error: true,
                images: Vec::new(),
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

/// What a session asks of every request beside the prompt: today, the
/// reasoning effort.
///
/// **Held by the provider rather than carried by the request**
/// ([`super::Provider::set_options`]), because it is a property of the session
/// and not of one prompt: `/model`'s effort column sets it once, and every
/// request after that -- a turn's first and each tool round after it -- carries
/// it until it is set again. A field on [`CompletionRequest`] would have to be
/// threaded through the agent loop that builds one per round, for a value none
/// of that loop reads.
///
/// `reasoning` is already decided: the effort the session is in, **if** the
/// model in force lists it (`crate::provider::effort::Effort::for_request`).
/// Each wire spells it its own way.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RequestOptions {
    /// The effort level to ask for, as the catalog names it; `None` asks for
    /// nothing and the provider decides.
    pub reasoning: Option<String>,
    /// Fast mode, for a model that offers it: the Gateway's
    /// `providerOptions.gateway.speed = "fast"` (`writeProviderOptions`,
    /// `vercel_protocol.zig:469-472`).
    pub fast: bool,
    /// Ultra mode, for an OpenAI model on the Gateway that offers it:
    /// `providerOptions.gateway.only = ["openai"]` and
    /// `providerOptions.openai.serviceTier = "ultrafast"` (`:478-494`). Never
    /// set with [`Self::fast`]; if both were, Ultra is what is sent, as
    /// upstream's writer does.
    pub ultrafast: bool,
}

/// The effort the Gateway's language-model route is sent for `label`.
///
/// The route validates against its own tiers and refuses `max`, which the
/// catalog still advertises for some models, so `max` is sent as `xhigh`, the
/// highest it takes (`reasoningWireValue`, `vercel_protocol.zig:302-308`,
/// `c1db919`; upstream's commit `09ec38c2`).
fn reasoning_wire_value(label: &str) -> &str {
    if label == "max" {
        "xhigh"
    } else {
        label
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
    /// The rendered prompt does not begin with a user turn.
    ///
    /// Anthropic requires it. The Gateway does not, which is why this lives
    /// beside [`Self::EmptyPrompt`] rather than replacing it.
    AssistantFirst,
    /// An advertised tool could not be rendered for the provider's wire.
    ///
    /// The tool list is opaque JSON because the schema belongs to the registry,
    /// but a wire that has to *rename* a key inside it has to be able to find
    /// that key. A tool sent without its schema is not a smaller tool: the model
    /// is left to invent arguments for something that runs on the operator's
    /// machine.
    UnusableTool { tool: String, reason: &'static str },
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
            Self::AssistantFirst => write!(
                f,
                "the conversation must begin with a user message; this one begins \
                 with an assistant turn"
            ),
            Self::UnusableTool { tool, reason } => {
                write!(f, "the advertised tool {tool} {reason}")
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
        self.body_with(&RequestOptions::default())
    }

    /// [`body`](Self::body), with what the session asks of every request
    /// ([`RequestOptions`]).
    ///
    /// The effort is a top-level `reasoning` string after `toolChoice`, the
    /// key and the place upstream writes it (`buildGatewayRequestBodyValidated`,
    /// `vercel_protocol.zig:376-379`), and absent when there is none to ask for.
    pub fn body_with(&self, options: &RequestOptions) -> Result<String, ProtocolError> {
        self.validate()?;
        Ok(serde_json::to_string(&WireRequest(self, options))
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
                    // Validation reads the tool calls from their own parts,
                    // which `assistant_raw` keeps alongside the blocks exactly
                    // so that duplicate-id and orphan-result checking does not
                    // have to interpret provider JSON.
                    ContentPart::Text { .. }
                    | ContentPart::RawBlocks(_)
                    | ContentPart::Image { .. } => {}
                }
            }
        }
        Ok(())
    }
}

/// The request body, written in upstream's key order.
struct WireRequest<'a>(&'a CompletionRequest, &'a RequestOptions);

impl Serialize for WireRequest<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("prompt", &WireMessages(&self.0.messages))?;
        map.serialize_entry("tools", &self.0.tools)?;
        map.serialize_entry("toolChoice", &ToolChoiceBody(self.0.tool_choice))?;
        if let Some(reasoning) = &self.1.reasoning {
            map.serialize_entry("reasoning", reasoning_wire_value(reasoning))?;
        }
        // `writeProviderOptions`: the key only when there is something in it,
        // `gateway` before `openai`.
        if self.1.ultrafast {
            map.serialize_entry(
                "providerOptions",
                &serde_json::json!({
                    "gateway": {"only": ["openai"]},
                    "openai": {"serviceTier": "ultrafast"},
                }),
            )?;
        } else if self.1.fast {
            map.serialize_entry(
                "providerOptions",
                &serde_json::json!({"gateway": {"speed": "fast"}}),
            )?;
        }
        map.end()
    }
}

struct WireMessages<'a>(&'a [Message]);

impl Serialize for WireMessages<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_seq(None)?;
        for (index, message) in self.0.iter().enumerate() {
            seq.serialize_element(&WireMessage(message))?;
            // **The tool results' images ride in a user message after the run
            // of results**, upstream's one image position every Gateway route
            // reads (`write_tool_image_user_message`,
            // `vercel_protocol.zig:679-713`): its sentence, then a file part
            // per image in the order the results stand.
            let run_ends = message.role == Role::Tool
                && self
                    .0
                    .get(index + 1)
                    .is_none_or(|next| next.role != Role::Tool);
            if !run_ends {
                continue;
            }
            let start = self.0[..index]
                .iter()
                .rposition(|earlier| earlier.role != Role::Tool)
                .map_or(0, |at| at + 1);
            let images: Vec<&crate::image::ToolImage> = self.0[start..=index]
                .iter()
                .flat_map(|result| result.content.iter())
                .flat_map(|part| match part {
                    ContentPart::ToolResult { images, .. } => images.iter(),
                    _ => [].iter(),
                })
                .collect();
            if !images.is_empty() {
                seq.serialize_element(&ToolImagesMessage(&images))?;
            }
        }
        seq.end()
    }
}

/// The user message after a run of tool results that returned images.
struct ToolImagesMessage<'a>(&'a [&'a crate::image::ToolImage]);

impl Serialize for ToolImagesMessage<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        struct Parts<'a>(&'a [&'a crate::image::ToolImage]);
        impl Serialize for Parts<'_> {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                let mut seq = serializer.serialize_seq(Some(self.0.len() + 1))?;
                seq.serialize_element(&TextPart {
                    r#type: "text",
                    text: crate::image::TOOL_IMAGES_FOLLOWUP,
                })?;
                for image in self.0 {
                    seq.serialize_element(&file_part(image.media_type, &image.data))?;
                }
                seq.end()
            }
        }
        let mut map = serializer.serialize_map(Some(2))?;
        map.serialize_entry("role", Role::User.label())?;
        map.serialize_entry("content", &Parts(self.0))?;
        map.end()
    }
}

/// The v4 file part (`writeVerifiedImageFilePartJsonWithBudget`,
/// `image_attachments.zig:1685-1705`): `mediaType`, then the bytes as standard
/// base64 under `data.data`.
fn file_part(media_type: crate::image::MediaType, data: &[u8]) -> FilePart {
    FilePart {
        r#type: "file",
        media_type: media_type.label(),
        data: FileData {
            r#type: "data",
            data: crate::image::base64(data),
        },
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
                // The Vercel wire has no equivalent of a signed reasoning block
                // and no contract requiring one back, so the blocks are reduced
                // to the text this wire understands. The reasoning is dropped
                // rather than translated: it belongs to the other provider, and
                // forwarding it here would send one model's private reasoning to
                // another one. The tool calls travel as their own parts already.
                ContentPart::RawBlocks(blocks) => {
                    let text: String = blocks
                        .iter()
                        .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
                        .filter_map(|block| block.get("text").and_then(Value::as_str))
                        .collect();
                    if !text.is_empty() {
                        seq.serialize_element(&TextPart {
                            r#type: "text",
                            text: &text,
                        })?;
                    }
                }
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
                    error,
                    images,
                } if images.is_empty() => seq.serialize_element(&ToolResultPart {
                    r#type: "tool-result",
                    tool_call_id: call_id,
                    tool_name: tool,
                    output: TextOutput {
                        r#type: if *error { "error-text" } else { "text" },
                        value: output,
                    },
                })?,
                // A result whose images follow in a user message keeps only
                // its text here, as a `content` output (`write_tool_result_part`,
                // `vercel_protocol.zig:728-739`).
                ContentPart::ToolResult {
                    call_id,
                    tool,
                    output,
                    error,
                    ..
                } => {
                    let text = if *error {
                        format!("Tool error: {output}")
                    } else {
                        output.clone()
                    };
                    seq.serialize_element(&ContentResultPart {
                        r#type: "tool-result",
                        tool_call_id: call_id,
                        tool_name: tool,
                        output: ContentOutput {
                            r#type: "content",
                            value: [TextPart {
                                r#type: "text",
                                text: &text,
                            }],
                        },
                    })?;
                }
                ContentPart::Image {
                    media_type, data, ..
                } => seq.serialize_element(&file_part(*media_type, data))?,
            }
        }
        seq.end()
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FilePart {
    r#type: &'static str,
    media_type: &'static str,
    data: FileData,
}

#[derive(Serialize)]
struct FileData {
    r#type: &'static str,
    data: String,
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
struct ContentResultPart<'a> {
    r#type: &'static str,
    tool_call_id: &'a str,
    tool_name: &'a str,
    output: ContentOutput<'a>,
}

#[derive(Serialize)]
struct ContentOutput<'a> {
    r#type: &'static str,
    value: [TextPart<'a>; 1],
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

/// What one response cost, as the provider accounted for it.
///
/// Present only when the provider said **everything** upstream requires
/// before it counts a request (`parseSseBilling`, `vercel-labs/fx@c1db9191
/// src/gateway/client.zig:3313-3395`): when it was billed, what it was billed
/// as, what it cost, and every token count it is made of. A response that
/// said less is a response whose spend is unknown, and it has no billing
/// rather than a partial one -- a partial bill would be summed as if whole.
#[derive(Debug, Clone, PartialEq)]
pub struct Billing {
    /// When the provider billed it, milliseconds since the epoch.
    pub at_ms: i64,
    /// What it was billed as.
    pub model: String,
    /// The provider's id for the generation, when it has one.
    pub generation_id: Option<String>,
    /// US dollars, as the provider said.
    pub cost: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// Part of `input_tokens`.
    pub cache_read_tokens: u64,
    /// Part of `input_tokens`.
    pub cache_write_tokens: u64,
    /// Part of `output_tokens`, when the provider said.
    pub reasoning_tokens: Option<u64>,
}

/// What is known of a response's bill.
#[derive(Debug, Clone, PartialEq)]
pub enum Bill {
    /// The stream said all of it ([`Billing`]).
    Exact(Billing),
    /// The stream named its generation and said nothing of what it cost:
    /// upstream's `.deferred` usage (`gatewayUsageOutcome`,
    /// `vercel-labs/fx@c1db9191 src/builtins/gateway.zig:650-660`), whose bill
    /// the Gateway is asked for later, by that id ([`crate::gateway::generation`]).
    Deferred(Deferred),
}

/// A generation whose bill is still to be asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deferred {
    /// The Gateway's id for it (`gen_…`).
    pub generation_id: String,
    /// Where it was made, and so where to ask -- the scheme and authority of
    /// the endpoint the request went to, as upstream records the `scope` of a
    /// deferred reference at request time (`gatewayUsageReference`,
    /// `gateway.zig:668-686`). Empty when the decoder was not told, which is
    /// no origin a lookup trusts ([`crate::gateway::is_trusted_generation_origin`]).
    pub origin: String,
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
    /// Every content block the provider sent, in arrival order, in the shape a
    /// follow-up request has to send back.
    ///
    /// Empty on the Vercel wire, which has no equivalent contract. On the
    /// Anthropic wire it is load-bearing: a tool continuation must return the
    /// assistant's prior `thinking` blocks unchanged, in order, with their
    /// signatures intact, or the next request is a 400. xfx cannot reconstruct
    /// a signature it did not keep, so it keeps the blocks verbatim rather than
    /// rebuilding an approximation of them from `text` and `tool_calls`.
    pub raw_content: Vec<Value>,
    /// Which wire produced this completion, and therefore which authority
    /// sealed anything replayable in it.
    ///
    /// Set by the decoder rather than by the caller: the caller knows which
    /// provider it *asked*, the decoder knows which wire actually answered, and
    /// the second is the one a replay contract is written against.
    pub wire: Wire,
    /// What the response cost, when the provider accounted for it, or the
    /// generation to ask about it ([`Bill`]). `None` is "unknown", never
    /// "free".
    pub billing: Option<Bill>,
    /// The tier the Gateway says it served the response at, from the finish
    /// event's `providerMetadata.gateway.serviceTier` (`parseSseServiceTier`,
    /// `client.zig:3451-3463`). `None` is "not confirmed" -- absent, unknown,
    /// or said anywhere else -- and never "standard".
    pub service_tier: Option<ServiceTier>,
}

/// A tier the Gateway confirms a response was served at
/// (`ProviderServiceTier`, `types.zig:1679-1692`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceTier {
    Standard,
    Flex,
    Priority,
    Ultrafast,
}

impl ServiceTier {
    /// Upstream's spellings: `default` is standard, and `fast` is priority.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "default" | "standard" => Some(Self::Standard),
            "priority" | "fast" => Some(Self::Priority),
            "flex" => Some(Self::Flex),
            "ultrafast" => Some(Self::Ultrafast),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_call_that_never_produced_a_result_is_answered_with_error_text() {
        // fx's request after a cancelled command
        // (`fx-interrupt-40x120-bodies/006-completion.json`):
        // `{"type":"tool-result",...,"output":{"type":"error-text","value":"aborted by user"}}`.
        let parts = |message: &Message| serde_json::to_value(WireParts(&message.content)).unwrap();
        assert_eq!(
            parts(&Message::tool_error(
                "call_1",
                "terminal",
                "aborted by user"
            )),
            json!([{
                "type": "tool-result",
                "toolCallId": "call_1",
                "toolName": "terminal",
                "output": { "type": "error-text", "value": "aborted by user" }
            }])
        );
        assert_eq!(
            parts(&Message::tool_result("call_1", "terminal", "ok"))[0]["output"]["type"],
            json!("text")
        );
    }

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
    fn an_effort_is_a_top_level_reasoning_string_after_tool_choice_and_max_goes_as_xhigh() {
        // `buildGatewayRequestBodyValidated` (`vercel_protocol.zig:376-379`,
        // `c1db919`): `"reasoning":"<label>"` after `toolChoice`, and the v4
        // route's own top tier for an advertised `max`.
        let request = CompletionRequest {
            model: "m".to_string(),
            messages: vec![Message::user("go")],
            tools: Vec::new(),
            tool_choice: ToolChoice::Auto,
        };
        let plain = request.body().expect("valid");
        assert!(!plain.contains("reasoning"), "{plain}");
        assert_eq!(
            request
                .body_with(&RequestOptions::default())
                .expect("valid"),
            plain,
            "asking for nothing changed the body"
        );
        let low = request
            .body_with(&RequestOptions {
                reasoning: Some("low".to_string()),
                ..RequestOptions::default()
            })
            .expect("valid");
        assert!(
            low.ends_with(r#""toolChoice":{"type":"auto"},"reasoning":"low"}"#),
            "{low}"
        );
        let max = request
            .body_with(&RequestOptions {
                reasoning: Some("max".to_string()),
                ..RequestOptions::default()
            })
            .expect("valid");
        assert!(max.ends_with(r#""reasoning":"xhigh"}"#), "{max}");
        // An invalid prompt is refused whatever is asked beside it.
        let empty = CompletionRequest {
            messages: Vec::new(),
            ..request
        };
        assert_eq!(
            empty.body_with(&RequestOptions {
                reasoning: Some("low".to_string()),
                ..RequestOptions::default()
            }),
            Err(ProtocolError::EmptyPrompt)
        );
    }

    #[test]
    fn fast_and_ultra_are_upstreams_provider_options_after_the_effort() {
        // `writeProviderOptions` (`vercel_protocol.zig:453-494`, `c1db919`),
        // less upstream's `caching` entry, which xfx does not send.
        let request = CompletionRequest {
            model: "m".to_string(),
            messages: vec![Message::user("go")],
            tools: Vec::new(),
            tool_choice: ToolChoice::Auto,
        };
        let fast = request
            .body_with(&RequestOptions {
                reasoning: Some("low".to_string()),
                fast: true,
                ultrafast: false,
            })
            .expect("valid");
        assert!(
            fast.ends_with(r#""reasoning":"low","providerOptions":{"gateway":{"speed":"fast"}}}"#),
            "{fast}"
        );
        let ultra = request
            .body_with(&RequestOptions {
                reasoning: None,
                fast: false,
                ultrafast: true,
            })
            .expect("valid");
        assert!(
            ultra.ends_with(
                r#""providerOptions":{"gateway":{"only":["openai"]},"openai":{"serviceTier":"ultrafast"}}}"#
            ),
            "{ultra}"
        );
        // Ultra wins a pair that should never be sent.
        let both = request
            .body_with(&RequestOptions {
                reasoning: None,
                fast: true,
                ultrafast: true,
            })
            .expect("valid");
        assert_eq!(both, ultra);
    }

    #[test]
    fn an_image_is_upstreams_v4_file_part_after_the_text_that_names_it() {
        // `cap/fx-image-40x120-bodies/001-completion.json`: the text keeps the
        // tokens, the file parts follow in attachment order, and the base64 is
        // the file's bytes unmodified.
        let shot: std::sync::Arc<[u8]> = std::sync::Arc::from(crate::image::fixture::SHOT_PNG);
        let gif: std::sync::Arc<[u8]> = std::sync::Arc::from(&b"GIF89a"[..]);
        let request = CompletionRequest {
            model: "m".to_string(),
            messages: vec![Message::user_with_images(
                "[Image #3][Image #4] what is in this image?",
                [
                    ContentPart::Image {
                        id: 3,
                        media_type: crate::image::MediaType::Png,
                        data: std::sync::Arc::clone(&shot),
                        saved: None,
                    },
                    ContentPart::Image {
                        id: 4,
                        media_type: crate::image::MediaType::Gif,
                        data: std::sync::Arc::clone(&gif),
                        saved: None,
                    },
                ],
            )],
            tools: Vec::new(),
            tool_choice: ToolChoice::Auto,
        };
        let parsed: Value = serde_json::from_str(&request.body().expect("valid")).expect("json");
        assert_eq!(
            parsed["prompt"][0]["content"],
            json!([
                {"type": "text", "text": "[Image #3][Image #4] what is in this image?"},
                {"type": "file", "mediaType": "image/png",
                 "data": {"type": "data", "data": crate::image::fixture::SHOT_PNG_BASE64}},
                {"type": "file", "mediaType": "image/gif",
                 "data": {"type": "data", "data": "R0lGODlh"}},
            ])
        );
    }

    /// A `read_file` call and its result carrying `shot.png`, then a second
    /// call whose result carries nothing.
    fn read_an_image() -> Vec<Message> {
        let call = |id: &str| ToolCall {
            id: id.to_string(),
            name: "read_file".to_string(),
            input: json!({"path": "shot.png"}),
        };
        vec![
            Message::user("look"),
            Message::assistant(None, vec![call("c1"), call("c2")]),
            Message::tool_result_with_images(
                "c1",
                "read_file",
                "<path>shot.png</path>\n<content>image attached (image/png, 73 bytes)</content>",
                vec![crate::image::ToolImage {
                    media_type: crate::image::MediaType::Png,
                    data: std::sync::Arc::from(crate::image::fixture::SHOT_PNG),
                    path: "shot.png".to_string(),
                }],
            ),
            Message::tool_result("c2", "read_file", "labels"),
        ]
    }

    #[test]
    fn a_tool_results_images_follow_the_run_of_results_in_a_user_message() {
        // `write_tool_result_part` and `write_tool_image_user_message`
        // (`vercel_protocol.zig:679-739`), and their case
        // `grouped tool results retain images and individual error status`
        // (`:1915-1965`): the result keeps its text as a `content` output, a
        // result without images is the `text` it always was, and one user
        // message after the run carries upstream's sentence and the file.
        let request = CompletionRequest {
            model: "m".to_string(),
            messages: read_an_image(),
            tools: Vec::new(),
            tool_choice: ToolChoice::Auto,
        };
        let parsed: Value = serde_json::from_str(&request.body().expect("valid")).expect("json");
        let prompt = parsed["prompt"].as_array().expect("a prompt");
        assert_eq!(prompt.len(), 5, "{parsed}");
        assert_eq!(
            prompt[2]["content"][0]["output"],
            json!({"type": "content", "value": [{"type": "text",
                "text": "<path>shot.png</path>\n<content>image attached (image/png, 73 bytes)</content>"}]})
        );
        assert_eq!(
            prompt[3]["content"][0]["output"],
            json!({"type": "text", "value": "labels"})
        );
        assert_eq!(
            prompt[4],
            json!({"role": "user", "content": [
                {"type": "text", "text": "Attached image(s) from the tool result."},
                {"type": "file", "mediaType": "image/png",
                 "data": {"type": "data", "data": crate::image::fixture::SHOT_PNG_BASE64}},
            ]})
        );
    }

    #[test]
    fn every_earlier_image_is_sent_again_with_the_history_that_holds_it() {
        // `002-completion.json`: the second request carries the first turn's
        // two images and the second turn's one.
        let shot: std::sync::Arc<[u8]> = std::sync::Arc::from(crate::image::fixture::SHOT_PNG);
        let part = |id: u32, data: &std::sync::Arc<[u8]>| ContentPart::Image {
            id,
            media_type: crate::image::MediaType::Png,
            data: std::sync::Arc::clone(data),
            saved: None,
        };
        let request = CompletionRequest {
            model: "m".to_string(),
            messages: vec![
                Message::user_with_images(
                    "[Image #3][Image #4] what?",
                    [part(3, &shot), part(4, &shot)],
                ),
                Message::assistant(Some("It is a small red square."), Vec::new()),
                Message::user_with_images("[Image #5] describe this too", [part(5, &shot)]),
            ],
            tools: Vec::new(),
            tool_choice: ToolChoice::Auto,
        };
        let body = request.body().expect("valid");
        assert_eq!(body.matches(r#""type":"file""#).count(), 3, "{body}");
    }

    #[test]
    fn an_image_is_refused_on_any_message_but_a_users() {
        let shot: std::sync::Arc<[u8]> = std::sync::Arc::from(crate::image::fixture::SHOT_PNG);
        let image = ContentPart::Image {
            id: 1,
            media_type: crate::image::MediaType::Png,
            data: shot,
            saved: None,
        };
        assert!(image.allowed_on(Role::User));
        for role in [Role::System, Role::Assistant, Role::Tool] {
            assert!(!image.allowed_on(role), "{role:?}");
        }
        let request = CompletionRequest {
            model: "m".to_string(),
            messages: vec![
                Message::user("go"),
                Message {
                    role: Role::Assistant,
                    content: vec![image],
                },
            ],
            tools: Vec::new(),
            tool_choice: ToolChoice::Auto,
        };
        assert_eq!(
            request.body(),
            Err(ProtocolError::MisplacedPart {
                role: "assistant",
                part: "file"
            })
        );
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
            error: false,
            images: Vec::new(),
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
