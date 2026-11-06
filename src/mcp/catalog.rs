//! What the model is told about MCP, and how it finds a tool.
//!
//! Upstream does **not** advertise MCP tools up front
//! (`cap/fx-mcp-fixture-40x120-bodies/001-completion.json`). Every request
//! carries one system block naming the configured servers and their states
//! ([`render_servers`], `vercel-labs/fx@c1db9191
//! src/core/mcp/model_catalog.zig:8-15`, `:206-276`), and two meta-tools:
//! `capability_search` finds tools by what they do and **selects** the ones it
//! finds, whose schemas are then advertised from the next model step on
//! (`selectSearchSchemas`, `tool_search.zig:242-270`); `mcp_select_tool`
//! selects one by exact name. (fx v0.0.16 dropped the third, `mcp_features`.)
//! Here is all of it but the tool specs themselves, which are in
//! [`crate::tools::mcp`].

use serde_json::{Map, Value};

use super::runtime::{McpRuntime, McpTool, ServerView, Status};

/// The block's header (`model_catalog.zig:9-12`).
const HEADER: &str = "Configured MCP servers visible to this model turn are listed below.\nA listed server is not a reason to use MCP. Use capability_search only when the task clearly needs a capability not already available locally. Pass the task and an optional exact server name. Call tools advertised after search directly. Use mcp_select_tool when a relevant result is not yet callable. Use returned identities and refine the query when needed.\n<mcp_servers>\n";
const FOOTER: &str = "</mcp_servers>\n";
const EMPTY_ENTRY: &str = "  <none />\n";

/// The block's budget (`max_prompt_bytes`, `model_catalog.zig:8`).
const MAX_BLOCK_BYTES: usize = 4 * 1024;

/// `mcp_description_bytes` (`context_limits.zig:24`).
const DESCRIPTION_BYTES: usize = 1024;
/// `mcp_search_result_bytes` (`:25`).
const SEARCH_RESULT_BYTES: usize = 16 * 1024;
/// `mcp_server_instructions_bytes` (`:26`).
const INSTRUCTIONS_BYTES: usize = 2 * 1024;
/// `mcp_selected_schema_bytes` (`:27`).
const SELECTED_SCHEMA_BYTES: usize = 64 * 1024;

/// How much of a description, a schema, and the server's instructions a
/// search reads (`tool_search.zig:18-20`).
const SEARCH_DESCRIPTION_BYTES: usize = 2 * 1024;
const SEARCH_SCHEMA_BYTES: usize = 4 * 1024;

/// How many results one search returns (`capability_retrieval.zig:7`).
const DEFAULT_LIMIT: usize = 5;

/// Upstream's scalar encoding for model context (`writeScalar`,
/// `src/core/shared/model_context_encoding.zig:4-37`): the XML specials and
/// every control byte as entities, and the three line separators JSON leaves
/// raw.
pub fn encode_scalar(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\u{85}' => out.push_str("&#x85;"),
            '\u{2028}' => out.push_str("&#x2028;"),
            '\u{2029}' => out.push_str("&#x2029;"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("&#x{:02x};", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

/// A model-visible availability (`Availability`, `model_catalog.zig:17-25`).
fn availability(view: &ServerView) -> &'static str {
    match view.status() {
        Status::Disabled => "disabled",
        Status::Connecting => "discovering",
        Status::Ready => "ready",
        Status::NeedsAuth => "authentication_required",
        Status::Failed => "failed",
        Status::Unavailable => "unavailable",
    }
}

/// The servers block a turn's requests carry, as `runtime`'s servers stand
/// now. A server's `tools="N"` is there only once the model's catalog has
/// read its list -- a search, a selection or a call has reached it (fx
/// v0.0.16's `modelSnapshot`, `vercel-labs/fx@481104fd
/// src/core/mcp_host/model.zig:472-500`: `.tool_count = if (l.state ==
/// .loaded) l.tools.len else null`). So a turn's first request has none
/// (`p5-mcp-turn`'s bodies), and a later turn's has the count of a server an
/// earlier turn searched; that count is the list's, whatever the server's
/// state now.
pub fn servers_block(runtime: &McpRuntime) -> String {
    let views: Vec<ServerView> = runtime
        .views()
        .into_iter()
        .map(|mut view| {
            let known = runtime
                .server(&view.name)
                .is_some_and(|server| server.tools_known());
            view.tool_count = known.then_some(view.tools_held);
            view
        })
        .collect();
    render_servers(&views)
}

/// Records what the model's catalog read of `name`'s list: a ready server's
/// list is known from now on, and one that failed or needs a login has none
/// (`refresh`, `vercel-labs/fx@481104fd src/core/mcp_host/model.zig:
/// 617-669`). A server still coming up keeps what it had: fx waits for it,
/// and xfx does not.
fn note_read(runtime: &McpRuntime, view: &ServerView) {
    let Some(server) = runtime.server(&view.name) else {
        return;
    };
    match view.status() {
        Status::Ready => server.set_tools_known(true),
        Status::Failed | Status::NeedsAuth => server.set_tools_known(false),
        Status::Connecting | Status::Disabled | Status::Unavailable => {}
    }
}

/// Records that the model's catalog read the list of the server holding
/// `alias` -- a selection or a call reached it (`ensure` and
/// `ensureCurrent`, `vercel-labs/fx@481104fd src/core/mcp_host/model.zig:
/// 513-533`). fx loads every server whose name `alias` could start with
/// until one holds it; xfx reads every ready server's list at once, so only
/// the holder is marked.
pub fn note_selected(runtime: &McpRuntime, server: &super::runtime::Server) {
    if runtime
        .views()
        .iter()
        .any(|view| view.name == server.config.name && view.status() == Status::Ready)
    {
        server.set_tools_known(true);
    }
}

/// The servers block every request carries (`render`,
/// `model_catalog.zig:206-276`): sorted by name, within its budget, with a
/// marker naming what did not fit.
pub fn render_servers(views: &[ServerView]) -> String {
    if views.is_empty() {
        return format!("{HEADER}{EMPTY_ENTRY}{FOOTER}");
    }
    let mut sorted: Vec<&ServerView> = views.iter().collect();
    sorted.sort_by(|a, b| a.name.as_bytes().cmp(b.name.as_bytes()));
    let entries: Vec<String> = sorted
        .iter()
        .map(|view| {
            let mut entry = format!(
                "  <server name=\"{}\" state=\"{}\"",
                encode_scalar(&view.name),
                availability(view)
            );
            if let Some(count) = view.tool_count {
                entry.push_str(&format!(" tools=\"{count}\""));
            }
            entry.push_str(" />\n");
            entry
        })
        .collect();
    let fits = |kept: &[String], marker: &str| {
        HEADER.len() + kept.iter().map(String::len).sum::<usize>() + marker.len() + FOOTER.len()
            <= MAX_BLOCK_BYTES
    };
    if fits(&entries, "") {
        return format!("{HEADER}{}{FOOTER}", entries.concat());
    }
    let marker = |omitted: usize| format!("  <catalog_truncated omitted_count=\"{omitted}\" />\n");
    let mut kept = entries.len();
    while kept > 0 && !fits(&entries[..kept], &marker(entries.len() - kept)) {
        kept -= 1;
    }
    format!(
        "{HEADER}{}{}{FOOTER}",
        entries[..kept].concat(),
        marker(entries.len() - kept)
    )
}

// ---------------------------------------------------------------------------
// selection
// ---------------------------------------------------------------------------

/// A tool whose schema is advertised from the next model step on.
#[derive(Debug, Clone, PartialEq)]
pub struct Selected {
    pub alias: String,
    /// The function envelope, as upstream's dynamic schema is written
    /// (`dynamicFunctionSchemaJsonAlloc`, `model_tool_schema.zig:138-150`):
    /// the server's own input schema verbatim, key order and all.
    pub schema: Value,
}

/// `project` (`selected_schema.zig:10-34`): the tool's envelope, its
/// description joined by the server's instructions when it has any.
pub fn select(tool: &McpTool, instructions: Option<&str>) -> Result<Selected, String> {
    let description = encode_scalar(&tool.description);
    let description = match instructions {
        Some(instructions) => {
            let (kept, truncated) = utf8_prefix(instructions, INSTRUCTIONS_BYTES);
            let mut merged = format!(
                "{description}\n\nServer instructions: {}",
                encode_scalar(kept)
            );
            if truncated {
                merged.push_str(&format!(
                    "\n<context_limit name=\"mcp_server_instructions_bytes\" action=\"truncated\" observed_bytes=\"{}\" effective_bytes=\"{INSTRUCTIONS_BYTES}\" source=\"default\" override=\"--context-limit mcp_server_instructions_bytes=BYTES|off\" />",
                    instructions.len()
                ));
            }
            merged
        }
        None => description,
    };
    let mut envelope = Map::new();
    envelope.insert("type".into(), Value::from("function"));
    envelope.insert("name".into(), Value::from(tool.alias.as_str()));
    envelope.insert("description".into(), Value::from(description));
    envelope.insert("inputSchema".into(), tool.input_schema.clone());
    let schema = Value::Object(envelope);
    let bytes = schema.to_string().len();
    if bytes > SELECTED_SCHEMA_BYTES {
        return Err(format!(
            "{{\"context_limit_rejection\":{{\"name\":\"mcp_selected_schema_bytes\",\"tool\":{},\"action\":\"rejected\",\"observed_bytes\":{bytes},\"effective_bytes\":{SELECTED_SCHEMA_BYTES},\"source\":\"default\",\"override\":\"--context-limit mcp_selected_schema_bytes=BYTES|off\"}}}}",
            Value::from(encode_scalar(&tool.alias))
        ));
    }
    Ok(Selected {
        alias: tool.alias.clone(),
        schema,
    })
}

/// The longest prefix of `text` within `limit` bytes that ends on a
/// character, and whether anything was cut.
fn utf8_prefix(text: &str, limit: usize) -> (&str, bool) {
    if text.len() <= limit {
        return (text, false);
    }
    let mut end = limit;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (&text[..end], true)
}

// ---------------------------------------------------------------------------
// search
// ---------------------------------------------------------------------------

/// One `capability_search`: the model's output, and the tools it selected.
#[derive(Debug, Clone, PartialEq)]
pub struct Search {
    pub output: String,
    pub selected: Vec<Selected>,
}

/// One searchable tool (`capability_retrieval.Document`).
struct Document {
    identities: [String; 2],
    primary: Vec<String>,
    secondary: [String; 3],
    stable_key: String,
}

/// The query's distinct alphanumeric tokens, case-insensitively
/// (`prepare`, `lexical_relevance.zig:48-71`).
fn tokens(query: &str) -> Vec<&str> {
    let mut tokens: Vec<&str> = Vec::new();
    for token in query.split(|c: char| !c.is_ascii_alphanumeric()) {
        if token.is_empty() || tokens.iter().any(|seen| seen.eq_ignore_ascii_case(token)) {
            continue;
        }
        tokens.push(token);
    }
    tokens
}

/// Occurrences of `token` in `text` as a whole alphanumeric token
/// (`termFrequency`, `capability_retrieval.zig:381-394`).
fn term_frequency(text: &str, token: &str) -> usize {
    let (text, token) = (text.as_bytes(), token.as_bytes());
    if token.is_empty() || token.len() > text.len() {
        return 0;
    }
    let mut count = 0;
    let mut start = 0;
    while start + token.len() <= text.len() {
        let end = start + token.len();
        let whole = text[start..end].eq_ignore_ascii_case(token)
            && (start == 0 || !text[start - 1].is_ascii_alphanumeric())
            && (end == text.len() || !text[end].is_ascii_alphanumeric());
        if whole {
            count += 1;
            start = end;
        } else {
            start += 1;
        }
    }
    count
}

fn token_count(text: &str) -> usize {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .count()
}

fn is_identity_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-'
}

/// `containsCompleteIdentity` (`lexical_relevance.zig:169-185`).
fn contains_identity(raw: &str, identities: &[&str]) -> bool {
    let raw = raw.as_bytes();
    for start in 0..raw.len() {
        for identity in identities {
            let identity = identity.as_bytes();
            if identity.is_empty() || identity.len() > raw.len() - start {
                continue;
            }
            let end = start + identity.len();
            if !raw[start..end].eq_ignore_ascii_case(identity) {
                continue;
            }
            if start > 0 && is_identity_byte(raw[start - 1]) {
                continue;
            }
            if end < raw.len() && is_identity_byte(raw[end]) {
                continue;
            }
            return true;
        }
    }
    false
}

/// Searches the ready servers' tools for `query`, optionally on one server
/// (`search`, `tool_search.zig:30-240`; `retrieve`,
/// `capability_retrieval.zig:152-247`, BM25 with the primary fields weighted
/// three times; a tool counts only on a clear match).
pub fn search(runtime: &McpRuntime, query: &str, server: Option<&str>) -> Search {
    let views = runtime.views();
    if let Some(name) = server {
        let Some(view) = views.iter().find(|view| view.name == name) else {
            return combined(&[], 0, Some(r#""server_not_found""#.to_string()), None);
        };
        note_read(runtime, view);
        if view.status() == Status::Failed {
            if let Some(failure) = &view.failure {
                let message = format!("MCP server '{name}' is unavailable: {failure}");
                return combined(
                    &[],
                    0,
                    Some(r#""server_failed""#.to_string()),
                    Some(message),
                );
            }
        }
    } else {
        // An open search reads every server's list (`searchTools`,
        // `vercel-labs/fx@481104fd src/core/mcp_host/model.zig:237-254`).
        for view in &views {
            note_read(runtime, view);
        }
    }
    let candidates: Vec<(std::sync::Arc<super::runtime::Server>, McpTool)> = runtime
        .tools()
        .into_iter()
        .filter(|(owner, _)| server.is_none_or(|name| owner.config.name == name))
        .collect();
    let documents: Vec<Document> = candidates
        .iter()
        .map(|(owner, tool)| Document {
            identities: [tool.name.clone(), tool.alias.clone()],
            primary: [
                owner.config.name.clone(),
                tool.name.clone(),
                tool.alias.clone(),
                tool.title.clone().unwrap_or_default(),
            ]
            .into_iter()
            .chain(tool.tags.iter().cloned())
            .collect(),
            secondary: [
                utf8_prefix(&tool.description, SEARCH_DESCRIPTION_BYTES)
                    .0
                    .to_string(),
                utf8_prefix(&tool.input_schema.to_string(), SEARCH_SCHEMA_BYTES)
                    .0
                    .to_string(),
                owner
                    .instructions()
                    .map(|text| utf8_prefix(&text, INSTRUCTIONS_BYTES).0.to_string())
                    .unwrap_or_default(),
            ],
            stable_key: tool.alias.clone(),
        })
        .collect();
    let ranked = rank(query, &documents, server.is_some());
    let total = ranked.len();
    let page: Vec<usize> = ranked.into_iter().take(DEFAULT_LIMIT).collect();
    let mut selected = Vec::new();
    let mut budget = SELECTED_SCHEMA_BYTES;
    for &index in &page {
        let (owner, tool) = &candidates[index];
        let Ok(choice) = select(tool, owner.instructions().as_deref()) else {
            continue;
        };
        let size = choice.schema.to_string().len();
        if size > budget {
            break;
        }
        budget -= size;
        selected.push(choice);
    }
    let items: Vec<String> = page
        .iter()
        .map(|&index| {
            let (owner, tool) = &candidates[index];
            tool_metadata(&owner.config.name, tool)
        })
        .collect();
    let mut result = combined(&items, total, None, None);
    // An open search that found nothing says which servers it could not look
    // in, and why (fx v0.0.16, `47154a8f`).
    if server.is_none() && total == 0 {
        result.output = with_unavailable(result.output, &unavailable(&views));
    }
    result.selected = selected;
    if result.output.len() > SEARCH_RESULT_BYTES {
        result.output = combined(
            &[],
            total,
            None,
            Some("CapabilitySearchResultLimitTooSmall".into()),
        )
        .output;
        result.selected.clear();
    }
    result
}

/// Why each server a search could not look in was skipped, in the
/// configuration's order (`withUnavailableLocked` and `unavailableText`,
/// `vercel-labs/fx@481104fd src/core/mcp_host/model.zig:282-294`,
/// `:708-719`): one that needs a login names the command that signs in to it
/// (`needsLoginText`, `.shell`: this is the TUI's catalog), one that failed
/// says why, in the runtime's words. fx also names a server held back for a
/// missing or unusable variable; xfx's runtime has no such state yet, and
/// such a server fails, so its reason is its failure's.
fn unavailable(views: &[ServerView]) -> Vec<String> {
    views
        .iter()
        .filter_map(|view| match view.status() {
            Status::NeedsAuth => Some(format!(
                "MCP server '{}' needs a login. Ask the user to run /mcp login {}, then try again.",
                view.name, view.name
            )),
            Status::Failed => Some(format!(
                "MCP server '{}' is unavailable: {}",
                view.name,
                view.failure.as_deref().unwrap_or("it did not start")
            )),
            _ => None,
        })
        .collect()
}

/// `output`, a combined answer, with `reasons` as its `mcp_unavailable` last
/// (`withUnavailable`, `vercel-labs/fx@481104fd src/core/mcp/tool_search.zig:
/// 25-41`; `renderCombined`, `src/tools/capabilities/capability_search.zig:
/// 301-304`), each encoded as every string the model reads here is; nothing
/// when there is none.
fn with_unavailable(mut output: String, reasons: &[String]) -> String {
    if reasons.is_empty() || !output.ends_with('}') {
        return output;
    }
    output.pop();
    output.push_str(",\"mcp_unavailable\":[");
    output.push_str(
        &reasons
            .iter()
            .map(|reason| Value::from(encode_scalar(reason)).to_string())
            .collect::<Vec<_>>()
            .join(","),
    );
    output.push_str("]}");
    output
}

/// Ranks `documents` for `query`, best first, keeping only clear matches.
fn rank(query: &str, documents: &[Document], server_scoped: bool) -> Vec<usize> {
    const PRIMARY_WEIGHT: f64 = 3.0;
    const K1: f64 = 1.2;
    const B: f64 = 0.75;
    let query_tokens = tokens(query);
    let n = documents.len();
    let primary_tf = |document: &Document, token: &str| -> usize {
        document
            .primary
            .iter()
            .map(|field| term_frequency(field, token))
            .sum()
    };
    let secondary_tf = |document: &Document, token: &str| -> usize {
        document
            .secondary
            .iter()
            .map(|field| term_frequency(field, token))
            .sum()
    };
    let primary_len = |document: &Document| -> usize {
        document
            .primary
            .iter()
            .map(|field| token_count(field))
            .sum()
    };
    let secondary_len = |document: &Document| -> usize {
        document
            .secondary
            .iter()
            .map(|field| token_count(field))
            .sum()
    };
    let average = |total: usize| {
        if n == 0 {
            1.0
        } else {
            (total as f64 / n as f64).max(1.0)
        }
    };
    let primary_average = average(documents.iter().map(primary_len).sum());
    let secondary_average = average(documents.iter().map(secondary_len).sum());
    let frequencies: Vec<usize> = query_tokens
        .iter()
        .map(|token| {
            documents
                .iter()
                .filter(|document| {
                    primary_tf(document, token) > 0 || secondary_tf(document, token) > 0
                })
                .count()
        })
        .collect();
    let bm25 = |tf: usize, length: usize, average: f64| -> f64 {
        if tf == 0 {
            return 0.0;
        }
        let tf = tf as f64;
        let normalization = 1.0 - B + B * length as f64 / average;
        tf * (K1 + 1.0) / (tf + K1 * normalization)
    };
    let mut ranked: Vec<(usize, bool, f64, usize)> = Vec::new();
    for (index, document) in documents.iter().enumerate() {
        let identities: Vec<&str> = document.identities.iter().map(String::as_str).collect();
        let exact = contains_identity(query, &identities)
            || contains_identity(query, &[document.primary[0].as_str()]);
        let mut primary_hits = 0usize;
        let mut secondary_hits = 0usize;
        let mut score = 0.0;
        for (position, token) in query_tokens.iter().enumerate() {
            let ptf = primary_tf(document, token);
            let stf = secondary_tf(document, token);
            if ptf > 0 {
                primary_hits += 1;
            }
            let df = frequencies[position];
            // `secondaryTokenProvidesEvidence` under the `intent` policy.
            let evidence = if token.len() < 4 {
                df <= n / 32
            } else {
                server_scoped || df < n
            };
            if stf > 0 && evidence {
                secondary_hits += 1;
            }
            if ptf == 0 && stf == 0 {
                continue;
            }
            let idf = (1.0 + (n as f64 - df as f64 + 0.5) / (df as f64 + 0.5)).ln();
            score += idf
                * (PRIMARY_WEIGHT * bm25(ptf, primary_len(document), primary_average)
                    + bm25(stf, secondary_len(document), secondary_average));
        }
        let inventory = query.is_empty();
        let clear = exact
            || (query_tokens.len() == 1 && primary_hits > 0)
            || primary_hits + secondary_hits >= 2;
        if inventory || clear {
            ranked.push((index, exact, score, primary_hits));
        }
    }
    let key = |index: usize| documents[index].stable_key.to_ascii_lowercase();
    ranked.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then(b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal))
            .then(b.3.cmp(&a.3))
            .then(key(a.0).cmp(&key(b.0)))
            .then(a.0.cmp(&b.0))
    });
    ranked.into_iter().map(|(index, ..)| index).collect()
}

/// One result's metadata (`writeToolMetadataJson`, `tool_search.zig:358-394`).
fn tool_metadata(server: &str, tool: &McpTool) -> String {
    let encoded = encode_scalar(&tool.description);
    let (bounded, truncated) = utf8_prefix(&encoded, DESCRIPTION_BYTES);
    let mut out = format!(
        "{{\"name\":{},\"server\":{},\"description\":{},\"purpose\":{},\"usage\":[",
        Value::from(encode_scalar(&tool.alias)),
        Value::from(encode_scalar(server)),
        Value::from(bounded),
        Value::from(bounded)
    );
    out.push_str(
        &tool
            .tags
            .iter()
            .map(|tag| Value::from(encode_scalar(tag)).to_string())
            .collect::<Vec<_>>()
            .join(","),
    );
    out.push(']');
    if truncated {
        out.push_str(&format!(
            ",\"context_limit\":{{\"name\":\"mcp_description_bytes\",\"action\":\"truncated\",\"observed_bytes\":{},\"effective_bytes\":{DESCRIPTION_BYTES},\"source\":\"default\",\"override\":\"--context-limit mcp_description_bytes=BYTES|off\"}}",
            encoded.len()
        ));
    }
    out.push('}');
    out
}

/// The answer `capability_search` gives (`renderCombined`,
/// `capability_search.zig:259-313`): no skills in this build, so that half is
/// always empty.
fn combined(
    items: &[String],
    total: usize,
    state: Option<String>,
    error: Option<String>,
) -> Search {
    let mut out = format!(
        "{{\"skills\":[],\"mcp_tools\":[{}],\"counts\":{{\"skills\":0,\"mcp_tools\":{}}},\"total_matches\":{{\"skills\":0,\"mcp_tools\":{total}}}",
        items.join(","),
        items.len()
    );
    if total == 0 && state.is_none() && error.is_none() {
        out.push_str(",\"state\":\"no_match\"");
    }
    if let Some(state) = state {
        out.push_str(",\"mcp_state\":");
        out.push_str(&state);
    }
    if let Some(error) = error {
        out.push_str(",\"mcp_error\":");
        out.push_str(&Value::from(encode_scalar(&error)).to_string());
    }
    out.push('}');
    Search {
        output: out,
        selected: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// results
// ---------------------------------------------------------------------------

/// What a `tools/call` answer tells the model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallAnswer {
    /// A tool result: the envelope, and whether the server said it failed.
    Result { envelope: String, is_error: bool },
    /// An answer that is not a tool result.
    Malformed(String),
}

/// A dynamic tool's result for the model (`write_envelope`,
/// `tool_result.zig:402-412`): the server, the tool, and the server's result
/// as fx v0.0.16 reads it -- its `content`, its `structuredContent` when it
/// has one, and `isError`, `false` when the server left it out, and nothing
/// else (`classifyCall`, `vercel-labs/fx@481104fd
/// src/mcp/protocol/tools.zig:255-273`, rewritten by `render`,
/// `src/core/mcp_host/model.zig:835-850`). A `content` that is not an array,
/// or an `isError` that is not a boolean, is no tool result: fx ends the call
/// as malformed, saying the tool may have run (`copyCall`,
/// `src/core/mcp_host/runtime.zig:2363-2378`; `ended`, `model.zig:855-878`).
pub fn call_answer(server: &str, alias: &str, result: &Value) -> CallAnswer {
    let content = result.get("content").filter(|content| content.is_array());
    let is_error = match result.get("isError") {
        None => Some(false),
        Some(Value::Bool(is_error)) => Some(*is_error),
        Some(_) => None,
    };
    let (Some(content), Some(is_error)) = (content, is_error) else {
        return CallAnswer::Malformed(format!(
            "MCP server '{server}' ended the call: the server's answer was malformed. The tool may have run."
        ));
    };
    let mut read = Map::new();
    read.insert("content".into(), content.clone());
    if let Some(structured) = result.get("structuredContent") {
        read.insert("structuredContent".into(), structured.clone());
    }
    read.insert("isError".into(), Value::Bool(is_error));
    let mut envelope = Map::new();
    envelope.insert("server".into(), Value::from(server));
    envelope.insert("tool".into(), Value::from(alias));
    envelope.insert("result".into(), Value::Object(read));
    CallAnswer::Result {
        envelope: Value::Object(envelope).to_string(),
        is_error,
    }
}

/// A call that could not run because its stopped server would not start
/// again (`restart_failed_result`, `tool_result.zig:196-215`).
pub fn restart_failed(server: &str, alias: &str, failure: &str) -> String {
    format!(
        "{{\"server\":{},\"tool\":{},\"error\":{{\"kind\":\"server_restart_failed\",\"message\":{}}}}}",
        Value::from(server),
        Value::from(alias),
        Value::from(format!("MCP server stopped and could not be restarted: {failure}"))
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::config::{Source, Transport};
    use crate::mcp::runtime::{Capabilities, Connection};

    fn view(name: &str, connection: Connection, tools: Option<usize>) -> ServerView {
        ServerView {
            name: name.into(),
            source: Source::Profile,
            transport: Transport::Stdio,
            admission: None,
            connection,
            needs_auth: false,
            required: false,
            reloading: false,
            protocol_version: None,
            capabilities: Capabilities::default(),
            tool_count: tools,
            tools_held: tools.unwrap_or(0),
            listed: Default::default(),
            failure: None,
            entry: None,
        }
    }

    #[test]
    fn the_block_is_upstreams_bytes_sorted_by_name() {
        // `cap/fx-mcp-fixture-40x120-bodies/001-completion.json`, prompt[3].
        let block = render_servers(&[
            view("proj", Connection::Disabled, None),
            view("fixture", Connection::Ready, Some(1)),
        ]);
        assert_eq!(
            block,
            "Configured MCP servers visible to this model turn are listed below.\nA listed server is not a reason to use MCP. Use capability_search only when the task clearly needs a capability not already available locally. Pass the task and an optional exact server name. Call tools advertised after search directly. Use mcp_select_tool when a relevant result is not yet callable. Use returned identities and refine the query when needed.\n<mcp_servers>\n  <server name=\"fixture\" state=\"ready\" tools=\"1\" />\n  <server name=\"proj\" state=\"disabled\" />\n</mcp_servers>\n"
        );
        assert!(render_servers(&[]).ends_with("<mcp_servers>\n  <none />\n</mcp_servers>\n"));
    }

    #[test]
    fn a_block_too_large_says_how_many_servers_it_left_out() {
        let views: Vec<ServerView> = (0..200)
            .map(|index| view(&format!("server-{index:03}"), Connection::Connecting, None))
            .collect();
        let block = render_servers(&views);
        assert!(block.len() <= MAX_BLOCK_BYTES, "{}", block.len());
        assert!(
            block.contains("<catalog_truncated omitted_count=\""),
            "{block}"
        );
        assert!(block.contains("state=\"discovering\""));
    }

    fn document(server: &str, name: &str, description: &str) -> Document {
        let alias = format!("mcp_{server}_{name}");
        Document {
            identities: [name.into(), alias.clone()],
            primary: vec![server.into(), name.into(), alias.clone(), String::new()]
                .into_iter()
                .chain(crate::mcp::names::tags(server, name))
                .collect(),
            secondary: [description.into(), String::new(), String::new()],
            stable_key: alias,
        }
    }

    #[test]
    fn a_search_keeps_clear_matches_only_and_puts_an_exact_name_first() {
        let documents = [
            document(
                "fixture",
                "echo",
                "Echo text through the legacy MCP fixture",
            ),
            document("github", "create_issue", "Open a new issue in a repository"),
            document("github", "list_issues", "List issues in a repository"),
        ];
        assert_eq!(
            rank("echo text through the fixture", &documents, false),
            [0]
        );
        let issues = rank("open an issue in a repository", &documents, false);
        assert_eq!(issues.first(), Some(&1), "{issues:?}");
        assert_eq!(rank("list_issues", &documents, false), [2]);
        assert!(rank("weather forecast", &documents, false).is_empty());
    }

    #[test]
    fn the_combined_answer_is_upstreams_shape() {
        let tool = McpTool {
            name: "echo".into(),
            alias: "mcp_fixture_echo".into(),
            title: None,
            description: "Echo text through the legacy MCP fixture".into(),
            input_schema: serde_json::json!({"type": "object"}),
            tags: crate::mcp::names::tags("fixture", "echo"),
        };
        assert_eq!(
            combined(&[tool_metadata("fixture", &tool)], 1, None, None).output,
            r#"{"skills":[],"mcp_tools":[{"name":"mcp_fixture_echo","server":"fixture","description":"Echo text through the legacy MCP fixture","purpose":"Echo text through the legacy MCP fixture","usage":["mcp","fixture","echo"]}],"counts":{"skills":0,"mcp_tools":1},"total_matches":{"skills":0,"mcp_tools":1}}"#
        );
        assert!(combined(&[], 0, None, None)
            .output
            .ends_with(r#","state":"no_match"}"#));
        let selected = select(&tool, None).expect("selected");
        assert_eq!(
            selected.schema.to_string(),
            r#"{"type":"function","name":"mcp_fixture_echo","description":"Echo text through the legacy MCP fixture","inputSchema":{"type":"object"}}"#
        );
    }

    #[test]
    fn a_call_answer_is_v0016s_content_structured_content_and_is_error() {
        use serde_json::json;
        // `p5-mcp-turn`'s `004-completion.json` on fx v0.0.16: the server's
        // `content`, then `"isError":false` the fixture never sent.
        assert_eq!(
            call_answer(
                "fixture",
                "mcp_fixture_echo",
                &json!({"content": [{"type": "text", "text": "LEGACY_MCP_TOOL_RESULT:hi"}]})
            ),
            CallAnswer::Result {
                envelope: r#"{"server":"fixture","tool":"mcp_fixture_echo","result":{"content":[{"type":"text","text":"LEGACY_MCP_TOOL_RESULT:hi"}],"isError":false}}"#.into(),
                is_error: false,
            }
        );
        // `structuredContent` between them, whatever it holds; every other
        // key dropped, wherever the server put it (`render`, `model.zig:838`).
        assert_eq!(
            call_answer(
                "s",
                "mcp_s_t",
                &json!({"_meta": {"k": 1}, "isError": true, "structuredContent": {"n": 2}, "content": [], "extra": "x"})
            ),
            CallAnswer::Result {
                envelope: r#"{"server":"s","tool":"mcp_s_t","result":{"content":[],"structuredContent":{"n":2},"isError":true}}"#.into(),
                is_error: true,
            }
        );
        let malformed = CallAnswer::Malformed(
            "MCP server 's' ended the call: the server's answer was malformed. The tool may have run."
                .into(),
        );
        for result in [
            json!({}),
            json!({"content": "text"}),
            json!({"content": [], "isError": "false"}),
            json!({"content": null}),
        ] {
            assert_eq!(call_answer("s", "mcp_s_t", &result), malformed, "{result}");
        }
    }

    #[test]
    fn an_open_search_that_found_nothing_names_what_it_could_not_look_in() {
        let mut login = view("linear", Connection::Failed, None);
        login.needs_auth = true;
        let mut failed = view("say \"hi\"", Connection::Failed, None);
        failed.failure = Some("its process exited with code 3".into());
        let silent = view("quiet", Connection::Failed, None);
        let reasons = unavailable(&[
            view("ready", Connection::Ready, Some(1)),
            login,
            failed,
            silent,
            view("off", Connection::Disabled, None),
            view("starting", Connection::Connecting, None),
        ]);
        assert_eq!(
            reasons,
            [
                "MCP server 'linear' needs a login. Ask the user to run /mcp login linear, then try again.",
                "MCP server 'say \"hi\"' is unavailable: its process exited with code 3",
                "MCP server 'quiet' is unavailable: it did not start",
            ]
        );
        // Last, after `state`, each encoded as upstream encodes text for the
        // model (`tool_search.zig:43-52`,
        // `capability_search.zig:423-434@481104fd`).
        assert_eq!(
            with_unavailable(combined(&[], 0, None, None).output, &reasons[..2]),
            r#"{"skills":[],"mcp_tools":[],"counts":{"skills":0,"mcp_tools":0},"total_matches":{"skills":0,"mcp_tools":0},"state":"no_match","mcp_unavailable":["MCP server 'linear' needs a login. Ask the user to run /mcp login linear, then try again.","MCP server 'say &quot;hi&quot;' is unavailable: its process exited with code 3"]}"#
        );
        let nothing = combined(&[], 0, None, None).output;
        assert_eq!(with_unavailable(nothing.clone(), &[]), nothing);
    }
}
