//! The argument lists fx completes from tables of its own: what follows
//! `/statusline `, `/sound `, `/permissions `, `/workspace ` and
//! `/allowlist ` (`nthSlashCompletion`'s table arms,
//! `src/core/slash_commands/command_specs.zig:610-631@481104fd`).
//!
//! Past a command's blank fx's slash popup goes on for a name with arguments
//! (`slashCompletionPrefix`, `:568-576`), and for these five it lists the
//! arguments alone -- each row's label is its line past the command
//! (`nthSlashCompletionLabel`, `:650-671`), with no description
//! (`nthSlashCompletionDescription`, `:673-684`) -- hung under where the
//! argument starts (`argCompletionAnchor`, `:636-644`). A row is matched by
//! the start of its word, case ignored, and an empty query matches every row
//! (`argCompletionMatches`, `:1031-1034`). The rows are shown, moved and taken
//! as `/mcp `'s are ([`super::mcp_args::McpArgs::fixed`]).

/// The blanks fx's command boundary and argument trim read (`argCompletionPrefix`,
/// `command_specs.zig:909-923@481104fd`).
const SPACES: [char; 2] = [' ', '\t'];

/// `statusline_arg_completions`' words (`command_specs.zig:750-754`).
const STATUSLINE: &[&str] = &["context", "session", "workspace"];
/// `notifications_arg_completions`' (`:756-760`). `max` is fx's and not xfx's
/// (`docs/parity.md`'s `/sound` row): listed as fx lists it, refused as xfx
/// refuses it.
const SOUND: &[&str] = &["on", "off", "max"];
/// `permissions_arg_completions`' (`:762-769`).
const PERMISSIONS: &[&str] = &["ask", "auto", "remember", "revoke", "full-access", "reset"];
/// `workspace_arg_completions`' (`:771-776`).
const WORKSPACE: &[&str] = &["list", "add", "remove", "clear"];
/// The two of them Tab completes with a space (`slashCompletionHasArgs`,
/// `:692-703`).
const WORKSPACE_SPACED: &[&str] = &["add", "remove"];
/// `allowlist_action_completions`' (`:778-785`).
const ALLOWLIST: &[&str] = &["view", "add", "remove", "reset", "local", "user"];
/// `allowlist_view_completions`' (`:787-791`).
const ALLOWLIST_VIEW: &[&str] = &["effective", "local", "user"];
/// `allowlist_scoped_action_suffixes` (`:793-797`).
const ALLOWLIST_SCOPED: &[&str] = &["add", "remove", "reset"];
/// `allowlist_add_kind_completions`' and `_remove_kind_` (`:799-811`).
const ALLOWLIST_KINDS: &[&str] = &["command", "tool", "url", "web-fetch-domain"];
/// `allowlist_reset_scope_completions`' (`:813-819`).
const ALLOWLIST_RESET: &[&str] = &["commands", "tools", "urls", "web-fetch-domains", "all"];
/// `allowlist_add_tool_completions`' and `_remove_tool_` (`:821-841`):
/// `install_skill` and `subagent` are fx's tools and not xfx's, listed as fx
/// lists them and refused as xfx refuses them.
const ALLOWLIST_TOOLS: &[&str] = &[
    "read_file",
    "write_file",
    "edit_file",
    "glob_files",
    "grep_files",
    "skill",
    "install_skill",
    "subagent",
];

/// One table's rows for one draft.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct List<'a> {
    /// What every row's line begins with, its space included: fx's spelling,
    /// whatever blanks and case were typed, so it is also how far past the
    /// draft's leading blanks the rows hang.
    pub(crate) head: String,
    /// The table's words, in fx's order.
    words: &'static [&'static str],
    /// The ones Tab completes with a space after them (`slashCompletionHasArgs`,
    /// `command_specs.zig:692-748`).
    spaced: &'static [&'static str],
    /// What the words are matched against.
    query: &'a str,
}

impl List<'_> {
    /// The rows the query finds, in the table's order: each line whole, and
    /// whether Tab follows it with a space.
    pub(crate) fn rows(&self) -> Vec<(String, bool)> {
        self.words
            .iter()
            .filter(|word| {
                word.get(..self.query.len())
                    .is_some_and(|head| head.eq_ignore_ascii_case(self.query))
            })
            .map(|word| (format!("{}{word}", self.head), self.spaced.contains(word)))
            .collect()
    }
}

/// The table `prefix` -- the draft past its leading blanks -- asks for, if it
/// is one of the five commands then a space or a tab (`nthSlashCompletion`,
/// `command_specs.zig:610-631@481104fd`). A list with no rows is still the
/// table's: nothing else completes such a line, the skills included
/// (`mixedSlashCompletionCount`, `src/ui/footer/picker_presentation.zig:1170-1175`).
pub(crate) fn of(prefix: &str) -> Option<List<'_>> {
    if let Some(query) = argument(prefix, "/allowlist") {
        return Some(allowlist("/allowlist ".into(), query, true));
    }
    let tables: [(&str, &'static [&'static str], &'static [&'static str]); 4] = [
        ("/statusline", STATUSLINE, &[]),
        ("/sound", SOUND, &[]),
        ("/permissions", PERMISSIONS, &[]),
        ("/workspace", WORKSPACE, WORKSPACE_SPACED),
    ];
    tables.into_iter().find_map(|(command, words, spaced)| {
        let query = argument(prefix, command)?.trim_end_matches(SPACES);
        Some(List {
            head: format!("{command} "),
            words,
            spaced,
            query,
        })
    })
}

/// Whether fx hangs an argument list under `prefix` -- `/mcp`, or one of the
/// five tables' commands, then a space or a tab (`argCompletionAnchor` not
/// zero, `command_specs.zig:636-644@481104fd`): such a line lists its
/// argument and never the skills (`mixedSlashCompletionCount`,
/// `src/ui/footer/picker_presentation.zig:1170-1175`), and so opens no slash
/// menu ([`super::picker::query_of`]).
pub(crate) fn anchored(prefix: &str) -> bool {
    argument(prefix, "/mcp").is_some() || of(prefix).is_some()
}

/// What follows `command` in `prefix` past a space or a tab, its leading
/// blanks trimmed (`rawArgCompletionPrefix`, `command_specs.zig:917-923`).
fn argument<'a>(prefix: &'a str, command: &str) -> Option<&'a str> {
    let rest = prefix.strip_prefix(command)?;
    rest.starts_with(SPACES)
        .then(|| rest.trim_start_matches(SPACES))
}

/// A word and what follows its first blank, that trimmed too, if anything
/// does (`splitAllowlistArgQuery`, `command_specs.zig:1264-1276`).
fn split(query: &str) -> (&str, Option<&str>) {
    let query = query.trim_start_matches(SPACES);
    match query.find(SPACES) {
        Some(at) => (
            &query[..at],
            Some(query[at + 1..].trim_start_matches(SPACES)),
        ),
        None => (query, None),
    }
}

/// `/allowlist `'s table for what follows `head` -- the command (`top`), or
/// past `local ` or `user ` its scope too, which has no `view` and no scopes of
/// its own (`allowlistArgCompletionState` and
/// `scopedAllowlistArgCompletionState`, `command_specs.zig:1042-1204`; the
/// anchor, `:1206-1249`, is the head's length in every arm).
fn allowlist(head: String, query: &str, top: bool) -> List<'_> {
    let (word, rest) = split(query);
    let Some(rest) = rest else {
        let words = if top { ALLOWLIST } else { ALLOWLIST_SCOPED };
        return list(head, words, words, word);
    };
    let is = |name: &str| word.eq_ignore_ascii_case(name);
    if top && is("view") {
        return list(head + "view ", ALLOWLIST_VIEW, &[], rest);
    }
    if let Some(scope) = ["local", "user"].into_iter().find(|scope| top && is(scope)) {
        return allowlist(format!("{head}{scope} "), rest, false);
    }
    if let Some(verb) = ["add", "remove"].into_iter().find(|verb| is(verb)) {
        // The kinds, or past `tool ` the tools (`allowlistToolArgQuery`,
        // `:1251-1256`).
        let head = format!("{head}{verb} ");
        return match split(rest) {
            (tool, Some(name)) if tool.eq_ignore_ascii_case("tool") => {
                list(head + "tool ", ALLOWLIST_TOOLS, &[], name)
            }
            _ => list(head, ALLOWLIST_KINDS, ALLOWLIST_KINDS, rest),
        };
    }
    if is("reset") {
        return list(head + "reset ", ALLOWLIST_RESET, &[], rest);
    }
    list(head, &[], &[], "")
}

fn list<'a>(
    head: String,
    words: &'static [&'static str],
    spaced: &'static [&'static str],
    query: &'a str,
) -> List<'a> {
    List {
        head,
        words,
        spaced,
        query,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The labels `prefix`'s rows show, and how far past the draft's blanks
    /// they hang.
    fn labels(prefix: &str) -> (Vec<String>, usize) {
        let list = of(prefix).expect("a table");
        let rows = list
            .rows()
            .into_iter()
            .map(|(line, _)| line[list.head.len()..].to_string())
            .collect();
        (rows, list.head.len())
    }

    #[test]
    fn each_table_is_fxs_matched_by_the_start_of_its_word_ignoring_case() {
        // `command_specs.zig`'s own cases (`:2080-2134@481104fd`).
        assert_eq!(labels("/statusline ").0, STATUSLINE);
        assert_eq!(labels("/statusline s"), (vec!["session".into()], 12));
        assert_eq!(labels("/sound of").0, ["off"]);
        assert_eq!(labels("/sound ma").0, ["max"]);
        assert_eq!(labels("/sound ").1, "/sound ".len());
        assert_eq!(labels("/permissions ").0, PERMISSIONS);
        assert_eq!(labels("/permissions RE").0, ["remember", "revoke", "reset"]);
        assert_eq!(labels("/workspace a").0, ["add"]);
        // The trim is both ends for these four: a finished word still matches,
        // and a second word matches nothing.
        assert_eq!(labels("/sound \ton \t").0, ["on"]);
        assert!(labels("/sound on off").0.is_empty());
        // A tab is a boundary as a space is; anything else is not one.
        assert_eq!(labels("/statusline\tco").0, ["context"]);
        for not in ["/statusline", "/statuslinex", "/Statusline x", "/sound\nx"] {
            assert_eq!(of(not), None, "{not:?}");
        }
    }

    #[test]
    fn allowlist_walks_its_words_and_hangs_under_the_one_being_typed() {
        assert_eq!(labels("/allowlist ").0, ALLOWLIST);
        assert_eq!(labels("/allowlist a"), (vec!["add".into()], 11));
        assert_eq!(labels("/allowlist add ").0, ALLOWLIST_KINDS);
        assert_eq!(
            labels("/allowlist add web-"),
            (vec!["web-fetch-domain".into()], 15)
        );
        assert_eq!(labels("/allowlist remove u").0, ["url"]);
        assert_eq!(labels("/allowlist add tool write").0, ["write_file"]);
        assert_eq!(labels("/allowlist remove tool ").0, ALLOWLIST_TOOLS);
        assert_eq!(labels("/allowlist reset c").0, ["commands"]);
        assert_eq!(labels("/allowlist reset web-").0, ["web-fetch-domains"]);
        assert_eq!(labels("/allowlist view ").0, ALLOWLIST_VIEW);
        assert_eq!(labels("/allowlist local ").0, ALLOWLIST_SCOPED);
        assert_eq!(
            labels("/allowlist user add tool g"),
            (vec!["glob_files".into(), "grep_files".into()], 25)
        );
        assert_eq!(labels("/allowlist local reset ").0, ALLOWLIST_RESET);
        // The words ignore case and the blanks between them; the head is fx's
        // spelling, so the rows hang where fx's do.
        assert_eq!(
            labels("/allowlist  ADD\t Tool  sk"),
            (vec!["skill".into()], 20)
        );
        // A word fx does not know past its blank lists nothing, and still
        // keeps the line from the slash menu.
        for nothing in [
            "/allowlist bogus x",
            "/allowlist local bogus x",
            "/allowlist add command git status",
            "/allowlist view effective x",
        ] {
            assert!(labels(nothing).0.is_empty(), "{nothing:?}");
        }
        assert_eq!(labels("/allowlist bogus x").1, "/allowlist ".len());
    }

    #[test]
    fn tab_follows_a_line_with_more_to_come_by_a_space() {
        // `slashCompletionHasArgs` and `allowlistCompletionHasArgs`
        // (`command_specs.zig:692-748`).
        let spaced = |prefix: &str| -> Vec<(String, bool)> { of(prefix).unwrap().rows() };
        assert_eq!(
            spaced("/workspace "),
            [
                ("/workspace list".into(), false),
                ("/workspace add".into(), true),
                ("/workspace remove".into(), true),
                ("/workspace clear".into(), false),
            ]
        );
        for more in [
            "/allowlist ",
            "/allowlist add ",
            "/allowlist user ",
            "/allowlist local remove ",
        ] {
            assert!(spaced(more).iter().all(|(_, more)| *more), "{more:?}");
        }
        for done in [
            "/statusline ",
            "/sound ",
            "/permissions ",
            "/allowlist view ",
            "/allowlist reset ",
            "/allowlist add tool ",
        ] {
            assert!(spaced(done).iter().all(|(_, more)| !*more), "{done:?}");
        }
    }
}
