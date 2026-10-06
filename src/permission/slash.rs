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
//! What is **not** here is upstream's `remember` and `revoke`: those manage a
//! per-session store of exact, user-authored rules with ids and generations
//! (`permissions.json`), and xfx has no such store. They are absent rather than
//! printed in a usage line that would promise them.

use std::path::Path;

use crate::config::PermissionMode;

/// The topic every answer is said under.
pub const TOPIC: &str = "permissions";

/// What `/permissions` takes, as upstream's usage line says it
/// (`session_commands.zig:25`) -- its first line only, because the other two
/// name `remember` and `revoke`.
pub const USAGE: &str = "usage: /permissions [ask|auto|full-access|reset]";

/// What a bare `/permissions` says about saved-session rules: upstream's
/// `writeSessionPermissionRules` with nothing stored
/// (`app_commands.zig:776-811`). xfx stores no such rules at all, so this is the
/// only thing it can truthfully say there.
pub const SAVED_RULES: &str = "saved-session permission rules: none";

/// What `/permissions reset` says (`session_commands.zig:388-391`).
pub const RESET: &str = "permissions reset to ask, session grants cleared";

/// What a line `/permissions <argument>` asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    /// Nothing after the name: say what is in force.
    Show,
    /// A mode, in any of upstream's spellings ([`PermissionMode::parse_choice`]).
    Select(PermissionMode),
    /// `reset`: ask, and every "always" of this conversation forgotten.
    Reset,
    /// Anything else, answered with [`USAGE`].
    Usage,
}

impl Request {
    /// Reads the rest of a `/permissions` line.
    ///
    /// Upstream's order (`handlePermissions`): nothing, then a mode, then
    /// `reset` -- case-insensitively, as both of those comparisons are
    /// (`types.zig:2396-2403`, `session_commands.zig:388`).
    pub fn parse(argument: &str) -> Self {
        let argument = argument.trim();
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

/// The body of a bare `/permissions`' first answer: the mode, the configured
/// rules and the session's grants, one per row (`renderInteractiveBody`,
/// `src/core/output/output_contracts.zig:722-744`), without the usage line a
/// caller puts under it.
///
/// `grants` are the rows [`grant_line`] made. `configured rules` is always
/// `(none)`: xfx reads no rules from settings ([`super::PermissionRules`] is
/// built only by its own tests), and `(none)` is upstream's own text for an
/// empty set.
pub fn status_rows(mode: PermissionMode, grants: &[String]) -> String {
    let mut body = format!("mode={}\nconfigured rules: (none)\n", mode.display_label());
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
        // `remember` and `revoke` are upstream's and not this port's: they are
        // answered as any other word is.
        for other in [
            "bogus",
            "remember allow write_file {}",
            "revoke 1",
            "reset now",
            "full",
        ] {
            assert_eq!(Request::parse(other), Request::Usage, "{other}");
        }
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
            status_rows(PermissionMode::Auto, &[]),
            "mode=auto\nconfigured rules: (none)\nsession grants: (none)"
        );
        assert_eq!(
            status_rows(
                PermissionMode::Yolo,
                &[
                    " - write_file -> a.txt".to_string(),
                    " - terminal -> ls in /w".to_string(),
                ],
            ),
            "mode=full access\nconfigured rules: (none)\nsession grants:\n \
             - write_file -> a.txt\n - terminal -> ls in /w"
        );
        assert!(!USAGE.contains("remember") && !USAGE.contains("revoke"));
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
