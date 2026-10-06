//! What `/permissions` means, on either front end.
//!
//! Upstream's command (`commandHandlePermissions`,
//! `vercel-labs/fx@c1db9191 src/core/app/app_commands.zig:767-774`, over
//! `handlePermissions`, `src/core/session/session_commands.zig:367-394`): what
//! its argument can be, and the words each answer is said in. The values live
//! here rather than in either shell because both shells answer the command --
//! the TUI as upstream's transcript notices (`crate::tui`'s `permissions`) and
//! the line shell as lines (`crate::interactive`) -- and two copies of one
//! sentence are two sentences that can drift.
//!
//! `remember` and `revoke` are here too: what they parse to and what they
//! say. The store they change is [`super::session_rules`]'s, and the
//! confirmation each is put through is each front end's own.

use std::path::Path;

use crate::config::PermissionMode;

use super::allowlist::Tone;
use super::rules::RuleSet;
use super::session_rules::{Decision, Event, State};

/// The topic every answer is said under.
pub const TOPIC: &str = "permissions";

/// What `/permissions` takes, as upstream's usage says it
/// (`permissions_usage`, `session_commands.zig:25`): the modes, then the two
/// rule-management forms.
pub const USAGE: &str = "usage: /permissions [ask|auto|full-access|reset]\n       /permissions remember <allow|deny> <tool-name> <arguments-json>\n       /permissions revoke <rule-id>";

/// What a `remember` or `revoke` that does not parse says
/// (`writePermissionManagementUsage`, `app_commands.zig:978-985`).
pub const MANAGEMENT_USAGE: &str = "usage: /permissions remember <allow|deny> <tool-name> <arguments-json>\n       /permissions revoke <rule-id>";

/// What a bare `/permissions` says about saved-session rules when the session
/// has none (`writeSessionPermissionRules`, `app_commands.zig:776-811`).
pub const SAVED_RULES: &str = "saved-session permission rules: none";

/// `remember` or `revoke` with no saved session to keep the rule in
/// (`app_commands.zig:828-833`), an error.
pub const NO_SESSION: &str = "saved-session permission rules require an active saved session";

/// `remember` or `revoke` while a question is up (`:836-842`), a warning.
pub const PROMPT_ACTIVE: &str =
    "finish the active permission prompt before managing saved-session rules";

/// A `remember` whose action cannot be prepared without running it
/// (`:880-889`), an error.
pub const NOT_PREPARED: &str =
    "could not prepare that exact permission action without executing it";

/// A `revoke` of an id the session does not have (`:940-947`), a warning.
pub const NOT_FOUND: &str = "saved-session permission rule not found";

/// A confirmed change that landed (`input_approval_runtime.zig:293-299`).
pub const UPDATED: &str = "saved-session permission rule updated";
/// A confirmed change the store moved under (`:300-307`), a warning.
pub const CHANGED: &str = "saved-session permission rule changed before confirmation";
/// A confirmed change with no room for it (`:308-315`), a warning.
pub const CAPACITY: &str = "saved-session permission rule capacity reached";
/// A confirmed change the store cannot take (`:316-323`), an error.
pub const INVALID: &str = "saved-session permission rule is invalid";
/// The second choice taken (`:264-272`).
pub const CANCELLED: &str = "saved-session permission change cancelled";

/// The confirmation's question for each form (`approvalQuestion`,
/// `approval_ui.zig:1855-1866`).
pub fn remember_question(decision: Decision) -> &'static str {
    match decision {
        Decision::Allow => "Remember allow for this saved session?",
        Decision::Deny => "Remember deny for this saved session?",
    }
}

/// The question a `revoke` is confirmed with.
pub const REVOKE_QUESTION: &str = "Revoke this saved-session permission rule?";

/// The line under a `remember`'s question: a new rule, or the replacement of
/// the one already kept for this action (`app_commands.zig:900-905`).
pub fn remember_explanation(replacing: bool) -> &'static str {
    if replacing {
        "Confirm replacement of the existing exact saved-session rule. The action will not run."
    } else {
        "Confirm this exact saved-session permission rule. The action will not run."
    }
}

/// The line under a `revoke`'s question (`:953`).
pub const REVOKE_EXPLANATION: &str =
    "Confirm revocation of this stored rule. No current action will be prepared or run.";

/// The second notice of a bare `/permissions`: the session's saved rules, one
/// `<id> <decision> <display>` row each in the store's order, or [`SAVED_RULES`].
/// The displays are the store's, which a person's own JSON put there: each
/// front end makes them safe for its terminal.
pub fn saved_rules_body(state: &State) -> String {
    if state.rules.is_empty() {
        return SAVED_RULES.to_string();
    }
    let mut body = format!("saved-session permission rules ({}):", state.rules.len());
    for rule in &state.rules {
        body.push_str(&format!(
            "\n{} {} {}",
            rule.id,
            rule.decision.label(),
            rule.display
        ));
    }
    body
}

/// What `/permissions reset` says (`session_commands.zig:388-391`).
pub const RESET: &str = "permissions reset to ask, session grants cleared";

/// A `remember` or `revoke` waiting for the person to confirm it: what the
/// confirmation shows, and the change it makes to the store at `path`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Confirmation {
    pub question: &'static str,
    pub explanation: &'static str,
    /// The action the rule is about, as the store shows it. Made safe by
    /// whichever front end prints it.
    pub display: String,
    pub path: std::path::PathBuf,
    pub event: Event,
}

/// What a rule-management line comes to before anything is confirmed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Managed {
    /// Put this to the person.
    Confirm(Confirmation),
    /// Say this, under `topic`, and change nothing.
    Say {
        tone: Tone,
        topic: &'static str,
        body: String,
    },
}

impl Managed {
    fn say(tone: Tone, topic: &'static str, body: impl Into<String>) -> Self {
        Self::Say {
            tone,
            topic,
            body: body.into(),
        }
    }
}

/// `handlePermissionRuleManagement` (`app_commands.zig:814-966`): `None` for a
/// request that is not `remember` or `revoke`.
///
/// Upstream's order, refusals first: no saved session to keep a rule in, then
/// a question already up, then the grammar, then -- for `remember` -- the
/// action prepared exactly as a model's call would be and **not run**
/// (`prepare` is [`crate::tools::prepare_rule_action`] over the session's
/// scope and table), or -- for `revoke` -- the rule found. What comes out is a
/// confirmation carrying the generation the person was shown, so the change
/// is a compare-and-swap against it ([`super::session_rules::State::apply`]).
pub fn manage(
    request: &Request,
    rules_path: Option<&Path>,
    prompt_active: bool,
    prepare: impl FnOnce(&str, &serde_json::Value) -> Result<crate::tools::RuleAction, String>,
) -> Option<Managed> {
    if !matches!(
        request,
        Request::Remember { .. } | Request::Revoke(_) | Request::ManagementUsage
    ) {
        return None;
    }
    let Some(path) = rules_path else {
        return Some(Managed::say(Tone::Error, TOPIC, NO_SESSION));
    };
    if prompt_active {
        return Some(Managed::say(Tone::Warning, TOPIC, PROMPT_ACTIVE));
    }
    let state = match super::session_rules::load(path) {
        Ok(state) => state,
        Err(reason) => return Some(Managed::say(Tone::Error, TOPIC, reason)),
    };
    Some(match request {
        Request::Remember {
            decision,
            tool,
            arguments,
        } => {
            let prepared = serde_json::from_str::<serde_json::Value>(arguments)
                .map_err(|err| err.to_string())
                .and_then(|arguments| prepare(tool, &arguments));
            match prepared {
                Ok(action) => {
                    let existing = state.rule_for_key(&action.key);
                    Managed::Confirm(Confirmation {
                        question: remember_question(*decision),
                        explanation: remember_explanation(existing.is_some()),
                        display: action.display.clone(),
                        path: path.to_path_buf(),
                        event: Event::Set {
                            key: action.key,
                            display: action.display,
                            decision: *decision,
                            expected: existing.map(|rule| rule.generation),
                        },
                    })
                }
                Err(_) => Managed::say(Tone::Error, TOPIC, NOT_PREPARED),
            }
        }
        Request::Revoke(id) => match state.rule_for_id(*id) {
            Some(rule) => Managed::Confirm(Confirmation {
                question: REVOKE_QUESTION,
                explanation: REVOKE_EXPLANATION,
                display: rule.display.clone(),
                path: path.to_path_buf(),
                event: Event::Revoke {
                    id: rule.id,
                    expected: rule.generation,
                },
            }),
            None => Managed::say(Tone::Warning, TOPIC, NOT_FOUND),
        },
        _ => Managed::say(Tone::Error, "", MANAGEMENT_USAGE),
    })
}

/// The confirmed change, made: what the store came to, in upstream's words
/// and tones (`submitRuleManagementChoice`, `input_approval_runtime.zig:255-327`).
/// A store that cannot be read or written is an error naming why, and the
/// file is left as it was.
pub fn confirm(confirmation: &Confirmation) -> (Tone, String) {
    use super::session_rules::Applied;
    match super::session_rules::update(&confirmation.path, &confirmation.event) {
        Ok(Applied::Applied(_)) => (Tone::Neutral, UPDATED.to_string()),
        Ok(Applied::Stale) => (Tone::Warning, CHANGED.to_string()),
        Ok(Applied::Full) => (Tone::Warning, CAPACITY.to_string()),
        Ok(Applied::Invalid) => (Tone::Error, INVALID.to_string()),
        Err(reason) => (Tone::Error, reason),
    }
}

/// The second notice of a bare `/permissions` for the session whose rules are
/// at `path` -- [`SAVED_RULES`] when there is none yet -- or why the store
/// could not be read.
pub fn saved_rules_at(path: Option<&Path>) -> Result<String, String> {
    match path {
        Some(path) => super::session_rules::load(path).map(|state| saved_rules_body(&state)),
        None => Ok(SAVED_RULES.to_string()),
    }
}

/// What a line `/permissions <argument>` asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// Nothing after the name: say what is in force.
    Show,
    /// A mode, in any of upstream's spellings ([`PermissionMode::parse_choice`]).
    Select(PermissionMode),
    /// `reset`: ask, and every "always" of this conversation forgotten.
    Reset,
    /// `remember <allow|deny> <tool-name> <arguments-json>`: keep a rule for
    /// exactly that action, once confirmed. The arguments are the rest of the
    /// line as typed; whether they are JSON is the preparation's question.
    Remember {
        decision: Decision,
        tool: String,
        arguments: String,
    },
    /// `revoke <rule-id>`: take the rule with that id away, once confirmed.
    Revoke(u64),
    /// A `remember` or `revoke` that does not parse: [`MANAGEMENT_USAGE`].
    ManagementUsage,
    /// Anything else, answered with [`USAGE`].
    Usage,
}

impl Request {
    /// Reads the rest of a `/permissions` line.
    ///
    /// Upstream's order (`commandHandlePermissions`, `app_commands.zig:767-774`):
    /// `remember` and `revoke` first (`handlePermissionRuleManagement`,
    /// `:814-966`), then nothing, a mode, `reset` (`handlePermissions`) --
    /// every word case-insensitive (`types.zig:2396-2403`,
    /// `session_commands.zig:388`).
    pub fn parse(argument: &str) -> Self {
        let argument = argument.trim();
        if let Some((action, rest)) = split_word(argument) {
            if action.eq_ignore_ascii_case("remember") {
                return parse_remember(rest);
            }
            if action.eq_ignore_ascii_case("revoke") {
                return parse_revoke(rest);
            }
        }
        if argument.is_empty() {
            return Self::Show;
        }
        if let Some(mode) = PermissionMode::parse_choice(argument) {
            return Self::Select(mode);
        }
        if argument.eq_ignore_ascii_case("reset") {
            return Self::Reset;
        }
        Self::Usage
    }
}

/// `splitPermissionWord` (`app_commands.zig:968-976`): the first word, and the
/// rest with its ends trimmed.
fn split_word(text: &str) -> Option<(&str, &str)> {
    let trimmed = text.trim_matches([' ', '\t']);
    if trimmed.is_empty() {
        return None;
    }
    let end = trimmed.find([' ', '\t']).unwrap_or(trimmed.len());
    Some((&trimmed[..end], trimmed[end..].trim_matches([' ', '\t'])))
}

fn parse_remember(rest: &str) -> Request {
    let Some((decision, rest)) = split_word(rest) else {
        return Request::ManagementUsage;
    };
    let decision = if decision.eq_ignore_ascii_case("allow") {
        Decision::Allow
    } else if decision.eq_ignore_ascii_case("deny") {
        Decision::Deny
    } else {
        return Request::ManagementUsage;
    };
    let Some((tool, arguments)) = split_word(rest) else {
        return Request::ManagementUsage;
    };
    if arguments.is_empty() {
        return Request::ManagementUsage;
    }
    Request::Remember {
        decision,
        tool: tool.to_string(),
        arguments: arguments.to_string(),
    }
}

fn parse_revoke(rest: &str) -> Request {
    let Some((id, after)) = split_word(rest) else {
        return Request::ManagementUsage;
    };
    if !after.is_empty() {
        return Request::ManagementUsage;
    }
    // `std.fmt.parseUnsigned(u64, word, 10)`: digits only, no sign.
    if !id.bytes().all(|byte| byte.is_ascii_digit()) {
        return Request::ManagementUsage;
    }
    match id.parse::<u64>() {
        Ok(id) => Request::Revoke(id),
        Err(_) => Request::ManagementUsage,
    }
}

/// The body of a bare `/permissions`' first answer: the mode, the configured
/// rules and the session's grants, one per row (`renderInteractiveBody`,
/// `src/core/output/output_contracts.zig:722-744`), without the usage line a
/// caller puts under it.
///
/// `rules` is the table in force -- this workspace's when it has one, else the
/// profile's (`crate::config::PermissionTables::effective`) -- one
/// ` - <action> <category> -> <pattern>` row per rule in the file's order.
/// `grants` are the rows [`grant_line`] made.
pub fn status_rows(mode: PermissionMode, rules: &RuleSet, grants: &[String]) -> String {
    let mut body = format!("mode={}\n", mode.display_label());
    if rules.is_empty() {
        body.push_str("configured rules: (none)\n");
    } else {
        body.push_str("configured rules:\n");
        for rule in rules.rules() {
            body.push_str(&format!(
                " - {} {} -> {}\n",
                rule.action.label(),
                rule.permission,
                rule.pattern
            ));
        }
    }
    if grants.is_empty() {
        body.push_str("session grants: (none)");
    } else {
        body.push_str("session grants:");
        for grant in grants {
            body.push('\n');
            body.push_str(grant);
        }
    }
    body
}

/// What setting `mode` says (`session_commands.zig:373-383`).
pub fn selected(mode: PermissionMode) -> String {
    format!("mode set to {}", mode.display_label())
}

/// One row of the `session grants:` list: ` - <tool> -> <target>`
/// (`renderInteractiveBody`, with `displayGrantTarget`,
/// `output_contracts.zig:1699-1709`).
///
/// A file's target is the canonical absolute path a grant is keyed by
/// ([`super::ProposedAction::target`]), said relative to the workspace when it
/// is inside it, as upstream says it; one outside it stays absolute rather than
/// becoming a chain of `..`. A command's target is the command and its
/// directory, as the grant holds it.
///
/// Not made safe here: a command a model wrote is not text a terminal may be
/// handed, and each front end has its own rule for that.
pub fn grant_line(tool: &str, target: &str, workspace_root: &Path) -> String {
    let shown = if tool == "terminal" {
        target.to_string()
    } else {
        match Path::new(target).strip_prefix(workspace_root) {
            Ok(relative) if !relative.as_os_str().is_empty() => {
                relative.to_string_lossy().into_owned()
            }
            _ => target.to_string(),
        }
    };
    format!(" - {tool} -> {shown}")
}

/// The mode Shift-Tab goes to from `mode`: ask, auto, full access, and round
/// again (`toggleMode`, `app_permission_runtime.zig:135-142`).
pub fn next(mode: PermissionMode) -> PermissionMode {
    match mode {
        PermissionMode::Ask => PermissionMode::Auto,
        PermissionMode::Auto => PermissionMode::Yolo,
        PermissionMode::Yolo => PermissionMode::Ask,
    }
}

/// What a settings write that failed says, under upstream's labels
/// (`reportUserSettingsFailure`, `session_commands.zig:110-133`):
/// `permission-mode` for a mode, `full-access-acknowledgment` for the warning.
///
/// "Active for this process" is true of both: the mode is applied whether or
/// not it could be recorded, and this process does not show the warning again
/// either way.
pub fn not_saved(reason: &str) -> String {
    format!("active for this process but not saved to user settings ({reason})")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn an_argument_is_read_in_upstreams_order_and_case() {
        assert_eq!(Request::parse(""), Request::Show);
        assert_eq!(Request::parse("   "), Request::Show);
        assert_eq!(Request::parse("ask"), Request::Select(PermissionMode::Ask));
        assert_eq!(
            Request::parse("AUTO"),
            Request::Select(PermissionMode::Auto)
        );
        for third in ["full-access", "full access", "yolo", "Full-Access", "YOLO"] {
            assert_eq!(
                Request::parse(third),
                Request::Select(PermissionMode::Yolo),
                "{third}"
            );
        }
        assert_eq!(Request::parse("reset"), Request::Reset);
        assert_eq!(Request::parse("Reset"), Request::Reset);
        for other in ["bogus", "reset now", "full", "remembering"] {
            assert_eq!(Request::parse(other), Request::Usage, "{other}");
        }
    }

    #[test]
    fn remember_and_revoke_are_read_as_upstream_reads_them() {
        assert_eq!(
            Request::parse("remember allow write_file {\"path\":\"a.txt\",\"content\":\"x\"}"),
            Request::Remember {
                decision: Decision::Allow,
                tool: "write_file".to_string(),
                arguments: "{\"path\":\"a.txt\",\"content\":\"x\"}".to_string(),
            }
        );
        assert_eq!(
            Request::parse("  REMEMBER  Deny\tterminal   {\"action\":\"exec\"}  "),
            Request::Remember {
                decision: Decision::Deny,
                tool: "terminal".to_string(),
                arguments: "{\"action\":\"exec\"}".to_string(),
            }
        );
        assert_eq!(Request::parse("revoke 1"), Request::Revoke(1));
        assert_eq!(Request::parse("Revoke 42"), Request::Revoke(42));
        for refused in [
            "remember",
            "remember allow",
            "remember allow write_file",
            "remember maybe write_file {}",
            "revoke",
            "revoke abc",
            "revoke -1",
            "revoke +1",
            "revoke 1 2",
            "revoke 99999999999999999999",
        ] {
            assert_eq!(
                Request::parse(refused),
                Request::ManagementUsage,
                "{refused}"
            );
        }
    }

    #[test]
    fn the_saved_rules_are_listed_as_upstream_lists_them() {
        use crate::permission::session_rules::{Event, KeyKind, RuleKey};
        assert_eq!(saved_rules_body(&State::default()), SAVED_RULES);
        let crate::permission::session_rules::Applied::Applied(state) =
            State::default().apply(&Event::Set {
                key: RuleKey::new(KeyKind::FileMutation, b"k".to_vec()).unwrap(),
                display: "write_file b.txt preimage=absent after=a1fce4363854ff88".to_string(),
                decision: Decision::Deny,
                expected: None,
            })
        else {
            panic!("applied");
        };
        // `cap/fx-permissions3-40x120-listed.txt`.
        assert_eq!(
            saved_rules_body(&state),
            "saved-session permission rules (1):\n1 deny write_file b.txt preimage=absent after=a1fce4363854ff88"
        );
    }

    #[test]
    fn shift_tab_goes_ask_auto_full_access_and_round() {
        assert_eq!(next(PermissionMode::Ask), PermissionMode::Auto);
        assert_eq!(next(PermissionMode::Auto), PermissionMode::Yolo);
        assert_eq!(next(PermissionMode::Yolo), PermissionMode::Ask);
    }

    #[test]
    fn the_status_rows_are_upstreams_three_facts() {
        // `cap/fx-permissions-40x120-bare.txt`, rows 3-5.
        assert_eq!(
            status_rows(PermissionMode::Auto, &RuleSet::default(), &[]),
            "mode=auto\nconfigured rules: (none)\nsession grants: (none)"
        );
        assert_eq!(
            status_rows(
                PermissionMode::Yolo,
                &RuleSet::default(),
                &[
                    " - write_file -> a.txt".to_string(),
                    " - terminal -> ls in /w".to_string(),
                ],
            ),
            "mode=full access\nconfigured rules: (none)\nsession grants:\n \
             - write_file -> a.txt\n - terminal -> ls in /w"
        );
        assert_eq!(USAGE.lines().count(), 3);
    }

    #[test]
    fn configured_rules_are_listed_in_the_files_order_as_upstream_lists_them() {
        // `output_contracts.zig:2178`: ` - <action> <permission> -> <pattern>`.
        use crate::permission::rules::{ConfiguredRule, RuleAction};
        let rules = RuleSet::new(vec![
            ConfiguredRule::new("edit", "src/*", RuleAction::Allow),
            ConfiguredRule::new("open_url", "*", RuleAction::Ask),
            ConfiguredRule::new("bash", "rm*", RuleAction::Deny),
        ]);
        assert_eq!(
            status_rows(PermissionMode::Ask, &rules, &[]),
            "mode=ask\nconfigured rules:\n - allow edit -> src/*\n - ask open_url -> *\n \
             - deny bash -> rm*\nsession grants: (none)"
        );
    }

    #[test]
    fn a_selection_is_said_in_the_word_a_person_reads() {
        assert_eq!(selected(PermissionMode::Ask), "mode set to ask");
        assert_eq!(selected(PermissionMode::Auto), "mode set to auto");
        assert_eq!(selected(PermissionMode::Yolo), "mode set to full access");
        assert_eq!(
            not_saved("Permission denied (os error 13)"),
            "active for this process but not saved to user settings \
             (Permission denied (os error 13))"
        );
    }

    #[test]
    fn a_grant_is_listed_the_way_upstream_lists_one() {
        let root = PathBuf::from("/w");
        assert_eq!(
            grant_line("write_file", "/w/src/a.txt", &root),
            " - write_file -> src/a.txt"
        );
        assert_eq!(
            grant_line("edit_file", "/elsewhere/b.txt", &root),
            " - edit_file -> /elsewhere/b.txt",
            "outside the workspace stays absolute"
        );
        assert_eq!(
            grant_line("terminal", "ls -la in /w", &root),
            " - terminal -> ls -la in /w"
        );
        assert_eq!(
            grant_line("create_folder", "/w", &root),
            " - create_folder -> /w"
        );
    }
}
