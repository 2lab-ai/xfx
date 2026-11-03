//! Configured permission rules: upstream's table, its pattern language, and
//! the verdict one table gives one target.
//!
//! A port, not a design. Every function here is a function of
//! `vercel-labs/fx@c1db9191 src/core/permissions/permissions.zig`, byte for
//! byte where upstream works on bytes, and cited at its definition. The table
//! is the one a person writes into `~/.xfx/settings.json` under `permission`
//! (`config_runtime.zig:2015-2060`): a category -- `bash`, `edit`, `read`, a
//! tool's own name, or a glob over those -- and either one action for the whole
//! category or one action per target pattern.
//!
//! # Why upstream's language and not a narrower one
//!
//! xfx's first release kept rules exact (`tool` + `target`), on the reasoning
//! that a pattern language is a second thing to get wrong. A settings file is
//! shared with nothing, but it *is* the same shape upstream's is, and a person
//! who writes `"bash": {"git *": "allow"}` expects what upstream does with it.
//! Upstream's matcher is not naive about that, and the port keeps every one of
//! its narrowings:
//!
//! - **An allow for a command is static-grammar only** (`ruleTargetMatches`,
//!   `:1620-1630`). A pattern with a wildcard allows only a candidate made of
//!   plain words and single-quoted literals, so `git *` never allows
//!   `git status && rm -rf ~`, `git $(...)`, or a leading `NAME=value`. A deny
//!   or an ask keeps the plain glob, which only ever makes them catch *more*.
//! - **The last matching rule wins** (`evaluateRulesetForTool`, `:1558-1575`),
//!   in the order the file lists them. That order is load-bearing, which is
//!   why the crate's JSON objects keep document order.
//! - **A `web_fetch` rule is an exact canonical domain** (`:1577-1592`) and a
//!   malformed one matches nothing.
//!
//! What a rule is matched *against* -- the canonical, workspace-relative path,
//! every directory a write would create, the command's text without its
//! directory -- is the caller's half, `super::policy`'s, because that is where
//! the prepared plan is.

use serde_json::Value;

/// The category every command call is judged under (`permissionNameForTool`,
/// `permissions.zig:1455-1465`: `run_command` is `bash`).
pub const BASH: &str = "bash";
/// The category both file mutations are judged under.
pub const EDIT: &str = "edit";
/// `read_file`'s category.
pub const READ: &str = "read";
/// `glob_files`' category.
pub const GLOB: &str = "glob";
/// `grep_files`' category.
pub const GREP: &str = "grep";
/// `web_fetch`'s category, whose patterns are canonical domains only
/// (`web_fetch_permission`, `:99`).
pub const WEB_FETCH: &str = "web_fetch";
/// `web_search`'s category (`web_search_permission`, `:98`).
pub const WEB_SEARCH: &str = "web_search";
/// The category upstream's `/allowlist add url` writes.
pub const URL: &str = "url";
/// The category upstream's skills are judged under.
pub const SKILL: &str = "skill";

/// The tool name a command call is judged under besides its category.
///
/// Upstream judges a command call as `(permission "bash", tool "run_command")`
/// (`permissionNameForCall`, `tool_admission.zig:161-165`). xfx's command tool
/// is `terminal`, and it is judged under upstream's name on purpose: a rule
/// keyed `"terminal"` means upstream's non-run terminal actions, which never
/// run a command, so judging xfx's command under its own name would let such
/// a rule allow commands upstream's never would.
pub const COMMAND_TOOL: &str = "run_command";

/// What a rule says.
///
/// `@tagName` is the spelling in the file and in every listing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleAction {
    Allow,
    Ask,
    Deny,
}

impl RuleAction {
    /// The action a settings value names, in any case
    /// (`parsePermissionAction`, `config_runtime.zig:1061-1066`).
    pub fn parse(raw: &str) -> Option<Self> {
        if raw.eq_ignore_ascii_case("allow") {
            Some(Self::Allow)
        } else if raw.eq_ignore_ascii_case("ask") {
            Some(Self::Ask)
        } else if raw.eq_ignore_ascii_case("deny") {
            Some(Self::Deny)
        } else {
            None
        }
    }

    /// The word the file and the listings use.
    pub fn label(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Ask => "ask",
            Self::Deny => "deny",
        }
    }
}

/// One row of the table: a category (or a glob over categories and tool
/// names), a target pattern, and what to do on a match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfiguredRule {
    pub permission: String,
    pub pattern: String,
    pub action: RuleAction,
}

impl ConfiguredRule {
    pub fn new(
        permission: impl Into<String>,
        pattern: impl Into<String>,
        action: RuleAction,
    ) -> Self {
        Self {
            permission: permission.into(),
            pattern: pattern.into(),
            action,
        }
    }
}

/// Why a `permission` value is not a table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableError {
    /// The value, or a category's value, is neither a string nor an object
    /// (`InvalidPermissionRulesType`).
    NotATable,
    /// A category name is blank once trimmed (`InvalidPermissionRuleTool`).
    BlankCategory,
    /// An action is not `allow`, `ask` or `deny`, or a pattern's action is not
    /// a string (`InvalidPermissionAction`).
    BadAction(String),
}

impl std::fmt::Display for TableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotATable => {
                f.write_str("`permission` must be an action or an object of categories")
            }
            Self::BlankCategory => f.write_str("a `permission` category name is blank"),
            Self::BadAction(found) => write!(
                f,
                "`{found}` is not a permission action; use allow, ask or deny"
            ),
        }
    }
}

/// A whole table, in the order the file lists it.
///
/// The order is the verdict: [`RuleSet::decide`] is last-match-wins.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuleSet {
    rules: Vec<ConfiguredRule>,
}

impl RuleSet {
    pub fn new(rules: Vec<ConfiguredRule>) -> Self {
        Self { rules }
    }

    pub fn rules(&self) -> &[ConfiguredRule] {
        &self.rules
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Reads a `permission` value (`parsePermissionConfig`,
    /// `config_runtime.zig:2015-2060`).
    ///
    /// - a string is one action for everything: `* * <action>`;
    /// - an object maps each category, trimmed, to either a string -- that
    ///   category's `*` -- or an object of trimmed patterns to actions.
    ///
    /// **All or nothing**, as upstream's parse is: one bad action refuses the
    /// whole table, so a typo cannot silently drop the deny written next to it
    /// while keeping the allows around it.
    pub fn parse(value: &Value) -> Result<Self, TableError> {
        let mut rules = Vec::new();
        match value {
            Value::String(action) => {
                rules.push(ConfiguredRule::new("*", "*", parse_action(action)?));
            }
            Value::Object(categories) => {
                for (category, entry) in categories {
                    let permission = trim_key(category);
                    if permission.is_empty() {
                        return Err(TableError::BlankCategory);
                    }
                    match entry {
                        Value::String(action) => {
                            rules.push(ConfiguredRule::new(permission, "*", parse_action(action)?));
                        }
                        Value::Object(patterns) => {
                            for (pattern, action) in patterns {
                                let Value::String(action) = action else {
                                    return Err(TableError::BadAction(action.to_string()));
                                };
                                rules.push(ConfiguredRule::new(
                                    permission,
                                    trim_key(pattern),
                                    parse_action(action)?,
                                ));
                            }
                        }
                        _ => return Err(TableError::NotATable),
                    }
                }
            }
            _ => return Err(TableError::NotATable),
        }
        Ok(Self { rules })
    }

    /// The verdict for one candidate: the action of the **last** rule whose
    /// category matches and whose pattern matches, or `None`
    /// (`evaluateRulesetForTool`, `permissions.zig:1558-1575`).
    ///
    /// `tool` is the tool's own name, which a rule's category may name instead
    /// of the category (`ruleMatchesPermission`, `:1604-1609`); `None` when
    /// only the category is being asked about.
    ///
    /// The candidate is bytes because a path is: upstream compares raw bytes,
    /// and a lossy conversion would let two different names read as one.
    pub fn decide(
        &self,
        permission: &str,
        tool: Option<&str>,
        candidate: impl AsRef<[u8]>,
    ) -> Option<RuleAction> {
        let candidate = candidate.as_ref();
        if permission == WEB_FETCH {
            return decide_web_fetch(&self.rules, candidate);
        }
        let mut matched = None;
        for rule in &self.rules {
            if !rule_matches_permission(&rule.permission, permission, tool) {
                continue;
            }
            if !rule_target_matches(permission, rule.action, &rule.pattern, candidate) {
                continue;
            }
            matched = Some(rule.action);
        }
        matched
    }

    /// Whether this table refuses the tool `tool` at every target it could
    /// ever be called with -- the test upstream hides a tool from the model by
    /// (`rulesDenyAllTargetsForTool`, `permissions.zig:1138-1141`): judged
    /// under its category ([`permission_name_for_tool`]) and its own name, and
    /// never for `web_fetch`, whose rules are domains and never global.
    pub fn denies_every_target_of_tool(&self, tool: &str) -> bool {
        if tool == WEB_FETCH {
            return false;
        }
        self.denies_every_target(permission_name_for_tool(tool), Some(tool))
    }

    /// `rulesDenyAllTargetsForPermissionAndTool` (`permissions.zig:1143-1165`):
    /// the last rule for this category whose pattern is all `*` denies, and no
    /// later allow or ask for it survives the rules after it.
    ///
    /// A later rule with a narrower pattern does not undo the deny by being
    /// there: it does only if the whole table, asked about that pattern as a
    /// target, still allows or asks -- so `edit: {"*": "deny", "src/*":
    /// "ask"}` advertises the edit tools and `edit: {"src/*": "allow", "*":
    /// "deny"}` hides them.
    pub fn denies_every_target(&self, permission: &str, tool: Option<&str>) -> bool {
        let mut last_global_deny = None;
        for (index, rule) in self.rules.iter().enumerate() {
            if !rule_matches_permission(&rule.permission, permission, tool)
                || !is_global_target_pattern(&rule.pattern)
            {
                continue;
            }
            last_global_deny = match rule.action {
                RuleAction::Allow | RuleAction::Ask => None,
                RuleAction::Deny => Some(index),
            };
        }
        let Some(deny) = last_global_deny else {
            return false;
        };
        !self.rules[deny + 1..].iter().any(|rule| {
            rule_matches_permission(&rule.permission, permission, tool)
                && rule.action != RuleAction::Deny
                && matches!(
                    self.decide(permission, tool, rule.pattern.as_bytes()),
                    Some(RuleAction::Allow | RuleAction::Ask)
                )
        })
    }
}

fn parse_action(raw: &str) -> Result<RuleAction, TableError> {
    RuleAction::parse(raw).ok_or_else(|| TableError::BadAction(raw.to_string()))
}

/// `std.mem.trim(u8, key, " \t\r\n")`.
fn trim_key(key: &str) -> &str {
    key.trim_matches([' ', '\t', '\r', '\n'])
}

/// The category a tool is judged under (`permissionNameForTool`,
/// `permissions.zig:1455-1465`), with xfx's own tools placed in upstream's
/// categories by what they do:
///
/// - `create_folder` is `edit`: upstream has no such tool, and the directories
///   its `write_file` creates on the way are judged as `edit` targets
///   (`decideFileTargetPolicy`, `:803-830`), so an `edit` rule decides
///   directory creation there too.
/// - `list_files` is `glob`: a listing of names without contents, which is what
///   upstream's `glob_files` is judged as.
/// - `terminal` is **not** mapped: a command call is judged as
///   `(`[`BASH`]`, `[`COMMAND_TOOL`]`)` whatever it is called
///   ([`COMMAND_TOOL`] says why), and the name on its own stays its own name,
///   as upstream's `shell` does.
pub fn permission_name_for_tool(tool: &str) -> &str {
    match tool {
        "read_file" => READ,
        "write_file" | "edit_file" | "create_folder" => EDIT,
        "glob_files" | "list_files" => GLOB,
        "grep_files" => GREP,
        "run_command" => BASH,
        "web_fetch" => WEB_FETCH,
        "skill" | "install_skill" => SKILL,
        other => other,
    }
}

/// `ruleMatchesPermission` (`permissions.zig:1604-1609`): the rule's category
/// is a glob over the call's category, and failing that over its tool name.
fn rule_matches_permission(rule_permission: &str, permission: &str, tool: Option<&str>) -> bool {
    if wildcard_match(rule_permission.as_bytes(), permission.as_bytes()) {
        return true;
    }
    match tool {
        Some(name) => wildcard_match(rule_permission.as_bytes(), name.as_bytes()),
        None => false,
    }
}

/// `permissionPatternMatchesTarget` (`permissions.zig:1615-1618`).
fn pattern_matches_target(pattern: &str, candidate: &[u8]) -> bool {
    directory_tree_pattern_matches(pattern.as_bytes(), candidate)
        || wildcard_match(pattern.as_bytes(), candidate)
}

/// `ruleTargetMatches` (`permissions.zig:1620-1630`): an **allow** for a
/// command is exact unless both sides are static command grammar; every other
/// pairing is the plain glob.
fn rule_target_matches(
    permission: &str,
    action: RuleAction,
    pattern: &str,
    candidate: &[u8],
) -> bool {
    if action != RuleAction::Allow || permission != BASH {
        return pattern_matches_target(pattern, candidate);
    }
    if !contains_wildcard(pattern) {
        return pattern.as_bytes() == candidate;
    }
    if !is_static_command(pattern.as_bytes(), true) || !is_static_command(candidate, false) {
        return false;
    }
    static_command_wildcard_match(pattern.as_bytes(), candidate)
}

/// `containsWildcard` (`permissions.zig:1632-1635`).
fn contains_wildcard(pattern: &str) -> bool {
    pattern.contains('*') || pattern.contains('?')
}

/// `isStaticCommand` (`permissions.zig:1637-1674`): plain words of a closed
/// byte set and single-quoted literals, separated by runs of spaces or tabs,
/// with nothing before the first word or after the last, and no leading
/// assignment. `allow_wildcards` admits `*` and `?` outside quotes, for the
/// pattern side.
fn is_static_command(command: impl AsRef<[u8]>, allow_wildcards: bool) -> bool {
    let bytes = command.as_ref();
    if bytes.is_empty() {
        return false;
    }
    if first_word_is_assignment(bytes) {
        return false;
    }
    let mut index = 0;
    let mut in_word = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b' ' || byte == b'\t' {
            if !in_word {
                return false;
            }
            while index < bytes.len() && (bytes[index] == b' ' || bytes[index] == b'\t') {
                index += 1;
            }
            if index == bytes.len() {
                return false;
            }
            in_word = false;
            continue;
        }
        if byte == b'\'' {
            let start = index + 1;
            let Some(offset) = bytes[start..].iter().position(|&b| b == b'\'') else {
                return false;
            };
            let literal = &bytes[start..start + offset];
            if std::str::from_utf8(literal).is_err() {
                return false;
            }
            if literal.iter().any(|&b| b == 0 || b == b'\r' || b == b'\n') {
                return false;
            }
            in_word = true;
            index = start + offset + 1;
            continue;
        }
        if !is_static_unquoted_byte(byte) && !(allow_wildcards && (byte == b'*' || byte == b'?')) {
            return false;
        }
        in_word = true;
        index += 1;
    }
    in_word
}

/// `firstWordIsAssignment` (`permissions.zig:1676-1686`).
fn first_word_is_assignment(command: &[u8]) -> bool {
    let end = command
        .iter()
        .position(|&b| b == b' ' || b == b'\t')
        .unwrap_or(command.len());
    let first = &command[..end];
    let Some(equals) = first.iter().position(|&b| b == b'=') else {
        return false;
    };
    let name = &first[..equals];
    if name.is_empty() || !(name[0].is_ascii_alphabetic() || name[0] == b'_') {
        return false;
    }
    name[1..]
        .iter()
        .all(|&b| b.is_ascii_alphanumeric() || b == b'_')
}

/// `isStaticUnquotedByte` (`permissions.zig:1688-1693`).
fn is_static_unquoted_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'_' | b'.' | b'/' | b',' | b':' | b'+' | b'=' | b'%' | b'@' | b'-'
        )
}

/// `staticCommandWildcardMatch` (`permissions.zig:1695-1728`): the glob, with
/// `*` and `?` literal inside single quotes.
fn static_command_wildcard_match(pattern: &[u8], candidate: &[u8]) -> bool {
    let mut pattern_index = 0;
    let mut candidate_index = 0;
    let mut in_single_quote = false;
    let mut star_index: Option<usize> = None;
    let mut star_candidate_index = 0;

    while candidate_index < candidate.len() {
        if pattern_index < pattern.len() {
            let byte = pattern[pattern_index];
            if !in_single_quote && byte == b'*' {
                star_index = Some(pattern_index);
                pattern_index += 1;
                star_candidate_index = candidate_index;
                continue;
            }
            if (!in_single_quote && byte == b'?') || byte == candidate[candidate_index] {
                if byte == b'\'' {
                    in_single_quote = !in_single_quote;
                }
                pattern_index += 1;
                candidate_index += 1;
                continue;
            }
        }
        let Some(star) = star_index else {
            return false;
        };
        star_candidate_index += 1;
        candidate_index = star_candidate_index;
        pattern_index = star + 1;
        in_single_quote = false;
    }
    while pattern_index < pattern.len() && !in_single_quote && pattern[pattern_index] == b'*' {
        pattern_index += 1;
    }
    pattern_index == pattern.len()
}

/// `directoryTreePatternMatches` (`permissions.zig:1730-1739`): `dir/**` is
/// `dir` itself and everything under it, and nothing that merely starts with
/// the same letters.
fn directory_tree_pattern_matches(pattern: &[u8], candidate: &[u8]) -> bool {
    let Some(dir) = pattern.strip_suffix(b"/**") else {
        return false;
    };
    if pattern == b"/**" {
        return candidate.starts_with(b"/");
    }
    if candidate == dir {
        return true;
    }
    candidate.len() > dir.len() && candidate.starts_with(dir) && candidate[dir.len()] == b'/'
}

/// `wildcardMatch` (`permissions.zig:1741-1770`): `*` is any run of bytes,
/// `/` included, and `?` is one byte. Case-sensitive, as upstream's is.
fn wildcard_match(pattern: &[u8], candidate: &[u8]) -> bool {
    let mut pattern_index = 0;
    let mut candidate_index = 0;
    let mut star_index: Option<usize> = None;
    let mut star_candidate_index = 0;

    while candidate_index < candidate.len() {
        if pattern_index < pattern.len()
            && (pattern[pattern_index] == b'?'
                || pattern[pattern_index] == candidate[candidate_index])
        {
            pattern_index += 1;
            candidate_index += 1;
            continue;
        }
        if pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
            star_index = Some(pattern_index);
            pattern_index += 1;
            star_candidate_index = candidate_index;
            continue;
        }
        let Some(star) = star_index else {
            return false;
        };
        star_candidate_index += 1;
        candidate_index = star_candidate_index;
        pattern_index = star + 1;
    }
    while pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
        pattern_index += 1;
    }
    pattern_index == pattern.len()
}

/// What a rule's pattern is matched against for a path: workspace-relative
/// when it is inside the workspace (`.` for the root itself), absolute
/// otherwise (`fileTargetRuleDecision` with `workspaceRelativePermissionPath`,
/// `permissions.zig:932-949`, and `displayPathTarget`, `:1884-1894`).
///
/// `target` must already be canonical -- symlinks resolved, case as the
/// filesystem stores it -- because a rule is about a file and not about a
/// spelling of one: `secret/*` must not be walked around through a link or a
/// capital letter on a filesystem that ignores case. That is the caller's to
/// guarantee; upstream's callers `realpath` for the same reason. Bytes, not
/// text, for the reason [`RuleSet::decide`] takes them.
pub fn rule_path(primary: &std::path::Path, target: &std::path::Path) -> Vec<u8> {
    match target.strip_prefix(primary) {
        Ok(relative) if relative.as_os_str().is_empty() => b".".to_vec(),
        Ok(relative) => path_bytes(relative),
        Err(_) => path_bytes(target),
    }
}

#[cfg(unix)]
fn path_bytes(path: &std::path::Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    path.as_os_str().as_bytes().to_vec()
}

#[cfg(not(unix))]
fn path_bytes(path: &std::path::Path) -> Vec<u8> {
    path.to_string_lossy().replace('\\', "/").into_bytes()
}

/// `isGlobalTargetPattern` (`permissions.zig:1772-1778`): one or more `*` and
/// nothing else.
pub fn is_global_target_pattern(pattern: &str) -> bool {
    !pattern.is_empty() && pattern.bytes().all(|b| b == b'*')
}

// ---------------------------------------------------------------------------
// web_fetch domains
// ---------------------------------------------------------------------------

/// `evaluateWebFetchRuleset` (`permissions.zig:1577-1592`): exact canonical
/// domains on both sides, last match wins, and anything malformed matches
/// nothing.
fn decide_web_fetch(rules: &[ConfiguredRule], candidate: &[u8]) -> Option<RuleAction> {
    let Ok(candidate) = std::str::from_utf8(candidate) else {
        return None;
    };
    if !is_canonical_web_fetch_domain_pattern(candidate) {
        return None;
    }
    let mut matched = None;
    for rule in rules {
        if rule.permission != WEB_FETCH
            || !is_canonical_web_fetch_domain_pattern(&rule.pattern)
            || rule.pattern != candidate
        {
            continue;
        }
        matched = Some(rule.action);
    }
    matched
}

/// How many `web_fetch` rules a listing must call malformed
/// (`webFetchRuleWarningCount`, `permissions.zig:1594-1602`).
pub fn web_fetch_rule_warning_count(rules: &[ConfiguredRule]) -> usize {
    rules
        .iter()
        .filter(|rule| {
            rule.permission == WEB_FETCH && !is_canonical_web_fetch_domain_pattern(&rule.pattern)
        })
        .count()
}

/// `domain:<host>` for what a person typed, lowercased, with one root dot
/// dropped, or `None` (`canonicalWebFetchDomainPattern`, `:1792-1809`).
pub fn canonical_web_fetch_domain_pattern(raw: &str) -> Option<String> {
    let host = raw.strip_prefix("domain:").unwrap_or(raw);
    if host.contains("://") || host.contains('/') || host.contains('*') || host.contains('?') {
        return None;
    }
    let host = strip_one_root_dot(host);
    if !is_canonicalizable_web_fetch_host(host) {
        return None;
    }
    Some(format!("domain:{}", host.to_ascii_lowercase()))
}

/// `isCanonicalWebFetchDomainPattern` (`permissions.zig:1811-1820`).
pub fn is_canonical_web_fetch_domain_pattern(pattern: &str) -> bool {
    let Some(host) = pattern.strip_prefix("domain:") else {
        return false;
    };
    is_canonicalizable_web_fetch_host(host)
        && !host.ends_with('.')
        && !host.bytes().any(|b| b.is_ascii_uppercase())
}

/// `stripOneRootDot` (`permissions.zig:1822-1825`).
fn strip_one_root_dot(host: &str) -> &str {
    if host.len() > 1 && host.ends_with('.') {
        &host[..host.len() - 1]
    } else {
        host
    }
}

/// `isCanonicalizableWebFetchHost` (`permissions.zig:1827-1844`).
fn is_canonicalizable_web_fetch_host(host: &str) -> bool {
    let bytes = host.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    if bytes[0] == b'[' {
        return is_canonicalizable_ipv6_literal(host);
    }
    if host.contains(':') {
        return false;
    }
    let mut label = 0usize;
    for &byte in bytes {
        if byte <= 0x20 || byte >= 0x7f {
            return false;
        }
        if matches!(byte, b'*' | b'?' | b'/' | b'\\') {
            return false;
        }
        if byte == b'.' {
            if label == 0 {
                return false;
            }
            label = 0;
            continue;
        }
        if !byte.is_ascii_alphanumeric() && byte != b'-' {
            return false;
        }
        label += 1;
    }
    label > 0
}

/// `isCanonicalizableWebFetchIpv6Literal` (`permissions.zig:1846-1857`); the
/// address itself is checked by the standard library's parser where upstream
/// uses its own.
fn is_canonicalizable_ipv6_literal(host: &str) -> bool {
    let bytes = host.as_bytes();
    if bytes.len() < 3 || bytes[bytes.len() - 1] != b']' {
        return false;
    }
    let inner = &host[1..host.len() - 1];
    if inner.contains('%') {
        return false;
    }
    for byte in inner.bytes() {
        if byte <= 0x20 || byte >= 0x7f || byte.is_ascii_uppercase() {
            return false;
        }
        if !byte.is_ascii_hexdigit() && byte != b':' && byte != b'.' {
            return false;
        }
    }
    inner.parse::<std::net::Ipv6Addr>().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn set(rules: &[(&str, &str, RuleAction)]) -> RuleSet {
        RuleSet::new(
            rules
                .iter()
                .map(|(permission, pattern, action)| {
                    ConfiguredRule::new(*permission, *pattern, *action)
                })
                .collect(),
        )
    }

    use RuleAction::{Allow, Ask, Deny};

    #[test]
    fn a_verdict_is_the_last_matching_rule_and_none_when_nothing_matches() {
        // `ruleDecisionForPermissionPattern matches direct inputs and fallback`
        // (`permissions.zig:2397-2410`).
        let rules = set(&[
            ("bash", "git *", Allow),
            ("bash", "git push *", Deny),
            ("edit", "src/?ain.zig", Ask),
        ]);
        assert_eq!(rules.decide(BASH, None, "git status"), Some(Allow));
        assert_eq!(rules.decide(BASH, None, "git push origin main"), Some(Deny));
        assert_eq!(rules.decide(EDIT, None, "src/main.zig"), Some(Ask));
        assert_eq!(rules.decide(READ, None, "README.md"), None);
    }

    #[test]
    fn the_order_of_the_table_decides_and_a_later_rule_overrides_an_earlier_one() {
        // The same two rules, the other way round: the verdict follows the
        // order, which is why a settings write must never reorder the table.
        let deny_last = set(&[("bash", "rm*", Allow), ("bash", "*", Deny)]);
        assert_eq!(deny_last.decide(BASH, None, "rm a"), Some(Deny));
        let allow_last = set(&[("bash", "*", Deny), ("bash", "rm*", Allow)]);
        assert_eq!(allow_last.decide(BASH, None, "rm a"), Some(Allow));
        assert_eq!(allow_last.decide(BASH, None, "ls"), Some(Deny));
    }

    #[test]
    fn a_wildcard_command_allow_admits_only_static_grammar() {
        // `configured wildcard command allows only static command grammar`
        // (`permissions.zig:2412-2422`).
        let rules = set(&[("*", "printf *", Allow)]);
        assert_eq!(rules.decide(BASH, None, "printf safe"), Some(Allow));
        assert_eq!(rules.decide(BASH, None, "printf 'safe value'"), Some(Allow));
        for dynamic in [
            "printf safe && touch /tmp/fx-marker",
            "printf \"$(touch /tmp/fx-marker)\"",
            "printf safe; rm -rf ~",
            "printf safe | sh",
            "printf `id`",
            "printf safe > out",
            "printf \"quoted\"",
            "printf $HOME",
            "printf safe\nrm x",
        ] {
            assert_eq!(rules.decide(BASH, None, dynamic), None, "{dynamic:?}");
        }
    }

    #[test]
    fn a_command_allow_with_no_wildcard_is_exact() {
        // `configured command rules require exact matching outside static
        // grammar` (`permissions.zig:2424-2440`).
        let exact = set(&[("bash", "printf \"$(date)\"", Allow)]);
        assert_eq!(exact.decide(BASH, None, "printf \"$(date)\""), Some(Allow));
        assert_eq!(exact.decide(BASH, None, "printf \"$(touch /tmp/x)\""), None);
        assert_eq!(exact.decide(BASH, None, "printf \"$(date)\" "), None);
        let dynamic_pattern = set(&[("bash", "printf \"*\"", Allow)]);
        assert_eq!(dynamic_pattern.decide(BASH, None, "printf \"safe\""), None);
    }

    #[test]
    fn a_command_deny_or_ask_keeps_the_plain_glob_so_it_catches_more() {
        // `configured command deny and ask retain generic wildcard matching`
        // (`permissions.zig:2442-2451`).
        let rules = set(&[("bash", "printf *", Deny), ("custom", "printf *", Ask)]);
        assert_eq!(
            rules.decide(BASH, None, "printf safe && touch /tmp/fx-marker"),
            Some(Deny)
        );
        assert_eq!(
            rules.decide("custom", None, "printf \"$(touch /tmp/fx-marker)\""),
            Some(Ask)
        );
    }

    #[test]
    fn the_static_command_grammar_is_an_explicit_allowlist() {
        // `static command grammar is an explicit allowlist`
        // (`permissions.zig:2453-2490`).
        for command in [
            "git status --short",
            "printf 'safe value'",
            "printf ''",
            "tool foo/bar:baz,+qux=value%@host",
            "printf 'Grüße'",
            "a\tb",
        ] {
            assert!(is_static_command(command, false), "{command:?}");
        }
        for command in [
            "",
            " leading",
            "trailing ",
            "FOO=bar command",
            "FOO='bar' command",
            "_X=1",
            "echo \"dynamic\"",
            "echo $HOME",
            "echo `pwd`",
            "echo ~",
            "echo [ab]",
            "echo #comment",
            "echo foo\\bar",
            "echo one && echo two",
            "echo one | cat",
            "echo one > out",
            "(echo one)",
            "echo one\necho two",
            "echo 'unterminated",
            "echo 'a\nb'",
            "echo 'a\rb'",
            "echo 'nul\0'",
            "echo Grüße",
        ] {
            assert!(!is_static_command(command, false), "{command:?}");
        }
        assert!(is_static_command("git * --?", true));
        assert!(!is_static_command("git * --?", false));
        assert!(is_static_command("printf '*'", false));
        // An `=` that is not an assignment is an ordinary byte.
        assert!(!is_static_command("a=b", false));
        assert!(is_static_command("--opt=value", false));
        assert!(is_static_command("9X=1", false));
    }

    #[test]
    fn a_quoted_wildcard_is_literal_and_an_unquoted_one_is_not() {
        let rules = set(&[("bash", "printf '*' *", Allow)]);
        assert_eq!(rules.decide(BASH, None, "printf '*' x"), Some(Allow));
        assert_eq!(rules.decide(BASH, None, "printf 'a' x"), None);
        let question = set(&[("bash", "git ?ull", Allow)]);
        assert_eq!(question.decide(BASH, None, "git pull"), Some(Allow));
        assert_eq!(question.decide(BASH, None, "git null"), Some(Allow));
        assert_eq!(question.decide(BASH, None, "git pulls"), None);
        let trailing = set(&[("bash", "ls**", Allow)]);
        assert_eq!(trailing.decide(BASH, None, "ls"), Some(Allow));
        assert_eq!(trailing.decide(BASH, None, "ls -la"), Some(Allow));
    }

    #[test]
    fn a_star_crosses_slashes_and_a_tree_pattern_is_the_directory_and_what_is_under_it() {
        // `directory tree permission patterns match directory and descendants
        // only` (`permissions.zig:2685-2694`).
        let tree = set(&[("read", "/tmp/external/**", Allow)]);
        assert_eq!(
            tree.decide(READ, Some("read_file"), "/tmp/external"),
            Some(Allow)
        );
        assert_eq!(
            tree.decide(READ, Some("read_file"), "/tmp/external/file.txt"),
            Some(Allow)
        );
        assert_eq!(
            tree.decide(READ, Some("read_file"), "/tmp/external-other/file.txt"),
            None
        );
        let star = set(&[("edit", "src/*", Allow)]);
        assert_eq!(star.decide(EDIT, None, "src/a/b/c.rs"), Some(Allow));
        assert_eq!(star.decide(EDIT, None, "src"), None);
        assert_eq!(star.decide(EDIT, None, "srcx/a"), None);
        let root = set(&[("read", "/**", Allow)]);
        assert_eq!(root.decide(READ, None, "/etc/passwd"), Some(Allow));
        assert_eq!(root.decide(READ, None, "relative"), None);
    }

    #[test]
    fn matching_is_case_sensitive_and_byte_wise() {
        let rules = set(&[("edit", "src/*", Deny)]);
        assert_eq!(rules.decide(EDIT, None, "SRC/a"), None);
        assert_eq!(rules.decide(EDIT, None, "src/a"), Some(Deny));
        // `?` is one byte, as upstream's is: a two-byte character is two.
        let one = set(&[("edit", "?", Allow)]);
        assert_eq!(one.decide(EDIT, None, "é"), None);
        let two = set(&[("edit", "??", Allow)]);
        assert_eq!(two.decide(EDIT, None, "é"), Some(Allow));
    }

    #[test]
    fn a_rule_names_a_category_a_tool_or_a_glob_over_either() {
        let by_tool = set(&[("read_file", "*", Deny)]);
        assert_eq!(by_tool.decide(READ, Some("read_file"), "a"), Some(Deny));
        assert_eq!(by_tool.decide(READ, None, "a"), None);
        let glob = set(&[("*_file", "*", Ask)]);
        assert_eq!(glob.decide(EDIT, Some("write_file"), "a"), Some(Ask));
        let everything = set(&[("*", "*", Deny)]);
        assert_eq!(everything.decide(GREP, Some("grep_files"), "."), Some(Deny));
        // A rule for another category never applies.
        let other = set(&[("edit", "*", Allow)]);
        assert_eq!(other.decide(BASH, Some(COMMAND_TOOL), "ls"), None);
    }

    #[test]
    fn a_command_is_judged_under_upstreams_names_and_not_under_terminal() {
        // `{"terminal": "allow"}` is upstream's non-run terminal actions; it
        // must not allow xfx's commands.
        let terminal = set(&[("terminal", "*", Allow)]);
        assert_eq!(terminal.decide(BASH, Some(COMMAND_TOOL), "ls"), None);
        let run = set(&[("run_command", "ls", Allow)]);
        assert_eq!(run.decide(BASH, Some(COMMAND_TOOL), "ls"), Some(Allow));
        assert_eq!(permission_name_for_tool("terminal"), "terminal");
        assert_eq!(permission_name_for_tool("run_command"), BASH);
    }

    #[test]
    fn each_tool_is_judged_under_its_category() {
        for (tool, category) in [
            ("read_file", READ),
            ("write_file", EDIT),
            ("edit_file", EDIT),
            ("create_folder", EDIT),
            ("glob_files", GLOB),
            ("list_files", GLOB),
            ("grep_files", GREP),
            ("web_fetch", WEB_FETCH),
            ("skill", SKILL),
            ("install_skill", SKILL),
            ("ask_user_question", "ask_user_question"),
        ] {
            assert_eq!(permission_name_for_tool(tool), category, "{tool}");
        }
    }

    #[test]
    fn a_path_is_matched_relative_to_the_workspace_and_absolute_outside_it() {
        use std::path::Path;
        let primary = Path::new("/w/project");
        assert_eq!(rule_path(primary, Path::new("/w/project")), b".");
        assert_eq!(
            rule_path(primary, Path::new("/w/project/src/a.rs")),
            b"src/a.rs"
        );
        assert_eq!(
            rule_path(primary, Path::new("/w/projectx/a")),
            b"/w/projectx/a"
        );
        assert_eq!(
            rule_path(primary, Path::new("/elsewhere/a")),
            b"/elsewhere/a"
        );
        assert_eq!(
            rule_path(Path::new("/"), Path::new("/etc/hosts")),
            b"etc/hosts"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_name_that_is_not_utf8_is_matched_by_its_bytes_and_not_by_a_lossy_spelling() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;
        use std::path::Path;
        let odd = Path::new(OsStr::from_bytes(b"/w/a\xff"));
        let candidate = rule_path(Path::new("/w"), odd);
        assert_eq!(candidate, b"a\xff");
        // A pattern spelled with the replacement character a lossy conversion
        // would produce does not reach the real name.
        let lossy = set(&[("edit", "a\u{fffd}", Allow)]);
        assert_eq!(lossy.decide(EDIT, None, &candidate), None);
        assert_eq!(
            set(&[("edit", "a?", Deny)]).decide(EDIT, None, &candidate),
            Some(Deny)
        );
    }

    #[test]
    fn the_global_pattern_is_stars_only() {
        assert!(is_global_target_pattern("*"));
        assert!(is_global_target_pattern("***"));
        assert!(!is_global_target_pattern(""));
        assert!(!is_global_target_pattern("*a"));
    }

    #[test]
    fn a_global_deny_covers_every_target_until_a_later_rule_still_lets_one_through() {
        // Upstream's own three cases (`permissions.zig:2500-2517`).
        assert!(
            !set(&[("edit", "*", Deny), ("edit", "src/*", Allow)]).denies_every_target(EDIT, None)
        );
        assert!(
            set(&[("edit", "src/*", Allow), ("edit", "*", Deny)]).denies_every_target(EDIT, None)
        );
        assert!(!set(&[("edit", "src/*", Deny)]).denies_every_target(EDIT, None));
        // An ask after the deny lets the asked targets through
        // (`tool_projection.zig:725-742`), and a later global allow or ask
        // clears it outright.
        assert!(
            !set(&[("edit", "*", Deny), ("edit", "src/*", Ask)]).denies_every_target(EDIT, None)
        );
        assert!(!set(&[("edit", "*", Deny), ("*", "*", Allow)]).denies_every_target(EDIT, None));
        // A later rule for another category is not a way through.
        assert!(set(&[("edit", "*", Deny), ("read", "*", Allow)]).denies_every_target(EDIT, None));
        // One action for the whole table is `* *`.
        assert!(RuleSet::parse(&json!("deny"))
            .unwrap()
            .denies_every_target(EDIT, None));
        assert!(!RuleSet::default().denies_every_target(EDIT, None));
    }

    #[test]
    fn a_tool_is_covered_under_its_category_or_its_own_name_and_never_web_fetch() {
        let edit = set(&[("edit", "*", Deny)]);
        assert!(edit.denies_every_target_of_tool("write_file"));
        assert!(edit.denies_every_target_of_tool("edit_file"));
        assert!(edit.denies_every_target_of_tool("create_folder"));
        assert!(!edit.denies_every_target_of_tool("read_file"));
        let named = set(&[("read_file", "*", Deny)]);
        assert!(named.denies_every_target_of_tool("read_file"));
        assert!(!named.denies_every_target_of_tool("grep_files"));
        // `rulesDenyAllTargetsForTool` returns before matching for web_fetch
        // (`permissions.zig:1139`).
        assert!(!set(&[("*", "*", Deny)]).denies_every_target_of_tool(WEB_FETCH));
        assert!(set(&[("*", "*", Deny)]).denies_every_target_of_tool("ask_user_question"));
        // `bash` is a command's category, not the shell tool's name: upstream's
        // `shell` is judged as `shell` here (`permissionNameForTool`, `:1455-1464`).
        assert!(!set(&[("bash", "*", Deny)]).denies_every_target_of_tool("shell"));
        assert!(set(&[("shell", "*", Deny)]).denies_every_target_of_tool("shell"));
    }

    #[test]
    fn generic_wildcard_matching_is_total_for_a_command_sized_input() {
        // `permissions.zig:2492-2498`.
        let candidate = vec![b'x'; 64 * 1024];
        assert!(wildcard_match(b"*", &candidate));
        assert!(!wildcard_match(b"?", &candidate));
        assert!(wildcard_match(b"*x*x*x*", &candidate));
    }

    #[test]
    fn a_web_fetch_rule_is_an_exact_canonical_domain_and_nothing_else() {
        // `permissions.zig:2562-2621`.
        let rules = set(&[
            ("web_fetch", "domain:example.com", Allow),
            ("web_fetch", "domain:*.example.com", Allow),
            ("*", "*", Ask),
        ]);
        assert_eq!(
            rules.decide(WEB_FETCH, None, "domain:example.com"),
            Some(Allow)
        );
        assert_eq!(rules.decide(WEB_FETCH, None, "domain:a.example.com"), None);
        assert_eq!(rules.decide(WEB_FETCH, None, "https://example.com/"), None);
        assert_eq!(web_fetch_rule_warning_count(rules.rules()), 1);
        assert_eq!(
            canonical_web_fetch_domain_pattern("Example.COM").as_deref(),
            Some("domain:example.com")
        );
        assert_eq!(
            canonical_web_fetch_domain_pattern("domain:example.com.").as_deref(),
            Some("domain:example.com")
        );
        assert_eq!(
            canonical_web_fetch_domain_pattern("[::1]").as_deref(),
            Some("domain:[::1]")
        );
        for refused in [
            "",
            "https://example.com",
            "example.com/path",
            "*.example.com",
            "exa?mple.com",
            "example..com",
            ".example.com",
            "example.com:443",
            "exa_mple.com",
            "[::1%en0]",
            "[not-v6]",
            "[::1",
        ] {
            assert_eq!(
                canonical_web_fetch_domain_pattern(refused),
                None,
                "{refused:?}"
            );
        }
        assert!(!is_canonical_web_fetch_domain_pattern("domain:Example.com"));
        assert!(!is_canonical_web_fetch_domain_pattern(
            "domain:example.com."
        ));
        assert!(!is_canonical_web_fetch_domain_pattern("example.com"));
    }

    #[test]
    fn a_table_is_read_in_document_order_with_its_keys_trimmed() {
        // `permission rules parse from workspace override`
        // (`config_runtime.zig:3062-3090`), and the string forms.
        let value: Value = serde_json::from_str(
            r#"{"edit":{" src/* ":"allow"},"bash":{"rm -rf*":"DENY"},"open_url":"ask"}"#,
        )
        .unwrap();
        let rules = RuleSet::parse(&value).unwrap();
        assert_eq!(
            rules.rules(),
            &[
                ConfiguredRule::new("edit", "src/*", Allow),
                ConfiguredRule::new("bash", "rm -rf*", Deny),
                ConfiguredRule::new("open_url", "*", Ask),
            ]
        );
        assert_eq!(
            RuleSet::parse(&json!("deny")).unwrap().rules(),
            &[ConfiguredRule::new("*", "*", Deny)]
        );
        // Document order, not sorted order: `z` before `a` stays so.
        let ordered: Value =
            serde_json::from_str(r#"{"z":{"b":"allow","a":"deny"},"a":"ask"}"#).unwrap();
        let rules = RuleSet::parse(&ordered).unwrap();
        let order: Vec<_> = rules
            .rules()
            .iter()
            .map(|rule| (rule.permission.as_str(), rule.pattern.as_str()))
            .collect();
        assert_eq!(order, [("z", "b"), ("z", "a"), ("a", "*")]);
        assert!(RuleSet::parse(&json!({})).unwrap().is_empty());
    }

    #[test]
    fn a_table_with_one_bad_entry_is_refused_whole() {
        assert_eq!(
            RuleSet::parse(&json!({"bash": {"ls": "allow", "rm": "maybe"}})),
            Err(TableError::BadAction("maybe".to_string()))
        );
        assert_eq!(
            RuleSet::parse(&json!({"bash": {"ls": true}})),
            Err(TableError::BadAction("true".to_string()))
        );
        assert_eq!(
            RuleSet::parse(&json!({" ": "allow"})),
            Err(TableError::BlankCategory)
        );
        assert_eq!(
            RuleSet::parse(&json!({"bash": ["ls"]})),
            Err(TableError::NotATable)
        );
        assert_eq!(RuleSet::parse(&json!(7)), Err(TableError::NotATable));
        assert_eq!(
            RuleSet::parse(&json!("yes")),
            Err(TableError::BadAction("yes".to_string()))
        );
    }
}
