//! `/mcp`'s text subcommands, in upstream's words.
//!
//! A port of `handleCommand` (`vercel-labs/fx@c1db9191
//! src/builtins/mcp.zig:30-346`) and of the renderers its callbacks use
//! (`app_commands.zig:1487-1695`), for the leaves this build has: `path`,
//! `reload`, `add NAME COMMAND [ARGS...]`, `add --transport http NAME URL`,
//! `remove`, `trust`, `resource`, `prompt`, and the validation half of `auth`
//! and `logout`. What is said comes back as [`Event`]s, in order.
//!
//! **Gated, and said so.** Signing in to a remote server opens a browser and
//! stores a token, which is a decision this build has not been given
//! (`docs/parity.md`). `auth` and `logout` still check what upstream checks
//! first -- the server exists, it is remote, it is approved -- and answer as
//! upstream does when the sign-in leaf is not there: `Interactive MCP
//! authentication is unavailable here.` and `MCP logout is unavailable here.`
//! (`mcp.zig:154-160`, `:206-207`). `add slack` is the same leaf and gets the
//! first of those, with nothing written.

use std::sync::Arc;

use serde_json::{Map, Value};

use super::config::{self, ServerConfig, Source, Transport, TrustAction};
use super::runtime::{McpRuntime, Origin};
use super::McpError;

/// The command's own usage line (`mcp.zig:341-345`).
pub const USAGE: &str =
    "usage: /mcp [list|resource|prompt|add|remove|path|reload|auth|logout|trust]";

/// `add`'s (`command_specs.zig:3`).
pub const ADD_USAGE: &str =
    "usage: /mcp add slack | mcp add NAME COMMAND [ARGS...] | mcp add --transport http NAME URL";

/// What a reload that started says after the line that started it
/// (`app_commands.zig:1395-1404`).
pub const RELOAD_STARTED: &str =
    "MCP reconnection started. Your existing MCP servers will stay active while the new configuration is checked.";

/// The sign-in leaf's answer while it is gated (`mcp.zig:154-160`).
pub const AUTH_UNAVAILABLE: &str = "Interactive MCP authentication is unavailable here.";

/// The sign-out leaf's (`mcp.zig:206-207`).
pub const LOGOUT_UNAVAILABLE: &str = "MCP logout is unavailable here.";

/// Runs `/mcp <rest>` and says what it did.
pub fn handle(runtime: &Arc<McpRuntime>, rest: &str) {
    let trimmed = rest.trim_matches([' ', '\t']);
    if trimmed.is_empty() || trimmed == "list" {
        // The menu: the UI's to open, and nothing to say here.
        return;
    }
    if let Some(rest) = trimmed.strip_prefix("resource ") {
        return resource(runtime, rest.trim_start_matches([' ', '\t']));
    }
    if let Some(rest) = trimmed.strip_prefix("prompt ") {
        return prompt(runtime, rest.trim_start_matches([' ', '\t']));
    }
    let Some(profile) = runtime.profile_path().map(std::path::Path::to_path_buf) else {
        return runtime.notice(false, "HOME is not available.");
    };
    if trimmed == "path" {
        return runtime.notice(false, profile.to_string_lossy());
    }
    if trimmed == "reload" {
        runtime.notice(false, RELOAD_STARTED);
        return runtime.reload(Origin::Transcript);
    }
    if let Some(rest) = trimmed.strip_prefix("trust ") {
        return trust(runtime, rest);
    }
    if trimmed == "auth" || trimmed.starts_with("auth ") {
        return auth(runtime, &trimmed[4..]);
    }
    if let Some(rest) = trimmed.strip_prefix("logout ") {
        return logout(runtime, rest.trim_matches([' ', '\t']));
    }
    if let Some(rest) = trimmed.strip_prefix("remove ") {
        let name = rest.trim_matches([' ', '\t']);
        if name.is_empty() {
            return runtime.notice(false, "usage: /mcp remove <name>");
        }
        return match config::remove_profile_server(&profile, name) {
            Err(err) => runtime.notice(
                false,
                format!("Failed to remove MCP server '{name}': {err}."),
            ),
            Ok(false) => runtime.notice(false, format!("MCP server '{name}' not found.")),
            Ok(true) => {
                runtime.notice(
                    false,
                    format!("Removed MCP server '{name}'.\n{RELOAD_STARTED}"),
                );
                runtime.reload(Origin::Transcript);
            }
        };
    }
    if let Some(rest) = trimmed.strip_prefix("add ") {
        return add(runtime, &profile, rest);
    }
    runtime.notice(false, USAGE);
}

/// `parseAddIntent` (`command_provider.zig:72-99`) and the save after it.
fn add(runtime: &Arc<McpRuntime>, profile: &std::path::Path, rest: &str) {
    let tokens: Vec<&str> = rest
        .split([' ', '\t'])
        .filter(|token| !token.is_empty())
        .collect();
    let config = match tokens.as_slice() {
        [] => return runtime.notice(false, ADD_USAGE),
        ["slack"] => return runtime.notice(false, AUTH_UNAVAILABLE),
        ["--transport", rest @ ..] => match rest {
            ["http", name, url] => {
                if !config::is_valid_server_name(name) {
                    return runtime.notice(
                        false,
                        "Failed to save MCP server config: McpInvalidServerName.",
                    );
                }
                if config::validate_endpoint(url).is_err() {
                    return runtime.notice(
                        false,
                        "Failed to save MCP server config: McpConfigInvalidUrl.",
                    );
                }
                ServerConfig::http(name, url)
            }
            _ => return runtime.notice(false, ADD_USAGE),
        },
        [_] => return runtime.notice(false, ADD_USAGE),
        [name, command, args @ ..] => {
            if !config::is_valid_server_name(name) {
                return runtime.notice(
                    false,
                    "Failed to save MCP server config: McpInvalidServerName.",
                );
            }
            let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
            ServerConfig::local(name, command, &args)
        }
    };
    let name = config.name.clone();
    match config::add_profile_server(profile, config) {
        Err(err) => runtime.notice(false, format!("Failed to save MCP server config: {err}.")),
        Ok(()) => {
            runtime.notice(
                false,
                format!("Saved MCP server '{name}'.\n{RELOAD_STARTED}"),
            );
            runtime.reload(Origin::Transcript);
        }
    }
}

/// `trust approve|reject <server> | approve-all | reset` (`mcp.zig:105-139`).
fn trust(runtime: &Arc<McpRuntime>, rest: &str) {
    let tokens: Vec<&str> = rest
        .split([' ', '\t'])
        .filter(|token| !token.is_empty())
        .collect();
    let (action, body) = match tokens.as_slice() {
        [] => {
            return runtime.notice(
                false,
                "usage: /mcp trust approve|reject <server> | approve-all | reset",
            )
        }
        ["approve-all"] => (
            TrustAction::ApproveAll,
            "Approving all project MCP servers for this workspace.".to_string(),
        ),
        ["approve-all", ..] => return runtime.notice(false, "usage: /mcp trust approve-all"),
        ["reset"] => (
            TrustAction::Reset,
            "Resetting project MCP choices for this workspace.".to_string(),
        ),
        ["reset", ..] => return runtime.notice(false, "usage: /mcp trust reset"),
        [_] | [_, _, _, ..] => {
            return runtime.notice(false, "usage: /mcp trust approve|reject <server>")
        }
        ["approve", name] => (
            TrustAction::Approve(name.to_string()),
            format!("Approving project MCP server '{name}'."),
        ),
        ["reject", name] => (
            TrustAction::Reject(name.to_string()),
            format!("Rejecting project MCP server '{name}'."),
        ),
        [_, _] => {
            return runtime.notice(
                false,
                "usage: /mcp trust approve|reject <server> | approve-all | reset",
            )
        }
    };
    runtime.run(super::runtime::Job::Trust { action, body });
}

/// `authenticationServer` (`mcp_runtime.zig:1671-1683`): the checks a
/// sign-in makes before it would open anything.
fn remote_server(runtime: &McpRuntime, name: &str) -> Result<(), McpError> {
    let server = runtime
        .server(name)
        .ok_or(McpError::Named("McpServerNotFound"))?;
    if server.config.transport == Transport::Stdio {
        return Err(McpError::Named("McpAuthenticationNotRemote"));
    }
    if server.config.source == Source::Workspace
        && server.config.admission != Some(config::Admission::Approved)
    {
        return Err(McpError::Named("McpWorkspaceApprovalRequired"));
    }
    Ok(())
}

fn auth(runtime: &McpRuntime, rest: &str) {
    let tokens: Vec<&str> = rest
        .split([' ', '\t'])
        .filter(|token| !token.is_empty())
        .collect();
    let name = match tokens.as_slice() {
        [name] | [name, "--open"] => *name,
        _ => return runtime.notice(false, "usage: /mcp auth <name> [--open]"),
    };
    match remote_server(runtime, name) {
        Err(err) => runtime.notice(
            false,
            format!("MCP authentication for '{name}' failed: {err}."),
        ),
        Ok(()) => runtime.notice(false, AUTH_UNAVAILABLE),
    }
}

fn logout(runtime: &McpRuntime, name: &str) {
    if name.is_empty() || name.contains([' ', '\t']) {
        return runtime.notice(false, "usage: /mcp logout <name>");
    }
    let server = match runtime.server(name) {
        None => {
            return runtime.notice(
                false,
                format!("MCP logout for '{name}' failed: McpServerNotFound."),
            )
        }
        Some(server) => server,
    };
    if server.config.transport == Transport::Stdio {
        return runtime.notice(
            false,
            format!("MCP logout for '{name}' failed: McpAuthenticationNotRemote."),
        );
    }
    runtime.notice(false, LOGOUT_UNAVAILABLE)
}

/// The next word of `input`, and what follows it (`takeToken`, `mcp.zig:480-487`).
fn take<'a>(input: &mut &'a str) -> Option<&'a str> {
    *input = input.trim_start_matches([' ', '\t']);
    if input.is_empty() {
        return None;
    }
    let end = input.find([' ', '\t']).unwrap_or(input.len());
    let token = &input[..end];
    *input = &input[end..];
    Some(token)
}

/// A feature operation needs a runtime with servers in it: upstream has none
/// to lease before one is configured (`acquireMcpRuntime`,
/// `cap/fx-mcp-empty-40x120-resource-nope`).
fn leased(runtime: &McpRuntime) -> Result<(), McpError> {
    if runtime.views().is_empty() {
        Err(McpError::Named("McpRuntimeUnavailable"))
    } else {
        Ok(())
    }
}

/// `resource list|templates|read|complete …` (`mcp.zig:395-440`).
fn resource(runtime: &McpRuntime, rest: &str) {
    let mut input = rest;
    let Some(action) = take(&mut input) else {
        return runtime.notice(
            false,
            "usage: /mcp resource [list|templates|read|complete] ...",
        );
    };
    match action {
        "list" | "templates" => {
            let usage = "usage: /mcp resource list <server> or /mcp resource templates <server>";
            let Some(server) = take(&mut input) else {
                return runtime.notice(false, usage);
            };
            if take(&mut input).is_some() {
                return runtime.notice(false, usage);
            }
            let templates = action == "templates";
            match leased(runtime).and_then(|()| runtime.list_resources(server, templates, None)) {
                Err(err) => runtime.notice(false, format!("MCP resource listing failed: {err}.")),
                Ok(items) => {
                    let mut out = format!(
                        "MCP {} from {server} ({}):\n",
                        if templates {
                            "resource templates"
                        } else {
                            "resources"
                        },
                        items.len()
                    );
                    for item in items {
                        out.push_str(&format!(
                            "  {server} :: {} — {}\n",
                            item.uri,
                            item.title.as_deref().unwrap_or(&item.name)
                        ));
                    }
                    runtime.notice(false, out);
                }
            }
        }
        "read" => {
            let usage = "usage: /mcp resource read <server> <uri>";
            let Some(server) = take(&mut input) else {
                return runtime.notice(false, usage);
            };
            let uri = input.trim_matches([' ', '\t']);
            if uri.is_empty() {
                return runtime.notice(false, usage);
            }
            match leased(runtime).and_then(|()| runtime.read_resource(server, uri, None)) {
                Err(err) => runtime.notice(false, format!("MCP resource read failed: {err}.")),
                Ok(contents) => {
                    let mut out = format!("[untrusted MCP resource content] {server} :: {uri}\n");
                    for content in contents {
                        out.push('\n');
                        out.push_str(content.get("uri").and_then(Value::as_str).unwrap_or(uri));
                        if let Some(mime) = content.get("mimeType").and_then(Value::as_str) {
                            out.push_str(&format!(" ({mime})"));
                        }
                        out.push('\n');
                        match (content.get("text"), content.get("blob")) {
                            (Some(Value::String(text)), _) => out.push_str(text),
                            (_, Some(Value::String(blob))) => out
                                .push_str(&format!("<base64 blob: {} bytes encoded>", blob.len())),
                            _ => {}
                        }
                        out.push('\n');
                    }
                    runtime.notice(false, out);
                }
            }
        }
        "complete" => {
            let usage = "usage: /mcp resource complete <server> <uri-template> <variable> [value]";
            let (Some(server), Some(template), Some(variable)) =
                (take(&mut input), take(&mut input), take(&mut input))
            else {
                return runtime.notice(false, usage);
            };
            let value = input.trim_matches([' ', '\t']);
            let reference = serde_json::json!({"type": "ref/resource", "uri": template});
            match leased(runtime).and_then(|()| {
                runtime.complete(server, reference, variable, value, Map::new(), None)
            }) {
                Err(err) => {
                    runtime.notice(false, format!("MCP resource completion failed: {err}."))
                }
                Ok(completion) => runtime.notice(false, completions(server, &completion)),
            }
        }
        _ => runtime.notice(
            false,
            "usage: /mcp resource [list|templates|read|complete] ...",
        ),
    }
}

/// `prompt list|get|complete …` (`mcp.zig:442-478`).
fn prompt(runtime: &McpRuntime, rest: &str) {
    let mut input = rest;
    let Some(action) = take(&mut input) else {
        return runtime.notice(false, "usage: /mcp prompt [list|get|complete] ...");
    };
    match action {
        "list" => {
            let usage = "usage: /mcp prompt list <server>";
            let Some(server) = take(&mut input) else {
                return runtime.notice(false, usage);
            };
            if take(&mut input).is_some() {
                return runtime.notice(false, usage);
            }
            match leased(runtime).and_then(|()| runtime.list_prompts(server, None)) {
                Err(err) => runtime.notice(false, format!("MCP prompt listing failed: {err}.")),
                Ok(items) => {
                    let mut out = format!("MCP prompts from {server} ({}):\n", items.len());
                    for item in items {
                        out.push_str(&format!("  {server} :: {}", item.name));
                        if let Some(label) = item.title.as_ref().or(item.description.as_ref()) {
                            out.push_str(&format!(" — {label}"));
                        }
                        if !item.arguments.is_empty() {
                            let arguments: Vec<String> = item
                                .arguments
                                .iter()
                                .map(|argument| {
                                    format!(
                                        "{}{}",
                                        argument.name,
                                        if argument.required { "*" } else { "" }
                                    )
                                })
                                .collect();
                            out.push_str(&format!(" [{}]", arguments.join(", ")));
                        }
                        out.push('\n');
                    }
                    runtime.notice(false, out);
                }
            }
        }
        "get" => {
            let usage = "usage: /mcp prompt get <server> <name> [arguments-json]";
            let (Some(server), Some(name)) = (take(&mut input), take(&mut input)) else {
                return runtime.notice(false, usage);
            };
            let raw = input.trim_matches([' ', '\t']);
            let raw = if raw.is_empty() { "{}" } else { raw };
            let arguments = match serde_json::from_str::<Value>(raw) {
                Ok(Value::Object(arguments)) if arguments.values().all(Value::is_string) => {
                    Ok(arguments)
                }
                _ => Err(McpError::Named("McpInvalidPromptArguments")),
            };
            match leased(runtime)
                .and(arguments)
                .and_then(|arguments| runtime.get_prompt(server, name, arguments, None))
            {
                Err(err) => runtime.notice(false, format!("MCP prompt invocation failed: {err}.")),
                Ok(result) => {
                    let mut out = format!("[untrusted MCP prompt content] {server} :: {name}\n");
                    if let Some(description) = result.get("description").and_then(Value::as_str) {
                        out.push_str(description);
                        out.push('\n');
                    }
                    for message in result
                        .get("messages")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                    {
                        let content = message.get("content").cloned().unwrap_or(Value::Null);
                        out.push_str(&format!(
                            "\n{} ({}):\n{}\n",
                            message
                                .get("role")
                                .and_then(Value::as_str)
                                .unwrap_or("user"),
                            content
                                .get("type")
                                .and_then(Value::as_str)
                                .unwrap_or("text"),
                            content
                        ));
                    }
                    runtime.notice(false, out);
                }
            }
        }
        "complete" => {
            let usage = "usage: /mcp prompt complete <server> <name> <argument> [value]";
            let (Some(server), Some(name), Some(argument)) =
                (take(&mut input), take(&mut input), take(&mut input))
            else {
                return runtime.notice(false, usage);
            };
            let value = input.trim_matches([' ', '\t']);
            let reference = serde_json::json!({"type": "ref/prompt", "name": name});
            match leased(runtime).and_then(|()| {
                runtime.complete(server, reference, argument, value, Map::new(), None)
            }) {
                Err(err) => runtime.notice(false, format!("MCP prompt completion failed: {err}.")),
                Ok(completion) => runtime.notice(false, completions(server, &completion)),
            }
        }
        _ => runtime.notice(false, "usage: /mcp prompt [list|get|complete] ..."),
    }
}

/// `renderMcpCompletions` (`app_commands.zig:1685-1695`).
fn completions(server: &str, completion: &Value) -> String {
    let values: Vec<&str> = completion
        .get("values")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let mut out = format!("MCP completions from {server} ({}", values.len());
    if let Some(total) = completion.get("total").and_then(Value::as_u64) {
        out.push_str(&format!(" of {total}"));
    }
    out.push_str("):\n");
    for value in values {
        out.push_str(&format!("  {value}\n"));
    }
    if completion.get("hasMore").and_then(Value::as_bool) == Some(true) {
        out.push_str("  … more available\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::runtime::{Event, Paths};

    fn said(runtime: &Arc<McpRuntime>, line: &str) -> Vec<String> {
        handle(runtime, line);
        runtime
            .take_events()
            .into_iter()
            .filter_map(|event| match event {
                Event::Notice { body, .. } => Some(body),
                _ => None,
            })
            .collect()
    }

    fn runtime(dir: &std::path::Path) -> Arc<McpRuntime> {
        McpRuntime::new(Paths {
            profile: Some(dir.join(".xfx").join("mcp.json")),
            settings: Some(dir.join(".xfx").join("settings.json")),
            workspace: dir.join("work"),
        })
    }

    #[test]
    fn every_local_answer_with_no_servers_is_upstreams_sentence() {
        // `cap/fx-mcp-empty-40x120-bogus.ansi`, row by row.
        let dir = tempfile::tempdir().expect("dir");
        let runtime = runtime(dir.path());
        let path = dir.path().join(".xfx").join("mcp.json");
        assert_eq!(
            said(&runtime, "path"),
            [path.to_string_lossy().into_owned()]
        );
        assert_eq!(said(&runtime, "auth"), ["usage: /mcp auth <name> [--open]"]);
        assert_eq!(
            said(&runtime, "auth nope"),
            ["MCP authentication for 'nope' failed: McpServerNotFound."]
        );
        assert_eq!(
            said(&runtime, "logout nope"),
            ["MCP logout for 'nope' failed: McpServerNotFound."]
        );
        assert_eq!(
            said(&runtime, "remove nope"),
            ["MCP server 'nope' not found."]
        );
        for bare in ["add", "prompt", "trust", "bogus"] {
            assert_eq!(said(&runtime, bare), [USAGE], "{bare}");
        }
        assert_eq!(
            said(&runtime, "resource list nope"),
            ["MCP resource listing failed: McpRuntimeUnavailable."]
        );
        assert_eq!(said(&runtime, "add slack"), [AUTH_UNAVAILABLE]);
        assert!(!path.exists(), "a gated `add slack` wrote the profile");
        assert_eq!(said(&runtime, "add only"), [ADD_USAGE]);
        assert_eq!(
            said(&runtime, "add bad/name cmd"),
            ["Failed to save MCP server config: McpInvalidServerName."]
        );
        assert_eq!(
            said(
                &runtime,
                "add --transport http remote http://example.test/mcp"
            ),
            ["Failed to save MCP server config: McpConfigInvalidUrl."]
        );
    }

    #[test]
    fn reload_says_it_started_and_the_reload_itself_says_nothing() {
        // fx v0.0.16 reloads in place and says nothing of it (`333d4764`);
        // the reload's own sentence is gone with it.
        let dir = tempfile::tempdir().expect("dir");
        let runtime = runtime(dir.path());
        assert_eq!(said(&runtime, "reload"), [RELOAD_STARTED]);
    }
}
