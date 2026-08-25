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

use serde_json::Value;
use xfx::cli::{help_text, parser_command_names, ADVERTISED_COMMANDS, ADVERTISED_ENTRYPOINTS};
use xfx::interactive::SLASH_COMMANDS;
use xfx::tools::{PermissionKind, Registry, ADVERTISED_TOOLS};

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
/// most dangerous drift lives: this file once said xfx "advertises the four
/// read-only tools below" while the table underneath listed eight, including
/// the three that rewrite files and the one that starts processes. Every row
/// was correct and the paragraph was a lie about what xfx is allowed to do.
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
        .expect("the Tools prose says how many tools xfx advertises");
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
        // advertising one would promise a session xfx cannot open.
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
    let help = xfx::interactive::help_text();
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
    let help = xfx::interactive::help_text();
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
        "XFX_MODEL",
        "XFX_PERMISSION_MODE",
        "XFX_MAX_AGENT_STEPS",
        "XFX_TUI",
        "XFX_THEME",
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
// reads before pointing this at a repository they care about. These tests exist
// because one of those promises was false: the README said no session event,
// snapshot, or tool result could carry a credential, while a `ToolResult` event
// stores whatever the model read -- so a secret xfx was asked to read was on
// disk in the session log at the moment the sentence denied it. The claim is
// now scoped to xfx's own Gateway token, and these tests are what stop the
// unscoped version from coming back in a new spelling, on a page with a smaller
// audience, or without the disclosure that makes the scoping honest.

/// Every page that makes the promise, by repository-relative path.
///
/// `CONTRIBUTING.md` is here because a contributor reads it as the rule they
/// must keep, the three module headers are here because `rustdoc` publishes
/// them -- the reader deciding whether to trust the session log lands on the
/// page that describes the log, not on the README -- and the design spec is
/// here because it is tracked, shipped in the repository, and is where the
/// claim was written down first. A claim that is scoped in one of these and
/// unscoped in another is the same lie with a smaller audience.
///
/// `src/output.rs` earns its place twice: it is where the snapshots a user
/// actually sees are built, and it made the same claim about *rendering* that
/// the session module made about *storage*.
fn safety_documents() -> Vec<(&'static str, String)> {
    [
        "README.md",
        "docs/parity.md",
        "docs/architecture.md",
        "CONTRIBUTING.md",
        "src/session/mod.rs",
        "src/session/event.rs",
        "src/output.rs",
        "docs/superpowers/specs/2026-08-21-xfx-rust-port-design.md",
    ]
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

/// Phrases that deny a whole *class* of recorded things can hold a secret.
///
/// These are the shapes the false claim actually took, not a style rule: "no
/// snapshot, session event, log line, or tool result may carry a credential"
/// was true of every event variant except the one that stores what a tool read,
/// which is the only one a user's own secret can reach. The same claim was also
/// written as "secrets never enter snapshots, logs, or tool output", which is
/// why both the "no X" and the "never Xs" spelling are listed: the first
/// version of this check caught only the former, and the design spec, which
/// says it in the second, walked straight through it.
const UNIVERSAL_DENIALS: [&str; 14] = [
    "no snapshot",
    "no session event",
    "no tool result",
    "no log line",
    "no variant",
    "none of its variants",
    "no credential",
    "no secret",
    "never enter",
    "never reach",
    "never appear",
    "never written",
    "never persisted",
    "not persisted",
];

/// Word stems a sentence uses when it is about *keeping* something, as opposed
/// to needing it or not needing it.
///
/// Stems rather than words because the claim is written in whatever tense suits
/// the paragraph, and the tense is not the point.
const RECORDING_WORDS: [&str; 12] = [
    "carr", "hold", "store", "persist", "record", "writ", "save", "contain", "enter", "reach",
    "appear", "leak",
];

/// Whether a sentence is promising secrecy for something xfx keeps.
///
/// Both halves are required. "the whole suite runs with no credential and no
/// network" names a credential and is true, because it is about what a test
/// needs rather than about what a record may hold; and a claim that project
/// context's bytes are not persisted is about bytes that are not a secret.
/// Judging either would only teach the next author to write around this test.
fn promises_secrecy_about_a_record(sentence: &str) -> bool {
    let names_a_secret = sentence.contains("credential") || sentence.contains("secret");
    let about_keeping = RECORDING_WORDS.iter().any(|stem| sentence.contains(stem));
    names_a_secret && about_keeping
}

/// The pages where the counterweight must be spelled out rather than linked: a
/// reader who finds the credential promise here has to find what *is* saved in
/// the same place.
const FULL_DISCLOSURE: [&str; 5] = [
    "README.md",
    "docs/parity.md",
    "CONTRIBUTING.md",
    "src/session/event.rs",
    "docs/superpowers/specs/2026-08-21-xfx-rust-port-design.md",
];

/// The published documentation of a page: all of a Markdown file, and only the
/// `//!` header plus the `///` item docs of a Rust one.
///
/// A claim lives in a doc comment; `use std::io::{self, Write};` is not one.
/// Reading a source file the way a reader reads prose -- whitespace collapsed,
/// sentences split on a full stop -- glued that import to the doc line under it
/// and invented a promise nobody had written, so the code is dropped before the
/// prose is judged.
fn documentation(name: &str, text: &str) -> String {
    if !name.ends_with(".rs") {
        return text.to_string();
    }
    text.lines()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            trimmed
                .strip_prefix("//!")
                .or_else(|| trimmed.strip_prefix("///"))
        })
        .collect::<Vec<&str>>()
        .join(" ")
}

/// The `//!` header of a Rust module: the page `rustdoc` publishes, and the
/// only part of a source file a reader of the documentation ever sees.
fn module_header(source: &str) -> String {
    source
        .lines()
        .take_while(|line| line.starts_with("//!") || line.trim().is_empty())
        .collect::<Vec<&str>>()
        .join(" ")
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
fn no_document_claims_that_nothing_xfx_saves_can_carry_a_credential() {
    // Two checks, because a false claim can return either as the exact sentence
    // that was removed or as a fresh unscoped one.
    let banned = [
        "no session event, snapshot, or tool result can carry",
        "no variant of the event union can carry one",
        "credentials are never persisted",
        "no snapshot, session event, log line, or tool result may carry a credential",
        "none of its variants can hold a secret",
        "no variant of [`sessionevent`] can hold a credential",
        "secrets never enter snapshots, logs, or tool output",
    ];
    for (name, text) in safety_documents() {
        let flowed = flow(&documentation(name, &text)).to_lowercase();
        for claim in banned {
            assert!(
                !flowed.contains(claim),
                "{name} carries the unscoped credential claim {claim:?}"
            );
        }
        // The general rule behind those: xfx may only promise that *its own*
        // credential is unsaved, so any sentence that denies a class of records
        // can hold a secret -- or that says a secret is not persisted -- has to
        // name what it is about. "xfx's own Gateway credential is never
        // persisted: no variant of the event union carries it" passes; the same
        // sentence with the scope removed does not.
        for sentence in flowed.split(". ") {
            if !promises_secrecy_about_a_record(sentence) {
                continue;
            }
            if !UNIVERSAL_DENIALS
                .iter()
                .any(|denial| sentence.contains(denial))
            {
                continue;
            }
            assert!(
                sentence.contains("gateway"),
                "{name} promises secrecy for a class of records without naming xfx's own \
                 Gateway credential, which is the only thing that is never written -- what a \
                 tool read is: {sentence:?}"
            );
        }
    }
}

#[test]
fn every_page_that_scopes_the_credential_promise_also_says_what_is_saved() {
    for (name, text) in safety_documents() {
        if !FULL_DISCLOSURE.contains(&name) {
            continue;
        }
        let flowed = flow(&documentation(name, &text)).to_lowercase();
        // What it is on disk, and how to not put it there. Scoping the promise
        // without these two is a narrower claim that still leaves the reader
        // believing the old one.
        for disclosure in ["plaintext", "--no-save"] {
            assert!(
                flowed.contains(disclosure),
                "{name} scopes the credential promise but never discloses {disclosure:?}"
            );
        }
        assert!(
            flowed.contains("tool_result")
                || flowed.contains("toolresult")
                || flowed.contains("tool result"),
            "{name} does not name the event that stores what the model read"
        );
    }

    // The session module header is a table of contents. A second copy of the
    // disclosure there would be a second thing to keep true, so it only has to
    // send the reader to the variant that owns it.
    let (_, module) = safety_documents()
        .into_iter()
        .find(|(name, _)| *name == "src/session/mod.rs")
        .expect("the session module header is a safety document");
    assert!(
        flow(&documentation("src/session/mod.rs", &module))
            .to_lowercase()
            .contains("toolresult"),
        "src/session/mod.rs promises secrecy without naming the variant that records what a \
         tool read"
    );

    // The renderer's page is where the same claim is made about display rather
    // than about storage, and `xfx session <id>` is what puts a recorded tool
    // result back on a terminal and into a JSON document. So the header has to
    // name the field that carries it and the bound that applies -- a reader who
    // only ever opens this module's docs still has to learn that a tool's
    // return is rendered, clipped rather than withheld.
    let (_, renderer) = safety_documents()
        .into_iter()
        .find(|(name, _)| *name == "src/output.rs")
        .expect("the renderer is a safety document");
    let header = module_header(&renderer);
    for disclosure in ["SessionStepRow::Tool", "MAX_DETAIL_TEXT_BYTES"] {
        assert!(
            header.contains(disclosure),
            "src/output.rs's module header does not say that {disclosure:?} is what carries a \
             tool's return into a snapshot:\n{header}"
        );
    }
}

// ---------------------------------------------------------------------------
// the provenance claim a redistributor acts on
// ---------------------------------------------------------------------------

/// The attribution shipped in every release archive.
fn notice() -> String {
    fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("NOTICE"))
        .expect("read NOTICE")
}

#[test]
fn the_notice_says_one_thing_about_provenance_and_it_is_the_true_one() {
    let flowed = flow(&notice());
    let lowered = flowed.to_lowercase();

    // xfx ships no upstream code, so the Apache boilerplate that announces
    // included software is not a formality here -- it contradicts UPSTREAM.md
    // and tells a redistributor to look for Zig sources that do not exist.
    for claim in [
        "includes software developed by",
        "incorporates software",
        "includes source",
        "contains source code",
        "portions of this work are derived from",
    ] {
        assert!(
            !lowered.contains(claim),
            "NOTICE claims xfx ships upstream software: {claim:?}"
        );
    }

    // What has to survive any rewrite: who upstream is, under what licence, at
    // which commit, that xfx is neither affiliated nor a copy, and that the
    // relationship is specification-to-reimplementation.
    for required in [
        "https://github.com/vercel-labs/fx",
        "580a0c5da9386317251968c09c1cee69e763487a",
        "Copyright 2025 Vercel, Inc.",
        "Apache License, Version 2.0",
        "not affiliated",
        "independent",
        "reimplementation",
        "specification",
        "No Zig source is copied",
    ] {
        assert!(
            flowed.contains(required),
            "NOTICE no longer states {required:?}"
        );
    }

    // The same sentence lives in UPSTREAM.md, which is what the NOTICE points a
    // reader at. If the two ever disagree the archive carries both.
    let upstream =
        fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("UPSTREAM.md"))
            .expect("read UPSTREAM.md");
    assert!(
        flow(&upstream).contains("No Zig source is copied"),
        "UPSTREAM.md and NOTICE disagree about whether upstream source was copied"
    );
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
    // have xfx read a file with a token in it has to learn all three here.
    for disclosure in [
        "~/.xfx/sessions/<id>/events.jsonl",
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
    let output = Command::new(env!("CARGO_BIN_EXE_xfx"))
        .args(["doctor", "--json"])
        .env_remove("VERCEL_OIDC_TOKEN")
        .env_remove("AI_GATEWAY_API_KEY")
        .output()
        .expect("spawn xfx doctor");
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

// ---------------------------------------------------------------------------
// one product name
// ---------------------------------------------------------------------------

#[test]
fn the_tracked_tree_carries_only_the_current_product_name() {
    // `scripts/check-xfx-identity.sh` is the machine form of the rule: the
    // tracked tree names this product `xfx` and upstream `fx`, and nothing
    // else. It is run here as well as in CI for the same reason the ledger is
    // reconciled in both directions -- a gate that only exists in a workflow is
    // a gate nobody runs before pushing, and the script proves itself awake on
    // its own controls before it reports on this repository.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let output = Command::new(root.join("scripts").join("check-xfx-identity.sh"))
        .current_dir(&root)
        .output()
        .expect("spawn scripts/check-xfx-identity.sh");
    assert!(
        output.status.success(),
        "the identity check failed:\n{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
