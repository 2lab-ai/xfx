//! What `/allowlist` means and says, on either front end.
//!
//! Upstream's command (`handleAllowlist`, `vercel-labs/fx@c1db9191
//! src/core/session/session_commands.zig:396-444`): view the persistent allow
//! rules, or add, remove and reset them in one of two tables of the profile --
//! `user`, the top-level `permission`, or `local`, this workspace's entry in
//! the same file (`workspaces["<root>"].permission`), never a file in the
//! repository. Every change is written under the profile's lock and then read
//! back, and the table read back is put in force at once
//! (`replaceEffectivePermissionRules`, `:927-941`), so the next tool call --
//! of a turn already running, too -- is judged by it.
//!
//! The grammar, the words and the file's shape are upstream's. What each
//! front end does with an [`Answer`] -- a transcript notice, or a line -- is
//! its own.

use std::path::Path;

use serde_json::{Map, Value};

use super::rules::{
    canonical_web_fetch_domain_pattern, is_canonical_web_fetch_domain_pattern,
    permission_name_for_tool, web_fetch_rule_warning_count, ConfiguredRule, RuleAction, RuleSet,
    BASH, URL, WEB_FETCH, WEB_SEARCH,
};
use crate::config::{PermissionTables, PERMISSION_KEY};

/// The topic every answer is said under.
pub const TOPIC: &str = "allowlist";

/// The command's own usage line (`writeAllowlistUsage`, `:509-515`).
pub const USAGE: &str =
    "usage: /allowlist [view [effective|local|user]|[local|user] add|remove|reset ...]";

/// `add`'s usage line (`handleAllowlistAdd`, `:524-530`).
pub const ADD_USAGE: &str = "usage: /allowlist add [command|tool|url|web-fetch-domain] <pattern>";

/// `remove`'s usage line (`handleAllowlistRemove`, `:568-574`).
pub const REMOVE_USAGE: &str =
    "usage: /allowlist remove [command|tool|url|web-fetch-domain] <pattern>";

/// `reset`'s usage line (`handleAllowlistReset`, `:613-619`).
pub const RESET_USAGE: &str = "usage: /allowlist reset [commands|tools|urls|web-fetch-domains|all]";

/// The categories `/allowlist add tool` takes besides a tool's own name
/// (`isKnownAllowlistTool`, `:1380-1395`).
const TOOL_CATEGORIES: &[&str] = &["edit", "read", "glob", "grep", "skill", WEB_SEARCH];

/// Which table a change goes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// The profile's own `permission`.
    User,
    /// This workspace's entry in the profile.
    Local,
}

impl Scope {
    /// `@tagName`, which the messages quote.
    pub fn label(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Local => "local",
        }
    }
}

/// Which table a listing shows (`AllowlistView`, `:1437-1449`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    /// The one in force.
    Effective,
    Local,
    User,
}

impl View {
    fn label(self) -> &'static str {
        match self {
            Self::Effective => "effective",
            Self::Local => "local",
            Self::User => "user",
        }
    }
}

/// What `add` and `remove` name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Command,
    Tool,
    Url,
    WebFetchDomain,
}

/// One rule as `add` and `remove` name it: its kind, the category it is kept
/// under and its pattern (`AllowlistTarget`, `:1320-1331`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub kind: Kind,
    pub category: String,
    pub pattern: String,
}

/// Which allow rules `reset` takes away (`AllowlistResetScope`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetScope {
    All,
    Commands,
    Tools,
    Urls,
    WebFetchDomains,
}

impl ResetScope {
    /// `allowlistResetScopeLabel` (`:1560-1568`).
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Commands => "commands",
            Self::Tools => "tools",
            Self::Urls => "urls",
            Self::WebFetchDomains => "web-fetch-domains",
        }
    }

    /// `parseAllowlistResetScope` (`:1427-1435`): either number, any case.
    fn parse(raw: &str) -> Option<Self> {
        let word = raw.trim_matches([' ', '\t']);
        let is = |a: &str, b: &str| word.eq_ignore_ascii_case(a) || word.eq_ignore_ascii_case(b);
        if word.eq_ignore_ascii_case("all") {
            Some(Self::All)
        } else if is("command", "commands") {
            Some(Self::Commands)
        } else if is("tool", "tools") {
            Some(Self::Tools)
        } else if is("url", "urls") {
            Some(Self::Urls)
        } else if is("web-fetch-domain", "web-fetch-domains") {
            Some(Self::WebFetchDomains)
        } else {
            None
        }
    }

    /// `scopeMatchesCategory` (`settings_store.zig:1774-1790`).
    fn covers(self, category: &str) -> bool {
        let canonical = category.trim_matches([' ', '\t', '\r', '\n']);
        let is_url = matches!(canonical, "url" | "open_url" | "browser_navigate");
        let is_fetch = canonical == WEB_FETCH;
        match self {
            Self::All => true,
            Self::Commands => canonical == BASH,
            Self::Urls => is_url,
            Self::WebFetchDomains => is_fetch,
            Self::Tools => canonical != BASH && !is_url && !is_fetch && canonical != "*",
        }
    }
}

/// What a `/allowlist` line asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    View(View),
    Add(Scope, Target),
    Remove(Scope, Target),
    Reset(Scope, ResetScope),
    /// Refused, with the usage line that says why.
    Usage(&'static str),
}

impl Request {
    /// Reads the rest of a `/allowlist` line (`handleAllowlist`, `:396-444`).
    ///
    /// Every word is case-insensitive; the scope is `local` unless `user` or
    /// `local` comes first. `known_tool` says whether a name is a tool this
    /// build registers, which `add tool` and `remove tool` accept besides
    /// upstream's categories.
    pub fn parse(argument: &str, known_tool: impl Fn(&str) -> bool) -> Self {
        let Some((first, rest)) = split_first_word(argument) else {
            return Self::View(View::Effective);
        };
        if first.eq_ignore_ascii_case("view") {
            return match parse_view(rest) {
                Some(view) => Self::View(view),
                None => Self::Usage(USAGE),
            };
        }
        let (scope, action, rest) =
            if first.eq_ignore_ascii_case("user") || first.eq_ignore_ascii_case("local") {
                let scope = if first.eq_ignore_ascii_case("user") {
                    Scope::User
                } else {
                    Scope::Local
                };
                let Some((action, rest)) = split_first_word(rest) else {
                    return Self::Usage(USAGE);
                };
                (scope, action, rest)
            } else {
                (Scope::Local, first, rest)
            };
        if action.eq_ignore_ascii_case("add") {
            return match parse_target(rest, &known_tool) {
                Some(target) => Self::Add(scope, target),
                None => Self::Usage(ADD_USAGE),
            };
        }
        if action.eq_ignore_ascii_case("remove") {
            return match parse_target(rest, &known_tool) {
                Some(target) => Self::Remove(scope, target),
                None => Self::Usage(REMOVE_USAGE),
            };
        }
        if action.eq_ignore_ascii_case("reset") {
            return match ResetScope::parse(rest) {
                Some(reset) => Self::Reset(scope, reset),
                None => Self::Usage(RESET_USAGE),
            };
        }
        Self::Usage(USAGE)
    }
}

/// `splitFirstWord` (`:1397-1410`): the first word, and the rest with its
/// leading blanks gone.
fn split_first_word(text: &str) -> Option<(&str, &str)> {
    let trimmed = text.trim_matches([' ', '\t']);
    if trimmed.is_empty() {
        return None;
    }
    match trimmed.find([' ', '\t']) {
        Some(at) => Some((
            &trimmed[..at],
            trimmed[at + 1..].trim_start_matches([' ', '\t']),
        )),
        None => Some((trimmed, "")),
    }
}

/// `parseQuotedOrRest` (`:1416-1425`): a leading `"` runs to the next one, or
/// to the end when there is none; otherwise the whole rest.
fn quoted_or_rest(raw: &str) -> &str {
    let trimmed = raw.trim_matches([' ', '\t']);
    match trimmed.strip_prefix('"') {
        Some(inner) => match inner.find('"') {
            Some(close) => &inner[..close],
            None => inner,
        },
        None => trimmed,
    }
}

fn parse_view(raw: &str) -> Option<View> {
    let word = raw.trim_matches([' ', '\t']);
    if word.is_empty() || word.eq_ignore_ascii_case("effective") {
        Some(View::Effective)
    } else if word.eq_ignore_ascii_case("local") {
        Some(View::Local)
    } else if word.eq_ignore_ascii_case("user") {
        Some(View::User)
    } else {
        None
    }
}

/// `parseAllowlistTargetAlloc` and `parseAllowlistTarget` (`:1333-1377`).
fn parse_target(raw: &str, known_tool: &impl Fn(&str) -> bool) -> Option<Target> {
    let (kind, rest) = split_first_word(raw)?;
    let pattern = quoted_or_rest(rest);
    if pattern.is_empty() {
        return None;
    }
    if kind.eq_ignore_ascii_case("web-fetch-domain") {
        return Some(Target {
            kind: Kind::WebFetchDomain,
            category: WEB_FETCH.to_string(),
            pattern: canonical_web_fetch_domain_pattern(pattern)?,
        });
    }
    if kind.eq_ignore_ascii_case("command") {
        return Some(Target {
            kind: Kind::Command,
            category: BASH.to_string(),
            pattern: pattern.to_string(),
        });
    }
    if kind.eq_ignore_ascii_case("url") {
        return Some(Target {
            kind: Kind::Url,
            category: URL.to_string(),
            pattern: pattern.to_string(),
        });
    }
    if kind.eq_ignore_ascii_case("tool") {
        // `web_fetch` is a domain list, not a tool switch: a `*` there would
        // be an allow for every host, which upstream's form refuses.
        if pattern == WEB_FETCH {
            return None;
        }
        if !known_tool(pattern) && !TOOL_CATEGORIES.contains(&pattern) {
            return None;
        }
        return Some(Target {
            kind: Kind::Tool,
            category: permission_name_for_tool(pattern).to_string(),
            pattern: "*".to_string(),
        });
    }
    None
}

// ---------------------------------------------------------------------------
// the change to the file
// ---------------------------------------------------------------------------

/// One change to a table (`PermissionPatch`, `settings_store.zig:53-64`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Patch {
    Add {
        category: String,
        pattern: String,
        action: RuleAction,
    },
    Remove {
        category: String,
        pattern: String,
    },
    Reset(ResetScope),
}

/// What a patch did to the document.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Applied {
    /// The document is different, and is to be written.
    pub changed: bool,
    /// How many rules it took away.
    pub removed: usize,
}

/// Applies `patch` to the table `workspace` names -- `None` for the user's,
/// `Some(key)` for that workspace's entry -- in a settings document
/// (`applyPermissionMutationToRoot` and `applyPermissionPatch`,
/// `settings_store.zig:1507-1735`).
///
/// Order is kept everywhere: a value replaced in place keeps its position, a
/// new key goes last, and a removed key closes the gap rather than pulling
/// the last key into it, because the table's order is its verdict
/// ([`RuleSet::decide`]).
pub fn apply(
    document: &mut Map<String, Value>,
    workspace: Option<&str>,
    patch: &Patch,
) -> Result<Applied, String> {
    let target = match workspace {
        None => document,
        Some(key) => workspace_object(document, key)?,
    };
    match patch {
        Patch::Add {
            category,
            pattern,
            action,
        } => {
            let permission = permission_object(target, true)?.expect("created when missing");
            // A category written as one action becomes that action for `*`,
            // so the new pattern can sit beside it.
            match permission.get_mut(category.as_str()) {
                Some(Value::Object(_)) => {}
                Some(other) => {
                    let mut replacement = Map::new();
                    replacement.insert("*".to_string(), other.take());
                    *other = Value::Object(replacement);
                }
                None => {
                    permission.insert(category.clone(), Value::Object(Map::new()));
                }
            }
            let Some(Value::Object(patterns)) = permission.get_mut(category.as_str()) else {
                unreachable!("the category was made an object above");
            };
            if patterns.get(pattern.as_str()).and_then(Value::as_str) == Some(action.label()) {
                return Ok(Applied::default());
            }
            patterns.insert(pattern.clone(), Value::String(action.label().to_string()));
            Ok(Applied {
                changed: true,
                removed: 0,
            })
        }
        Patch::Remove { category, pattern } => {
            let Some(permission) = permission_object(target, false)? else {
                return Ok(Applied::default());
            };
            let Some(category_key) = trimmed_key(permission, category) else {
                return Ok(Applied::default());
            };
            let Some(Value::Object(patterns)) = permission.get_mut(&category_key) else {
                return Ok(Applied::default());
            };
            let Some(pattern_key) = trimmed_key(patterns, pattern) else {
                return Ok(Applied::default());
            };
            patterns.shift_remove(&pattern_key);
            if patterns.is_empty() {
                permission.shift_remove(&category_key);
            }
            Ok(Applied {
                changed: true,
                removed: 1,
            })
        }
        Patch::Reset(scope) => {
            let Some(permission) = permission_object(target, false)? else {
                return Ok(Applied::default());
            };
            let mut removed = 0;
            while remove_one_allow_rule(permission, *scope) {
                removed += 1;
            }
            if permission.is_empty() {
                target.shift_remove(PERMISSION_KEY);
            }
            Ok(Applied {
                changed: removed > 0,
                removed,
            })
        }
    }
}

/// `workspaceObject` (`settings_store.zig:1572-1593`).
fn workspace_object<'a>(
    document: &'a mut Map<String, Value>,
    key: &str,
) -> Result<&'a mut Map<String, Value>, String> {
    let workspaces = document
        .entry("workspaces")
        .or_insert_with(|| Value::Object(Map::new()));
    let Value::Object(workspaces) = workspaces else {
        return Err("`workspaces` in the settings file is not an object".to_string());
    };
    let entry = workspaces
        .entry(key)
        .or_insert_with(|| Value::Object(Map::new()));
    match entry {
        Value::Object(entry) => Ok(entry),
        _ => Err(format!(
            "the settings entry for workspace `{key}` is not an object"
        )),
    }
}

/// `permissionObject` (`settings_store.zig:1737-1749`).
fn permission_object(
    target: &mut Map<String, Value>,
    create: bool,
) -> Result<Option<&mut Map<String, Value>>, String> {
    if !target.contains_key(PERMISSION_KEY) {
        if !create {
            return Ok(None);
        }
        target.insert(PERMISSION_KEY.to_string(), Value::Object(Map::new()));
    }
    match target.get_mut(PERMISSION_KEY) {
        Some(Value::Object(permission)) => Ok(Some(permission)),
        _ => Err("`permission` in the settings file is not an object".to_string()),
    }
}

/// `canonicalPermissionKey` (`settings_store.zig:1792-1805`): the first key
/// that is `expected` once trimmed.
fn trimmed_key(object: &Map<String, Value>, expected: &str) -> Option<String> {
    object
        .keys()
        .find(|key| key.trim_matches([' ', '\t', '\r', '\n']) == expected)
        .cloned()
}

/// `removeOneAllowlistRule` (`settings_store.zig:1751-1772`): the first allow
/// rule `scope` covers, whether a category's single action or one pattern's.
fn remove_one_allow_rule(permission: &mut Map<String, Value>, scope: ResetScope) -> bool {
    let keys: Vec<String> = permission.keys().cloned().collect();
    for key in keys {
        if !scope.covers(&key) {
            continue;
        }
        match permission.get_mut(&key) {
            Some(Value::String(action)) if action.eq_ignore_ascii_case("allow") => {
                permission.shift_remove(&key);
                return true;
            }
            Some(Value::Object(patterns)) => {
                let found = patterns.iter().find_map(|(pattern, action)| {
                    action
                        .as_str()
                        .filter(|action| action.eq_ignore_ascii_case("allow"))
                        .map(|_| pattern.clone())
                });
                if let Some(pattern) = found {
                    patterns.shift_remove(&pattern);
                    if patterns.is_empty() {
                        permission.shift_remove(&key);
                    }
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

// ---------------------------------------------------------------------------
// what is said
// ---------------------------------------------------------------------------

/// How an answer is said.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Neutral,
    Warning,
    Error,
}

/// One answer: what a front end says, under which topic, in which tone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    pub tone: Tone,
    /// `allowlist`, or empty for a usage line.
    pub topic: &'static str,
    pub body: String,
}

impl Answer {
    fn new(tone: Tone, topic: &'static str, body: impl Into<String>) -> Self {
        Self {
            tone,
            topic,
            body: body.into(),
        }
    }
}

/// Everything one `/allowlist` line does: what to say, and the table to put
/// in force when the file was read back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub answers: Vec<Answer>,
    /// The table now in force, when it changed or was read afresh; `None`
    /// leaves the one in force alone.
    pub in_force: Option<RuleSet>,
}

impl Outcome {
    fn say(answer: Answer) -> Self {
        Self {
            answers: vec![answer],
            in_force: None,
        }
    }
}

/// Runs one `/allowlist` line against the settings file at `settings` for the
/// workspace at `workspace_root` (canonical, as the loader keys its entry).
///
/// `known_tool` is [`Request::parse`]'s. Writes go through
/// [`crate::provider::profile::write_permission`]: under the settings lock,
/// order kept, nothing written when nothing changed.
pub fn run(
    argument: &str,
    settings: Option<&Path>,
    workspace_root: &Path,
    known_tool: impl Fn(&str) -> bool,
) -> Outcome {
    let request = Request::parse(argument, known_tool);
    let (scope, target, patch, verb) = match request {
        Request::Usage(usage) => return Outcome::say(Answer::new(Tone::Error, "", usage)),
        Request::View(view) => return view_outcome(view, settings, workspace_root),
        Request::Reset(scope, reset) => {
            return reset_outcome(scope, reset, settings, workspace_root)
        }
        Request::Add(scope, target) => {
            let patch = Patch::Add {
                category: target.category.clone(),
                pattern: target.pattern.clone(),
                action: RuleAction::Allow,
            };
            (scope, target, patch, "added")
        }
        Request::Remove(scope, target) => {
            let patch = Patch::Remove {
                category: target.category.clone(),
                pattern: target.pattern.clone(),
            };
            (scope, target, patch, "removed")
        }
    };
    let applied = match write(settings, scope, workspace_root, &patch) {
        Ok(applied) => applied,
        Err(reason) => {
            let (doing, place) = match patch {
                Patch::Remove { .. } => ("remove rule from", "settings"),
                _ => ("add rule to", "settings"),
            };
            return Outcome::say(Answer::new(
                Tone::Error,
                TOPIC,
                format!(
                    "failed to {doing} {place} (scope={}, error={reason})",
                    scope.label()
                ),
            ));
        }
    };
    // `remove` of nothing says so and reads nothing back; `add` of a rule
    // already there is "added" again, as upstream's is (trap (c)).
    if verb == "removed" && !applied.changed {
        return Outcome::say(Answer::new(
            Tone::Neutral,
            TOPIC,
            change(verb_unchanged(), &target, scope, ""),
        ));
    }
    match read_back(settings, workspace_root) {
        Ok(tables) => {
            let detail = match shadow_label(&tables, scope) {
                Some(label) => format!("user rules shadowed by {label}"),
                None => String::new(),
            };
            Outcome {
                answers: vec![Answer::new(
                    Tone::Neutral,
                    TOPIC,
                    change(verb, &target, scope, &detail),
                )],
                in_force: Some(tables.effective().clone()),
            }
        }
        Err(reason) => Outcome::say(Answer::new(
            Tone::Warning,
            TOPIC,
            change(
                verb,
                &target,
                scope,
                &format!("saved but effective source unknown and runtime reload failed ({reason})"),
            ),
        )),
    }
}

fn verb_unchanged() -> &'static str {
    "no matching rule for"
}

/// `formatAllowlistChange` (`:1594-1613`).
fn change(verb: &str, target: &Target, scope: Scope, detail: &str) -> String {
    let named = match target.kind {
        Kind::Command => format!("command: \"{}\"", target.pattern),
        Kind::Url => format!("url: \"{}\"", target.pattern),
        Kind::Tool => format!("tool {}: \"{}\"", target.category, target.pattern),
        Kind::WebFetchDomain => format!("web-fetch-domain: \"{}\"", target.pattern),
    };
    let mut message = format!("{verb} {named} (scope={})", scope.label());
    if !detail.is_empty() {
        message.push_str("; ");
        message.push_str(detail);
    }
    message
}

/// `permissionShadowLabel` (`:1451-1458`): said only about a change to the
/// user table, and only when it has rules a workspace entry puts out of force.
fn shadow_label(tables: &PermissionTables, scope: Scope) -> Option<&'static str> {
    (scope == Scope::User && !tables.user.is_empty() && tables.user_shadowed_by_local)
        .then_some("local settings")
}

fn write(
    settings: Option<&Path>,
    scope: Scope,
    workspace_root: &Path,
    patch: &Patch,
) -> Result<Applied, String> {
    let Some(path) = settings else {
        return Err("no home directory is set".to_string());
    };
    let workspace = match scope {
        Scope::User => None,
        Scope::Local => Some(workspace_key(workspace_root)?),
    };
    crate::provider::profile::write_permission(path, workspace, patch)
        .map_err(|err| err.to_string())
}

/// The key this workspace's entry is kept under: its canonical path, the
/// string the loader looks it up by.
fn workspace_key(workspace_root: &Path) -> Result<&str, String> {
    workspace_root.to_str().ok_or_else(|| {
        "the workspace path is not valid UTF-8 and cannot key a settings entry".to_string()
    })
}

fn read_back(settings: Option<&Path>, workspace_root: &Path) -> Result<PermissionTables, String> {
    match settings {
        Some(path) => crate::config::read_permission_tables(path, workspace_root),
        None => Ok(PermissionTables::default()),
    }
}

fn reset_outcome(
    scope: Scope,
    reset: ResetScope,
    settings: Option<&Path>,
    workspace_root: &Path,
) -> Outcome {
    let removed = match write(settings, scope, workspace_root, &Patch::Reset(reset)) {
        Ok(applied) => applied.removed,
        Err(reason) => {
            return Outcome::say(Answer::new(
                Tone::Error,
                TOPIC,
                format!(
                    "failed to reset rules in settings (scope={}, error={reason})",
                    scope.label()
                ),
            ))
        }
    };
    let counted = format!(
        "reset {}: removed {removed} rule{} (scope={})",
        reset.label(),
        if removed == 1 { "" } else { "s" },
        scope.label()
    );
    match read_back(settings, workspace_root) {
        Ok(tables) => {
            let mut body = counted;
            if let Some(label) = shadow_label(&tables, scope) {
                body.push_str("; user rules shadowed by ");
                body.push_str(label);
            }
            Outcome {
                answers: vec![Answer::new(Tone::Neutral, TOPIC, body)],
                in_force: Some(tables.effective().clone()),
            }
        }
        Err(reason) => Outcome::say(Answer::new(
            Tone::Warning,
            TOPIC,
            format!("{counted}; saved but effective source unknown and runtime reload failed ({reason})"),
        )),
    }
}

/// `writeAllowlistStatus` (`:987-1045`): the file is read afresh, what it
/// says is put in force, and the table asked about is listed.
///
/// **A file that does not read changes nothing in force.** Upstream's view
/// replaces its engine's rules with whatever its reload found, a broken
/// layer's nothing included (`replaceEffectivePermissionRules`, `:1009`), so a
/// deny in force lapses because somebody looked; it returns early, in its
/// settings error (`writeSettingsLoadError`, `:1137-1141`), only for an
/// unsafe or unsupported profile path (`:993-1006`). Here every reload that
/// fails takes that early return: `✗ settings: Failed to load settings:
/// <reason>`, and the table in force -- the launch's refusal of every tool
/// included ([`crate::config::PermissionTables::unreadable`]) -- stays.
fn view_outcome(view: View, settings: Option<&Path>, workspace_root: &Path) -> Outcome {
    let tables = match read_back(settings, workspace_root) {
        Ok(tables) => tables,
        Err(reason) => {
            return Outcome::say(Answer::new(
                Tone::Error,
                "settings",
                crate::output::safe_one_line(
                    &format!(
                        "Failed to load settings: {reason}; the permission rules in force are unchanged"
                    ),
                    usize::MAX,
                ),
            ))
        }
    };
    let rules = match view {
        View::Effective => tables.effective(),
        View::Local => &tables.local,
        View::User => &tables.user,
    };
    let mut body = String::new();
    if rules.rules().iter().any(displayable) {
        body.push_str(&format!("{} persistent allow rules:\n", view.label()));
        write_groups(&mut body, rules.rules());
    } else {
        body.push_str(&format!(
            "{} persistent allow rules: (none)\n",
            view.label()
        ));
    }
    if matches!(view, View::Effective | View::User) {
        if let Some(label) = shadow_label(&tables, Scope::User) {
            body.push_str(&format!("user rules are shadowed by {label}\n"));
        }
    }
    let malformed = web_fetch_rule_warning_count(rules.rules());
    if malformed > 0 {
        body.push_str(&format!(
            "ignored {malformed} malformed web_fetch rule{}; expected domain:<canonical-hostname>\n",
            if malformed == 1 { "" } else { "s" }
        ));
    }
    let tone = if malformed == 0 {
        Tone::Neutral
    } else {
        Tone::Warning
    };
    Outcome {
        answers: vec![Answer::new(tone, TOPIC, body.trim_end_matches('\n'))],
        in_force: Some(tables.effective().clone()),
    }
}

/// Where a rule is listed (`AllowlistDisplayKind`, `:1570-1575`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Group {
    Tool,
    Command,
    Url,
    WebFetchDomain,
}

/// `allowlistRuleGroup` (`:1572-1584`).
fn group_of(rule: &ConfiguredRule) -> (Group, &str) {
    match rule.permission.as_str() {
        BASH => (Group::Command, "command"),
        URL => (Group::Url, "url"),
        WEB_FETCH => (Group::WebFetchDomain, "web-fetch-domain"),
        other => (Group::Tool, other),
    }
}

/// `allowlistRuleDisplayable` (`:1586-1592`): allow rules only, and a
/// `web_fetch` rule only when it is a canonical domain.
fn displayable(rule: &ConfiguredRule) -> bool {
    rule.action == RuleAction::Allow
        && (rule.permission != WEB_FETCH || is_canonical_web_fetch_domain_pattern(&rule.pattern))
}

/// `writeAllowlistRuleGroups` and the group writer (`:1489-1545`): tools,
/// commands, urls, web-fetch domains, each under its header; one row per
/// distinct group, its patterns joined by `, `, in the table's order; a `*` on
/// a path tool says `workspace`.
fn write_groups(out: &mut String, rules: &[ConfiguredRule]) {
    for (group, header) in [
        (Group::Tool, "  tools:\n"),
        (Group::Command, "  commands:\n"),
        (Group::Url, "  urls:\n"),
        (Group::WebFetchDomain, "  web-fetch domains:\n"),
    ] {
        let members: Vec<&ConfiguredRule> = rules
            .iter()
            .filter(|rule| displayable(rule) && group_of(rule).0 == group)
            .collect();
        if members.is_empty() {
            continue;
        }
        out.push_str(header);
        let mut seen: Vec<&str> = Vec::new();
        for rule in &members {
            let name = group_of(rule).1;
            if seen.contains(&name) {
                continue;
            }
            seen.push(name);
            out.push_str("    ");
            if group == Group::Tool {
                out.push_str(name);
                out.push_str(": ");
            }
            let patterns: Vec<&str> = members
                .iter()
                .filter(|other| group_of(other).1 == name)
                .map(|other| {
                    if group == Group::Tool
                        && other.pattern == "*"
                        && matches!(name, "edit" | "read" | "glob" | "grep")
                    {
                        "workspace"
                    } else {
                        other.pattern.as_str()
                    }
                })
                .collect();
            out.push_str(&patterns.join(", "));
            out.push('\n');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn known(name: &str) -> bool {
        crate::tools::ADVERTISED_TOOLS.contains(&name)
    }

    fn target(kind: Kind, category: &str, pattern: &str) -> Target {
        Target {
            kind,
            category: category.to_string(),
            pattern: pattern.to_string(),
        }
    }

    #[test]
    fn every_form_is_read_as_upstream_reads_it() {
        let parse = |line: &str| Request::parse(line, known);
        assert_eq!(parse(""), Request::View(View::Effective));
        assert_eq!(parse("view"), Request::View(View::Effective));
        assert_eq!(parse("VIEW Effective"), Request::View(View::Effective));
        assert_eq!(parse("view local"), Request::View(View::Local));
        assert_eq!(parse("view user"), Request::View(View::User));
        assert_eq!(parse("view mine"), Request::Usage(USAGE));
        assert_eq!(
            parse("add command \"git *\""),
            Request::Add(Scope::Local, target(Kind::Command, "bash", "git *"))
        );
        // Unquoted, the rest of the line is the pattern; an unclosed quote
        // runs to the end.
        assert_eq!(
            parse("add command git push"),
            Request::Add(Scope::Local, target(Kind::Command, "bash", "git push"))
        );
        assert_eq!(
            parse("add command \"git status"),
            Request::Add(Scope::Local, target(Kind::Command, "bash", "git status"))
        );
        assert_eq!(
            parse("User Add url https://example.com/*"),
            Request::Add(
                Scope::User,
                target(Kind::Url, "url", "https://example.com/*")
            )
        );
        assert_eq!(
            parse("local add tool read"),
            Request::Add(Scope::Local, target(Kind::Tool, "read", "*"))
        );
        // A tool is kept under its category (trap (e)).
        assert_eq!(
            parse("add tool write_file"),
            Request::Add(Scope::Local, target(Kind::Tool, "edit", "*"))
        );
        assert_eq!(
            parse("add tool create_folder"),
            Request::Add(Scope::Local, target(Kind::Tool, "edit", "*"))
        );
        assert_eq!(
            parse("add tool terminal"),
            Request::Add(Scope::Local, target(Kind::Tool, "terminal", "*"))
        );
        assert_eq!(
            parse("add web-fetch-domain Example.COM"),
            Request::Add(
                Scope::Local,
                target(Kind::WebFetchDomain, "web_fetch", "domain:example.com")
            )
        );
        for refused in [
            "add tool nonsense",
            "add tool web_fetch",
            "add",
            "add command",
            "add command \"\"",
            "add thing x",
            "add web-fetch-domain *.example.com",
        ] {
            assert_eq!(parse(refused), Request::Usage(ADD_USAGE), "{refused}");
        }
        assert_eq!(
            parse("remove command \"nope\""),
            Request::Remove(Scope::Local, target(Kind::Command, "bash", "nope"))
        );
        assert_eq!(parse("remove"), Request::Usage(REMOVE_USAGE));
        assert_eq!(
            parse("local reset tools"),
            Request::Reset(Scope::Local, ResetScope::Tools)
        );
        assert_eq!(
            parse("reset Command"),
            Request::Reset(Scope::Local, ResetScope::Commands)
        );
        assert_eq!(
            parse("user reset all"),
            Request::Reset(Scope::User, ResetScope::All)
        );
        assert_eq!(
            parse("reset web-fetch-domains"),
            Request::Reset(Scope::Local, ResetScope::WebFetchDomains)
        );
        assert_eq!(parse("local reset"), Request::Usage(RESET_USAGE));
        assert_eq!(parse("reset everything"), Request::Usage(RESET_USAGE));
        assert_eq!(parse("bogus"), Request::Usage(USAGE));
        assert_eq!(parse("user"), Request::Usage(USAGE));
        assert_eq!(parse("user frob"), Request::Usage(USAGE));
    }

    fn add(category: &str, pattern: &str) -> Patch {
        Patch::Add {
            category: category.to_string(),
            pattern: pattern.to_string(),
            action: RuleAction::Allow,
        }
    }

    fn object(value: Value) -> Map<String, Value> {
        match value {
            Value::Object(object) => object,
            _ => panic!("an object"),
        }
    }

    #[test]
    fn add_writes_upstreams_shape_and_a_repeat_changes_nothing() {
        let mut document = Map::new();
        let applied = apply(&mut document, Some("/w"), &add("bash", "git *")).unwrap();
        assert_eq!(
            applied,
            Applied {
                changed: true,
                removed: 0
            }
        );
        assert_eq!(
            Value::Object(document.clone()),
            json!({"workspaces": {"/w": {"permission": {"bash": {"git *": "allow"}}}}})
        );
        assert_eq!(
            apply(&mut document, Some("/w"), &add("bash", "git *")).unwrap(),
            Applied::default()
        );
        // A category written as one action becomes that action for `*`.
        let mut document = object(json!({"permission": {"bash": "deny"}}));
        apply(&mut document, None, &add("bash", "ls")).unwrap();
        assert_eq!(
            Value::Object(document),
            json!({"permission": {"bash": {"*": "deny", "ls": "allow"}}})
        );
        // A deny already on the pattern becomes an allow, in place.
        let mut document =
            object(json!({"permission": {"bash": {"a": "allow", "ls": "deny", "z": "allow"}}}));
        apply(&mut document, None, &add("bash", "ls")).unwrap();
        let keys: Vec<_> = document["permission"]["bash"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| format!("{k}={}", v.as_str().unwrap()))
            .collect();
        assert_eq!(keys, ["a=allow", "ls=allow", "z=allow"]);
    }

    #[test]
    fn a_table_that_is_not_an_object_is_refused_rather_than_overwritten() {
        let mut document = object(json!({"permission": "deny"}));
        assert!(apply(&mut document, None, &add("bash", "ls")).is_err());
        let mut document = object(json!({"workspaces": 3}));
        assert!(apply(&mut document, Some("/w"), &add("bash", "ls")).is_err());
        let mut document = object(json!({"workspaces": {"/w": []}}));
        assert!(apply(&mut document, Some("/w"), &add("bash", "ls")).is_err());
    }

    #[test]
    fn remove_finds_a_trimmed_key_of_any_action_and_leaves_an_empty_table_behind() {
        let mut document = object(json!({"permission": {" bash ": {"x": "deny", "y": "allow"}}}));
        let remove = |pattern: &str| Patch::Remove {
            category: "bash".to_string(),
            pattern: pattern.to_string(),
        };
        // Upstream's remove does not look at the action.
        assert_eq!(
            apply(&mut document, None, &remove("x")).unwrap(),
            Applied {
                changed: true,
                removed: 1
            }
        );
        assert_eq!(
            apply(&mut document, None, &remove("nope")).unwrap(),
            Applied::default()
        );
        apply(&mut document, None, &remove("y")).unwrap();
        // The last rule gone leaves `{}`, which still shadows.
        assert_eq!(Value::Object(document), json!({"permission": {}}));
        // A category that is one action has no pattern to remove.
        let mut single = object(json!({"permission": {"bash": "allow"}}));
        assert_eq!(
            apply(&mut single, None, &remove("*")).unwrap(),
            Applied::default()
        );
        // Nothing there is nothing changed.
        assert_eq!(
            apply(&mut Map::new(), None, &remove("x")).unwrap(),
            Applied::default()
        );
    }

    #[test]
    fn reset_takes_allow_rules_by_scope_and_keeps_every_other_rule() {
        // `removeAllowlistRules removes allow entries by scope and preserves
        // policy rules` (`config_runtime.zig:3343-3420`).
        let seeded = json!({"permission": {
            "bash": {"git *": "allow", "rm*": "deny"},
            "read": "allow",
            "edit": {"*": "allow", "secret": "ask"},
            "url": {"https://a/*": "allow"},
            "web_fetch": {"domain:a.com": "allow"},
            "*": "allow"
        }});
        let mut commands = object(seeded.clone());
        assert_eq!(
            apply(&mut commands, None, &Patch::Reset(ResetScope::Commands)).unwrap(),
            Applied {
                changed: true,
                removed: 1
            }
        );
        assert_eq!(commands["permission"]["bash"], json!({"rm*": "deny"}));
        let mut tools = object(seeded.clone());
        assert_eq!(
            apply(&mut tools, None, &Patch::Reset(ResetScope::Tools))
                .unwrap()
                .removed,
            2
        );
        assert_eq!(tools["permission"]["edit"], json!({"secret": "ask"}));
        assert!(tools["permission"].get("read").is_none());
        assert_eq!(
            tools["permission"]["*"],
            json!("allow"),
            "tools never take `*`"
        );
        let mut all = object(seeded);
        assert_eq!(
            apply(&mut all, None, &Patch::Reset(ResetScope::All))
                .unwrap()
                .removed,
            6
        );
        assert_eq!(
            Value::Object(all),
            json!({"permission": {"bash": {"rm*": "deny"}, "edit": {"secret": "ask"}}})
        );
        // A table emptied by reset is taken away whole.
        let mut emptied =
            object(json!({"workspaces": {"/w": {"permission": {"bash": {"a": "allow"}}}}}));
        apply(&mut emptied, Some("/w"), &Patch::Reset(ResetScope::All)).unwrap();
        assert_eq!(Value::Object(emptied), json!({"workspaces": {"/w": {}}}));
    }

    #[test]
    fn a_change_is_said_in_upstreams_words() {
        assert_eq!(
            change(
                "added",
                &target(Kind::Command, "bash", "git *"),
                Scope::Local,
                ""
            ),
            "added command: \"git *\" (scope=local)"
        );
        assert_eq!(
            change("added", &target(Kind::Tool, "edit", "*"), Scope::Local, ""),
            "added tool edit: \"*\" (scope=local)"
        );
        assert_eq!(
            change(
                "added",
                &target(Kind::Url, "url", "https://example.com/*"),
                Scope::User,
                "user rules shadowed by local settings"
            ),
            "added url: \"https://example.com/*\" (scope=user); user rules shadowed by local settings"
        );
        assert_eq!(
            change(
                "no matching rule for",
                &target(Kind::Command, "bash", "nope"),
                Scope::Local,
                ""
            ),
            "no matching rule for command: \"nope\" (scope=local)"
        );
        assert_eq!(
            change(
                "added",
                &target(Kind::WebFetchDomain, "web_fetch", "domain:example.com"),
                Scope::Local,
                ""
            ),
            "added web-fetch-domain: \"domain:example.com\" (scope=local)"
        );
    }

    #[test]
    fn a_listing_groups_the_allow_rules_in_upstreams_order() {
        let rules = RuleSet::parse(&json!({
            "bash": {"git *": "allow", "npm test": "allow", "rm*": "deny"},
            "read": {"*": "allow"},
            "edit": "allow",
            "web_fetch": {"domain:example.com": "allow", "Example.org": "allow"},
            "url": {"https://example.com/*": "allow"},
            "terminal": "allow",
            "open_url": "ask"
        }))
        .unwrap();
        let mut body = String::new();
        write_groups(&mut body, rules.rules());
        assert_eq!(
            body,
            "  tools:\n    read: workspace\n    edit: workspace\n    terminal: *\n  commands:\n    git *, npm test\n  urls:\n    https://example.com/*\n  web-fetch domains:\n    domain:example.com\n"
        );
        assert_eq!(web_fetch_rule_warning_count(rules.rules()), 1);
    }

    /// A profile file under a fresh home, the canonical workspace its entry is
    /// keyed by, and a runner over both.
    struct Profile {
        _dir: tempfile::TempDir,
        settings: std::path::PathBuf,
        workspace: std::path::PathBuf,
    }

    impl Profile {
        fn new() -> Self {
            let dir = tempfile::tempdir().expect("a home");
            let settings = dir.path().join(".xfx").join("settings.json");
            let workspace = dir.path().join("work");
            std::fs::create_dir_all(&workspace).expect("a workspace");
            let workspace = workspace.canonicalize().expect("canonical");
            Self {
                _dir: dir,
                settings,
                workspace,
            }
        }

        fn run(&self, line: &str) -> Outcome {
            run(line, Some(&self.settings), &self.workspace, known)
        }

        /// The one answer a line gave, as `(tone, topic, body)`.
        fn say(&self, line: &str) -> (Tone, &'static str, String) {
            let outcome = self.run(line);
            assert_eq!(outcome.answers.len(), 1, "{outcome:?}");
            let answer = outcome.answers.into_iter().next().unwrap();
            (answer.tone, answer.topic, answer.body)
        }

        fn file(&self) -> Value {
            serde_json::from_str(&std::fs::read_to_string(&self.settings).expect("read"))
                .expect("json")
        }

        fn key(&self) -> String {
            self.workspace.to_str().unwrap().to_string()
        }
    }

    #[test]
    fn the_research_session_reads_as_upstreams_did() {
        // `research/appearance-security-ext.md` §`/allowlist` 1, row by row
        // (`cap/fx-allowlist-40x120-*`, `cap/fx-allowlist2-40x120-*`).
        let profile = Profile::new();
        let neutral = |body: &str| (Tone::Neutral, TOPIC, body.to_string());
        assert_eq!(
            profile.say(""),
            neutral("effective persistent allow rules: (none)")
        );
        assert_eq!(
            profile.say("view"),
            neutral("effective persistent allow rules: (none)")
        );
        assert!(!profile.settings.exists(), "a listing writes nothing");
        assert_eq!(
            profile.say("add command \"git *\""),
            neutral("added command: \"git *\" (scope=local)")
        );
        assert_eq!(
            profile.say("add command \"git *\""),
            neutral("added command: \"git *\" (scope=local)"),
            "a repeat is added again (trap c)"
        );
        assert_eq!(
            profile.say("add tool read"),
            neutral("added tool read: \"*\" (scope=local)")
        );
        assert_eq!(
            profile.say("add tool write_file"),
            neutral("added tool edit: \"*\" (scope=local)")
        );
        assert_eq!(
            profile.say("add tool nonsense"),
            (Tone::Error, "", ADD_USAGE.to_string())
        );
        assert_eq!(
            profile.say("add web-fetch-domain Example.COM"),
            neutral("added web-fetch-domain: \"domain:example.com\" (scope=local)")
        );
        assert_eq!(
            profile.say("user add url https://example.com/*"),
            neutral(
                "added url: \"https://example.com/*\" (scope=user); user rules shadowed by local settings"
            )
        );
        assert_eq!(
            profile.say("view local"),
            neutral(
                "local persistent allow rules:\n  tools:\n    read: workspace\n    edit: workspace\n  commands:\n    git *\n  web-fetch domains:\n    domain:example.com"
            )
        );
        assert_eq!(
            profile.say("view user"),
            neutral(
                "user persistent allow rules:\n  urls:\n    https://example.com/*\nuser rules are shadowed by local settings"
            )
        );
        // `view effective` with a local table: only the local rules, and the
        // shadow line.
        let effective = profile.say("view effective").2;
        assert!(
            effective.starts_with("effective persistent allow rules:\n  tools:"),
            "{effective}"
        );
        assert!(
            effective.ends_with("user rules are shadowed by local settings"),
            "{effective}"
        );
        assert!(!effective.contains("https://example.com/*"), "{effective}");
        assert_eq!(
            profile.say("remove command \"nope\""),
            neutral("no matching rule for command: \"nope\" (scope=local)")
        );
        assert_eq!(
            profile.say("remove command \"git *\""),
            neutral("removed command: \"git *\" (scope=local)")
        );
        assert_eq!(
            profile.say("local reset tools"),
            neutral("reset tools: removed 2 rules (scope=local)")
        );
        assert_eq!(
            profile.say("local reset all"),
            neutral("reset all: removed 1 rule (scope=local)")
        );
        assert_eq!(
            profile.say("local reset"),
            (Tone::Error, "", RESET_USAGE.to_string())
        );
        assert_eq!(profile.say("bogus"), (Tone::Error, "", USAGE.to_string()));
        // The local table is gone, so the user URL is in force now
        // (`cap/fx-allowlist2-40x120-final`).
        assert_eq!(
            profile.say("view"),
            neutral("effective persistent allow rules:\n  urls:\n    https://example.com/*")
        );
        assert_eq!(
            profile.say("user reset all"),
            neutral("reset all: removed 1 rule (scope=user)")
        );
        assert_eq!(profile.file(), json!({"workspaces": {profile.key(): {}}}));
    }

    #[test]
    fn a_change_puts_the_table_read_back_in_force_and_a_listing_reads_it_afresh() {
        let profile = Profile::new();
        let added = profile.run("add command \"git *\"");
        assert_eq!(
            added.in_force,
            Some(RuleSet::new(vec![ConfiguredRule::new(
                "bash",
                "git *",
                RuleAction::Allow
            )]))
        );
        // A hand edit is in force after the next listing, as upstream's view
        // reloads its engine.
        std::fs::write(
            &profile.settings,
            serde_json::to_string(&json!({"permission": {"edit": "deny"}})).unwrap(),
        )
        .unwrap();
        let viewed = profile.run("view user");
        assert_eq!(
            viewed.in_force,
            Some(RuleSet::new(vec![ConfiguredRule::new(
                "edit",
                "*",
                RuleAction::Deny
            )]))
        );
        // A deny is not an allow rule, so the listing has nothing to show.
        assert_eq!(
            viewed.answers[0].body,
            "user persistent allow rules: (none)"
        );
        // A removal of nothing reads nothing back and changes nothing in force.
        assert_eq!(profile.run("remove command \"x\"").in_force, None);
    }

    #[test]
    fn an_emptied_table_still_shadows_until_a_reset_takes_it_away() {
        // `cap/fx-allowlist3-40x120-after.ansi` and its dump.
        let profile = Profile::new();
        profile.run("add command \"x\"");
        profile.run("remove command \"x\"");
        let (_, _, body) = profile.say("user add url https://u/*");
        assert!(
            body.ends_with("; user rules shadowed by local settings"),
            "{body}"
        );
        assert_eq!(
            profile.say("view effective").2,
            "effective persistent allow rules: (none)\nuser rules are shadowed by local settings"
        );
        assert_eq!(
            profile.file()["workspaces"][profile.key()],
            json!({"permission": {}})
        );
    }

    #[test]
    fn a_write_that_cannot_happen_says_why_and_keeps_the_table_in_force() {
        let profile = Profile::new();
        std::fs::create_dir_all(profile.settings.parent().unwrap()).unwrap();
        std::fs::write(&profile.settings, "{\"permission\": \"deny\"}").unwrap();
        let outcome = profile.run("user add command ls");
        assert_eq!(outcome.in_force, None);
        assert_eq!(outcome.answers[0].tone, Tone::Error);
        assert_eq!(
            outcome.answers[0].body,
            "failed to add rule to settings (scope=user, error=`permission` in the settings file is not an object)"
        );
        // Nothing was written over it.
        assert_eq!(profile.file(), json!({"permission": "deny"}));
        let removed = profile.run("user remove command ls");
        assert!(removed.answers[0]
            .body
            .starts_with("failed to remove rule from settings (scope=user"));
        let reset = profile.run("user reset all");
        assert!(reset.answers[0]
            .body
            .starts_with("failed to reset rules in settings (scope=user"));
        // No home: nothing to write into.
        let homeless = run("add command ls", None, &profile.workspace, known);
        assert_eq!(
            homeless.answers[0].body,
            "failed to add rule to settings (scope=local, error=no home directory is set)"
        );
    }

    #[test]
    fn a_read_back_that_fails_keeps_the_table_in_force_and_says_so() {
        let profile = Profile::new();
        crate::profile_home::made(profile.settings.parent().unwrap());
        // The user table does not parse; the local write lands and the read
        // back cannot say what is in force.
        std::fs::write(
            &profile.settings,
            "{\"permission\": {\"bash\": \"perhaps\"}}",
        )
        .unwrap();
        let outcome = profile.run("add command ls");
        assert_eq!(outcome.in_force, None);
        assert_eq!(outcome.answers[0].tone, Tone::Warning);
        assert!(
            outcome.answers[0].body.starts_with(
                "added command: \"ls\" (scope=local); saved but effective source unknown and runtime reload failed ("
            ),
            "{}",
            outcome.answers[0].body
        );
        assert_eq!(
            profile.file()["workspaces"][profile.key()],
            json!({"permission": {"bash": {"ls": "allow"}}})
        );
    }

    #[test]
    fn a_view_of_a_file_that_does_not_read_keeps_the_table_in_force_and_says_why() {
        // Upstream's view puts whatever its reload found in force, a broken
        // layer's nothing included (`writeAllowlistStatus`,
        // `session_commands.zig:987-1009`): a deny in force would lapse because
        // somebody looked. Here the table in force stays, and the reason is said.
        let profile = Profile::new();
        std::fs::create_dir_all(profile.settings.parent().unwrap()).unwrap();
        let entry = |permission: &str| {
            format!(
                "{{\"workspaces\":{{{}:{{\"permission\":{permission}}}}}}}",
                json!(profile.key())
            )
        };
        for (broken, why) in [
            ("{not json".to_string(), "user config diagnostic: malformed_settings"),
            (
                r#"{"permission":{"read":{"secrets/*":"deny"},"bash":"perhaps"}}"#.to_string(),
                "user config diagnostic: invalid_value key=permission `perhaps` is not a permission action; use allow, ask or deny",
            ),
            (
                entry("[\"x\"]"),
                "user config diagnostic: invalid_value key=permission `permission` must be an action or an object of categories",
            ),
            (
                r#"{"workspaces":["x"]}"#.to_string(),
                "user config diagnostic: invalid_value key=workspaces",
            ),
        ] {
            std::fs::write(&profile.settings, &broken).unwrap();
            for line in ["", "view", "view local", "view user"] {
                let viewed = profile.run(line);
                assert_eq!(viewed.in_force, None, "{broken} / {line:?}");
                assert_eq!(
                    viewed.answers,
                    vec![Answer::new(
                        Tone::Error,
                        "settings",
                        format!(
                            "Failed to load settings: {why}; the permission rules in force are unchanged"
                        )
                    )],
                    "{broken} / {line:?}"
                );
            }
        }
        // A file that reads again is put in force by the next view.
        std::fs::write(&profile.settings, entry("{\"read\":\"deny\"}")).unwrap();
        assert_eq!(
            profile.run("view").in_force,
            Some(RuleSet::new(vec![ConfiguredRule::new(
                "read",
                "*",
                RuleAction::Deny
            )]))
        );
    }

    #[test]
    fn a_change_keeps_the_rest_of_the_file_and_its_order() {
        let profile = Profile::new();
        crate::profile_home::made(profile.settings.parent().unwrap());
        std::fs::write(
            &profile.settings,
            "{\"model\":\"m\",\"permission\":{\"bash\":{\"rm*\":\"allow\",\"*\":\"deny\"}},\"zeta\":true}",
        )
        .unwrap();
        profile.run("user add command ls");
        let text = std::fs::read_to_string(&profile.settings).unwrap();
        let order: Vec<usize> = ["\"model\"", "\"rm*\"", "\"*\"", "\"ls\"", "\"zeta\""]
            .iter()
            .map(|needle| text.find(needle).expect(needle))
            .collect();
        let mut sorted = order.clone();
        sorted.sort_unstable();
        assert_eq!(order, sorted, "{text}");
    }
}
