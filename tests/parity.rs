//! The parity ledger, reconciled against the binary that ships.
//!
//! `scripts/check-no-stubs.sh` performs the same reconciliation on the source
//! text, so that a repository with a broken build still cannot hide a broken
//! promise. This file does it the other way round: it asks the *running*
//! product what it advertises -- the parser's own subcommand list, the tool
//! schemas exactly as they are serialized into a Gateway request, the rendered
//! help pages, and the shell's slash table -- and requires each answer to line
//! up with exactly one row of `docs/parity.md`.
//!
//! Both directions matter, and each catches what the other cannot. A surface
//! with no row is an undocumented promise. A row with no surface is a claim
//! about something that does not exist, which is the more embarrassing of the
//! two.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use fxr::cli::{help_text, parser_command_names, ADVERTISED_COMMANDS, ADVERTISED_ENTRYPOINTS};
use fxr::interactive::SLASH_COMMANDS;
use fxr::tools::{PermissionKind, Registry, ADVERTISED_TOOLS};
use serde_json::Value;

/// One row of the ledger.
#[derive(Debug, Clone)]
struct Row {
    /// The row's own surface name, without backticks. A grouped row's name is
    /// its prose heading.
    name: String,
    kind: String,
    status: String,
    /// Every backticked identifier in the surface column, which for a grouped
    /// row is the list of surfaces it actually covers.
    mentioned: Vec<String>,
}

fn parity_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("docs")
        .join("parity.md")
}

/// Parses every inventory row of the ledger.
///
/// The legend table at the top has three columns and is skipped by field count,
/// exactly as the shell script skips it.
fn rows() -> Vec<Row> {
    let text = fs::read_to_string(parity_path()).expect("read docs/parity.md");
    let mut rows = Vec::new();
    for line in text.lines() {
        if !line.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        // A leading and a trailing empty cell, plus four real ones.
        if cells.len() < 6 {
            continue;
        }
        let (surface, kind, status) = (cells[1], cells[2], cells[3]);
        if surface.is_empty() || surface.starts_with("---") || kind == "Kind" {
            continue;
        }
        let mentioned = backticked(surface);
        let name = surface.trim_matches('`').to_string();
        rows.push(Row {
            name,
            kind: kind.to_string(),
            status: status.to_string(),
            mentioned,
        });
    }
    assert!(rows.len() > 40, "the ledger looks unparsed: {}", rows.len());
    rows
}

/// Every `` `identifier` `` in `text`, in order.
fn backticked(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find('`') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('`') else { break };
        found.push(after[..close].to_string());
        rest = &after[close + 1..];
    }
    found
}

/// The names of every row of one kind and status.
fn named(kind: &str, status: &str) -> BTreeSet<String> {
    rows()
        .into_iter()
        .filter(|row| row.kind == kind && row.status == status)
        .map(|row| row.name)
        .collect()
}

/// Every identifier named by a deferred row of one of `kinds`, grouped rows
/// included.
///
/// An empty result is treated as a parsing failure rather than as good news: a
/// check that has nothing to check passes for the wrong reason, and every kind
/// this is called with has deferred rows.
fn deferred_identifiers(kinds: &[&str]) -> BTreeSet<String> {
    let found: BTreeSet<String> = rows()
        .into_iter()
        .filter(|row| row.status == "deferred" && kinds.contains(&row.kind.as_str()))
        .flat_map(|row| row.mentioned)
        .collect();
    assert!(
        !found.is_empty(),
        "no deferred {kinds:?} names parsed out of the ledger"
    );
    found
}

fn set_of<I: IntoIterator<Item = S>, S: Into<String>>(items: I) -> BTreeSet<String> {
    items.into_iter().map(Into::into).collect()
}

// ---------------------------------------------------------------------------
// every row is well formed and unique
// ---------------------------------------------------------------------------

#[test]
fn every_row_carries_a_known_kind_and_status() {
    for row in rows() {
        assert!(
            matches!(
                row.kind.as_str(),
                "command"
                    | "slash"
                    | "tool"
                    | "tool group"
                    | "provider"
                    | "persistence"
                    | "ui"
                    | "embedding"
            ),
            "row `{}` has kind {:?}",
            row.name,
            row.kind
        );
        assert!(
            matches!(row.status.as_str(), "implemented" | "partial" | "deferred"),
            "row `{}` has status {:?}",
            row.name,
            row.status
        );
    }
}

#[test]
fn every_surface_has_exactly_one_row() {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for row in rows() {
        *counts.entry(row.name).or_default() += 1;
    }
    let repeated: Vec<&String> = counts
        .iter()
        .filter(|(_, count)| **count > 1)
        .map(|(name, _)| name)
        .collect();
    assert!(repeated.is_empty(), "documented twice: {repeated:?}");
}

// ---------------------------------------------------------------------------
// commands
// ---------------------------------------------------------------------------

/// Everything a user can invoke: the parser's subcommands, plus the surfaces
/// that are reached without naming one.
fn advertised_commands() -> BTreeSet<String> {
    let mut advertised: BTreeSet<String> = parser_command_names().into_iter().collect();
    advertised.extend(ADVERTISED_ENTRYPOINTS.iter().map(|name| name.to_string()));
    advertised
}

#[test]
fn the_implemented_commands_are_exactly_the_commands_the_binary_has() {
    assert_eq!(named("command", "implemented"), advertised_commands());
}

#[test]
fn the_declared_command_inventory_matches_the_real_parser() {
    assert_eq!(
        set_of(ADVERTISED_COMMANDS.to_vec()),
        set_of(parser_command_names()),
        "ADVERTISED_COMMANDS has drifted from clap"
    );
}

#[test]
fn no_deferred_command_is_reachable_or_advertised() {
    let advertised = advertised_commands();
    let help = help_text();
    let listed = help_command_names(&help);
    for name in deferred_identifiers(&["command"]) {
        assert!(
            !advertised.contains(&name),
            "`{name}` is deferred but reachable"
        );
        assert!(
            !listed.contains(&name),
            "`{name}` is deferred but listed in help"
        );
    }
}

/// The command names clap prints under `Commands:`.
///
/// Parsed as names rather than searched for as substrings: `resume` is a
/// deferred command *and* a fragment of the `--resume` flag, so "does the help
/// text contain the word" is the wrong question.
fn help_command_names(help: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut inside = false;
    for line in help.lines() {
        if line.starts_with("Commands:") {
            inside = true;
            continue;
        }
        if inside {
            if line.trim().is_empty() || !line.starts_with("  ") {
                break;
            }
            if let Some(name) = line.split_whitespace().next() {
                names.insert(name.to_string());
            }
        }
    }
    assert!(!names.is_empty(), "no commands parsed out of help: {help}");
    names
}

#[test]
fn the_help_page_lists_every_command_the_parser_accepts_and_no_other() {
    assert_eq!(
        help_command_names(&help_text()),
        set_of(parser_command_names())
    );
}

#[test]
fn the_shell_has_no_subcommand_name() {
    // The entrypoint is a bare invocation. If it also became a subcommand there
    // would be two spellings of one surface, and the ledger promises one.
    for name in ADVERTISED_ENTRYPOINTS {
        assert!(
            !parser_command_names().iter().any(|other| other == name),
            "`{name}` is both an entrypoint and a subcommand"
        );
    }
}

// ---------------------------------------------------------------------------
// tools
// ---------------------------------------------------------------------------

/// The tool names as the model actually receives them, read back out of the
/// serialized advertisement rather than from the declaration next to it.
fn advertised_tool_names() -> BTreeSet<String> {
    Registry::builtin()
        .advertisement()
        .iter()
        .map(|tool| {
            tool["name"]
                .as_str()
                .expect("every advertised tool has a name")
                .to_string()
        })
        .collect()
}

#[test]
fn the_implemented_tools_are_exactly_the_tools_the_model_is_offered() {
    assert_eq!(named("tool", "implemented"), advertised_tool_names());
    assert_eq!(set_of(ADVERTISED_TOOLS.to_vec()), advertised_tool_names());
}

/// The prose above the tool table, which is what a reader actually reads.
///
/// A row-by-row validator cannot see a sentence, and a sentence is where the
/// most dangerous drift lives: this file once said fxr "advertises the four
/// read-only tools below" while the table underneath listed eight, including
/// the three that rewrite files and the one that starts processes. Every row
/// was correct and the paragraph was a lie about what fxr is allowed to do.
fn tools_prose() -> String {
    let ledger = fs::read_to_string(parity_path()).expect("read docs/parity.md");
    let start = ledger.find("## Tools").expect("the Tools section exists");
    let section = &ledger[start..];
    let end = section
        .find("\n| Surface")
        .expect("the Tools section has a table");
    section[..end].to_string()
}

/// The count declared immediately before `label`, and the names in the
/// parentheses immediately after it.
fn prose_group(label: &str) -> (usize, BTreeSet<String>) {
    let text = tools_prose();
    let marker = format!(" {label} (");
    let at = text
        .find(&marker)
        .unwrap_or_else(|| panic!("the Tools prose declares no `{label}` group:\n{text}"));
    let count = text[..at]
        .split_whitespace()
        .next_back()
        .and_then(|word| word.parse::<usize>().ok())
        .unwrap_or_else(|| panic!("the `{label}` group is not preceded by a count:\n{text}"));
    let rest = &text[at + marker.len()..];
    let close = rest
        .find(')')
        .unwrap_or_else(|| panic!("the `{label}` group is not closed:\n{text}"));
    (count, backticked(&rest[..close]).into_iter().collect())
}

/// The tool names of one permission kind, as the registry really classifies them.
fn tools_of_kind(kind: PermissionKind) -> BTreeSet<String> {
    Registry::builtin()
        .specs()
        .iter()
        .filter(|spec| spec.permission() == kind)
        .map(|spec| spec.name().to_string())
        .collect()
}

#[test]
fn the_tools_prose_declares_the_number_of_tools_that_exist() {
    let text = tools_prose();
    let at = text
        .find("advertises the ")
        .expect("the Tools prose says how many tools fxr advertises");
    let declared: usize = text[at + "advertises the ".len()..]
        .split_whitespace()
        .next()
        .and_then(|word| word.parse().ok())
        .expect("the declared count is a number");
    assert_eq!(
        declared,
        advertised_tool_names().len(),
        "the Tools prose claims {declared} tools; the registry advertises {}",
        advertised_tool_names().len()
    );
}

#[test]
fn the_tools_prose_splits_them_the_way_the_permission_system_does() {
    let (read_only_count, read_only) = prose_group("read-only");
    let (mutating_count, mutating) = prose_group("mutating");
    let (command_count, command) = prose_group("command");

    // Each group says how many it has, and has that many.
    assert_eq!(read_only_count, read_only.len(), "{read_only:?}");
    assert_eq!(mutating_count, mutating.len(), "{mutating:?}");
    assert_eq!(command_count, command.len(), "{command:?}");

    // The groups are the registry's own classification rather than a
    // description of it. A tool that changed from a read to a mutation without
    // the paragraph changing would fail here.
    assert_eq!(read_only, tools_of_kind(PermissionKind::ReadOnly));
    assert_eq!(mutating, tools_of_kind(PermissionKind::MutateFile));
    assert_eq!(command, tools_of_kind(PermissionKind::RunCommand));

    // Together they are every advertised tool, once.
    let mut union: BTreeSet<String> = BTreeSet::new();
    for group in [&read_only, &mutating, &command] {
        for name in group {
            assert!(union.insert(name.clone()), "`{name}` is in two groups");
        }
    }
    assert_eq!(union, advertised_tool_names());
    assert_eq!(
        read_only_count + mutating_count + command_count,
        advertised_tool_names().len()
    );
}

#[test]
fn the_tools_prose_says_what_the_dangerous_half_can_do() {
    let text = tools_prose();
    // Not a style check. That paragraph is the only place a reader is told, in
    // one breath, that this registry is not just an observer.
    for claim in ["change files", "start processes", "sandbox"] {
        assert!(
            text.contains(claim),
            "the Tools prose does not mention {claim:?}:\n{text}"
        );
    }
    assert!(
        !tools_of_kind(PermissionKind::MutateFile).is_empty()
            && !tools_of_kind(PermissionKind::RunCommand).is_empty(),
        "the warning above would itself be the drift, if this ever became false"
    );
}

#[test]
fn no_deferred_tool_name_appears_anywhere_in_the_schema() {
    let schema = serde_json::to_string(&Registry::builtin().advertisement()).expect("serialize");
    let schema: Value = serde_json::from_str(&schema).expect("parse");
    let names = advertised_tool_names();

    for name in deferred_identifiers(&["tool", "tool group"]) {
        assert!(
            !names.contains(&name),
            "`{name}` is deferred but advertised"
        );
        // Also absent as an accepted *value*: the deferred durable-terminal
        // actions are not tool names, they are enum members of `terminal`, and
        // advertising one would promise a session fxr cannot open.
        assert!(
            !schema_allows_value(&schema, &name),
            "`{name}` is deferred but is an accepted value in a tool schema"
        );
    }
}

/// Whether any `enum` in the schema accepts `value`.
fn schema_allows_value(node: &Value, value: &str) -> bool {
    match node {
        Value::Object(map) => map.iter().any(|(key, child)| {
            if key == "enum" {
                child
                    .as_array()
                    .is_some_and(|values| values.iter().any(|item| item == value))
            } else {
                schema_allows_value(child, value)
            }
        }),
        Value::Array(items) => items.iter().any(|item| schema_allows_value(item, value)),
        _ => false,
    }
}

#[test]
fn the_terminal_tool_offers_one_action_and_it_is_exec() {
    let advertisement = Registry::builtin().advertisement();
    let terminal = advertisement
        .iter()
        .find(|tool| tool["name"] == "terminal")
        .expect("the terminal tool is advertised");
    let actions = terminal["inputSchema"]["properties"]["action"]["enum"]
        .as_array()
        .expect("the action property is a closed enum");
    assert_eq!(actions, &vec![Value::from("exec")]);
}

// ---------------------------------------------------------------------------
// the shell's slash commands
// ---------------------------------------------------------------------------

#[test]
fn the_implemented_slash_commands_are_exactly_the_ones_the_shell_answers() {
    assert_eq!(
        named("slash", "implemented"),
        set_of(SLASH_COMMANDS.to_vec())
    );
}

#[test]
fn no_deferred_slash_command_is_answered_or_listed() {
    let help = fxr::interactive::help_text();
    for name in deferred_identifiers(&["slash"]) {
        assert!(
            !SLASH_COMMANDS.contains(&name.as_str()),
            "`{name}` is deferred but answered"
        );
        assert!(!help.contains(&name), "`{name}` is deferred but in /help");
    }
}

#[test]
fn the_shell_help_lists_every_slash_command_it_has() {
    let help = fxr::interactive::help_text();
    for name in SLASH_COMMANDS {
        assert!(help.contains(name), "/help omits {name}");
    }
}

// ---------------------------------------------------------------------------
// the rest of the runtime surface
// ---------------------------------------------------------------------------

#[test]
fn every_turn_event_kind_the_binary_can_emit_is_documented() {
    let ledger = fs::read_to_string(parity_path()).expect("read docs/parity.md");
    for kind in [
        "assistant_delta",
        "tool_start",
        "tool_result",
        "final",
        "error",
    ] {
        assert!(
            ledger.contains(&format!("`{kind}`")),
            "the JSONL event `{kind}` has no row describing it"
        );
    }
}

#[test]
fn every_environment_override_the_binary_reads_is_documented() {
    let ledger = fs::read_to_string(parity_path()).expect("read docs/parity.md");
    for variable in [
        "FXR_MODEL",
        "FXR_PERMISSION_MODE",
        "FXR_MAX_AGENT_STEPS",
        "VERCEL_OIDC_TOKEN",
        "AI_GATEWAY_API_KEY",
    ] {
        assert!(
            ledger.contains(variable),
            "the environment variable {variable} is undocumented"
        );
    }
}

// ---------------------------------------------------------------------------
// the safety claims a reader acts on
// ---------------------------------------------------------------------------
//
// A row-by-row validator cannot see a promise, and a promise is what a user
// reads before pointing this at a repository they care about. These two tests
// exist because one of those promises was false: the README said no session
// event, snapshot, or tool result could carry a credential, while a
// `ToolResult` event stores whatever the model read -- so a secret fxr was
// asked to read was on disk in the session log at the moment the sentence
// denied it. The claim is now scoped to fxr's own Gateway token, and these
// tests are what stop the unscoped version from coming back in a new spelling.

/// The documents a user reads for safety claims, by repository-relative path.
fn safety_documents() -> Vec<(&'static str, String)> {
    ["README.md", "docs/parity.md", "docs/architecture.md"]
        .into_iter()
        .map(|name| {
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(name);
            (
                name,
                fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {name}: {err}")),
            )
        })
        .collect()
}

/// `text` with every run of whitespace collapsed to one space.
///
/// A claim that is true on one line and false when a paragraph is rewrapped is
/// not a claim anyone can check, so the check reads the prose the way a reader
/// does rather than the way the file is wrapped.
fn flow(text: &str) -> String {
    text.split_whitespace().collect::<Vec<&str>>().join(" ")
}

#[test]
fn no_document_claims_that_nothing_fxr_saves_can_carry_a_credential() {
    // Two checks, because a false claim can return either as the exact sentence
    // that was removed or as a fresh unscoped one.
    let banned = [
        "no session event, snapshot, or tool result can carry",
        "no variant of the event union can carry one",
        "credentials are never persisted",
    ];
    for (name, text) in safety_documents() {
        let flowed = flow(&text).to_lowercase();
        for claim in banned {
            assert!(
                !flowed.contains(claim),
                "{name} carries the unscoped credential claim {claim:?}"
            );
        }
        // The general rule behind those three: fxr may only promise that *its
        // own* credential is unsaved, so any sentence making the promise has to
        // name what it is about.
        for sentence in flowed.split(". ") {
            if sentence.contains("never persisted") || sentence.contains("not persisted") {
                assert!(
                    sentence.contains("gateway"),
                    "{name} promises something is never persisted without naming fxr's own \
                     Gateway credential, which is the only thing that is: {sentence:?}"
                );
            }
        }
    }
}

#[test]
fn the_readme_says_plainly_that_what_the_model_reads_is_saved() {
    let readme = fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("README.md"))
        .expect("read README.md");
    let start = readme
        .find("## Safety, in plain terms")
        .expect("the README has a Safety section");
    let section = &readme[start..];
    let end = section[1..]
        .find("\n## ")
        .map(|at| at + 1)
        .unwrap_or(section.len());
    let safety = flow(&section[..end]);

    // Where it goes, what it is, and how to not do it. A reader who is about to
    // have fxr read a file with a token in it has to learn all three here.
    for disclosure in [
        "~/.fxr/sessions/<id>/events.jsonl",
        "0600",
        "plaintext",
        "--no-save",
    ] {
        assert!(
            safety.contains(disclosure),
            "the README's Safety section does not disclose {disclosure:?}:\n{safety}"
        );
    }
}

#[test]
fn every_doctor_check_the_binary_emits_is_documented() {
    // Run the real binary: the check list is built at runtime from the
    // configuration, so the only honest inventory is the one it prints.
    let output = Command::new(env!("CARGO_BIN_EXE_fxr"))
        .args(["doctor", "--json"])
        .env_remove("VERCEL_OIDC_TOKEN")
        .env_remove("AI_GATEWAY_API_KEY")
        .output()
        .expect("spawn fxr doctor");
    let document: Value = serde_json::from_slice(&output.stdout).expect("doctor prints one JSON");
    let ledger = fs::read_to_string(parity_path()).expect("read docs/parity.md");
    let doctor_row = ledger
        .lines()
        .find(|line| line.starts_with("| `doctor` |"))
        .expect("the doctor row exists");
    for check in document["checks"].as_array().expect("checks array") {
        let name = check["name"].as_str().expect("a check has a name");
        assert!(
            doctor_row.contains(&format!("`{name}`")),
            "the doctor check `{name}` is not named in the ledger's doctor row"
        );
    }
}
