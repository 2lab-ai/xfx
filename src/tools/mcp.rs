//! The model's MCP tools: two meta-tools in the closed registry, and the
//! servers' own tools, which are advertised once one is selected.
//!
//! Upstream's surface (`vercel-labs/fx@c1db9191 src/builtins/tools.zig:
//! 510-662`, `cap/fx-mcp-fixture-40x120-bodies/001-completion.json`):
//!
//! - `capability_search` finds configured MCP tools for a described task and
//!   **selects** the ones it finds, so their schemas are advertised from the
//!   model's next step on ([`crate::mcp::catalog::search`]).
//! - `mcp_select_tool` selects one tool by its exact name.
//!
//! fx v0.0.16 dropped the third, `mcp_features` (resources, prompts and
//! completion): its registry has no such spec (`vercel-labs/fx@481104fd
//! src/builtins/tools.zig`, against `c1db9191 :664-700`), "fx doesn't
//! support MCP resources and prompts yet" (`src/core/mcp_host/verbs.zig:
//! 69-70`), and the request bodies of `p5-mcp-turn` advertise no such tool.
//! So does xfx.
//!
//! The two are read-only: they ask servers what they have and change
//! nothing. A server's **own** tool is the opposite -- what it does is
//! whatever the server does -- so every call is put to the session's
//! permission policy first, under the tool's own name, exactly as upstream
//! admits a dynamic tool ([`crate::permission::McpCallPlan`]). A call to a
//! live one the turn never selected is selected then and judged the same
//! way ([`select_unselected`]).
//!
//! One sentence differs from upstream's, on purpose: `capability_search`'s
//! description does not offer skills or a `skill` tool, because this build has
//! neither and the description is a promise (`CONTRIBUTING.md`, rule 1).

use serde_json::{Map, Value};

use super::spec::{
    Activity, InputSchema, PermissionKind, Presentation, Property, PropertyKind, Shown,
    ToolContext, ToolInput, ToolResult, ToolSpec,
};
use crate::gateway::protocol::ToolCall;
use crate::mcp::catalog;
use crate::mcp::McpError;
use crate::permission::{McpCallPlan, PolicyDecision, ProposedAction};

/// `capability_search`'s query bound (`lexical_relevance.max_query_bytes`).
const MAX_QUERY_BYTES: usize = 4 * 1024;

/// How much of a call's arguments a person is shown before deciding.
const MAX_SHOWN_ARGUMENT_BYTES: usize = 2 * 1024;

const CAPABILITY_SEARCH_DESCRIPTION: &str = "Find configured MCP tools for a described capability. Optionally restrict MCP results to one exact configured server. Results describe this query; no_match does not rule out another query. Matching MCP schemas are loaded automatically within the schema budget; call advertised tools directly or use mcp_select_tool for explicit selection. Do not guess identities.";

const MCP_SELECT_TOOL_DESCRIPTION: &str = "Exact-select one configured MCP/dynamic tool by name so its executable schema is advertised on the next model step. When to use: after discovering the exact specialized tool name in configured metadata. When NOT to use: guessing partial names, selecting built-in tools, or executing the dynamic tool directly.";

static SEARCH_SCHEMA: InputSchema = InputSchema {
    properties: &[
        Property {
            name: "query",
            kind: PropertyKind::BoundedString {
                min: 1,
                max: Some(MAX_QUERY_BYTES),
            },
            description: "Natural-language capability needed for the current task.",
            allowed: &[],
            array: None,
        },
        Property {
            name: "server",
            kind: PropertyKind::BoundedString { min: 1, max: None },
            description: "Optional exact configured MCP server alias.",
            allowed: &[],
            array: None,
        },
    ],
    required: &["query"],
};

static SELECT_SCHEMA: InputSchema = InputSchema {
    properties: &[Property {
        name: "name",
        kind: PropertyKind::String,
        description: "Exact dynamic MCP tool name discovered in configured metadata, such as mcp_server_tool.",
        allowed: &[],
        array: None,
    }],
    required: &["name"],
};

pub const CAPABILITY_SEARCH: ToolSpec = ToolSpec::new(
    "capability_search",
    CAPABILITY_SEARCH_DESCRIPTION,
    PermissionKind::ReadOnly,
    Presentation::new(
        Activity::Read,
        "Searching capabilities",
        "Searched capabilities",
        "capabilities",
    ),
    SEARCH_SCHEMA,
    decode_search,
    accept,
    execute_search,
);

pub const MCP_SELECT_TOOL: ToolSpec = ToolSpec::new(
    "mcp_select_tool",
    MCP_SELECT_TOOL_DESCRIPTION,
    PermissionKind::ReadOnly,
    Presentation::new(
        Activity::Read,
        "Selecting MCP tool",
        "Selected MCP tool",
        "dynamic tool",
    ),
    SELECT_SCHEMA,
    decode_select,
    accept,
    execute_select,
);

/// The names of the two, which are advertised only where an MCP client runs
/// ([`super::Registry::advertisement_for`]).
pub const META_TOOLS: &[&str] = &["capability_search", "mcp_select_tool"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilitySearchInput {
    pub query: String,
    pub server: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpSelectToolInput {
    pub name: String,
}

fn accept(_: &ToolInput) -> Result<(), String> {
    Ok(())
}

/// Upstream's decoder and its words (`capability_search.zig:33-84`).
fn decode_search(input: &Value) -> Result<ToolInput, String> {
    let fields = input
        .as_object()
        .ok_or_else(|| "capability_search arguments must be an object".to_string())?;
    let query = match fields.get("query") {
        None => return Err("capability_search field \"query\" is required".into()),
        Some(Value::String(query)) => query,
        Some(_) => return Err("capability_search field \"query\" must be a string".into()),
    };
    if query.is_empty() {
        return Err("capability_search field \"query\" must not be empty".into());
    }
    if query.len() > MAX_QUERY_BYTES {
        return Err("capability_search query must not exceed 4096 bytes".into());
    }
    let server = match fields.get("server") {
        None => None,
        Some(Value::String(server)) if server.is_empty() => {
            return Err("capability_search field \"server\" must not be empty".into())
        }
        Some(Value::String(server)) => Some(server.clone()),
        Some(_) => return Err("capability_search field \"server\" must be a string".into()),
    };
    Ok(ToolInput::CapabilitySearch(CapabilitySearchInput {
        query: query.clone(),
        server,
    }))
}

/// `decodeSelect` (`tool_mcp_dispatch.zig:17-42`).
fn decode_select(input: &Value) -> Result<ToolInput, String> {
    let fields = input
        .as_object()
        .ok_or_else(|| "Invalid mcp_select_tool arguments.".to_string())?;
    match fields.get("name") {
        Some(Value::String(name)) => Ok(ToolInput::McpSelectTool(McpSelectToolInput {
            name: name.clone(),
        })),
        _ => Err("mcp_select_tool requires an exact dynamic tool name.".into()),
    }
}

/// Whether `name` is a rule's to refuse outright: a configured deny for the
/// tool's own name keeps it out of every search and selection
/// (`rulesDenyAllTargetsForPermission`, `tool_search.zig:91`).
///
/// In every mode, full access too: upstream filters its MCP catalog by the
/// table whatever the mode (`snapshotToolDefinition` is handed the engine's
/// rules, `vercel-labs/fx@c1db9191 src/main.zig:1957-1959`). A server's tool
/// has one target, its own name, so a deny there is a deny at every target.
fn denied(context: &ToolContext, alias: &str) -> bool {
    context
        .permissions()
        .rules()
        .decide(alias, Some(alias), alias.as_bytes())
        == Some(crate::permission::RuleAction::Deny)
}

/// The schemas of the MCP tools this turn has selected that the table in
/// force now does not refuse, in the order they were selected: what a step
/// advertises after the registry.
///
/// Read at every step, as upstream re-reads each selected definition before
/// each request (`snapshotDynamicTools`, `vercel-labs/fx@c1db9191
/// src/core/agent/runtime/gateway_step.zig`), and its snapshot is
/// `unavailable` once a rule denies the tool (`snapshotToolDefinition`,
/// `src/core/mcp/mcp_runtime.zig:2093`): a deny written mid-turn takes a
/// selected tool out of the turn's next request.
pub fn offered_selected(context: &ToolContext) -> Vec<Value> {
    context
        .mcp_selected()
        .into_iter()
        .filter(|selected| !denied(context, &selected.alias))
        .map(|selected| selected.schema)
        .collect()
}

fn execute_search(input: &ToolInput, context: &ToolContext) -> ToolResult {
    let ToolInput::CapabilitySearch(input) = input else {
        return ToolResult::failure("capability_search received another tool's input");
    };
    let Some(link) = context.mcp() else {
        return ToolResult::success(
            r#"{"skills":[],"mcp_tools":[],"counts":{"skills":0,"mcp_tools":0},"total_matches":{"skills":0,"mcp_tools":0},"mcp_state":"unavailable"}"#,
            "MCP is unavailable",
        );
    };
    let mut search = catalog::search(&link.runtime, &input.query, input.server.as_deref());
    search
        .selected
        .retain(|selected| !denied(context, &selected.alias));
    let found = search.selected.len();
    context.select_mcp(search.selected);
    ToolResult::success(
        search.output,
        format!(
            "{found} MCP tool{} selected",
            if found == 1 { "" } else { "s" }
        ),
    )
}

fn execute_select(input: &ToolInput, context: &ToolContext) -> ToolResult {
    let ToolInput::McpSelectTool(input) = input else {
        return ToolResult::failure("mcp_select_tool received another tool's input");
    };
    let Some(link) = context.mcp() else {
        return ToolResult::failure("No MCP runtime is available.");
    };
    // The list is read before the table is asked, so even a refused name
    // makes its server's count known (`toolSchema`'s `ensure`,
    // `vercel-labs/fx@481104fd src/core/mcp_host/model.zig:296-318`).
    let found = link.runtime.tool_by_alias(&input.name);
    if let Some((server, _)) = &found {
        catalog::note_selected(&link.runtime, server);
    }
    let found = found.filter(|_| !denied(context, &input.name));
    let Some((server, tool)) = found else {
        return ToolResult::failure(format!(
            "Dynamic MCP tool not found or not allowed: {}",
            input.name
        ));
    };
    match catalog::select(&tool, server.instructions().as_deref()) {
        Err(rejection) => ToolResult::failure(rejection),
        Ok(selected) => {
            context.select_mcp(vec![selected]);
            let name = catalog::encode_scalar(&input.name);
            ToolResult::success(
                format!("Selected dynamic MCP tool `{name}`. Its executable schema will be available on the next model step; call `{name}` with arguments matching the selected schema."),
                format!("selected {}", input.name),
            )
        }
    }
}

// ---------------------------------------------------------------------------
// a server's own tools
// ---------------------------------------------------------------------------

/// The presentation a dynamic tool's row has (`dynamicMcpActionLabel`,
/// `app_agent_runtime.zig:1431-1437`): `Running MCP <tool>`, then `Ran MCP
/// <tool>`. It is what marks a call as an MCP call wherever a call is
/// counted or drawn. Its activity is a command's, so a stop reaches and
/// settles the call as it does a command; the row itself is not a command
/// line, and fx v0.0.16 draws it in the row's grey throughout, no word of it
/// shell-highlighted (an unregistered `mcp_` name is kind `.mcp`,
/// `vercel-labs/fx@481104fd src/core/tooling/tool_dispatch.zig:574-578`;
/// `highlightCommandPhrase` only for `.command`,
/// `src/ui/transcript/tool_group_projection.zig:749`).
pub const DYNAMIC: Presentation =
    Presentation::new(Activity::Command, "Running", "Ran", "MCP dynamic tool");

/// Whether `name` is a tool this turn has selected, and so may call.
pub fn is_selected(context: &ToolContext, name: &str) -> bool {
    context
        .mcp_selected()
        .iter()
        .any(|selected| selected.alias == name)
}

/// Whether the model may call `name`, a name the registry does not hold: a
/// tool this turn selected, or a live server's tool it calls without having
/// searched for it or selected it, which is selected now. fx v0.0.16
/// advertises such a call's tool from then on and sends the call through
/// normal validation and permission instead of failing it as unselected
/// (`advertiseUnselectedMcpCalls`, `vercel-labs/fx@481104fd
/// src/core/agent/runtime/orchestrator.zig:2497-2538`, `0fb5cb8e`); before,
/// it failed with `Dynamic MCP tool not selected for this model step`
/// (`c1db9191 tool_mcp_runtime.zig:426`).
///
/// **This admits nothing.** It only decides the name is a tool. The call is
/// then judged by [`call`] exactly as a selected one is -- under its own
/// name, a configured rule, an "always", the mode, `auto` holding it -- and
/// that is the only way it runs. A name resolves only as `mcp_select_tool`
/// would select it (`resolveUnselectedMcpTool`, `tool_runtime.zig:1039-
/// 1065`: unknown, denied or oversized is no tool): on a ready server, not
/// refused outright by the table in force, and its schema within budget.
///
/// So the rules that keep a server's tools from the model are the ones about
/// the tools themselves -- a deny on the alias, on a glob such as
/// `mcp_fixture_*` or `mcp_*`, or `*` -- and no longer a deny on
/// `capability_search` or `mcp_select_tool`, which refuses those two and
/// leaves a call by name to the rules of the tool it names: an ask rule still
/// asks, `auto` still holds it. fx is the same: the lookup is handed the
/// table and the tool's own name and nothing about the meta-tools
/// (`tool_runtime.zig:1042-1053`; `toolSchema`,
/// `src/core/mcp_host/model.zig:296-318`).
pub fn select_unselected(context: &ToolContext, name: &str) -> bool {
    if is_selected(context, name) {
        return true;
    }
    let Some(link) = context.mcp() else {
        return false;
    };
    let Some((server, tool)) = link.runtime.tool_by_alias(name) else {
        return false;
    };
    // Read before the table is asked, as `mcp_select_tool` reads it.
    catalog::note_selected(&link.runtime, &server);
    if denied(context, name) {
        return false;
    }
    match catalog::select(&tool, server.instructions().as_deref()) {
        Ok(selected) => {
            context.select_mcp(vec![selected]);
            true
        }
        Err(_) => false,
    }
}

/// What a person watching is shown before a dynamic call runs.
pub fn shown(name: &str) -> Shown {
    Shown {
        presentation: DYNAMIC,
        target: Some(format!("MCP {name}")),
    }
}

/// Runs one call of a selected MCP tool: judged first, then sent.
///
/// fx v0.0.16 reads the answer as a tool result -- `content`,
/// `structuredContent`, `isError` and nothing else -- or ends the call as
/// malformed ([`catalog::call_answer`]).
pub fn call(call: &ToolCall, context: &ToolContext) -> ToolResult {
    let alias = call.name.as_str();
    // Before anything, and in full access too: a tool the table refuses is
    // not in this step's advertisement ([`offered_selected`]), and upstream
    // answers a call to a tool it did not advertise as an unknown one
    // (`isAvailableDynamicTool` reads the step's advertised names, then
    // `unknown tool: <name>`, `vercel-labs/fx@c1db9191
    // src/core/tooling/tool_dispatch.zig:845`) -- nothing reaches the server.
    if denied(context, alias) {
        return ToolResult::failure(format!("unknown tool: {alias}"));
    }
    let Some(link) = context.mcp() else {
        return ToolResult::failure("No MCP runtime is available.");
    };
    let Some((server, _)) = link.runtime.tool_by_alias(alias) else {
        return ToolResult::failure(format!(
            "Dynamic MCP tool not found or not allowed: {alias}"
        ));
    };
    catalog::note_selected(&link.runtime, &server);
    let arguments = match &call.input {
        Value::Object(_) => call.input.clone(),
        Value::Null => Value::Object(Map::new()),
        _ => return ToolResult::failure(format!("{alias} arguments must be a JSON object")),
    };
    let plan = McpCallPlan {
        alias: alias.to_string(),
        server: server.config.name.clone(),
        arguments: crate::output::safe_one_line(&arguments.to_string(), MAX_SHOWN_ARGUMENT_BYTES),
        key: arguments.to_string(),
    };
    let decided = context
        .permissions()
        .decide_with_feedback(ProposedAction::Mcp(&plan));
    if let PolicyDecision::Deny { reason, .. } = decided.decision {
        return ToolResult::refused(alias, &reason).with_feedback(decided.feedback);
    }
    let result = match link
        .runtime
        .call_tool(alias, arguments, Some(context.cancel()))
    {
        Ok((server, _, result)) => {
            match catalog::call_answer(&server.config.name, alias, &result) {
                catalog::CallAnswer::Result {
                    envelope,
                    is_error: false,
                } => ToolResult::success(envelope, format!("called {alias}")),
                catalog::CallAnswer::Result { envelope, .. } => ToolResult::failure(envelope),
                catalog::CallAnswer::Malformed(text) => ToolResult::failure(text),
            }
        }
        Err(McpError::Restart(failure)) => ToolResult::failure(catalog::restart_failed(
            &server.config.name,
            alias,
            &failure,
        )),
        Err(McpError::Protocol { code, message, .. }) => {
            ToolResult::failure(catalog::protocol_error(alias, code, &message))
        }
        Err(err) => ToolResult::failure(format!(
            "{{\"server\":{},\"tool\":{},\"error\":{{\"kind\":{},\"message\":{}}}}}",
            Value::from(server.config.name.as_str()),
            Value::from(alias),
            Value::from(err.to_string()),
            Value::from(format!("MCP tool call failed: {err}"))
        )),
    };
    result.with_feedback(decided.feedback)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_two_schemas_carry_upstreams_fields_and_words() {
        // `cap/fx-mcp-fixture-40x120-bodies/001-completion.json`: the same
        // fields, bounds and words, in xfx's sorted, closed layout.
        let search = CAPABILITY_SEARCH.advertisement();
        assert_eq!(
            search["inputSchema"]["properties"]["query"],
            json!({"type": "string", "minLength": 1, "maxLength": 4096, "description": "Natural-language capability needed for the current task."})
        );
        assert_eq!(
            search["inputSchema"]["properties"]["server"],
            json!({"type": "string", "minLength": 1, "description": "Optional exact configured MCP server alias."})
        );
        assert_eq!(search["inputSchema"]["required"], json!(["query"]));
        let select = MCP_SELECT_TOOL.advertisement();
        assert_eq!(select["description"], MCP_SELECT_TOOL_DESCRIPTION);
        assert_eq!(select["inputSchema"]["required"], json!(["name"]));
        assert!(!CAPABILITY_SEARCH_DESCRIPTION.contains("skill"));
    }

    #[test]
    fn each_decoder_refuses_in_upstreams_words() {
        for (input, expected) in [
            (json!([]), "capability_search arguments must be an object"),
            (json!({}), "capability_search field \"query\" is required"),
            (
                json!({"query": 1}),
                "capability_search field \"query\" must be a string",
            ),
            (
                json!({"query": ""}),
                "capability_search field \"query\" must not be empty",
            ),
            (
                json!({"query": "x".repeat(4097)}),
                "capability_search query must not exceed 4096 bytes",
            ),
            (
                json!({"query": "q", "server": ""}),
                "capability_search field \"server\" must not be empty",
            ),
        ] {
            assert_eq!(decode_search(&input).unwrap_err(), expected);
        }
        assert_eq!(
            decode_select(&json!({})).unwrap_err(),
            "mcp_select_tool requires an exact dynamic tool name."
        );
    }
}
