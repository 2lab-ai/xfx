//! The names a model calls MCP tools by.
//!
//! `mcp_<server>_<tool>`, each part with every byte that is not `[A-Za-z0-9_-]`
//! turned into `_`, cut to 64 bytes, and given `_2`, `_3`, … when it would
//! collide with a built-in tool or with another server's tool
//! (`vercel-labs/fx@c1db9191 src/core/mcp/tool_names.zig:37-127`). One
//! registry per session, and a name once given is never given to anything
//! else: a later catalog cannot make a name the model already saw mean a
//! different tool.

use std::collections::HashMap;

/// The longest name (`max_name_len`, `tool_names.zig:117`).
const MAX_NAME_BYTES: usize = 64;

/// How many tags one tool gets (`max_mcp_tool_tags`).
const MAX_TAGS: usize = 16;

/// The session's names.
#[derive(Debug, Clone)]
pub struct ToolNames {
    builtins: Vec<String>,
    /// `<len>:<server><tool>` to its name.
    by_identity: HashMap<String, String>,
    taken: HashMap<String, String>,
}

impl ToolNames {
    /// A registry that never hands out one of `builtins`.
    pub fn new(builtins: &[&str]) -> Self {
        Self {
            builtins: builtins.iter().map(|name| name.to_string()).collect(),
            by_identity: HashMap::new(),
            taken: HashMap::new(),
        }
    }

    /// The name of `tool` on `server`, the same every time it is asked.
    pub fn name(&mut self, server: &str, tool: &str) -> Option<String> {
        let identity = format!("{}:{server}{tool}", server.len());
        if let Some(existing) = self.by_identity.get(&identity) {
            return Some(existing.clone());
        }
        if self.by_identity.len() >= 64 * 1024 {
            return None;
        }
        let base = base_name(server, tool);
        let mut suffix: Option<usize> = None;
        loop {
            let candidate = with_suffix(&base, suffix);
            if self.builtins.contains(&candidate) || self.taken.contains_key(&candidate) {
                suffix = Some(suffix.map_or(2, |value| value + 1));
                continue;
            }
            self.taken.insert(candidate.clone(), server.to_string());
            self.by_identity.insert(identity, candidate.clone());
            return Some(candidate);
        }
    }
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-'
}

fn sanitized(segment: &str, empty: &str) -> String {
    if segment.is_empty() {
        return empty.to_string();
    }
    segment
        .bytes()
        .map(|byte| {
            if is_identifier_byte(byte) {
                byte as char
            } else {
                '_'
            }
        })
        .collect()
}

/// `buildBaseToolName` (`tool_names.zig:84-101`).
fn base_name(server: &str, tool: &str) -> String {
    format!(
        "mcp_{}_{}",
        sanitized(server, "server"),
        sanitized(tool, "tool")
    )
}

/// `candidateWithSuffix` (`tool_names.zig:116-127`).
fn with_suffix(base: &str, suffix: Option<usize>) -> String {
    match suffix {
        Some(index) => {
            let suffix = format!("_{index}");
            let keep = base.len().min(MAX_NAME_BYTES - suffix.len());
            format!("{}{suffix}", &base[..keep])
        }
        None => base[..base.len().min(MAX_NAME_BYTES)].to_string(),
    }
}

/// `tagsFor` (`tool_names.zig:146-183`).
pub fn tags(server: &str, tool: &str) -> Vec<String> {
    let mut tags: Vec<String> = vec!["mcp".to_string()];
    for text in [server, tool] {
        for token in text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-')) {
            if token.is_empty() || tags.len() >= MAX_TAGS {
                continue;
            }
            let lowered = token.to_ascii_lowercase();
            if !tags.contains(&lowered) {
                tags.push(lowered);
            }
        }
    }
    tags
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_the_server_and_the_tool_sanitized_and_stable() {
        let mut names = ToolNames::new(&["read_file"]);
        assert_eq!(
            names.name("fixture", "echo").as_deref(),
            Some("mcp_fixture_echo")
        );
        assert_eq!(names.name("a/b", "c").as_deref(), Some("mcp_a_b_c"));
        // `a_b` collides with `a/b` once sanitized, so it gets the suffix,
        // and asking again gives each the name it was given.
        assert_eq!(names.name("a_b", "c").as_deref(), Some("mcp_a_b_c_2"));
        assert_eq!(names.name("a/b", "c").as_deref(), Some("mcp_a_b_c"));
        assert_eq!(names.name("", "").as_deref(), Some("mcp_server_tool"));
        let long = "x".repeat(100);
        assert_eq!(names.name(&long, "t").unwrap().len(), 64);
    }

    #[test]
    fn tags_are_the_identifier_tokens_lowercased_once() {
        assert_eq!(tags("fixture", "echo"), ["mcp", "fixture", "echo"]);
        assert_eq!(
            tags("My.Server", "Get Thing"),
            ["mcp", "my", "server", "get", "thing"]
        );
    }
}
