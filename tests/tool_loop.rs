//! The read-only tool registry, its executors, and the multi-step turn.
//!
//! Three promises are proven here, and each one is a product promise:
//!
//! 1. the registry is a closed, ordered set of four read-only tools whose
//!    schemas are closed and whose outputs are deterministic and bounded;
//! 2. no tool call ever reads outside the primary workspace or an explicitly
//!    configured additional root, symlinks included; and
//! 3. one model tool call becomes exactly one local execution and exactly one
//!    correlated tool result in the next Gateway request.
//!
//! Nothing here uses a real credential or a real endpoint. Upstream evidence is
//! pinned to `vercel-labs/fx@580a0c5da9386317251968c09c1cee69e763487a`.

mod support;

use std::collections::VecDeque;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use tempfile::TempDir;

use xfx::agent::tokens::Progress;
use xfx::agent::{run_turn, run_turn_saved, Steering, TurnError, TurnJournal, TurnRequest};
use xfx::config::PermissionMode;
use xfx::gateway::protocol::{
    Bill, Completion, CompletionRequest, ContentPart, Deferred, FinishReason, Message,
    ResultStatus, Role, ToolCall, ToolChoice, Usage,
};
use xfx::gateway::{CancelToken, DeltaSink, Provider, ProviderError};
use xfx::llmux;
use xfx::output::{Event, RecordingSink};
use xfx::permission::{
    ApprovalAnswer, ApprovalDiff, ApprovalPrompter, ApprovalRequest, ApprovalResponse,
    PermissionSession,
};
use xfx::provider::Wire;
use xfx::session::store::DurableState;
use xfx::session::{
    LineCounts, NewSession, SessionEvent, SessionId, SessionStore, TerminalReason, ToolEnding,
    TurnConclusion,
};
use xfx::tools::{Registry, ToolContext, ToolLimits, ToolResult, ADVERTISED_TOOLS};
use xfx::workspace::{AccessScope, PathError, ProjectContext};

use support::fake_gateway::{
    content_only, finish, sse_body, text_delta, tool_call, FakeGateway, Reply,
};

/// Environment variables that must never leak in from the developer's shell.
const CONTROLLED_VARS: &[&str] = &[
    "VERCEL_OIDC_TOKEN",
    "AI_GATEWAY_API_KEY",
    "XFX_MODEL",
    "XFX_PERMISSION_MODE",
    "XFX_MAX_AGENT_STEPS",
    "XFX_GATEWAY_URL",
];

/// A test secret that must never appear on stdout or stderr.
const TEST_KEY: &str = "xfx-test-tool-key-must-not-appear";

// ---------------------------------------------------------------------------
// fixtures
// ---------------------------------------------------------------------------

/// A temporary directory tree that tool calls are pointed at.
struct Tree {
    /// Held so the tree outlives the test body; never read directly.
    _dir: TempDir,
    root: PathBuf,
}

impl Tree {
    fn new() -> Self {
        let dir = TempDir::new().expect("create a temporary tree");
        let root = dir.path().canonicalize().expect("canonicalize the tree");
        Self { _dir: dir, root }
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn write(&self, relative: &str, contents: &str) -> PathBuf {
        self.write_bytes(relative, contents.as_bytes())
    }

    fn write_bytes(&self, relative: &str, contents: &[u8]) -> PathBuf {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent directories");
        }
        fs::write(&path, contents).expect("write the fixture file");
        path
    }

    fn mkdir(&self, relative: &str) -> PathBuf {
        let path = self.root.join(relative);
        fs::create_dir_all(&path).expect("create the fixture directory");
        path
    }

    /// A named pipe that no one ever writes to: opening it for reading blocks
    /// until a writer arrives, which in these tests is never.
    ///
    /// `mkfifo(1)` rather than a crate call: rustix does not expose `mkfifoat`
    /// on Apple targets, and the fixture has to exist on every unix the tests
    /// run on.
    fn mkfifo(&self, relative: &str) -> PathBuf {
        let path = self.root.join(relative);
        let status = Command::new("mkfifo")
            .arg(&path)
            .status()
            .expect("run mkfifo");
        assert!(status.success(), "mkfifo failed: {status:?}");
        path
    }
}

fn context(tree: &Tree) -> ToolContext {
    ToolContext::new(AccessScope::primary_only(tree.root()).expect("a usable primary root"))
}

fn context_with(tree: &Tree, additional: &[&Path]) -> ToolContext {
    ToolContext::new(AccessScope::new(tree.root(), additional).expect("usable roots"))
}

/// Runs one tool call the way a turn would, and requires it to be advertised.
fn call(context: &ToolContext, tool: &str, input: Value) -> ToolResult {
    Registry::builtin()
        .execute(
            &ToolCall {
                id: "call-1".to_string(),
                name: tool.to_string(),
                input,
            },
            context,
        )
        .expect("the tool is advertised")
}

fn succeeds(context: &ToolContext, tool: &str, input: Value) -> String {
    let result = call(context, tool, input);
    assert!(result.ok, "expected success, got {result:?}");
    result.output
}

fn fails(context: &ToolContext, tool: &str, input: Value) -> String {
    let result = call(context, tool, input);
    assert!(!result.ok, "expected a refusal, got {result:?}");
    result.output
}

/// Runs one tool call off the test thread and requires an answer within two
/// seconds.
///
/// A tool that opens a writer-less FIFO never returns, and a turn that never
/// returns is the defect these callers are pinning. Waiting on a channel turns
/// that into a failure with a name instead of a suite that hangs.
fn answers_within_two_seconds(root: &Path, tool: &'static str, input: Value) -> ToolResult {
    let (sender, receiver) = mpsc::channel();
    let root = root.to_path_buf();
    std::thread::spawn(move || {
        let scope = AccessScope::primary_only(&root).expect("a usable primary root");
        let context = ToolContext::new(scope);
        let _ = sender.send(call(&context, tool, input));
    });
    match receiver.recv_timeout(std::time::Duration::from_secs(2)) {
        Ok(result) => result,
        // The worker is left parked on the open it will never return from. That
        // leak is deliberate: a blocking `open` cannot be interrupted from here,
        // and the process is about to fail this test and exit anyway.
        Err(RecvTimeoutError::Timeout) => panic!("{tool} blocked on a FIFO"),
        // A dropped sender means the worker panicked or vanished. Calling that a
        // block would send the next reader after a defect that is not there.
        Err(RecvTimeoutError::Disconnected) => {
            panic!("the {tool} worker disappeared without answering")
        }
    }
}

// ---------------------------------------------------------------------------
// the registry and its advertisement
// ---------------------------------------------------------------------------

#[test]
fn the_read_tools_come_first_in_upstream_order_and_the_set_stays_closed() {
    // The order is upstream's (`src/builtins/tools.zig:1352-1355`), and the set
    // is closed: a name here that is not in `docs/parity.md` as `implemented`
    // would be a promise this build cannot keep.
    let names = Registry::builtin().names();
    assert_eq!(
        &names[..4],
        ["list_files", "glob_files", "grep_files", "read_file"]
    );
    assert_eq!(ADVERTISED_TOOLS, names);
}

/// Asserts `property` (named `name`, inside `schema`, shown whole on failure)
/// is either a bounded scalar or a bounded array of closed objects, and
/// recurses into an array's item properties so nesting is checked all the
/// way down (`ask_user_question`'s `questions[].options[]` is two levels).
fn assert_bounded_property(schema: &Value, name: &str, property: &Value) {
    assert!(
        property["description"]
            .as_str()
            .is_some_and(|d| !d.is_empty()),
        "`{name}` in {schema} has no description"
    );
    match property["type"].as_str() {
        Some("string" | "integer" | "boolean") => {}
        Some("array") => assert_bounded_array_of_closed_objects(schema, name, property),
        other => panic!("`{name}` in {schema} has an unbounded type: {other:?}"),
    }
}

/// Asserts `property` is a JSON-Schema array with a finite, positive,
/// well-ordered size bound (`minItems <= maxItems`, `maxItems > 0`) whose
/// items are a closed, non-empty object -- then recurses into that object's
/// own properties with the same rule.
fn assert_bounded_array_of_closed_objects(schema: &Value, name: &str, property: &Value) {
    let min_items = property["minItems"]
        .as_u64()
        .unwrap_or_else(|| panic!("`{name}` in {schema} has no integer minItems"));
    let max_items = property["maxItems"]
        .as_u64()
        .unwrap_or_else(|| panic!("`{name}` in {schema} has no finite integer maxItems"));
    assert!(
        max_items > 0,
        "`{name}` in {schema} has a non-positive maxItems"
    );
    assert!(
        min_items <= max_items,
        "`{name}` in {schema} has minItems ({min_items}) > maxItems ({max_items})"
    );

    let items = &property["items"];
    assert_eq!(
        items["type"], "object",
        "`{name}` in {schema} has array items that are not an object"
    );
    // Closed for the same reason the top-level schema is: an item field the
    // model invents should be reported, not silently accepted.
    assert_eq!(
        items["additionalProperties"],
        json!(false),
        "`{name}` in {schema} has array items that are not closed"
    );
    let item_properties = items["properties"]
        .as_object()
        .unwrap_or_else(|| panic!("`{name}` in {schema} has array items with no properties"));
    assert!(
        !item_properties.is_empty(),
        "`{name}` in {schema} has array items with no properties"
    );
    for (item_name, item_property) in item_properties {
        assert_bounded_property(schema, item_name, item_property);
    }
    for required in items["required"].as_array().unwrap_or(&Vec::new()) {
        let required_name = required.as_str().expect("a required name is a string");
        assert!(
            item_properties.contains_key(required_name),
            "`{required_name}` is required by `{name}`'s items in {schema} but is not a property"
        );
    }
}

#[test]
fn the_advertisement_carries_one_closed_schema_per_tool_in_registry_order() {
    let advertisement = Registry::builtin().advertisement();
    assert_eq!(advertisement.len(), ADVERTISED_TOOLS.len());

    for (schema, expected) in advertisement.iter().zip(ADVERTISED_TOOLS) {
        assert_eq!(schema["type"], "function", "{schema}");
        assert_eq!(schema["name"], *expected, "{schema}");
        let description = schema["description"]
            .as_str()
            .unwrap_or_else(|| panic!("{schema} has no description"));
        assert!(!description.is_empty(), "{schema}");

        let input = &schema["inputSchema"];
        assert_eq!(input["type"], "object", "{schema}");
        // Closed: the model may not invent a field, so a typo is reported by
        // the Gateway rather than silently ignored by xfx.
        assert_eq!(input["additionalProperties"], json!(false), "{schema}");

        let properties = input["properties"]
            .as_object()
            .unwrap_or_else(|| panic!("{schema} has no properties"));
        assert!(!properties.is_empty(), "{schema}");
        for (name, property) in properties {
            assert_bounded_property(schema, name, property);
        }
        for required in input["required"].as_array().unwrap_or(&Vec::new()) {
            let name = required.as_str().expect("a required name is a string");
            assert!(
                properties.contains_key(name),
                "`{name}` is required by {schema} but is not a property"
            );
        }
    }

    // Explicit ninth-schema assertion: `ask_user_question`'s bounds and
    // required labels, not just "some bound exists" (the loop above already
    // proved that generically).
    let ask_user_question = advertisement
        .iter()
        .find(|schema| schema["name"] == "ask_user_question")
        .expect("`ask_user_question` is advertised");
    assert_eq!(
        ask_user_question["inputSchema"]["required"],
        json!(["questions"]),
        "{ask_user_question}"
    );
    let questions = &ask_user_question["inputSchema"]["properties"]["questions"];
    assert_eq!(questions["minItems"], json!(1), "{questions}");
    assert_eq!(questions["maxItems"], json!(4), "{questions}");
    let question_item = &questions["items"];
    assert_eq!(
        question_item["required"],
        json!(["question", "options"]),
        "{question_item}"
    );
    let options = &question_item["properties"]["options"];
    assert_eq!(options["minItems"], json!(2), "{options}");
    assert_eq!(options["maxItems"], json!(6), "{options}");
    let option_item = &options["items"];
    assert_eq!(option_item["required"], json!(["label"]), "{option_item}");
}

#[test]
fn the_advertisement_names_no_deferred_tool() {
    // Advertisement is a promise. Every name below is `deferred` in
    // `docs/parity.md`, so it must not reach a model schema.
    let rendered = serde_json::to_string(&Registry::builtin().advertisement()).unwrap();
    for deferred in [
        "delete_file",
        "rename_file",
        "copy_file",
        "file_info",
        "open_file",
        "web_fetch",
        "web_search",
        "memory",
        "semantic_search",
        "skill",
        "install_skill",
        "subagent",
        "mcp_search_tools",
        "mcp_select_tool",
        "mcp_features",
        "vision",
        "read_tool_result",
    ] {
        assert!(
            !rendered.contains(deferred),
            "the advertisement names the deferred `{deferred}` tool"
        );
    }
}

#[test]
fn every_advertised_tool_has_an_implemented_parity_row() {
    let parity = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/parity.md"))
        .expect("read the parity ledger");
    for name in ADVERTISED_TOOLS {
        let row = format!("| `{name}` | tool | implemented |");
        assert!(
            parity.contains(&row),
            "docs/parity.md has no implemented row for `{name}`"
        );
    }
}

#[test]
fn a_call_naming_a_tool_the_registry_does_not_have_is_not_executed() {
    let tree = Tree::new();
    let context = context(&tree);
    let unadvertised = Registry::builtin().execute(
        &ToolCall {
            id: "c1".to_string(),
            name: "delete_file".to_string(),
            input: json!({ "path": "x" }),
        },
        &context,
    );
    let err = unadvertised.expect_err("an unadvertised tool has no executor");
    assert_eq!(err.name, "delete_file");
}

// ---------------------------------------------------------------------------
// canonical paths and the access scope
// ---------------------------------------------------------------------------

#[test]
fn a_root_that_is_not_a_directory_is_refused_when_the_scope_is_built() {
    let tree = Tree::new();
    tree.write("file.txt", "x");
    assert!(matches!(
        AccessScope::primary_only(tree.root().join("file.txt")),
        Err(PathError::RootUnavailable { .. })
    ));
    assert!(matches!(
        AccessScope::primary_only(tree.root().join("missing")),
        Err(PathError::RootUnavailable { .. })
    ));
}

#[test]
fn a_workspace_relative_path_resolves_inside_the_primary_root() {
    let tree = Tree::new();
    tree.write("src/main.rs", "fn main() {}\n");
    let scope = AccessScope::primary_only(tree.root()).unwrap();
    let resolved = scope.resolve_existing("src/main.rs").expect("resolves");
    assert_eq!(resolved.absolute(), tree.root().join("src/main.rs"));
    assert_eq!(resolved.root(), tree.root());
    assert_eq!(scope.display_path(resolved.absolute()), "src/main.rs");
}

#[test]
fn an_absolute_path_outside_every_root_is_refused_before_it_is_read() {
    let tree = Tree::new();
    let outside = Tree::new();
    let secret = outside.write("secret.txt", "SENSITIVE-OUTSIDE-VALUE\n");

    let scope = AccessScope::primary_only(tree.root()).unwrap();
    assert!(matches!(
        scope.resolve_existing(secret.to_str().unwrap()),
        Err(PathError::OutsideScope { .. })
    ));

    let refusal = fails(
        &context(&tree),
        "read_file",
        json!({ "path": secret.to_str().unwrap() }),
    );
    assert!(
        !refusal.contains("SENSITIVE-OUTSIDE-VALUE"),
        "the refusal leaked the file it refused: {refusal}"
    );
}

#[test]
fn a_relative_escape_above_the_workspace_is_refused() {
    let tree = Tree::new();
    let nested = tree.mkdir("project");
    let scope = AccessScope::primary_only(&nested).unwrap();
    tree.write("outside.txt", "OUTSIDE\n");
    assert!(matches!(
        scope.resolve_existing("../outside.txt"),
        Err(PathError::OutsideScope { .. })
    ));
}

#[test]
fn a_symlink_that_escapes_the_workspace_is_refused_and_never_read() {
    let tree = Tree::new();
    let outside = Tree::new();
    let secret = outside.write("secret.txt", "SENSITIVE-OUTSIDE-VALUE\n");
    symlink(&secret, tree.root().join("escape.txt")).expect("create the escaping symlink");

    let scope = AccessScope::primary_only(tree.root()).unwrap();
    assert!(
        matches!(
            scope.resolve_existing("escape.txt"),
            Err(PathError::OutsideScope { .. })
        ),
        "a symlink target outside the scope must fail containment, not be read"
    );

    let refusal = fails(
        &context(&tree),
        "read_file",
        json!({ "path": "escape.txt" }),
    );
    assert!(
        !refusal.contains("SENSITIVE-OUTSIDE-VALUE"),
        "the refusal leaked the symlink target: {refusal}"
    );

    // The same escape through a directory symlink.
    symlink(outside.root(), tree.root().join("escape-dir")).expect("create a directory symlink");
    let listed = fails(
        &context(&tree),
        "list_files",
        json!({ "path": "escape-dir" }),
    );
    assert!(!listed.contains("secret.txt"), "{listed}");
}

#[test]
fn a_symlink_that_stays_inside_the_workspace_is_followed() {
    let tree = Tree::new();
    tree.write("real/target.txt", "inside\n");
    symlink(
        tree.root().join("real/target.txt"),
        tree.root().join("link.txt"),
    )
    .expect("create an internal symlink");
    let output = succeeds(&context(&tree), "read_file", json!({ "path": "link.txt" }));
    assert!(output.contains("inside"), "{output}");
}

#[test]
fn an_explicitly_configured_additional_root_is_readable_and_its_sibling_is_not() {
    let tree = Tree::new();
    let shared = Tree::new();
    let allowed = shared.write("notes.md", "shared note\n");
    let sibling = Tree::new();
    let denied = sibling.write("private.md", "private note\n");

    let context = context_with(&tree, &[shared.root()]);
    let output = succeeds(
        &context,
        "read_file",
        json!({ "path": allowed.to_str().unwrap() }),
    );
    assert!(output.contains("shared note"), "{output}");

    let refusal = fails(
        &context,
        "read_file",
        json!({ "path": denied.to_str().unwrap() }),
    );
    assert!(!refusal.contains("private note"), "{refusal}");

    // A path in an additional root is shown absolutely, because it has no
    // meaningful position relative to the primary workspace.
    assert!(output.contains(allowed.to_str().unwrap()), "{output}");
}

#[test]
fn an_empty_or_blank_path_is_refused() {
    let tree = Tree::new();
    let scope = AccessScope::primary_only(tree.root()).unwrap();
    for blank in ["", "   ", "\t\n"] {
        assert!(matches!(
            scope.resolve_existing(blank),
            Err(PathError::Empty)
        ));
    }
}

// ---------------------------------------------------------------------------
// read_file
// ---------------------------------------------------------------------------

#[test]
fn read_file_numbers_every_line_and_adds_no_sentinel_when_it_showed_them_all() {
    let tree = Tree::new();
    tree.write("small.txt", "alpha\nbeta\ngamma\n");
    let output = succeeds(&context(&tree), "read_file", json!({ "path": "small.txt" }));
    assert_eq!(
        output,
        "<path>small.txt</path>\n<content>\n1\talpha\n2\tbeta\n3\tgamma\n</content>"
    );
}

#[test]
fn read_file_shows_four_hundred_lines_by_default_and_says_how_many_it_showed() {
    let tree = Tree::new();
    let body: String = (1..=500).map(|n| format!("line {n}\n")).collect();
    tree.write("long.txt", &body);
    let output = succeeds(&context(&tree), "read_file", json!({ "path": "long.txt" }));
    assert!(output.contains("\n400\tline 400\n"), "{output}");
    assert!(!output.contains("line 401"), "{output}");
    assert!(
        output.contains("... [showing 400 of 500 lines; use start_line/line_count to read more.]"),
        "{output}"
    );
}

#[test]
fn read_file_honors_an_explicit_line_range() {
    let tree = Tree::new();
    tree.write("range.txt", "one\ntwo\nthree\nfour\n");
    let output = succeeds(
        &context(&tree),
        "read_file",
        json!({ "path": "range.txt", "start_line": 2, "line_count": 2 }),
    );
    assert!(output.contains("2\ttwo\n3\tthree\n"), "{output}");
    assert!(!output.contains("one"), "{output}");
    assert!(output.contains("... [showing 2 of 4 lines"), "{output}");
}

#[test]
fn read_file_says_when_the_requested_start_is_past_the_end_of_the_file() {
    let tree = Tree::new();
    tree.write("short.txt", "only\n");
    let output = succeeds(
        &context(&tree),
        "read_file",
        json!({ "path": "short.txt", "start_line": 99 }),
    );
    assert!(
        output.contains("... [start_line 99 is beyond end of file; total lines 1]"),
        "{output}"
    );
}

#[test]
fn read_file_clips_an_overlong_line_and_says_it_did() {
    let tree = Tree::new();
    tree.write("wide.txt", &format!("{}\nshort\n", "x".repeat(5_000)));
    let output = succeeds(&context(&tree), "read_file", json!({ "path": "wide.txt" }));
    assert!(output.contains("... (line truncated)"), "{output}");
    assert!(
        !output.contains(&"x".repeat(2_001)),
        "a line was rendered past the 2000-byte clip"
    );
}

#[test]
fn read_file_names_a_binary_file_instead_of_dumping_its_bytes() {
    let tree = Tree::new();
    tree.write_bytes("blob.bin", &[0x00, 0xff, 0xfe, b'a', 0x00]);
    let output = succeeds(&context(&tree), "read_file", json!({ "path": "blob.bin" }));
    assert_eq!(
        output,
        "<path>blob.bin</path>\n<content>binary or non-utf8 file omitted (5 bytes)</content>"
    );
}

#[test]
fn read_file_preserves_multibyte_text() {
    let tree = Tree::new();
    tree.write("utf8.txt", "한국어 텍스트\nemoji 🌈\n");
    let output = succeeds(&context(&tree), "read_file", json!({ "path": "utf8.txt" }));
    assert!(output.contains("한국어 텍스트"), "{output}");
    assert!(output.contains("emoji 🌈"), "{output}");
}

#[test]
fn read_file_reports_a_missing_path_as_a_failed_result_rather_than_a_dead_turn() {
    let tree = Tree::new();
    let result = call(&context(&tree), "read_file", json!({ "path": "nope.txt" }));
    assert!(!result.ok);
    assert!(result.output.contains("nope.txt"), "{result:?}");
}

#[test]
fn read_file_rejects_arguments_that_do_not_match_its_schema() {
    let tree = Tree::new();
    let context = context(&tree);
    for bad in [
        json!({}),
        json!({ "path": 7 }),
        json!({ "path": "" }),
        json!({ "path": "x.txt", "start_line": 0 }),
        json!({ "path": "x.txt", "line_count": 0 }),
        json!({ "path": "x.txt", "line_count": "many" }),
        json!(["path"]),
    ] {
        let result = call(&context, "read_file", bad.clone());
        assert!(!result.ok, "{bad} was accepted: {result:?}");
        assert!(
            result.output.contains("read_file"),
            "{bad} produced {result:?}"
        );
    }
}

/// `read_file`'s refusals in upstream's words: its decoder's
/// (`vercel-labs/fx@c1db9191 src/tools/filesystem/read_file.zig:44-52, 85`),
/// its validation's (`:101`), and its permission preflight's for a path that
/// does not resolve (`permissionTargetResolutionFailureMessage`,
/// `src/core/tooling/tool_admission.zig:2465-2495`) -- the sentence the row
/// shows after `Failed missing.txt:` (`fx-resume-tools-40x120-live-ctrl-o`).
#[test]
fn read_file_refuses_in_upstreams_words() {
    let tree = Tree::new();
    let context = context(&tree);
    let outside = TempDir::new().expect("outside");
    fs::write(outside.path().join("o.txt"), "o\n").expect("write");
    let outside_path = outside.path().join("o.txt").display().to_string();
    for (input, said) in [
        (
            json!({ "path": "missing.txt" }),
            "Path not found: missing.txt".to_string(),
        ),
        // The argument as the model wrote it, spaces and all
        // (`fx-read-failures-40x120-bodies/003-completion.json`).
        (
            json!({ "path": "  missing.txt  " }),
            "Path not found:   missing.txt  ".to_string(),
        ),
        (
            json!({ "path": outside_path }),
            format!("Path is outside the workspace: {outside_path}"),
        ),
        (
            json!(["path"]),
            "read_file arguments must be an object".to_string(),
        ),
        (
            json!({}),
            "read_file requires string field \"path\"".to_string(),
        ),
        (
            json!({ "path": 7 }),
            "read_file field \"path\" must be a string".to_string(),
        ),
        (
            json!({ "path": "  " }),
            "read_file field \"path\" must not be empty".to_string(),
        ),
        (
            json!({ "path": "x.txt", "start_line": 0 }),
            "read_file field \"start_line\" must be a positive integer".to_string(),
        ),
        (
            json!({ "path": "x.txt", "line_count": "many" }),
            "read_file field \"line_count\" must be a positive integer".to_string(),
        ),
    ] {
        let result = call(&context, "read_file", input.clone());
        assert!(!result.ok, "{input}: {result:?}");
        assert_eq!(result.output, said, "{input}");
    }
}

/// A read that resolved and then could not be done is upstream's
/// `tool_execution_failed` envelope, as its model is sent it
/// (`readFileFailure`, `read_file.zig:184-216`; `toolExecutionFailureJson`,
/// `tool_result_errors.zig:323-347`; `fx-read-failures-40x120-bodies/
/// 003-completion.json`), and its row says no more than `Failed src`
/// (`failureStatusDetail`, `tool_presentation.zig:1253-1301`: an in-call
/// failure of a built-in tool has no status detail).
#[test]
fn a_read_that_resolved_and_failed_is_upstreams_envelope_and_its_row_says_nothing_more() {
    let tree = Tree::new();
    fs::create_dir(tree.root().join("src")).expect("a directory");
    let result = call(&context(&tree), "read_file", json!({ "path": "src" }));
    assert!(!result.ok);
    let absolute = fs::canonicalize(tree.root().join("src")).expect("canonical");
    assert_eq!(
        result.output,
        format!(
            r#"{{"error":{{"type":"tool_execution_failed","tool_name":"read_file","message":"read_file requires a regular file","details":{{"field":"path","path":{},"error":"NotRegularFile"}},"suggestion":"Use glob_files to inspect directory contents, then choose a regular file."}}}}"#,
            serde_json::to_string(&absolute.display().to_string()).expect("a string")
        )
    );
    assert_eq!(&*result.detail, "", "the row was given a reason");
}

/// The search tools' and `list_files`'s roots that do not resolve, in the
/// preflight's words (`tool_admission.zig:2465-2495`;
/// `fx-read-failures-40x120-live`: `Failed *.rs: Path not found: nope`).
#[test]
fn a_search_root_that_does_not_resolve_is_upstreams_preflight_sentence() {
    let tree = Tree::new();
    let context = context(&tree);
    for (tool, input) in [
        ("glob_files", json!({ "pattern": "*.rs", "path": "nope" })),
        ("grep_files", json!({ "pattern": "alpha", "path": "nope" })),
        ("list_files", json!({ "path": "nope" })),
    ] {
        let result = call(&context, tool, input);
        assert!(!result.ok, "{tool}");
        assert_eq!(result.output, "Path not found: nope", "{tool}");
        assert_eq!(&*result.detail, "Path not found: nope", "{tool}");
    }
}

/// A search pointed at a file searches that file, as upstream's does
/// (`glob_files.zig:189-198`, `grep_files.zig:278-286`); one pointed at a
/// FIFO is `grep_files`'s plain refusal (`:287`); and a root the system will
/// not open is its envelope or sentence, none of them with a reason on the
/// row.
#[cfg(unix)]
#[test]
fn a_search_over_a_file_searches_it_and_one_the_system_refuses_is_upstreams_failure() {
    use std::os::unix::fs::PermissionsExt;
    let tree = Tree::new();
    tree.write("notes.txt", "alpha one\nbeta\nalpha two\n");
    let context = context(&tree);
    let globbed = call(
        &context,
        "glob_files",
        json!({ "pattern": "*.txt", "path": "notes.txt" }),
    );
    assert!(globbed.ok, "{globbed:?}");
    assert_eq!(globbed.output, "[glob] 1 matches for *.txt\n - notes.txt\n");
    let missed = call(
        &context,
        "glob_files",
        json!({ "pattern": "*.rs", "path": "notes.txt" }),
    );
    assert_eq!(missed.output, "[glob] no matches for *.rs\n");
    let grepped = call(
        &context,
        "grep_files",
        json!({ "pattern": "alpha", "path": "notes.txt" }),
    );
    assert!(grepped.ok, "{grepped:?}");
    assert!(
        grepped.output.contains("notes.txt:1: alpha one"),
        "{}",
        grepped.output
    );
    assert!(
        grepped.output.contains("notes.txt:3: alpha two"),
        "{}",
        grepped.output
    );

    tree.mkfifo("pipe");
    let piped = call(
        &context,
        "grep_files",
        json!({ "pattern": "x", "path": "pipe" }),
    );
    let absolute = fs::canonicalize(tree.root().join("pipe")).expect("canonical");
    assert_eq!(
        piped.output,
        format!("Not a regular file or directory: {}", absolute.display())
    );
    assert_eq!(&*piped.detail, "");

    // A file the system will not open (not as root, whom nothing refuses).
    if unsafe { libc::geteuid() } != 0 {
        let locked = tree.root().join("locked.txt");
        fs::write(&locked, "alpha\n").expect("write");
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).expect("chmod");
        let refused = call(
            &context,
            "grep_files",
            json!({ "pattern": "alpha", "path": "locked.txt" }),
        );
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o600)).expect("chmod");
        let path = fs::canonicalize(&locked).expect("canonical");
        assert!(
            refused.output.starts_with(&format!(
                r#"{{"error":{{"type":"tool_execution_failed","tool_name":"grep_files","message":"Operating system denied filesystem access","details":{{"path":{},"error":"AccessDenied"}}"#,
                serde_json::to_string(&path.display().to_string()).expect("a string")
            )),
            "{}",
            refused.output
        );
        assert_eq!(&*refused.detail, "");
    }
}

#[cfg(unix)]
#[test]
fn read_file_refuses_a_fifo_rather_than_parking_the_turn_on_it() {
    // A FIFO is not a directory, so the directory refusal lets it through, and
    // with no writer on the other end `fs::read` never returns: a tool call
    // that never returns is a turn the user cannot get out of.
    let tree = Tree::new();
    tree.mkfifo("pipe");

    let result = answers_within_two_seconds(tree.root(), "read_file", json!({ "path": "pipe" }));

    assert!(!result.ok, "expected a refusal, got {result:?}");
    assert!(result.output.contains("pipe"), "{result:?}");
    // Upstream's refusal of anything but a regular file
    // (`read_file.zig:189-200`).
    assert!(
        result
            .output
            .contains(r#""message":"read_file requires a regular file""#),
        "{result:?}"
    );
}

// ---------------------------------------------------------------------------
// list_files
// ---------------------------------------------------------------------------

#[test]
fn list_files_sorts_entries_and_marks_directories_and_symlinks() {
    let tree = Tree::new();
    tree.write("beta.txt", "b");
    tree.write("alpha.txt", "a");
    tree.mkdir("zeta");
    symlink(tree.root().join("alpha.txt"), tree.root().join("mid.txt")).expect("symlink");

    let output = succeeds(&context(&tree), "list_files", json!({}));
    assert_eq!(output, ".:\n- alpha.txt\n- beta.txt\n- mid.txt@\n- zeta/\n");
}

#[test]
fn list_files_omits_the_ignored_directory_names() {
    let tree = Tree::new();
    tree.write("kept.txt", "k");
    for ignored in [".git", "node_modules", "dist", "coverage"] {
        tree.mkdir(ignored);
    }
    let output = succeeds(&context(&tree), "list_files", json!({ "path": "." }));
    assert_eq!(output, ".:\n- kept.txt\n");
}

#[test]
fn list_files_reports_an_empty_directory_as_empty() {
    let tree = Tree::new();
    tree.mkdir("hollow");
    let output = succeeds(&context(&tree), "list_files", json!({ "path": "hollow" }));
    assert_eq!(output, "hollow:\n(empty)\n");
}

#[test]
fn list_files_caps_its_entries_and_says_that_it_did() {
    let tree = Tree::new();
    for index in 0..150 {
        tree.write(&format!("f{index:03}.txt"), "x");
    }
    let output = succeeds(&context(&tree), "list_files", json!({}));
    assert_eq!(output.lines().filter(|l| l.starts_with("- ")).count(), 100);
    assert!(
        output.contains("... and more entries (showing first 100)"),
        "{output}"
    );
    // Deterministic: the cap keeps the first 100 in sorted order, not 100
    // arbitrary names.
    assert!(output.contains("- f000.txt\n"), "{output}");
    assert!(output.contains("- f099.txt\n"), "{output}");
    assert!(!output.contains("- f100.txt\n"), "{output}");
}

#[test]
fn list_files_reports_a_missing_directory_as_a_failed_result() {
    let tree = Tree::new();
    let result = call(&context(&tree), "list_files", json!({ "path": "gone" }));
    assert!(!result.ok);
    assert!(result.output.contains("gone"), "{result:?}");
}

#[cfg(unix)]
#[test]
fn list_files_names_a_fifo_without_ever_opening_it() {
    // Listing reports what `readdir` and `lstat` already know, so a named pipe
    // is a name like any other and nothing about it is opened. The bounded wait
    // is what keeps that a fact rather than an assumption.
    let tree = Tree::new();
    tree.write("a.txt", "a");
    tree.mkfifo("pipe");

    let result = answers_within_two_seconds(tree.root(), "list_files", json!({}));

    assert!(result.ok, "expected a listing, got {result:?}");
    assert_eq!(result.output, ".:\n- a.txt\n- pipe\n");
}

// ---------------------------------------------------------------------------
// glob_files
// ---------------------------------------------------------------------------

#[test]
fn glob_files_matches_recursively_and_returns_sorted_workspace_relative_paths() {
    let tree = Tree::new();
    tree.write("src/b.rs", "");
    tree.write("src/a.rs", "");
    tree.write("src/nested/c.rs", "");
    tree.write("README.md", "");
    let output = succeeds(
        &context(&tree),
        "glob_files",
        json!({ "pattern": "src/**/*.rs" }),
    );
    assert_eq!(
        output,
        "[glob] 3 matches for src/**/*.rs\n - src/a.rs\n - src/b.rs\n - src/nested/c.rs\n"
    );
}

#[test]
fn glob_files_counts_without_listing_when_asked() {
    let tree = Tree::new();
    tree.write("a.md", "");
    tree.write("b.md", "");
    let output = succeeds(
        &context(&tree),
        "glob_files",
        json!({ "pattern": "*.md", "mode": "count" }),
    );
    assert_eq!(output, "[glob] count 2 matches for *.md\n");
}

#[test]
fn glob_files_says_so_when_nothing_matches() {
    let tree = Tree::new();
    tree.write("a.md", "");
    let output = succeeds(&context(&tree), "glob_files", json!({ "pattern": "*.zig" }));
    assert_eq!(output, "[glob] no matches for *.zig\n");
}

#[test]
fn glob_files_skips_ignored_directories() {
    let tree = Tree::new();
    tree.write("keep.rs", "");
    tree.write("node_modules/skip.rs", "");
    tree.write(".git/skip.rs", "");
    let output = succeeds(
        &context(&tree),
        "glob_files",
        json!({ "pattern": "**/*.rs" }),
    );
    assert_eq!(output, "[glob] 1 matches for **/*.rs\n - keep.rs\n");
}

#[test]
fn glob_files_can_be_narrowed_to_a_subdirectory() {
    let tree = Tree::new();
    tree.write("src/a.rs", "");
    tree.write("tests/b.rs", "");
    let output = succeeds(
        &context(&tree),
        "glob_files",
        json!({ "pattern": "*.rs", "path": "tests" }),
    );
    assert_eq!(output, "[glob] 1 matches for *.rs\n - tests/b.rs\n");
}

#[test]
fn glob_files_caps_its_matches_and_says_that_it_did() {
    let tree = Tree::new();
    for index in 0..150 {
        tree.write(&format!("f{index:03}.txt"), "");
    }
    let output = succeeds(&context(&tree), "glob_files", json!({ "pattern": "*.txt" }));
    assert_eq!(output.lines().filter(|l| l.starts_with(" - ")).count(), 100);
    assert!(
        output.contains("... truncated to first 100 matches"),
        "{output}"
    );
}

#[test]
fn glob_files_requires_a_pattern() {
    let tree = Tree::new();
    let context = context(&tree);
    for bad in [json!({}), json!({ "pattern": 3 }), json!({ "pattern": "" })] {
        let result = call(&context, "glob_files", bad.clone());
        assert!(!result.ok, "{bad} was accepted: {result:?}");
    }
}

#[cfg(unix)]
#[test]
fn glob_files_never_offers_a_fifo_as_a_match() {
    // The walk keeps only regular files, so a named pipe is not a candidate for
    // any pattern -- and a path glob offered would be a path the model may then
    // ask `read_file` for.
    let tree = Tree::new();
    tree.write("kept.txt", "k");
    tree.mkfifo("pipe.txt");

    let result =
        answers_within_two_seconds(tree.root(), "glob_files", json!({ "pattern": "*.txt" }));

    assert!(result.ok, "expected a match list, got {result:?}");
    assert_eq!(result.output, "[glob] 1 matches for *.txt\n - kept.txt\n");
}

// ---------------------------------------------------------------------------
// grep_files
// ---------------------------------------------------------------------------

#[test]
fn grep_files_finds_literal_matches_with_paths_and_line_numbers_in_order() {
    let tree = Tree::new();
    tree.write("b.txt", "nothing\nneedle here\n");
    tree.write("a.txt", "needle first\nnothing\nneedle again\n");
    let output = succeeds(
        &context(&tree),
        "grep_files",
        json!({ "pattern": "needle" }),
    );
    assert_eq!(
        output,
        "[grep] 3 matches for needle\n \
         - a.txt:1: needle first\n \
         - a.txt:3: needle again\n \
         - b.txt:2: needle here\n"
    );
}

#[test]
fn grep_files_treats_its_pattern_as_a_literal_not_a_regular_expression() {
    let tree = Tree::new();
    tree.write("a.txt", "a.c\nabc\n");
    let output = succeeds(&context(&tree), "grep_files", json!({ "pattern": "a.c" }));
    assert_eq!(output, "[grep] 1 matches for a.c\n - a.txt:1: a.c\n");
}

#[test]
fn grep_files_can_ignore_case_when_asked() {
    let tree = Tree::new();
    tree.write("a.txt", "Needle Here\n");
    let sensitive = succeeds(
        &context(&tree),
        "grep_files",
        json!({ "pattern": "needle" }),
    );
    assert_eq!(sensitive, "[grep] no matches for needle\n");
    let insensitive = succeeds(
        &context(&tree),
        "grep_files",
        json!({ "pattern": "needle", "case_insensitive": true }),
    );
    assert_eq!(
        insensitive,
        "[grep] 1 matches for needle\n - a.txt:1: Needle Here\n"
    );
}

#[test]
fn grep_files_narrows_candidates_with_an_include_glob() {
    let tree = Tree::new();
    tree.write("keep.rs", "needle\n");
    tree.write("skip.md", "needle\n");
    let output = succeeds(
        &context(&tree),
        "grep_files",
        json!({ "pattern": "needle", "include": "*.rs" }),
    );
    assert_eq!(
        output,
        "[grep] 1 matches for needle\n - keep.rs:1: needle\n"
    );
}

#[test]
fn grep_files_paginates_with_head_limit_and_offset() {
    let tree = Tree::new();
    tree.write("a.txt", "needle 1\nneedle 2\nneedle 3\n");
    let context = context(&tree);
    let first = succeeds(
        &context,
        "grep_files",
        json!({ "pattern": "needle", "head_limit": 2 }),
    );
    assert!(
        first.starts_with("[grep] 2 matches for needle (showing 1-2 of 3)\n"),
        "{first}"
    );
    assert!(
        first.contains("... more matches available; use offset 2 to continue\n"),
        "{first}"
    );
    let second = succeeds(
        &context,
        "grep_files",
        json!({ "pattern": "needle", "head_limit": 2, "offset": 2 }),
    );
    assert_eq!(
        second,
        "[grep] 1 matches for needle (showing 3-3 of 3)\n - a.txt:3: needle 3\n"
    );
}

#[test]
fn grep_files_can_report_files_with_matches_or_exact_counts() {
    let tree = Tree::new();
    tree.write("a.txt", "needle\nneedle\n");
    tree.write("b.txt", "needle\n");
    let context = context(&tree);
    assert_eq!(
        succeeds(
            &context,
            "grep_files",
            json!({ "pattern": "needle", "mode": "files_with_matches" })
        ),
        "[grep] 2 files with matches for needle\n - a.txt\n - b.txt\n"
    );
    assert_eq!(
        succeeds(
            &context,
            "grep_files",
            json!({ "pattern": "needle", "mode": "count" })
        ),
        "[grep] count 3 matching lines in 2 files for needle\n"
    );
}

#[test]
fn grep_files_emits_bounded_context_lines_around_a_match() {
    let tree = Tree::new();
    tree.write("a.txt", "one\ntwo\nneedle\nfour\nfive\n");
    let output = succeeds(
        &context(&tree),
        "grep_files",
        json!({ "pattern": "needle", "context_lines": 1 }),
    );
    assert_eq!(
        output,
        "[grep] 1 matches for needle\n   a.txt:2- two\n - a.txt:3: needle\n   a.txt:4- four\n"
    );
}

#[test]
fn grep_files_does_not_search_binary_files_and_says_it_did_not() {
    let tree = Tree::new();
    tree.write_bytes("blob.bin", b"needle\x00\xffmore");
    tree.write("a.txt", "needle\n");
    let output = succeeds(
        &context(&tree),
        "grep_files",
        json!({ "pattern": "needle" }),
    );
    // The binary file holds the pattern too. Skipping it is right; skipping it
    // silently would report one match where there are arguably two.
    assert_eq!(
        output,
        "[grep] 1 matches for needle\n \
         - a.txt:1: needle\n\
         ... skipped 1 file (too large or not text)\n"
    );
}

#[test]
fn grep_files_says_so_when_nothing_matches() {
    let tree = Tree::new();
    tree.write("a.txt", "nothing\n");
    let output = succeeds(
        &context(&tree),
        "grep_files",
        json!({ "pattern": "needle" }),
    );
    assert_eq!(output, "[grep] no matches for needle\n");
}

#[cfg(unix)]
#[test]
fn grep_files_walks_past_a_fifo_instead_of_parking_on_it() {
    // A search reads every candidate it is given, so the guard has to be in the
    // walk: a named pipe is never a candidate, and the files around it are
    // searched as if it were not there. Were it a candidate, `read_searchable`
    // would open it and the search would never end.
    let tree = Tree::new();
    tree.write("a.txt", "needle here\n");
    tree.mkfifo("pipe.txt");

    let result =
        answers_within_two_seconds(tree.root(), "grep_files", json!({ "pattern": "needle" }));

    assert!(result.ok, "expected a search result, got {result:?}");
    assert_eq!(
        result.output,
        "[grep] 1 matches for needle\n - a.txt:1: needle here\n"
    );
}

// ---------------------------------------------------------------------------
// what a search could not see
// ---------------------------------------------------------------------------
//
// A search that silently excludes files makes `no matches` mean two different
// things -- "there is none" and "there is none in what I looked at" -- and the
// caller cannot tell which. Every exclusion is therefore either counted in the
// output or disclosed in the advertised description, and these tests pin both.

#[test]
fn grep_files_counts_the_files_it_could_not_search() {
    let tree = Tree::new();
    tree.write("a.txt", "needle\n");
    tree.write_bytes("blob.bin", b"needle\x00\xffbinary");
    tree.write("huge.txt", &"needle padding\n".repeat(200));

    // A cap small enough that `huge.txt` is over it and `a.txt` is not.
    let context = ToolContext::with_limits(
        AccessScope::primary_only(tree.root()).expect("scope"),
        ToolLimits {
            max_grep_file_bytes: 64,
            ..ToolLimits::default()
        },
    );
    let output = succeeds(&context, "grep_files", json!({ "pattern": "needle" }));
    assert_eq!(
        output,
        "[grep] 1 matches for needle\n \
         - a.txt:1: needle\n\
         ... skipped 2 files (too large or not text)\n"
    );
}

#[test]
fn grep_files_qualifies_no_matches_when_it_skipped_a_file() {
    let tree = Tree::new();
    // The only candidate holds the pattern and is unsearchable, so an
    // unqualified `no matches` here would be an outright false negative.
    tree.write_bytes("blob.bin", b"needle\x00");
    let output = succeeds(
        &context(&tree),
        "grep_files",
        json!({ "pattern": "needle" }),
    );
    assert_eq!(
        output,
        "[grep] no matches for needle\n... skipped 1 file (too large or not text)\n"
    );
}

#[test]
fn grep_files_leaves_a_complete_search_unqualified() {
    // The contrapositive of the two tests above: no `... skipped` line means
    // every candidate really was searched, in every mode.
    let tree = Tree::new();
    tree.write("a.txt", "needle\n");
    tree.write("b.txt", "nothing\n");
    let context = context(&tree);
    for mode in ["matches", "files_with_matches", "count"] {
        let output = succeeds(
            &context,
            "grep_files",
            json!({ "pattern": "needle", "mode": mode }),
        );
        assert!(!output.contains("... skipped"), "{mode}: {output}");
        assert!(!output.contains("... candidate list"), "{mode}: {output}");
    }
}

#[test]
fn grep_files_counts_skipped_files_in_every_mode() {
    let tree = Tree::new();
    tree.write("a.txt", "needle\n");
    tree.write_bytes("blob.bin", b"needle\x00");
    let context = context(&tree);
    let note = "... skipped 1 file (too large or not text)\n";
    assert_eq!(
        succeeds(
            &context,
            "grep_files",
            json!({ "pattern": "needle", "mode": "files_with_matches" })
        ),
        format!("[grep] 1 files with matches for needle\n - a.txt\n{note}")
    );
    assert_eq!(
        succeeds(
            &context,
            "grep_files",
            json!({ "pattern": "needle", "mode": "count" })
        ),
        format!("[grep] count 1 matching lines in 1 files for needle\n{note}")
    );
}

#[test]
fn the_grep_description_discloses_what_the_search_does_not_see() {
    let schema = Registry::builtin()
        .advertisement()
        .into_iter()
        .find(|tool| tool["name"] == "grep_files")
        .expect("grep_files is advertised");
    let description = schema["description"].as_str().expect("a description");
    for disclosed in [
        ".gitignore",
        "hidden",
        "symlinks are not followed",
        "not UTF-8 text",
        "size cap",
        "... skipped",
        "no matches among the files actually searched",
    ] {
        assert!(
            description.contains(disclosed),
            "the grep description does not disclose `{disclosed}`: {description}"
        );
    }
}

#[test]
fn the_glob_description_discloses_what_the_search_does_not_see() {
    let schema = Registry::builtin()
        .advertisement()
        .into_iter()
        .find(|tool| tool["name"] == "glob_files")
        .expect("glob_files is advertised");
    let description = schema["description"].as_str().expect("a description");
    for disclosed in [
        ".gitignore",
        "hidden dot-paths",
        "symlinks are not followed",
        ".github/**/*.yml",
    ] {
        assert!(
            description.contains(disclosed),
            "the glob description does not disclose `{disclosed}`: {description}"
        );
    }
}

#[test]
fn a_gitignore_applies_even_when_the_tree_is_not_a_git_checkout() {
    // No `.git` directory anywhere: the rules still apply, so results do not
    // change the moment someone runs `git init`.
    let tree = Tree::new();
    tree.write(".gitignore", "ignored/\n*.log\n");
    tree.write("kept.rs", "needle\n");
    tree.write("ignored/hidden-by-git.rs", "needle\n");
    tree.write("noisy.log", "needle\n");

    let context = context(&tree);
    assert_eq!(
        succeeds(&context, "glob_files", json!({ "pattern": "**/*" })),
        "[glob] 1 matches for **/*\n - kept.rs\n"
    );
    assert_eq!(
        succeeds(&context, "grep_files", json!({ "pattern": "needle" })),
        "[grep] 1 matches for needle\n - kept.rs:1: needle\n"
    );
}

#[test]
fn a_hidden_path_is_reached_only_by_a_pattern_that_names_it() {
    let tree = Tree::new();
    tree.write(".github/workflows/ci.yml", "on: push\n");
    tree.write("visible.yml", "on: push\n");
    let context = context(&tree);

    // An ordinary pattern does not wander into dot-directories.
    assert_eq!(
        succeeds(&context, "glob_files", json!({ "pattern": "**/*.yml" })),
        "[glob] 1 matches for **/*.yml\n - visible.yml\n"
    );
    // A pattern that names one opts in.
    assert_eq!(
        succeeds(
            &context,
            "glob_files",
            json!({ "pattern": ".github/**/*.yml" })
        ),
        "[glob] 1 matches for .github/**/*.yml\n - .github/workflows/ci.yml\n"
    );
    // The same rule governs grep, through its `include` glob.
    assert_eq!(
        succeeds(&context, "grep_files", json!({ "pattern": "on: push" })),
        "[grep] 1 matches for on: push\n - visible.yml:1: on: push\n"
    );
    assert_eq!(
        succeeds(
            &context,
            "grep_files",
            json!({ "pattern": "on: push", "include": ".github/**/*.yml" })
        ),
        "[grep] 1 matches for on: push\n - .github/workflows/ci.yml:1: on: push\n"
    );
}

#[test]
fn a_walk_that_hits_the_candidate_cap_says_the_list_may_be_incomplete() {
    let tree = Tree::new();
    for index in 0..5 {
        tree.write(&format!("f{index}.txt"), "needle\n");
    }
    let context = ToolContext::with_limits(
        AccessScope::primary_only(tree.root()).expect("scope"),
        ToolLimits {
            max_candidates: 2,
            ..ToolLimits::default()
        },
    );
    let incomplete = "... candidate list may be incomplete; candidate cap 2 reached before all files were discovered\n";
    assert_eq!(
        succeeds(&context, "glob_files", json!({ "pattern": "*.txt" })),
        format!("[glob] 2 matches for *.txt\n - f0.txt\n - f1.txt\n{incomplete}")
    );
    assert_eq!(
        succeeds(&context, "grep_files", json!({ "pattern": "needle" })),
        format!(
            "[grep] 2 matches for needle\n - f0.txt:1: needle\n - f1.txt:1: needle\n{incomplete}"
        )
    );
}

#[test]
fn grep_files_requires_a_pattern_and_bounds_its_numeric_fields() {
    let tree = Tree::new();
    let context = context(&tree);
    for bad in [
        json!({}),
        json!({ "pattern": 1 }),
        json!({ "pattern": "" }),
        json!({ "pattern": "x", "head_limit": 0 }),
        json!({ "pattern": "x", "offset": -1 }),
        json!({ "pattern": "x", "context_lines": -1 }),
    ] {
        let result = call(&context, "grep_files", bad.clone());
        assert!(!result.ok, "{bad} was accepted: {result:?}");
    }
}

// ---------------------------------------------------------------------------
// the two-request tool loop
// ---------------------------------------------------------------------------

struct ScriptedProvider {
    results: std::cell::RefCell<VecDeque<ScriptedStep>>,
    seen: std::cell::RefCell<Vec<CompletionRequest>>,
}

/// One scripted model step: text fragments, then a completion.
struct ScriptedStep {
    deltas: Vec<String>,
    completion: Completion,
}

impl ScriptedProvider {
    fn new(steps: Vec<ScriptedStep>) -> Self {
        Self {
            results: std::cell::RefCell::new(steps.into()),
            seen: std::cell::RefCell::new(Vec::new()),
        }
    }

    fn requests(&self) -> Vec<CompletionRequest> {
        self.seen.borrow().clone()
    }
}

#[async_trait::async_trait(?Send)]
impl Provider for ScriptedProvider {
    async fn stream(
        &self,
        request: &CompletionRequest,
        deltas: &mut dyn DeltaSink,
    ) -> Result<Completion, ProviderError> {
        self.seen.borrow_mut().push(request.clone());
        let next = self.results.borrow_mut().pop_front();
        match next {
            Some(step) => {
                for fragment in step.deltas {
                    deltas.text_delta(&fragment).map_err(ProviderError::Sink)?;
                }
                Ok(step.completion)
            }
            None => panic!("the provider was called more times than the script allows"),
        }
    }
}

fn calls_step(calls: Vec<ToolCall>) -> ScriptedStep {
    ScriptedStep {
        deltas: Vec::new(),
        completion: Completion {
            text: String::new(),
            tool_calls: calls,
            finish_reason: FinishReason::ToolCalls,
            usage: Usage::default(),
            provider_detail: None,
            raw_content: Vec::new(),
            wire: Wire::VercelGateway,
            billing: None,
            service_tier: None,
        },
    }
}

fn final_step(text: &str) -> ScriptedStep {
    ScriptedStep {
        deltas: vec![text.to_string()],
        completion: Completion {
            text: text.to_string(),
            tool_calls: Vec::new(),
            finish_reason: FinishReason::Stop,
            usage: Usage::default(),
            provider_detail: None,
            raw_content: Vec::new(),
            wire: Wire::VercelGateway,
            billing: None,
            service_tier: None,
        },
    }
}

fn turn(prompt: &str, tools: ToolContext) -> TurnRequest {
    TurnRequest {
        images: Vec::new(),
        model: "vendor/model".to_string(),
        prompt: prompt.to_string(),
        history: Vec::new(),
        max_steps: 4,
        max_attempts: 1,
        cancel: CancelToken::new(),
        tools,
    }
}

fn kinds(sink: &RecordingSink) -> Vec<&'static str> {
    sink.events()
        .iter()
        .map(|event| match event {
            Event::AssistantDelta { .. } => "assistant_delta",
            Event::ToolStart { .. } => "tool_start",
            Event::ToolResult { .. } => "tool_result",
            Event::ToolFeedback { .. } => "tool_feedback",
            Event::Final { .. } => "final",
            Event::Error { .. } => "error",
        })
        .collect()
}

#[tokio::test]
async fn the_first_request_advertises_the_registry_and_lets_the_model_choose() {
    let tree = Tree::new();
    let provider = ScriptedProvider::new(vec![final_step("nothing to read")]);
    let mut sink = RecordingSink::new();
    run_turn(turn("hi", context(&tree)), &provider, &mut sink)
        .await
        .expect("a content-only turn still completes");

    let request = &provider.requests()[0];
    assert_eq!(request.tool_choice, ToolChoice::Auto);
    assert_eq!(request.tools, Registry::builtin().advertisement());
}

#[tokio::test]
async fn one_tool_call_becomes_one_execution_and_one_correlated_result() {
    let tree = Tree::new();
    tree.write("hello.txt", "hello from the workspace\n");

    let provider = ScriptedProvider::new(vec![
        calls_step(vec![ToolCall {
            id: "call_1".to_string(),
            name: "read_file".to_string(),
            input: json!({ "path": "hello.txt" }),
        }]),
        final_step("the file greets you"),
    ]);
    let mut sink = RecordingSink::new();
    let outcome = run_turn(turn("read it", context(&tree)), &provider, &mut sink)
        .await
        .expect("the turn completes");

    assert_eq!(outcome.output, "the file greets you");
    assert_eq!(outcome.steps, 2);

    let requests = provider.requests();
    assert_eq!(requests.len(), 2, "exactly two Gateway requests");

    // Request 2 carries the user message, exactly one assistant tool call, and
    // exactly one correlated tool result.
    let prompt = &requests[1].messages;
    assert_eq!(prompt.len(), 3);
    let body: Value = serde_json::from_str(&requests[1].body().expect("serializable")).unwrap();
    let wire = body["prompt"].as_array().unwrap();
    assert_eq!(wire[1]["role"], "assistant");
    let assistant_calls: Vec<&Value> = wire[1]["content"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|part| part["type"] == "tool-call")
        .collect();
    assert_eq!(assistant_calls.len(), 1);
    assert_eq!(assistant_calls[0]["toolCallId"], "call_1");
    assert_eq!(assistant_calls[0]["toolName"], "read_file");
    assert_eq!(assistant_calls[0]["input"], json!({ "path": "hello.txt" }));

    assert_eq!(wire[2]["role"], "tool");
    let results = wire[2]["content"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["toolCallId"], "call_1");
    assert_eq!(results[0]["toolName"], "read_file");
    let value = results[0]["output"]["value"].as_str().unwrap();
    assert!(value.contains("hello from the workspace"), "{value}");
    assert_eq!(
        value.matches("hello from the workspace").count(),
        1,
        "the tool ran more than once"
    );

    // Exactly one start, one result, and one terminal event.
    assert_eq!(
        kinds(&sink),
        ["tool_start", "tool_result", "assistant_delta", "final"]
    );
}

#[tokio::test]
async fn two_calls_in_one_step_each_run_once_and_answer_in_provider_order() {
    let tree = Tree::new();
    tree.write("first.txt", "first body\n");
    tree.write("second.txt", "second body\n");

    let provider = ScriptedProvider::new(vec![
        calls_step(vec![
            ToolCall {
                id: "b".to_string(),
                name: "read_file".to_string(),
                input: json!({ "path": "second.txt" }),
            },
            ToolCall {
                id: "a".to_string(),
                name: "read_file".to_string(),
                input: json!({ "path": "first.txt" }),
            },
        ]),
        final_step("read both"),
    ]);
    let mut sink = RecordingSink::new();
    run_turn(turn("read both", context(&tree)), &provider, &mut sink)
        .await
        .expect("the turn completes");

    let requests = provider.requests();
    let body: Value = serde_json::from_str(&requests[1].body().expect("serializable")).unwrap();
    let wire = body["prompt"].as_array().unwrap();
    // user, assistant(2 calls), tool(b), tool(a): results follow the order the
    // provider named the calls in.
    assert_eq!(wire.len(), 4);
    assert_eq!(wire[2]["content"][0]["toolCallId"], "b");
    assert!(wire[2]["content"][0]["output"]["value"]
        .as_str()
        .unwrap()
        .contains("second body"));
    assert_eq!(wire[3]["content"][0]["toolCallId"], "a");
    assert!(wire[3]["content"][0]["output"]["value"]
        .as_str()
        .unwrap()
        .contains("first body"));

    assert_eq!(
        kinds(&sink),
        [
            "tool_start",
            "tool_result",
            "tool_start",
            "tool_result",
            "assistant_delta",
            "final",
        ]
    );
}

#[tokio::test]
async fn a_duplicate_call_id_in_one_step_fails_the_turn_before_anything_runs() {
    let tree = Tree::new();
    tree.write("hello.txt", "hello\n");
    let provider = ScriptedProvider::new(vec![calls_step(vec![
        ToolCall {
            id: "same".to_string(),
            name: "read_file".to_string(),
            input: json!({ "path": "hello.txt" }),
        },
        ToolCall {
            id: "same".to_string(),
            name: "read_file".to_string(),
            input: json!({ "path": "hello.txt" }),
        },
    ])]);
    let mut sink = RecordingSink::new();
    let err = run_turn(turn("read", context(&tree)), &provider, &mut sink)
        .await
        .expect_err("two calls cannot share one identifier");
    assert!(
        matches!(err, TurnError::DuplicateToolCallId { .. }),
        "{err}"
    );
    assert_eq!(kinds(&sink), ["error"], "nothing ran");
}

#[tokio::test]
async fn a_call_naming_an_unadvertised_tool_fails_the_turn() {
    let tree = Tree::new();
    let provider = ScriptedProvider::new(vec![calls_step(vec![ToolCall {
        id: "c1".to_string(),
        name: "delete_file".to_string(),
        input: json!({ "path": "x" }),
    }])]);
    let mut sink = RecordingSink::new();
    let err = run_turn(turn("delete", context(&tree)), &provider, &mut sink)
        .await
        .expect_err("xfx does not advertise delete_file");
    assert!(
        matches!(err, TurnError::ToolCallUnsupported { .. }),
        "{err}"
    );
    assert_eq!(kinds(&sink), ["error"]);
}

#[tokio::test]
async fn malformed_arguments_are_answered_with_a_failed_result_and_the_turn_continues() {
    let tree = Tree::new();
    let provider = ScriptedProvider::new(vec![
        calls_step(vec![ToolCall {
            id: "c1".to_string(),
            name: "read_file".to_string(),
            input: json!({ "path": 7 }),
        }]),
        final_step("I will try again"),
    ]);
    let mut sink = RecordingSink::new();
    run_turn(turn("read", context(&tree)), &provider, &mut sink)
        .await
        .expect("a bad argument is the model's problem to fix, not a dead turn");

    match &sink.events()[1] {
        Event::ToolResult { ok, call_id, .. } => {
            assert!(!ok);
            assert_eq!(call_id, "c1");
        }
        other => panic!("expected a tool result, got {other:?}"),
    }
    let requests = provider.requests();
    assert_eq!(requests.len(), 2);
    let body: Value = serde_json::from_str(&requests[1].body().unwrap()).unwrap();
    let value = body["prompt"][2]["content"][0]["output"]["value"]
        .as_str()
        .unwrap();
    assert!(value.contains("read_file"), "{value}");
}

#[tokio::test]
async fn a_loop_that_never_stops_calling_tools_ends_at_the_step_limit() {
    let tree = Tree::new();
    tree.write("hello.txt", "hello\n");
    let steps: Vec<ScriptedStep> = (0..2)
        .map(|index| {
            calls_step(vec![ToolCall {
                id: format!("c{index}"),
                name: "read_file".to_string(),
                input: json!({ "path": "hello.txt" }),
            }])
        })
        .collect();
    let provider = ScriptedProvider::new(steps);

    let mut request = turn("loop", context(&tree));
    request.max_steps = 2;
    let mut sink = RecordingSink::new();
    let err = run_turn(request, &provider, &mut sink)
        .await
        .expect_err("a turn that never finishes must stop");
    assert!(matches!(err, TurnError::StepLimit { limit: 2 }), "{err}");
    assert_eq!(provider.requests().len(), 2, "the bound is on model steps");
    assert_eq!(kinds(&sink).last(), Some(&"error"));
}

// ---------------------------------------------------------------------------
// the amended approval, on the wire and in the journal
// ---------------------------------------------------------------------------
//
// A user who answers an approval may say something while answering. That
// sentence is *context*: it reaches the model as a user message after the
// result of the call it was about, it never edits the arguments that ran, and
// it never widens the authority the answer bought
// (`vercel-labs/fx@580a0c5d src/core/agent/runtime/orchestrator.zig:6598-6618`,
// `:7431-7458`).
//
// Where it goes is the load-bearing choice: `src/llmux/protocol.rs:24-32`
// merges consecutive user messages and hoists `tool_result` blocks to the front
// of the merged one, so a sentence interleaved between two results of a batch
// would arrive above the result it was about. Every sentence of a step
// therefore lands after every result of that step, and the `call_id` in the
// journal keeps the correlation the position no longer can.

/// One scripted approval: the answer, and the sentence typed beside it.
struct Amendment {
    answer: ApprovalAnswer,
    feedback: Option<&'static str>,
    /// Stops the turn as this answer is given -- the Ctrl-C a user types while
    /// the call that asked still holds the thread.
    then_cancel: bool,
}

impl Amendment {
    /// Allowed once, with something said about it.
    fn amended(feedback: &'static str) -> Self {
        Self {
            answer: ApprovalAnswer::Once,
            feedback: Some(feedback),
            then_cancel: false,
        }
    }

    /// Allowed once, with nothing said.
    fn plain() -> Self {
        Self {
            answer: ApprovalAnswer::Once,
            feedback: None,
            then_cancel: false,
        }
    }

    /// Allowed for the rest of the session, with something said about it.
    fn always(feedback: &'static str) -> Self {
        Self {
            answer: ApprovalAnswer::Always,
            feedback: Some(feedback),
            then_cancel: false,
        }
    }

    fn cancelling(mut self) -> Self {
        self.then_cancel = true;
        self
    }
}

/// The approval channel a scripted turn answers on.
///
/// One entry per question, in order. A script that runs out panics rather than
/// inventing an answer: a batch that asked more times than the test said it
/// would is the test being wrong about the turn, not a default to guess at.
#[derive(Clone)]
struct ScriptedAmender {
    script: Arc<Mutex<VecDeque<Amendment>>>,
    /// The turn's own token, so a scripted answer can also be the moment the
    /// user stopped the turn.
    cancel: CancelToken,
}

impl ScriptedAmender {
    fn new(script: Vec<Amendment>, cancel: CancelToken) -> Self {
        Self {
            script: Arc::new(Mutex::new(script.into())),
            cancel,
        }
    }

    fn next(&mut self) -> Amendment {
        let step = self
            .script
            .lock()
            .expect("the script lock")
            .pop_front()
            .expect("the approval channel was asked more times than the script allows");
        if step.then_cancel {
            self.cancel.cancel();
        }
        step
    }

    fn unasked(&self) -> usize {
        self.script.lock().expect("the script lock").len()
    }
}

impl ApprovalPrompter for ScriptedAmender {
    fn request(&mut self, _request: &ApprovalRequest) -> std::io::Result<ApprovalAnswer> {
        Ok(self.next().answer)
    }

    fn respond(&mut self, _request: &ApprovalRequest) -> std::io::Result<ApprovalResponse> {
        let step = self.next();
        Ok(ApprovalResponse {
            answer: step.answer,
            feedback: step.feedback.map(str::to_string),
        })
    }
}

/// Every event a turn recorded, in order.
#[derive(Default)]
struct RecordingJournal {
    events: Vec<SessionEvent>,
}

impl RecordingJournal {
    fn kinds(&self) -> Vec<&'static str> {
        self.events.iter().map(SessionEvent::kind).collect()
    }

    /// Every amendment recorded, as `(call_id, text)` in journal order.
    fn feedback(&self) -> Vec<(String, String)> {
        self.events
            .iter()
            .filter_map(|event| match event {
                SessionEvent::ToolFeedback { call_id, text } => {
                    Some((call_id.clone(), text.clone()))
                }
                _ => None,
            })
            .collect()
    }
}

impl TurnJournal for RecordingJournal {
    fn record(&mut self, event: SessionEvent) {
        self.events.push(event);
    }
}

/// The kinds of what a turn recorded, and the changes it handed over between
/// them as `change`.
#[derive(Default)]
struct ChangeJournal {
    kinds: Vec<&'static str>,
    changes: Vec<(String, ApprovalDiff)>,
}

impl TurnJournal for ChangeJournal {
    fn record(&mut self, event: SessionEvent) {
        self.kinds.push(event.kind());
    }

    fn change_made(&mut self, call_id: &str, change: &ApprovalDiff) {
        self.kinds.push("change");
        self.changes.push((call_id.to_string(), change.clone()));
    }
}

#[tokio::test]
async fn a_change_a_turn_made_is_handed_to_the_journal_after_its_result_with_no_approval() {
    // The full transcript shows the diff of every change, reviewed or not;
    // `yolo` reviews none. The read records nothing of the kind, and the log
    // a session keeps is the same with or without the change beside it.
    let tree = Tree::new();
    tree.write("notes.md", "old\n");
    let provider = ScriptedProvider::new(vec![
        calls_step(vec![
            read_call("r", "notes.md"),
            write_call("w", "notes.md", "new\n"),
        ]),
        final_step("rewritten"),
    ]);
    let tools = context(&tree).with_permissions(PermissionSession::new(PermissionMode::Yolo));
    let mut sink = RecordingSink::new();
    let mut journal = ChangeJournal::default();
    run_turn_saved(
        turn("rewrite the note", tools),
        ProjectContext::none(),
        &provider,
        &mut sink,
        &mut journal,
    )
    .await
    .expect("the turn completes");

    assert_eq!(
        fs::read_to_string(tree.root().join("notes.md")).expect("the written file"),
        "new\n"
    );
    assert_eq!(
        journal.kinds,
        [
            "user_message",
            "assistant_message",
            "tool_result",
            "tool_result",
            "change",
            "assistant_message",
            "usage_recorded",
            "turn_concluded",
        ]
    );
    assert_eq!(journal.changes.len(), 1);
    let (call_id, change) = &journal.changes[0];
    assert_eq!(call_id, "w");
    assert_eq!(
        (change.before.as_str(), change.after.as_str()),
        ("old\n", "new\n")
    );
}

/// A context in `ask` mode whose approvals are answered by `prompter`.
fn asking_context(tree: &Tree, limits: ToolLimits, prompter: ScriptedAmender) -> ToolContext {
    ToolContext::with_limits(
        AccessScope::primary_only(tree.root()).expect("a usable primary root"),
        limits,
    )
    .with_permissions(PermissionSession::new(PermissionMode::Ask).with_prompter(Box::new(prompter)))
}

/// A turn that carries `tools` and can be stopped through `cancel`.
fn asking_turn(prompt: &str, tools: ToolContext, cancel: CancelToken) -> TurnRequest {
    TurnRequest {
        images: Vec::new(),
        model: "vendor/model".to_string(),
        prompt: prompt.to_string(),
        history: Vec::new(),
        max_steps: 4,
        max_attempts: 1,
        cancel,
        tools,
    }
}

fn write_call(id: &str, path: &str, content: &str) -> ToolCall {
    ToolCall {
        id: id.to_string(),
        name: "write_file".to_string(),
        input: json!({ "path": path, "content": content }),
    }
}

fn read_call(id: &str, path: &str) -> ToolCall {
    ToolCall {
        id: id.to_string(),
        name: "read_file".to_string(),
        input: json!({ "path": path }),
    }
}

/// The tool result carried by a `tool` message, or a panic naming what it was.
fn tool_output(message: &Message) -> &str {
    match message.content.first() {
        Some(ContentPart::ToolResult { output, .. }) => output.as_str(),
        other => panic!("expected a tool result, got {other:?}"),
    }
}

fn roles(messages: &[Message]) -> Vec<Role> {
    messages.iter().map(|message| message.role).collect()
}

/// The state a resumed session rebuilds from `events`, through the real store.
fn replayed(events: &[SessionEvent], workspace: &Path) -> DurableState {
    let profile = TempDir::new().expect("a profile root");
    let store = SessionStore::open(&profile.path().join(".xfx")).expect("open the store");
    let mut session = store
        .create(
            SessionId::parse("amended-turn").expect("a safe session id"),
            NewSession {
                origin_workspace_root: workspace.to_path_buf(),
                workspace_root: workspace.to_path_buf(),
                model: "vendor/model".to_string(),
                permission_mode: PermissionMode::Ask,
                provider: None,
            },
        )
        .expect("create the session");
    for event in events {
        store.append(&mut session, event.clone()).expect("append");
    }
    store.publish(&mut session).expect("publish");
    session.state().clone()
}

/// Runs one three-call step whose first and third calls were amended, and
/// returns what the provider saw and what the journal recorded.
///
/// Shared by the ordering, the encoding, and the replay case so all three are
/// arguing about one turn rather than three lookalikes.
async fn amended_batch(tree: &Tree) -> (Vec<CompletionRequest>, Vec<SessionEvent>) {
    tree.write("notes.md", "the note\n");
    let cancel = CancelToken::new();
    let prompter = ScriptedAmender::new(
        vec![
            Amendment::amended("prefer bullet points"),
            Amendment::amended("and keep the heading"),
        ],
        cancel.clone(),
    );
    let provider = ScriptedProvider::new(vec![
        calls_step(vec![
            write_call("c1", "draft-a.md", "alpha\n"),
            read_call("c2", "notes.md"),
            write_call("c3", "draft-b.md", "beta\n"),
        ]),
        final_step("drafted"),
    ]);
    let mut sink = RecordingSink::new();
    let mut journal = RecordingJournal::default();
    run_turn_saved(
        asking_turn(
            "draft two files",
            asking_context(tree, ToolLimits::default(), prompter),
            cancel,
        ),
        ProjectContext::none(),
        &provider,
        &mut sink,
        &mut journal,
    )
    .await
    .expect("the turn completes");

    (provider.requests(), journal.events)
}

/// How an interrupted turn ended is written beside its sentence, as
/// upstream's `terminal_reason` is (`runtime/interruption.zig:254`): a stop is
/// `cancelled`, anything else `failed`.
#[tokio::test]
async fn an_interrupted_turn_records_whether_it_was_stopped_or_failed() {
    let tree = Tree::new();
    let concluded = |journal: &RecordingJournal| {
        journal.events.iter().find_map(|event| match event {
            SessionEvent::TurnConcluded {
                outcome:
                    TurnConclusion::Interrupted {
                        terminal_reason, ..
                    },
            } => Some(*terminal_reason),
            _ => None,
        })
    };
    let stopped = CancelToken::new();
    stopped.cancel();
    let provider = ScriptedProvider::new(vec![final_step("never")]);
    let mut journal = RecordingJournal::default();
    let err = run_turn_saved(
        asking_turn("stop", context(&tree), stopped),
        ProjectContext::none(),
        &provider,
        &mut RecordingSink::new(),
        &mut journal,
    )
    .await
    .expect_err("a stopped turn");
    assert!(matches!(err, TurnError::Cancelled), "{err}");
    assert_eq!(concluded(&journal), Some(Some(TerminalReason::Cancelled)));

    let provider = ScriptedProvider::new(vec![calls_step(vec![ToolCall {
        id: "c1".to_string(),
        name: "delete_file".to_string(),
        input: json!({ "path": "x" }),
    }])]);
    let mut journal = RecordingJournal::default();
    run_turn_saved(
        turn("delete", context(&tree)),
        ProjectContext::none(),
        &provider,
        &mut RecordingSink::new(),
        &mut journal,
    )
    .await
    .expect_err("xfx does not advertise delete_file");
    assert_eq!(concluded(&journal), Some(Some(TerminalReason::Failed)));
}

/// The screen's half of a result reaches the log beside the model's half: a
/// change's line counts and a denial, so a resumed session's rows say what
/// the live ones did (`xfx::session::ToolEnding`).
#[tokio::test]
async fn the_journal_keeps_how_each_call_ended_for_the_row_and_the_lines_it_changed() {
    let tree = Tree::new();
    let cancel = CancelToken::new();
    let prompter = ScriptedAmender::new(
        vec![
            Amendment::plain(),
            Amendment {
                answer: ApprovalAnswer::Deny,
                feedback: None,
                then_cancel: false,
            },
        ],
        cancel.clone(),
    );
    let provider = ScriptedProvider::new(vec![
        calls_step(vec![
            write_call("c1", "draft.md", "one\ntwo\n"),
            write_call("c2", "refused.md", "no\n"),
            read_call("c3", "absent.md"),
        ]),
        final_step("done"),
    ]);
    let mut sink = RecordingSink::new();
    let mut journal = RecordingJournal::default();
    run_turn_saved(
        asking_turn(
            "write two",
            asking_context(&tree, ToolLimits::default(), prompter),
            cancel,
        ),
        ProjectContext::none(),
        &provider,
        &mut sink,
        &mut journal,
    )
    .await
    .expect("the turn completes");

    // The change itself, as its review showed it, for a resumed session's
    // full transcript.
    let change = journal
        .events
        .iter()
        .find_map(|event| match event {
            SessionEvent::ToolResult {
                call_id, change, ..
            } if call_id == "c1" => change.clone(),
            _ => None,
        })
        .expect("the write's change is in the log");
    assert_eq!(
        (change.before.as_str(), change.after.as_str()),
        ("", "one\ntwo\n")
    );
    let results: Vec<(String, bool, Option<ToolEnding>, Option<LineCounts>)> = journal
        .events
        .iter()
        .filter_map(|event| match event {
            SessionEvent::ToolResult {
                call_id,
                ok,
                ending,
                lines,
                ..
            } => Some((call_id.clone(), *ok, *ending, *lines)),
            _ => None,
        })
        .collect();
    assert_eq!(
        results,
        [
            (
                "c1".to_string(),
                true,
                None,
                Some(LineCounts {
                    added: 2,
                    removed: 0
                })
            ),
            ("c2".to_string(), false, Some(ToolEnding::Denied), None),
            // A refusal of the tool's own is what `ok` already says.
            ("c3".to_string(), false, None, None),
        ]
    );
    // And older readers lose nothing: a result with nothing more to say is
    // written exactly as it was before the fields existed.
    let plain = journal
        .events
        .iter()
        .find(|event| matches!(event, SessionEvent::ToolResult { call_id, .. } if call_id == "c3"))
        .map(|event| serde_json::to_value(event).expect("encodes"))
        .expect("the read's result");
    assert!(
        plain.get("ending").is_none() && plain.get("lines").is_none(),
        "{plain}"
    );
}

#[tokio::test]
async fn feedback_reaches_the_prompt_after_the_result_it_was_about() {
    let tree = Tree::new();
    let cancel = CancelToken::new();
    let prompter = ScriptedAmender::new(
        vec![Amendment::amended("use tabs, not spaces")],
        cancel.clone(),
    );
    let provider = ScriptedProvider::new(vec![
        calls_step(vec![write_call("c1", "notes.md", "alpha\n")]),
        final_step("written"),
    ]);
    let mut sink = RecordingSink::new();
    let mut journal = RecordingJournal::default();

    run_turn_saved(
        asking_turn(
            "write the note",
            asking_context(&tree, ToolLimits::default(), prompter),
            cancel,
        ),
        ProjectContext::none(),
        &provider,
        &mut sink,
        &mut journal,
    )
    .await
    .expect("the turn completes");

    let prompt = &provider.requests()[1].messages;
    assert_eq!(
        roles(prompt),
        [Role::User, Role::Assistant, Role::Tool, Role::User],
        "the sentence follows the result it was about"
    );
    assert_eq!(prompt[3].text(), "use tabs, not spaces");

    // The arguments that ran are the arguments that were judged.
    assert_eq!(
        fs::read_to_string(tree.root().join("notes.md")).expect("the written file"),
        "alpha\n"
    );

    // And the sentence is a message of its own: nothing merged it into the
    // tool's own report of what it did.
    assert!(
        !tool_output(&prompt[2]).contains("use tabs"),
        "the sentence was folded into the tool output: {}",
        tool_output(&prompt[2])
    );

    assert_eq!(
        journal.kinds(),
        [
            "user_message",
            "assistant_message",
            "tool_result",
            "tool_feedback",
            "assistant_message",
            "usage_recorded",
            "turn_concluded"
        ]
    );
    assert_eq!(
        journal.feedback(),
        [("c1".to_string(), "use tabs, not spaces".to_string())]
    );
}

#[tokio::test]
async fn a_multi_call_batch_puts_every_result_before_every_sentence() {
    let tree = Tree::new();
    let (requests, events) = amended_batch(&tree).await;

    let prompt = &requests[1].messages;
    assert_eq!(
        roles(prompt),
        [
            Role::User,
            Role::Assistant,
            Role::Tool,
            Role::Tool,
            Role::Tool,
            Role::User,
            Role::User
        ],
        "every result of the step precedes every sentence of it"
    );
    // In call order, and only for the calls that were amended: `read_file`
    // never reached an approval, so it has nothing to carry.
    assert_eq!(prompt[5].text(), "prefer bullet points");
    assert_eq!(prompt[6].text(), "and keep the heading");

    let journal = RecordingJournal { events };
    assert_eq!(
        journal.kinds(),
        [
            "user_message",
            "assistant_message",
            "tool_result",
            "tool_result",
            "tool_result",
            "tool_feedback",
            "tool_feedback",
            "assistant_message",
            "usage_recorded",
            "turn_concluded"
        ]
    );
    assert_eq!(
        journal.feedback(),
        [
            ("c1".to_string(), "prefer bullet points".to_string()),
            ("c3".to_string(), "and keep the heading".to_string())
        ]
    );
}

#[tokio::test]
async fn an_amended_batch_renders_to_a_valid_request_on_both_wires() {
    let tree = Tree::new();
    let (requests, _) = amended_batch(&tree).await;
    let request = &requests[1];

    // The Gateway wire validates inside `body`, which is the only place that
    // produces bytes: no orphan result, no repeated call id.
    let gateway = request.body().expect("a valid Gateway request");
    assert!(gateway.contains("prefer bullet points"), "{gateway}");

    // The Anthropic wire merges the run of user messages -- the three results
    // and both sentences -- into one, with the results leading it.
    let body: Value =
        serde_json::from_str(&llmux::protocol::body(request).expect("a valid llmux request"))
            .expect("llmux bodies are JSON");
    let messages = body["messages"].as_array().expect("messages");
    assert_eq!(
        messages
            .iter()
            .map(|message| message["role"].as_str().expect("a role"))
            .collect::<Vec<_>>(),
        ["user", "assistant", "user"],
        "{body}"
    );
    let merged = messages[2]["content"].as_array().expect("content blocks");
    let kinds: Vec<&str> = merged
        .iter()
        .map(|block| block["type"].as_str().expect("a block type"))
        .collect();
    assert_eq!(
        kinds,
        ["tool_result", "tool_result", "tool_result", "text", "text"],
        "the results lead the merged message: {body}"
    );
    assert_eq!(merged[3]["text"], "prefer bullet points");
    assert_eq!(merged[4]["text"], "and keep the heading");
}

#[tokio::test]
async fn a_resumed_session_replays_the_same_order_with_no_orphan_calls() {
    let tree = Tree::new();
    let (requests, events) = amended_batch(&tree).await;

    let state = replayed(&events, tree.root());
    let replay = state.history_messages(Wire::VercelGateway);

    // Message for message, the prompt the turn actually sent -- results then
    // sentences, in call order, with no orphan call and no orphan sentence.
    let live = &requests[1].messages;
    assert_eq!(
        &replay.messages[..live.len()],
        live.as_slice(),
        "a resumed prompt is not the prompt the turn actually sent"
    );
    // Plus the answering step, which arrived after that request was built.
    assert_eq!(replay.messages.len(), live.len() + 1);
    assert_eq!(replay.messages[live.len()].text(), "drafted");
    assert!(replay.notices.is_empty(), "{:?}", replay.notices);
}

#[tokio::test]
async fn a_fatal_turn_journals_every_sentence_even_when_the_stream_refuses_the_first() {
    // **The journal is not the display's to lose.** This turn is already ending
    // for a reason of its own -- an authority stopped describing the filesystem
    // -- and the journal is the only copy a resumed session can read. A stream
    // that has gone must therefore cost the *echo* and nothing else: every call
    // that ran and was answered keeps its sentence.
    //
    // The refusal is on the **first** amendment, which is the case that
    // separates the two policies: stopping at it would drop `c3`'s sentence as
    // well, and `c3` is the call whose own authority was revoked -- the one a
    // reader of the log is most likely to be looking for.
    let tree = Tree::new();
    let mut sink = RefusingSink::refusing_feedback_from(0);
    let (err, journal, provider) = a_fatal_amended_turn(&tree, &mut sink).await;

    assert!(
        matches!(err, TurnError::ToolAuthorityRevoked { .. }),
        "the revocation stopped being the fact the turn ends with: {err}"
    );
    assert_eq!(
        journal.feedback(),
        [
            ("c1".to_string(), "draft it as markdown".to_string()),
            ("c3".to_string(), "keep the heading".to_string())
        ],
        "a broken display cost the journal the sentences it is the only copy of"
    );
    assert_eq!(
        journal.kinds(),
        [
            "user_message",
            "assistant_message",
            "tool_result",
            "tool_result",
            "tool_result",
            "tool_feedback",
            "tool_feedback",
            // The turn's one request completed before the revocation, and a
            // request that completed is spend `/usage` counts: it is recorded
            // even though the turn failed, just ahead of the conclusion.
            "usage_recorded",
            "turn_concluded"
        ],
        "the order changed when the stream broke"
    );
    assert_eq!(
        sink.shown(),
        Vec::<(String, String)>::new(),
        "the stream that refused was written to anyway"
    );
    // Sent: nothing, exactly as when the stream was there. A fatal turn asks
    // for no second request, so no sentence travels whatever the reader saw.
    assert_eq!(provider.requests().len(), 1);
}

#[tokio::test]
async fn a_stream_that_breaks_part_way_through_a_fatal_flush_still_journals_all_of_it_once() {
    // The other half of the same ruling, and the one that catches a fix that
    // merely moved the `?`: the stream takes the first sentence and refuses the
    // second. Both are journaled, each exactly once and in call order, and the
    // one that was shown is not shown twice.
    let tree = Tree::new();
    let mut sink = RefusingSink::refusing_feedback_from(1);
    let (err, journal, _provider) = a_fatal_amended_turn(&tree, &mut sink).await;

    assert!(
        matches!(err, TurnError::ToolAuthorityRevoked { .. }),
        "{err}"
    );
    assert_eq!(
        journal.feedback(),
        [
            ("c1".to_string(), "draft it as markdown".to_string()),
            ("c3".to_string(), "keep the heading".to_string())
        ],
        "the sentence after the break was dropped from the journal"
    );
    assert_eq!(
        journal
            .kinds()
            .iter()
            .filter(|kind| **kind == "tool_feedback")
            .count(),
        2,
        "a sentence was journaled twice: {:?}",
        journal.kinds()
    );
    assert_eq!(
        sink.shown(),
        [("c1".to_string(), "draft it as markdown".to_string())],
        "the reader saw something other than exactly the sentences that reached it"
    );
}

/// One turn whose **third** call loses its authority while two earlier calls
/// were amended, driven against `sink`.
///
/// The fixture the fatal cases share, so they argue about one turn rather than
/// three lookalikes: `c1` is amended and completes, `c2` reads, `c3` is amended
/// and then finds the file changed under its authority, and `c4` is never
/// admitted. What differs between the cases is only the stream.
async fn a_fatal_amended_turn(
    tree: &Tree,
    sink: &mut dyn xfx::output::EventSink,
) -> (TurnError, RecordingJournal, ScriptedProvider) {
    tree.write("notes.md", "original 0\n");
    // Every visit rewrites the file with different bytes, so the authority
    // minted for the edit no longer describes the filesystem when it is spent.
    let root = tree.root().to_path_buf();
    let visits = Arc::new(std::sync::atomic::AtomicUsize::new(1));
    let interlude = {
        let visits = Arc::clone(&visits);
        Arc::new(move || {
            let visit = visits.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            fs::write(root.join("notes.md"), format!("original {visit}\n"))
                .expect("swap the preimage");
        })
    };

    let cancel = CancelToken::new();
    let prompter = ScriptedAmender::new(
        vec![
            Amendment::amended("draft it as markdown"),
            Amendment::amended("keep the heading"),
        ],
        cancel.clone(),
    );
    let context =
        asking_context(tree, ToolLimits::default(), prompter).with_race_interlude(interlude);
    let provider = ScriptedProvider::new(vec![calls_step(vec![
        write_call("c1", "draft.md", "alpha\n"),
        read_call("c2", "notes.md"),
        ToolCall {
            id: "c3".to_string(),
            name: "edit_file".to_string(),
            input: json!({ "path": "notes.md", "old_string": "original", "new_string": "edited" }),
        },
        read_call("c4", "draft.md"),
    ])]);
    let mut journal = RecordingJournal::default();

    let err = run_turn_saved(
        asking_turn("edit the note", context, cancel),
        ProjectContext::none(),
        &provider,
        sink,
        &mut journal,
    )
    .await
    .expect_err("a lost authority ends the turn");
    (err, journal, provider)
}

#[tokio::test]
async fn a_fatal_result_journals_the_sentence_and_sends_no_request() {
    let tree = Tree::new();
    let mut sink = RefusingSink::accepting();
    let (err, journal, provider) = a_fatal_amended_turn(&tree, &mut sink).await;
    assert!(
        matches!(err, TurnError::ToolAuthorityRevoked { .. }),
        "{err}"
    );
    assert_eq!(
        sink.shown(),
        [
            ("c1".to_string(), "draft it as markdown".to_string()),
            ("c3".to_string(), "keep the heading".to_string())
        ],
        "a reader who was there did not see both sentences"
    );

    // Journaled: every completed call's sentence, the fatal call's own
    // included, and each of them after every result of the step.
    assert_eq!(
        journal.kinds(),
        [
            "user_message",
            "assistant_message",
            "tool_result",
            "tool_result",
            "tool_result",
            "tool_feedback",
            "tool_feedback",
            // The completed request, recorded for `/usage` though the turn failed.
            "usage_recorded",
            "turn_concluded"
        ]
    );
    assert_eq!(
        journal.feedback(),
        [
            ("c1".to_string(), "draft it as markdown".to_string()),
            ("c3".to_string(), "keep the heading".to_string())
        ]
    );

    // Sent: nothing. The turn ended, so there is no second request for the
    // sentences to travel in.
    assert_eq!(provider.requests().len(), 1);

    // Replayed (claim changed: this was "nothing", the incomplete group
    // dropped with its sentences). A turn that did not finish is replayed as
    // upstream replays one (`vercel-labs/fx@c1db9191
    // src/core/session/session.zig:2695-2747`): the call that never ran is
    // answered `aborted by user` as a failure, so the group is whole -- its
    // answer beside the step's other results, and the sentences after every
    // result, never between a call and its answer -- and `<turn_aborted>`
    // closes the turn.
    let state = replayed(&journal.events, tree.root());
    let replay = state.history_messages(Wire::VercelGateway);
    assert_eq!(
        replay_shape(&replay.messages),
        [
            "user: edit the note",
            "assistant: Interrupted by user after completing 3 tool calls: write_file, read_file, \
             edit_file. | call c1 | call c2 | call c3 | call c4",
            "tool: c1",
            "tool: c2",
            "tool: c3 error",
            "tool: c4 error aborted by user",
            "user: draft it as markdown",
            "user: keep the heading",
            "user: <turn_aborted>",
        ],
        "{:#?}",
        replay.messages
    );
}

/// Each message as one line: its role, then its text, calls and results --
/// a result by its call, `error` when it is sent as a failure, and its output
/// only when that is `aborted by user`. A `<turn_aborted>` message is named by
/// its tag.
fn replay_shape(messages: &[Message]) -> Vec<String> {
    messages
        .iter()
        .map(|message| {
            let parts: Vec<String> = message
                .content
                .iter()
                .map(|part| match part {
                    ContentPart::Text { text } if text.starts_with("<turn_aborted>") => {
                        "<turn_aborted>".to_string()
                    }
                    ContentPart::Text { text } => text.clone(),
                    ContentPart::ToolCall(call) => format!("call {}", call.id),
                    ContentPart::ToolResult {
                        call_id,
                        output,
                        status,
                        ..
                    } => match (status, output.as_str()) {
                        (ResultStatus::Success, _) => call_id.clone(),
                        (ResultStatus::Denied, _) => format!("{call_id} denied"),
                        (ResultStatus::Failure, "aborted by user") => {
                            format!("{call_id} error aborted by user")
                        }
                        (ResultStatus::Failure, _) => format!("{call_id} error"),
                    },
                    ContentPart::RawBlocks(_) => "raw".to_string(),
                    ContentPart::Image { media_type, .. } => {
                        format!("image {}", media_type.label())
                    }
                })
                .collect();
            format!("{}: {}", message.role.label(), parts.join(" | "))
        })
        .collect()
}

#[tokio::test]
async fn an_output_limit_does_not_clip_a_sentence_into_the_result() {
    let sentence = "please rerun it with --quiet, the log is enormous and I only care about the last line of it";
    let limits = ToolLimits {
        max_command_output_bytes: 64,
        ..ToolLimits::default()
    };

    // The same command, once amended and once not: whatever the cap does to
    // the output, it does the same either way.
    let mut outputs = Vec::new();
    let mut amended_prompt = Vec::new();
    for feedback in [Some(sentence), None] {
        let tree = Tree::new();
        tree.write("big.txt", &"x".repeat(4096));
        let cancel = CancelToken::new();
        let answer = match feedback {
            Some(text) => Amendment::amended(text),
            None => Amendment::plain(),
        };
        let prompter = ScriptedAmender::new(vec![answer], cancel.clone());
        let provider = ScriptedProvider::new(vec![
            calls_step(vec![ToolCall {
                id: "c1".to_string(),
                name: "terminal".to_string(),
                input: json!({ "action": "exec", "command": "cat big.txt" }),
            }]),
            final_step("read it"),
        ]);
        let mut sink = RecordingSink::new();
        run_turn(
            asking_turn(
                "show me the file",
                asking_context(&tree, limits, prompter),
                cancel,
            ),
            &provider,
            &mut sink,
        )
        .await
        .expect("the turn completes");

        let prompt = provider.requests()[1].messages.clone();
        outputs.push(tool_output(&prompt[2]).to_string());
        if feedback.is_some() {
            amended_prompt = prompt;
        }
    }

    assert!(
        outputs[0].contains("truncated"),
        "the fixture did not reach the cap: {}",
        outputs[0]
    );
    assert_eq!(
        outputs[0], outputs[1],
        "the presence of a sentence changed the tool's own output"
    );

    // The sentence is a message of its own, whole: it is not tool output, so
    // the tool output limit is not its limit.
    assert_eq!(roles(&amended_prompt).last(), Some(&Role::User));
    let carried = amended_prompt.last().expect("a sentence").text();
    assert_eq!(carried, sentence);
    assert!(
        !carried.contains("truncated") && !carried.contains("clipped"),
        "a truncation marker landed inside the sentence: {carried}"
    );
}

#[tokio::test]
async fn a_cancelled_final_call_neither_journals_nor_sends_its_sentence() {
    // Cancellation is *observed* at the yield after a call and *acted on* at
    // the top of the next iteration -- and after the last call there is no next
    // iteration. Without a guard on the normal flush, this turn journals and
    // sends a sentence from a turn the user had already stopped.
    let tree = Tree::new();
    let cancel = CancelToken::new();
    let prompter = ScriptedAmender::new(
        vec![Amendment::amended("actually, never mind").cancelling()],
        cancel.clone(),
    );
    let provider = ScriptedProvider::new(vec![calls_step(vec![write_call(
        "c1", "notes.md", "alpha\n",
    )])]);
    let mut sink = RecordingSink::new();
    let mut journal = RecordingJournal::default();

    let err = run_turn_saved(
        asking_turn(
            "write the note",
            asking_context(&tree, ToolLimits::default(), prompter),
            cancel,
        ),
        ProjectContext::none(),
        &provider,
        &mut sink,
        &mut journal,
    )
    .await
    .expect_err("a cancelled turn does not complete");
    assert!(matches!(err, TurnError::Cancelled), "{err}");

    assert_eq!(
        journal.feedback(),
        Vec::<(String, String)>::new(),
        "a stopped turn recorded a sentence it was told to drop"
    );
    assert!(
        !journal.kinds().contains(&"tool_feedback"),
        "{:?}",
        journal.kinds()
    );
    assert_eq!(provider.requests().len(), 1, "a stopped turn asked again");
}

/// Every streamed amendment, as `(call_id, text)` in emission order.
fn streamed_feedback(sink: &RecordingSink) -> Vec<(String, String)> {
    sink.events()
        .iter()
        .filter_map(|event| match event {
            Event::ToolFeedback { call_id, text } => Some((call_id.clone(), text.clone())),
            _ => None,
        })
        .collect()
}

/// A sink that refuses amendments from the `n`th onwards, and records the rest.
///
/// Written here rather than in the product for the reason every other fixture
/// in this file is: what it proves is how a **turn** behaves when the thing
/// watching it goes away, and a stream that cannot be broken on purpose cannot
/// prove it. The index matters because a stream that breaks part-way through a
/// batch is the case where "journal everything" and "stop at the first refusal"
/// give different journals.
struct RefusingSink {
    recorded: RecordingSink,
    /// The first amendment this stream refuses, if it refuses one.
    refuse_from: Option<usize>,
    seen: usize,
}

impl RefusingSink {
    fn refusing_feedback_from(index: usize) -> Self {
        Self {
            recorded: RecordingSink::new(),
            refuse_from: Some(index),
            seen: 0,
        }
    }

    fn accepting() -> Self {
        Self {
            recorded: RecordingSink::new(),
            refuse_from: None,
            seen: 0,
        }
    }

    /// How many amendments really reached the reader.
    fn shown(&self) -> Vec<(String, String)> {
        streamed_feedback(&self.recorded)
    }
}

impl xfx::output::EventSink for RefusingSink {
    fn emit(&mut self, event: &Event) -> std::io::Result<()> {
        if matches!(event, Event::ToolFeedback { .. }) {
            let index = self.seen;
            self.seen += 1;
            if self.refuse_from.is_some_and(|from| index >= from) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "the terminal session is gone",
                ));
            }
        }
        self.recorded.emit(event)
    }
}

#[tokio::test]
async fn an_amendment_is_streamed_at_the_flush_that_journals_and_sends_it() {
    // **The display is not a fourth copy of the decision, it is the same
    // flush.** A sentence the user typed is shown to them exactly when the
    // model is told it -- after every result of the step, in call order -- so
    // there is no state in which the transcript says something the wire does
    // not, or the other way round.
    let tree = Tree::new();
    let cancel = CancelToken::new();
    let prompter = ScriptedAmender::new(
        vec![Amendment::amended("use tabs, not spaces")],
        cancel.clone(),
    );
    let provider = ScriptedProvider::new(vec![
        calls_step(vec![write_call("c1", "notes.md", "alpha\n")]),
        final_step("written"),
    ]);
    let mut sink = RecordingSink::new();
    let mut journal = RecordingJournal::default();

    run_turn_saved(
        asking_turn(
            "write the note",
            asking_context(&tree, ToolLimits::default(), prompter),
            cancel,
        ),
        ProjectContext::none(),
        &provider,
        &mut sink,
        &mut journal,
    )
    .await
    .expect("the turn completes");

    // Behind the result it is about and ahead of the turn's conclusion, which
    // is the same place it takes on the wire and in the journal.
    assert_eq!(
        kinds(&sink),
        [
            "tool_start",
            "tool_result",
            "tool_feedback",
            "assistant_delta",
            "final"
        ],
        "the amendment was not streamed where the model was told it"
    );
    assert_eq!(
        streamed_feedback(&sink),
        [("c1".to_string(), "use tabs, not spaces".to_string())],
        "the streamed sentence is not the one the user typed"
    );
    // Exactly once, and the same sentence the journal and the prompt carry.
    assert_eq!(streamed_feedback(&sink), journal.feedback());
}

#[tokio::test]
async fn a_multi_call_batch_streams_every_sentence_after_every_result() {
    let tree = Tree::new();
    tree.write("notes.md", "the note\n");
    let cancel = CancelToken::new();
    let prompter = ScriptedAmender::new(
        vec![
            Amendment::amended("prefer bullet points"),
            Amendment::amended("and keep the heading"),
        ],
        cancel.clone(),
    );
    let provider = ScriptedProvider::new(vec![
        calls_step(vec![
            write_call("c1", "draft-a.md", "alpha\n"),
            read_call("c2", "notes.md"),
            write_call("c3", "draft-b.md", "beta\n"),
        ]),
        final_step("drafted"),
    ]);
    let mut sink = RecordingSink::new();
    let mut journal = RecordingJournal::default();
    run_turn_saved(
        asking_turn(
            "draft two files",
            asking_context(&tree, ToolLimits::default(), prompter),
            cancel,
        ),
        ProjectContext::none(),
        &provider,
        &mut sink,
        &mut journal,
    )
    .await
    .expect("the turn completes");

    assert_eq!(
        kinds(&sink),
        [
            "tool_start",
            "tool_result",
            "tool_start",
            "tool_result",
            "tool_start",
            "tool_result",
            "tool_feedback",
            "tool_feedback",
            "assistant_delta",
            "final"
        ],
        "a sentence was streamed between two results of one step"
    );
    assert_eq!(
        streamed_feedback(&sink),
        [
            ("c1".to_string(), "prefer bullet points".to_string()),
            ("c3".to_string(), "and keep the heading".to_string())
        ]
    );
    assert_eq!(streamed_feedback(&sink), journal.feedback());
}

#[tokio::test]
async fn an_interrupted_amendment_is_never_streamed() {
    // The display half of the drop contract. A sentence the user typed and then
    // interrupted out of reaches the model, the journal and the **transcript**
    // in exactly the same way: not at all. A transcript that showed it would be
    // telling the user something was said that never was.
    let tree = Tree::new();
    let cancel = CancelToken::new();
    let prompter = ScriptedAmender::new(
        vec![Amendment::amended("never sent").cancelling()],
        cancel.clone(),
    );
    let provider = ScriptedProvider::new(vec![
        calls_step(vec![write_call("c1", "notes.md", "alpha\n")]),
        final_step("written"),
    ]);
    let mut sink = RecordingSink::new();
    let mut journal = RecordingJournal::default();

    let err = run_turn_saved(
        asking_turn(
            "write the note",
            asking_context(&tree, ToolLimits::default(), prompter),
            cancel,
        ),
        ProjectContext::none(),
        &provider,
        &mut sink,
        &mut journal,
    )
    .await
    .expect_err("a cancelled turn does not complete");
    assert!(matches!(err, TurnError::Cancelled), "{err}");

    assert!(
        !kinds(&sink).contains(&"tool_feedback"),
        "a sentence the user interrupted out of was shown to them anyway: {:?}",
        kinds(&sink)
    );
    assert_eq!(streamed_feedback(&sink), Vec::<(String, String)>::new());
    assert_eq!(journal.feedback(), Vec::<(String, String)>::new());
    assert!(
        !format!("{:?}", provider.requests()).contains("never sent"),
        "the interrupted sentence reached the wire"
    );
}

#[tokio::test]
async fn a_sink_that_refuses_an_amendment_ends_the_turn_rather_than_swallowing_it() {
    // **The display error is not swallowed.** The emit comes first and the
    // journal second -- the order every other event on this path uses
    // (`machine.rs`'s `ToolResult` site) -- so a stream that has gone ends the
    // turn with the sink's own error, and the sentence is left out of the
    // journal and out of the prompt rather than recorded as delivered to a
    // reader who never saw it.
    let tree = Tree::new();
    let cancel = CancelToken::new();
    let prompter = ScriptedAmender::new(
        vec![Amendment::amended("use tabs, not spaces")],
        cancel.clone(),
    );
    let provider = ScriptedProvider::new(vec![
        calls_step(vec![write_call("c1", "notes.md", "alpha\n")]),
        final_step("written"),
    ]);
    let mut sink = RefusingSink::refusing_feedback_from(0);
    let mut journal = RecordingJournal::default();

    let err = run_turn_saved(
        asking_turn(
            "write the note",
            asking_context(&tree, ToolLimits::default(), prompter),
            cancel,
        ),
        ProjectContext::none(),
        &provider,
        &mut sink,
        &mut journal,
    )
    .await
    .expect_err("a turn whose stream refused an event does not complete");
    assert!(
        matches!(&err, TurnError::Sink(inner) if inner.kind() == std::io::ErrorKind::BrokenPipe),
        "the display failure was not reported as the turn's own: {err}"
    );
    assert_eq!(
        journal.feedback(),
        Vec::<(String, String)>::new(),
        "the journal recorded a sentence the reader was never shown"
    );
    assert_eq!(
        provider.requests().len(),
        1,
        "a turn whose stream broke asked for another completion"
    );

    // The call itself still ran and was still journaled: an amendment is
    // context, and a broken stream is not a reason to un-write a file.
    assert_eq!(
        fs::read_to_string(tree.root().join("notes.md")).expect("the written file"),
        "alpha\n"
    );
    assert!(
        journal.kinds().contains(&"tool_result"),
        "{:?}",
        journal.kinds()
    );
}

#[tokio::test]
async fn a_cancelled_multi_call_batch_drops_every_buffered_sentence() {
    let tree = Tree::new();
    let cancel = CancelToken::new();
    let prompter = ScriptedAmender::new(
        vec![
            Amendment::amended("prefer bullet points"),
            Amendment::amended("and keep the heading").cancelling(),
            Amendment::amended("never asked"),
        ],
        cancel.clone(),
    );
    let asked = prompter.clone();
    let provider = ScriptedProvider::new(vec![calls_step(vec![
        write_call("c1", "draft-a.md", "alpha\n"),
        write_call("c2", "draft-b.md", "beta\n"),
        write_call("c3", "draft-c.md", "gamma\n"),
    ])]);
    let mut sink = RecordingSink::new();
    let mut journal = RecordingJournal::default();

    let err = run_turn_saved(
        asking_turn(
            "draft three files",
            asking_context(&tree, ToolLimits::default(), prompter),
            cancel,
        ),
        ProjectContext::none(),
        &provider,
        &mut sink,
        &mut journal,
    )
    .await
    .expect_err("a cancelled turn does not complete");
    assert!(matches!(err, TurnError::Cancelled), "{err}");

    // The third call was abandoned before it was admitted, so its approval was
    // never put to anybody.
    assert_eq!(asked.unasked(), 1);
    assert!(
        !tree.root().join("draft-c.md").exists(),
        "a stopped turn ran the call behind the stop"
    );

    assert_eq!(
        journal.feedback(),
        Vec::<(String, String)>::new(),
        "a stopped turn recorded sentences it was told to drop"
    );
    assert_eq!(provider.requests().len(), 1, "a stopped turn asked again");

    // And a resume replays no sentence: none was recorded. What it does
    // replay (claim changed: this was the prompt alone) is the stopped batch
    // as upstream replays a stopped turn (`session.zig:2695-2747`): the calls
    // that ran, the one that never did answered `aborted by user` as a
    // failure, and `<turn_aborted>`.
    let state = replayed(&journal.events, tree.root());
    let replay = state.history_messages(Wire::VercelGateway);
    assert_eq!(
        replay_shape(&replay.messages),
        [
            "user: draft three files",
            "assistant: Interrupted by user after completing 2 tool calls: write_file, \
             write_file. | call c1 | call c2 | call c3",
            "tool: c1",
            "tool: c2",
            "tool: c3 error aborted by user",
            "user: <turn_aborted>",
        ],
        "{:#?}",
        replay.messages
    );
}

/// A provider that sends a steer into the turn during its call `at`
/// (0-based), as the person would while that request was answering -- and,
/// when `hang`, streams `PARTIAL ` and then never answers that call, as a
/// long answer the steer cuts short.
struct SteeredProvider {
    inner: ScriptedProvider,
    steering: Steering,
    at: usize,
    text: &'static str,
    hang: bool,
    calls: std::cell::Cell<usize>,
}

#[async_trait::async_trait(?Send)]
impl Provider for SteeredProvider {
    async fn stream(
        &self,
        request: &CompletionRequest,
        deltas: &mut dyn DeltaSink,
    ) -> Result<Completion, ProviderError> {
        let call = self.calls.get();
        self.calls.set(call + 1);
        if call == self.at {
            if self.hang {
                self.inner.seen.borrow_mut().push(request.clone());
                deltas.text_delta("PARTIAL ").map_err(ProviderError::Sink)?;
                assert!(self.steering.offer(1, self.text.to_string()));
                return std::future::pending().await;
            }
            assert!(self.steering.offer(1, self.text.to_string()));
        }
        self.inner.stream(request, deltas).await
    }
}

/// Runs `steps` with a steer sent during call `at`, and hands back the
/// requests, the journal and the steers the sink was told were taken.
async fn steered_turn(
    tree: &Tree,
    steps: Vec<ScriptedStep>,
    at: usize,
    hang: bool,
) -> (Vec<CompletionRequest>, RecordingJournal, Vec<Event>) {
    let steering = Steering::new();
    steering.open();
    let provider = SteeredProvider {
        inner: ScriptedProvider::new(steps),
        steering: steering.clone(),
        at,
        text: "change tone",
        hang,
        calls: std::cell::Cell::new(0),
    };
    let mut sink = RecordingSink::new();
    let mut journal = RecordingJournal::default();
    xfx::agent::TurnMachine::new(turn("go", context(tree)))
        .with_steering(steering)
        .run_journaled(&provider, &mut sink, &mut journal)
        .await
        .expect("the turn completes");
    (provider.inner.requests(), journal, sink.events().to_vec())
}

/// Each message as `role: text`, results by their call, for a case about the
/// order the model is told things in.
fn told(messages: &[Message]) -> Vec<String> {
    messages
        .iter()
        .map(|message| {
            let parts: Vec<String> = message
                .content
                .iter()
                .map(|part| match part {
                    ContentPart::Text { text } => text.clone(),
                    ContentPart::ToolCall(call) => format!("call {}", call.id),
                    ContentPart::ToolResult { call_id, .. } => format!("result {call_id}"),
                    ContentPart::RawBlocks(_) => "raw".to_string(),
                    ContentPart::Image { media_type, .. } => {
                        format!("image {}", media_type.label())
                    }
                })
                .collect();
            format!("{}: {}", message.role.label(), parts.join(" | "))
        })
        .collect()
}

#[tokio::test]
async fn a_steer_cuts_a_tool_free_answer_and_the_turn_goes_on_with_it() {
    // `fx-interrupt-steer-40x120-bodies/004-completion.json`: the answer as far
    // as it got, then the steer in upstream's wrapper -- no `ended before
    // completion`, no `<turn_aborted>` (e2e `tui-interrupt-recovery.test.ts:
    // 291`). One turn, two requests.
    let tree = Tree::new();
    let (requests, journal, _) =
        steered_turn(&tree, vec![final_step("steered answer.")], 0, true).await;
    assert_eq!(requests.len(), 2);
    assert_eq!(
        told(&requests[1].messages),
        [
            "user: go".to_string(),
            "assistant: PARTIAL ".to_string(),
            format!("user: {}", xfx::agent::steering::message("change tone")),
        ]
    );
    assert_eq!(
        journal.kinds(),
        [
            "user_message",
            "assistant_message",
            "steered",
            "assistant_message",
            "usage_recorded",
            "turn_concluded"
        ]
    );
    // A later turn is told the steer as the person typed it
    // (`fx-interrupt-steer-40x120-bodies/005-completion.json`).
    let state = replayed(&journal.events, tree.root());
    assert_eq!(
        told(&state.history_messages(Wire::VercelGateway).messages),
        [
            "user: go",
            "assistant: PARTIAL ",
            "user: change tone",
            "assistant: steered answer."
        ]
    );
}

#[tokio::test]
async fn a_steer_sent_while_a_tool_runs_is_told_after_its_results() {
    // `fx-interrupt-steer-40x120-bodies/006-completion.json`: the call, its
    // result, then the steer -- inside the same turn.
    let tree = Tree::new();
    tree.write("notes.md", "alpha\n");
    let (requests, _, events) = steered_turn(
        &tree,
        vec![
            calls_step(vec![read_call("c1", "notes.md")]),
            final_step("after steer."),
        ],
        0,
        false,
    )
    .await;
    assert_eq!(requests.len(), 2);
    assert_eq!(
        told(&requests[1].messages),
        [
            "user: go".to_string(),
            "assistant: call c1".to_string(),
            "tool: result c1".to_string(),
            format!("user: {}", xfx::agent::steering::message("change tone")),
        ]
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, Event::ToolResult { .. })),
        "the call never ran"
    );
}

#[tokio::test]
async fn a_steer_that_arrives_with_the_answer_is_told_after_it_and_the_turn_goes_on() {
    // `append_pending_steering_after_assistant` (`orchestrator.zig:215-238`):
    // the model's answer, then the steer, and another request.
    let tree = Tree::new();
    let (requests, _, _) = steered_turn(
        &tree,
        vec![final_step("first answer."), final_step("second answer.")],
        0,
        false,
    )
    .await;
    assert_eq!(requests.len(), 2);
    assert_eq!(
        told(&requests[1].messages),
        [
            "user: go".to_string(),
            "assistant: first answer.".to_string(),
            format!("user: {}", xfx::agent::steering::message("change tone")),
        ]
    );
}

#[tokio::test]
async fn a_refused_call_goes_back_as_a_refusal_live_and_on_resume() {
    // Upstream says a refused call as `execution-denied` with the refusal as
    // its reason (`vercel-labs/fx@c1db9191 src/gateway/vercel_protocol.zig:
    // 742-744`), and a call that failed on its own as `error-text`. Both the
    // next request of the turn and a resumed session's replay say it so.
    let tree = Tree::new();
    let cancel = CancelToken::new();
    let prompter = ScriptedAmender::new(
        vec![Amendment {
            answer: ApprovalAnswer::Deny,
            feedback: None,
            then_cancel: false,
        }],
        cancel.clone(),
    );
    let provider = ScriptedProvider::new(vec![
        calls_step(vec![write_call("c1", "notes.md", "alpha\n")]),
        final_step("not written"),
    ]);
    let mut sink = RecordingSink::new();
    let mut journal = RecordingJournal::default();
    run_turn_saved(
        asking_turn(
            "write it",
            asking_context(&tree, ToolLimits::default(), prompter),
            cancel,
        ),
        ProjectContext::none(),
        &provider,
        &mut sink,
        &mut journal,
    )
    .await
    .expect("the turn completes");

    let refusal = |messages: &[Message]| {
        messages.iter().find_map(|message| {
            message.content.iter().find_map(|part| match part {
                ContentPart::ToolResult { status, output, .. } => Some((*status, output.clone())),
                _ => None,
            })
        })
    };
    let (status, output) = refusal(&provider.requests()[1].messages).expect("the result");
    assert_eq!(status, ResultStatus::Denied, "{output}");
    assert!(
        output.starts_with("write_file was not permitted: "),
        "{output}"
    );

    let state = replayed(&journal.events, tree.root());
    let replay = state.history_messages(Wire::VercelGateway);
    assert_eq!(
        refusal(&replay.messages),
        Some((ResultStatus::Denied, output)),
        "the replay said the refusal as something else"
    );
}

#[tokio::test]
async fn a_call_the_previous_answer_already_granted_carries_no_second_sentence() {
    // "Always" buys a grant keyed by tool and target, and nothing else. The
    // next call on that target is allowed without asking, so nobody says
    // anything about it -- which is also the proof that no slot anywhere is
    // holding the last sentence for whoever asks next.
    let tree = Tree::new();
    let cancel = CancelToken::new();
    let prompter = ScriptedAmender::new(vec![Amendment::always("keep it short")], cancel.clone());
    let asked = prompter.clone();
    let provider = ScriptedProvider::new(vec![
        calls_step(vec![
            write_call("c1", "notes.md", "alpha\n"),
            write_call("c2", "notes.md", "alpha\nbeta\n"),
        ]),
        final_step("written twice"),
    ]);
    let mut sink = RecordingSink::new();
    let mut journal = RecordingJournal::default();

    run_turn_saved(
        asking_turn(
            "write it twice",
            asking_context(&tree, ToolLimits::default(), prompter),
            cancel,
        ),
        ProjectContext::none(),
        &provider,
        &mut sink,
        &mut journal,
    )
    .await
    .expect("the turn completes");

    assert_eq!(asked.unasked(), 0, "the script was not exhausted");
    assert_eq!(
        journal.feedback(),
        [("c1".to_string(), "keep it short".to_string())],
        "a sentence outlived the call it was typed at"
    );
    let prompt = &provider.requests()[1].messages;
    assert_eq!(
        roles(prompt),
        [
            Role::User,
            Role::Assistant,
            Role::Tool,
            Role::Tool,
            Role::User
        ]
    );
    assert_eq!(prompt[4].text(), "keep it short");
}

// ---------------------------------------------------------------------------
// binary-level acceptance
// ---------------------------------------------------------------------------

struct Sandbox {
    _root: TempDir,
    home: PathBuf,
    workspace: PathBuf,
}

impl Sandbox {
    fn new() -> Self {
        let root = TempDir::new().expect("create sandbox root");
        let home = root.path().join("home");
        let workspace = root.path().join("workspace");
        fs::create_dir_all(&home).expect("create home");
        fs::create_dir_all(&workspace).expect("create workspace");
        Self {
            home: home.canonicalize().expect("canonicalize home"),
            workspace: workspace.canonicalize().expect("canonicalize workspace"),
            _root: root,
        }
    }

    fn write(&self, relative: &str, contents: &str) -> PathBuf {
        let path = self.workspace.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent directories");
        }
        fs::write(&path, contents).expect("write the fixture file");
        path
    }

    fn run(&self, args: &[&str], env: &[(&str, &str)]) -> Run {
        let mut command = Command::new(env!("CARGO_BIN_EXE_xfx"));
        command.current_dir(&self.workspace);
        command.env("HOME", &self.home);
        for key in CONTROLLED_VARS {
            command.env_remove(key);
        }
        for (key, value) in env {
            command.env(key, value);
        }
        command.args(args);
        Run::of(command.output().expect("spawn xfx"))
    }
}

struct Run {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

impl Run {
    fn of(output: Output) -> Self {
        Self {
            code: output.status.code(),
            stdout: String::from_utf8(output.stdout).expect("stdout is utf-8"),
            stderr: String::from_utf8(output.stderr).expect("stderr is utf-8"),
        }
    }

    fn events(&self) -> Vec<Value> {
        assert!(
            self.stdout.is_empty() || self.stdout.ends_with('\n'),
            "JSONL must be newline terminated, got {:?}",
            self.stdout
        );
        self.stdout
            .lines()
            .map(|line| {
                serde_json::from_str(line)
                    .unwrap_or_else(|err| panic!("`{line}` is not JSON ({err})"))
            })
            .collect()
    }

    fn kinds(&self) -> Vec<String> {
        self.events()
            .iter()
            .map(|event| event["kind"].as_str().expect("a kind").to_string())
            .collect()
    }

    fn assert_no_secret(&self) {
        assert!(!self.stdout.contains(TEST_KEY), "the key reached stdout");
        assert!(!self.stderr.contains(TEST_KEY), "the key reached stderr");
    }
}

/// The scripted reply that asks for one `read_file` call.
fn read_file_reply(call_id: &str, path: &str) -> Reply {
    Reply::Sse(sse_body(&[
        tool_call(call_id, "read_file", json!({ "path": path })),
        finish("tool-calls"),
    ]))
}

#[test]
fn ask_advertises_the_whole_registry_in_its_first_request() {
    let gateway = FakeGateway::start(vec![Reply::Sse(content_only(&["ok"]))]);
    let sandbox = Sandbox::new();
    let run = sandbox.run(
        &["ask", "--json", "--no-save", "hello"],
        &[
            ("AI_GATEWAY_API_KEY", TEST_KEY),
            ("XFX_GATEWAY_URL", &gateway.chat_url()),
        ],
    );
    assert_eq!(run.code, Some(0), "stderr={:?}", run.stderr);

    let body = gateway.only_request().json();
    let names: Vec<&str> = body["tools"]
        .as_array()
        .expect("tools is an array")
        .iter()
        .map(|tool| tool["name"].as_str().expect("a tool name"))
        .collect();
    assert_eq!(names, ADVERTISED_TOOLS);
    assert_eq!(body["toolChoice"], json!({ "type": "auto" }));
    run.assert_no_secret();
}

#[test]
fn ask_reads_a_workspace_file_and_reports_the_call_between_the_deltas() {
    let gateway = FakeGateway::start(vec![
        read_file_reply("c1", "notes.md"),
        Reply::Sse(sse_body(&[
            text_delta("a0", "the note says hi"),
            finish("stop"),
        ])),
    ]);
    let sandbox = Sandbox::new();
    sandbox.write("notes.md", "hi from the note\n");
    let run = sandbox.run(
        &["ask", "--json", "--no-save", "read", "the", "note"],
        &[
            ("AI_GATEWAY_API_KEY", TEST_KEY),
            ("XFX_GATEWAY_URL", &gateway.chat_url()),
        ],
    );
    assert_eq!(run.code, Some(0), "stderr={:?}", run.stderr);
    assert_eq!(
        run.kinds(),
        ["tool_start", "tool_result", "assistant_delta", "final"]
    );

    let events = run.events();
    assert_eq!(events[0]["call_id"], "c1");
    assert_eq!(events[0]["tool"], "read_file");
    assert_eq!(events[1]["ok"], json!(true));

    let requests = gateway.requests();
    assert_eq!(requests.len(), 2);
    let second = requests[1].json();
    let wire = second["prompt"].as_array().unwrap();
    assert_eq!(wire.len(), 3);
    assert_eq!(wire[2]["content"][0]["toolCallId"], "c1");
    let value = wire[2]["content"][0]["output"]["value"].as_str().unwrap();
    assert!(value.contains("hi from the note"), "{value}");
    run.assert_no_secret();
}

#[test]
fn ask_refuses_a_path_outside_the_workspace_when_no_directory_was_added() {
    let outside = TempDir::new().expect("outside directory");
    let secret = outside.path().join("secret.txt");
    fs::write(&secret, "SENSITIVE-OUTSIDE-VALUE\n").expect("write outside file");

    let gateway = FakeGateway::start(vec![
        read_file_reply("c1", secret.to_str().unwrap()),
        Reply::Sse(sse_body(&[
            text_delta("a0", "I cannot read that"),
            finish("stop"),
        ])),
    ]);
    let sandbox = Sandbox::new();
    let run = sandbox.run(
        &["ask", "--json", "--no-save", "read", "it"],
        &[
            ("AI_GATEWAY_API_KEY", TEST_KEY),
            ("XFX_GATEWAY_URL", &gateway.chat_url()),
        ],
    );
    assert_eq!(run.code, Some(0), "stderr={:?}", run.stderr);
    assert_eq!(run.events()[1]["ok"], json!(false));
    assert!(
        !run.stdout.contains("SENSITIVE-OUTSIDE-VALUE"),
        "the outside file reached the model"
    );
    let second = gateway.requests()[1].json();
    assert!(
        !second.to_string().contains("SENSITIVE-OUTSIDE-VALUE"),
        "the outside file reached the Gateway"
    );
    run.assert_no_secret();
}

#[test]
fn ask_reads_a_file_from_an_explicitly_added_directory() {
    let shared = TempDir::new().expect("shared directory");
    let shared_root = shared.path().canonicalize().expect("canonicalize shared");
    let note = shared_root.join("shared.md");
    fs::write(&note, "shared knowledge\n").expect("write shared file");

    let gateway = FakeGateway::start(vec![
        read_file_reply("c1", note.to_str().unwrap()),
        Reply::Sse(sse_body(&[text_delta("a0", "read it"), finish("stop")])),
    ]);
    let sandbox = Sandbox::new();
    let run = sandbox.run(
        &[
            "ask",
            "--json",
            "--no-save",
            "--add-dir",
            shared_root.to_str().unwrap(),
            "read",
            "the",
            "shared",
            "note",
        ],
        &[
            ("AI_GATEWAY_API_KEY", TEST_KEY),
            ("XFX_GATEWAY_URL", &gateway.chat_url()),
        ],
    );
    assert_eq!(run.code, Some(0), "stderr={:?}", run.stderr);
    assert_eq!(run.events()[1]["ok"], json!(true));
    let second = gateway.requests()[1].json();
    let value = second["prompt"][2]["content"][0]["output"]["value"]
        .as_str()
        .unwrap();
    assert!(value.contains("shared knowledge"), "{value}");
    run.assert_no_secret();
}

#[test]
fn ask_reads_from_a_directory_workspace_add_saved_and_skips_one_that_is_gone() {
    // One `/workspace add` means the same directory to every surface: `ask`
    // runs in the saved directories too, as upstream's does
    // (`cli_ask.zig:1193`). A saved one that is no longer there is not
    // something this invocation named, so it is skipped, not fatal.
    let shared = TempDir::new().expect("shared directory");
    let shared_root = shared.path().canonicalize().expect("canonicalize shared");
    let note = shared_root.join("shared.md");
    fs::write(&note, "saved root knowledge\n").expect("write shared file");

    let sandbox = Sandbox::new();
    let gone = sandbox.home.join("gone");
    fs::create_dir_all(sandbox.home.join(".xfx")).expect("profile");
    fs::write(
        sandbox.home.join(".xfx").join("settings.json"),
        json!({"workspaces": {sandbox.workspace.to_str().unwrap(): {
            "additional_directories": [gone.to_str().unwrap(), shared_root.to_str().unwrap()]
        }}})
        .to_string(),
    )
    .expect("settings");

    let gateway = FakeGateway::start(vec![
        read_file_reply("c1", note.to_str().unwrap()),
        Reply::Sse(sse_body(&[text_delta("a0", "read it"), finish("stop")])),
    ]);
    let run = sandbox.run(
        &["ask", "--json", "--no-save", "read", "the", "saved", "note"],
        &[
            ("AI_GATEWAY_API_KEY", TEST_KEY),
            ("XFX_GATEWAY_URL", &gateway.chat_url()),
        ],
    );
    assert_eq!(run.code, Some(0), "stderr={:?}", run.stderr);
    assert_eq!(run.events()[1]["ok"], json!(true), "{:?}", run.events());
    let second = gateway.requests()[1].json();
    let value = second["prompt"][2]["content"][0]["output"]["value"]
        .as_str()
        .unwrap();
    assert!(value.contains("saved root knowledge"), "{value}");
    run.assert_no_secret();
}

#[test]
fn add_dir_rejects_a_path_that_is_not_a_usable_directory() {
    let gateway = FakeGateway::start(Vec::new());
    let sandbox = Sandbox::new();
    let missing = sandbox.workspace.join("no-such-dir");
    let run = sandbox.run(
        &[
            "ask",
            "--json",
            "--no-save",
            "--add-dir",
            missing.to_str().unwrap(),
            "hello",
        ],
        &[
            ("AI_GATEWAY_API_KEY", TEST_KEY),
            ("XFX_GATEWAY_URL", &gateway.chat_url()),
        ],
    );
    assert_eq!(run.code, Some(1), "stdout={:?}", run.stdout);
    assert_eq!(run.kinds(), ["error"]);
    assert_eq!(
        gateway.request_count(),
        0,
        "an unusable root must fail before a request is sent"
    );
    run.assert_no_secret();
}

#[test]
fn the_ask_help_page_documents_add_dir() {
    let sandbox = Sandbox::new();
    let run = sandbox.run(&["ask", "--help"], &[]);
    assert_eq!(run.code, Some(0));
    assert!(run.stdout.contains("--add-dir"), "{}", run.stdout);
}

// ---------------------------------------------------------------------------
// what a turn records for `/usage`
// ---------------------------------------------------------------------------

/// A provider that answers each step with the next result of its script,
/// failures included.
struct FallibleProvider {
    results: std::cell::RefCell<VecDeque<Result<Completion, ProviderError>>>,
}

impl FallibleProvider {
    fn new(results: Vec<Result<Completion, ProviderError>>) -> Self {
        Self {
            results: std::cell::RefCell::new(results.into()),
        }
    }
}

#[async_trait::async_trait(?Send)]
impl Provider for FallibleProvider {
    async fn stream(
        &self,
        _request: &CompletionRequest,
        _deltas: &mut dyn DeltaSink,
    ) -> Result<Completion, ProviderError> {
        self.results
            .borrow_mut()
            .pop_front()
            .expect("the provider was called more times than the script allows")
    }
}

/// A Gateway bill for one request.
fn bill(
    model: &str,
    cost: f64,
    input: u64,
    output: u64,
    id: &str,
) -> xfx::gateway::protocol::Billing {
    xfx::gateway::protocol::Billing {
        at_ms: 1_700_000_000_000,
        model: model.to_string(),
        generation_id: Some(id.to_string()),
        cost,
        input_tokens: input,
        output_tokens: output,
        cache_read_tokens: input / 2,
        cache_write_tokens: 0,
        reasoning_tokens: Some(0),
    }
}

/// The one `usage_recorded` frame a turn journaled.
fn usage_frame(journal: &RecordingJournal) -> &SessionEvent {
    let mut frames = journal
        .events
        .iter()
        .filter(|event| matches!(event, SessionEvent::UsageRecorded { .. }));
    let frame = frames.next().expect("the turn recorded its usage");
    assert!(frames.next().is_none(), "one frame per turn");
    frame
}

/// Whether `program` is on this process's `PATH`, which the terminal tool
/// passes to its commands.
fn on_path(program: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(program).is_file()))
}

/// A command the terminal tool runs starts with no signal blocked, whichever
/// thread runs the tool. The TUI runs it on a thread that blocks every signal
/// the session owns, and a child born with that mask holds a `SIGTERM`
/// pending: `timeout 1 sleep 5` would have waited out the whole sleep
/// (`xfx::process`).
#[test]
fn a_timeout_inside_a_command_ends_it_on_time_from_a_thread_that_blocks_signals() {
    // `timeout` is coreutils'; where it is not installed, the shell does what
    // it does -- `TERM` to the command after a second.
    let command = if on_path("timeout") {
        "timeout 1 sleep 5"
    } else {
        "sleep 5 & pid=$!; (sleep 1; kill -TERM $pid) & wait $pid"
    };
    let (result, took) = std::thread::spawn(move || {
        // SAFETY: `set` is a valid set this closure owns for the call.
        unsafe {
            let mut set: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut set);
            for signal in [
                libc::SIGINT,
                libc::SIGTERM,
                libc::SIGHUP,
                libc::SIGTSTP,
                libc::SIGCONT,
                libc::SIGWINCH,
            ] {
                libc::sigaddset(&mut set, signal);
            }
            assert_eq!(
                libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut()),
                0
            );
        }
        let tree = Tree::new();
        let tools = context(&tree).with_permissions(PermissionSession::new(PermissionMode::Yolo));
        let started = std::time::Instant::now();
        let result = call(
            &tools,
            "terminal",
            json!({ "action": "exec", "command": command }),
        );
        (result, started.elapsed())
    })
    .join()
    .expect("the tool's thread");
    assert!(
        took >= std::time::Duration::from_millis(900) && took < std::time::Duration::from_secs(3),
        "{command} took {took:?}: {result:?}"
    );
}

#[tokio::test]
async fn every_request_of_a_turn_is_recorded_with_what_it_cost_and_what_it_changed() {
    let tree = Tree::new();
    let tools = context(&tree).with_permissions(PermissionSession::new(PermissionMode::Yolo));
    let mut first = calls_step(vec![write_call("c1", "new.md", "one\ntwo\n")]);
    first.completion.billing = Some(Bill::Exact(bill(
        "fake/model-a",
        0.0125,
        12000,
        800,
        "gen_01JABCDEFGHJKMNPQRSTVWXYZ0",
    )));
    let mut second = final_step("done");
    second.completion.billing = Some(Bill::Exact(bill(
        "anthropic/claude-opus-5",
        0.421,
        52000,
        3100,
        "gen_01JABCDEFGHJKMNPQRSTVWXYZ1",
    )));
    second.completion.usage = Usage {
        input_tokens: Some(52000),
        output_tokens: Some(3100),
    };
    let provider = ScriptedProvider::new(vec![first, second]);
    let mut journal = RecordingJournal::default();
    run_turn_saved(
        turn("write it", tools),
        ProjectContext::none(),
        &provider,
        &mut RecordingSink::new(),
        &mut journal,
    )
    .await
    .expect("the turn completes");

    let SessionEvent::UsageRecorded {
        input_tokens,
        output_tokens,
        turn_output_tokens,
        requests,
        unaccounted,
        lines_added,
        lines_removed,
        settled,
        rejected,
    } = usage_frame(&journal)
    else {
        unreachable!()
    };
    // The meter's two totals are still the answering step's.
    assert_eq!((*input_tokens, *output_tokens), (Some(52000), Some(3100)));
    // The summary's out is every request's: the first reported no usage and
    // streamed nothing -- its call arrived whole -- so it adds nothing.
    assert_eq!(*turn_output_tokens, Some(3100));
    let billed: Vec<(&str, Option<f64>, Option<&str>)> = requests
        .iter()
        .map(|request| {
            (
                request.model.as_str(),
                request.cost,
                request.generation_id.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        billed,
        [
            (
                "fake/model-a",
                Some(0.0125),
                Some("gen_01JABCDEFGHJKMNPQRSTVWXYZ0")
            ),
            (
                "anthropic/claude-opus-5",
                Some(0.421),
                Some("gen_01JABCDEFGHJKMNPQRSTVWXYZ1")
            ),
        ]
    );
    assert_eq!(requests[0].cache_read_tokens, 6000);
    assert_eq!(requests[0].at_ms, 1_700_000_000_000);
    assert!(!unaccounted);
    assert_eq!((*lines_added, *lines_removed), (2, 0));
    // A turn's own frame settles nothing: that is the runtime's, later.
    assert!(settled.is_empty() && rejected.is_empty());
    assert_eq!(
        journal.kinds()[journal.kinds().len() - 2..],
        ["usage_recorded", "turn_concluded"]
    );
}

#[tokio::test]
async fn a_request_nobody_billed_is_recorded_as_asked_with_no_cost() {
    let tree = Tree::new();
    let mut answer = final_step("hi");
    answer.completion.usage = Usage {
        input_tokens: Some(1200),
        output_tokens: Some(40),
    };
    let provider = ScriptedProvider::new(vec![answer]);
    let mut journal = RecordingJournal::default();
    run_turn_saved(
        turn("hi", context(&tree)),
        ProjectContext::none(),
        &provider,
        &mut RecordingSink::new(),
        &mut journal,
    )
    .await
    .expect("the turn completes");
    let SessionEvent::UsageRecorded { requests, .. } = usage_frame(&journal) else {
        unreachable!()
    };
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request.model, "vendor/model");
    assert_eq!((request.input_tokens, request.output_tokens), (1200, 40));
    assert_eq!(
        (request.cost, request.generation_id.as_deref()),
        (None, None)
    );
    assert!(request.at_ms > 0);
}

/// Upstream's `.deferred` (`gatewayUsageOutcome`, `gateway.zig:650-660`): a
/// request whose stream named its generation and not its bill is recorded
/// pending -- no cost yet, and the id and the origin to ask it of.
#[tokio::test]
async fn a_request_whose_bill_comes_later_is_recorded_pending_with_where_to_ask() {
    let tree = Tree::new();
    let mut answer = final_step("hi");
    answer.completion.usage = Usage {
        input_tokens: Some(1200),
        output_tokens: Some(40),
    };
    answer.completion.billing = Some(Bill::Deferred(Deferred {
        generation_id: "gen_01JABCDEFGHJKMNPQRSTVWXYZ2".to_string(),
        origin: "http://127.0.0.1:4100".to_string(),
    }));
    let provider = ScriptedProvider::new(vec![answer]);
    let mut journal = RecordingJournal::default();
    run_turn_saved(
        turn("hi", context(&tree)),
        ProjectContext::none(),
        &provider,
        &mut RecordingSink::new(),
        &mut journal,
    )
    .await
    .expect("the turn completes");
    let SessionEvent::UsageRecorded { requests, .. } = usage_frame(&journal) else {
        unreachable!()
    };
    let request = &requests[0];
    assert!(request.pending());
    assert_eq!(request.cost, None);
    assert_eq!(
        request.generation_id.as_deref(),
        Some("gen_01JABCDEFGHJKMNPQRSTVWXYZ2")
    );
    assert_eq!(request.origin.as_deref(), Some("http://127.0.0.1:4100"));
    assert_eq!((request.input_tokens, request.output_tokens), (1200, 40));
}

#[tokio::test]
async fn a_request_lost_after_it_was_sent_is_recorded_as_spend_nobody_reported() {
    let tree = Tree::new();
    let provider = FallibleProvider::new(vec![
        Ok(calls_step(vec![read_call("c1", "missing.md")]).completion),
        Err(ProviderError::Transport {
            subject: "the Gateway",
            detail: "the connection reset".to_string(),
        }),
    ]);
    let mut journal = RecordingJournal::default();
    run_turn_saved(
        turn("read it", context(&tree)),
        ProjectContext::none(),
        &provider,
        &mut RecordingSink::new(),
        &mut journal,
    )
    .await
    .expect_err("a broken stream ends the turn");
    let SessionEvent::UsageRecorded {
        input_tokens,
        output_tokens,
        requests,
        unaccounted,
        ..
    } = usage_frame(&journal)
    else {
        unreachable!()
    };
    // A failed turn had no answering step, so it has no totals.
    assert_eq!((*input_tokens, *output_tokens), (None, None));
    assert_eq!(requests.len(), 1, "the step that completed is kept");
    assert!(*unaccounted, "the lost one may have been billed");
}

#[tokio::test]
async fn a_request_the_endpoint_refused_records_nothing_about_spend() {
    let tree = Tree::new();
    let provider = FallibleProvider::new(vec![Err(ProviderError::Status {
        subject: "the Gateway",
        status: 400,
        body: "bad request".to_string(),
        retryable: false,
        retry_after: None,
    })]);
    let mut journal = RecordingJournal::default();
    run_turn_saved(
        turn("hi", context(&tree)),
        ProjectContext::none(),
        &provider,
        &mut RecordingSink::new(),
        &mut journal,
    )
    .await
    .expect_err("a refusal ends the turn");
    assert_eq!(journal.kinds(), ["user_message", "turn_concluded"]);
}

// ---------------------------------------------------------------------------
// the turn's out, as its summary row counts it
// ---------------------------------------------------------------------------

/// One piece of what an attempt streams, by the stream it arrives on.
enum Chunk {
    Text(&'static str),
    Reasoning(&'static str),
    ToolInput(&'static str),
}

/// One scripted attempt: what it streams, then how it ends.
type Attempt = (Vec<Chunk>, Result<Completion, ProviderError>);

/// A provider whose every attempt streams its chunks, then ends as scripted.
struct StreamingProvider {
    attempts: std::cell::RefCell<VecDeque<Attempt>>,
}

impl StreamingProvider {
    fn new(attempts: Vec<Attempt>) -> Self {
        Self {
            attempts: std::cell::RefCell::new(attempts.into()),
        }
    }
}

#[async_trait::async_trait(?Send)]
impl Provider for StreamingProvider {
    async fn stream(
        &self,
        _request: &CompletionRequest,
        deltas: &mut dyn DeltaSink,
    ) -> Result<Completion, ProviderError> {
        let (chunks, end) = self
            .attempts
            .borrow_mut()
            .pop_front()
            .expect("the provider was called more times than the script allows");
        for chunk in chunks {
            match chunk {
                Chunk::Text(text) => deltas.text_delta(text).map_err(ProviderError::Sink)?,
                Chunk::Reasoning(text) => deltas.reasoning_delta(text),
                Chunk::ToolInput(text) => deltas.tool_input_delta(text),
            }
        }
        end
    }
}

/// What the turn told its reader about its out, in order.
#[derive(Default)]
struct Counted {
    progress: Vec<Progress>,
}

impl xfx::output::EventSink for Counted {
    fn emit(&mut self, _event: &Event) -> std::io::Result<()> {
        Ok(())
    }

    fn output_tokens(&mut self, progress: Progress) {
        self.progress.push(progress);
    }
}

/// A completion with `output` as the provider's count of what it wrote.
fn reporting(step: ScriptedStep, output: Option<u64>) -> Result<Completion, ProviderError> {
    let mut completion = step.completion;
    completion.usage = Usage {
        input_tokens: output.map(|_| 1200),
        output_tokens: output,
    };
    Ok(completion)
}

/// Runs `request` against `provider`, and what its sink heard and its usage
/// frame kept: `(progress, turn_output_tokens, output_tokens)`.
async fn counted(
    request: TurnRequest,
    provider: &StreamingProvider,
) -> (Vec<Progress>, Option<u64>, Option<u64>) {
    let mut sink = Counted::default();
    let mut journal = RecordingJournal::default();
    let _ = run_turn_saved(
        request,
        ProjectContext::none(),
        provider,
        &mut sink,
        &mut journal,
    )
    .await;
    let SessionEvent::UsageRecorded {
        turn_output_tokens,
        output_tokens,
        ..
    } = usage_frame(&journal)
    else {
        unreachable!()
    };
    (sink.progress, *turn_output_tokens, *output_tokens)
}

#[tokio::test]
async fn a_turn_counts_out_over_every_request_it_made() {
    // fx's `allowlist-turn`: a call, then the answer, 40 each, is `↓80`
    // (`vercel-labs/fx@c1db9191 src/core/agent/runtime/telemetry.zig:165-168`).
    // Red before: the turn said nothing until it ended, and then the
    // answering step's 40.
    let tree = Tree::new();
    let provider = StreamingProvider::new(vec![
        (
            Vec::new(),
            reporting(calls_step(vec![read_call("c1", "missing.md")]), Some(40)),
        ),
        (
            vec![Chunk::Text("done")],
            reporting(final_step("done"), Some(40)),
        ),
    ]);
    let (progress, turn, answering) = counted(turn("read it", context(&tree)), &provider).await;
    assert_eq!(progress, [Progress::Settled(40), Progress::Settled(80)]);
    assert_eq!(turn, Some(80));
    assert_eq!(
        answering,
        Some(40),
        "the meter's count is still the answer's"
    );
}

#[tokio::test]
async fn a_request_with_no_usage_counts_the_estimate_of_all_it_streamed() {
    // Its reasoning and its call's arguments as well as its text, each
    // estimated apart (`telemetry.zig:25-35`, `:140`): `let me look` is 3,
    // `{"path":"a.md"}` is 4, and `all done here` is 3. What the reader is not
    // shown is told as it moves; the answer it counts for itself.
    let tree = Tree::new();
    let provider = StreamingProvider::new(vec![
        (
            vec![
                Chunk::Reasoning("let me look"),
                Chunk::ToolInput("{\"path\":"),
                Chunk::ToolInput("\"a.md\"}"),
            ],
            reporting(calls_step(vec![read_call("c1", "a.md")]), None),
        ),
        (
            vec![Chunk::Text("all done "), Chunk::Text("here")],
            reporting(final_step("all done here"), None),
        ),
    ]);
    let (progress, turn, answering) = counted(turn("read it", context(&tree)), &provider).await;
    assert_eq!(
        progress,
        [
            Progress::Unshown(3),
            Progress::Unshown(5),
            Progress::Unshown(7),
            Progress::Settled(7),
            Progress::Settled(10),
        ]
    );
    assert_eq!((turn, answering), (Some(10), None));
}

#[tokio::test]
async fn a_retried_attempt_counts_what_it_streamed_before_it_failed() {
    // Upstream opens a token request per attempt and settles a failed one at
    // its estimate (`orchestrator.zig:7493`, `:7577`). Reasoning is not an
    // answer the user saw, so the attempt is still replayed.
    let tree = Tree::new();
    let mut request = turn("hi", context(&tree));
    request.max_attempts = 2;
    let provider = StreamingProvider::new(vec![
        (
            vec![Chunk::Reasoning("thinking hard")],
            Err(ProviderError::Status {
                subject: "the Gateway",
                status: 503,
                body: "busy".to_string(),
                retryable: true,
                retry_after: Some(std::time::Duration::ZERO),
            }),
        ),
        (
            vec![Chunk::Text("ok")],
            reporting(final_step("ok"), Some(40)),
        ),
    ]);
    let (progress, turn, _) = counted(request, &provider).await;
    assert_eq!(
        progress,
        [
            Progress::Unshown(3),
            Progress::Settled(3),
            Progress::Settled(43),
        ]
    );
    assert_eq!(turn, Some(43));
}

/// Streams `half an ans`, sends a steer into the turn and never answers its
/// first call, as a long answer a steer cuts short; then answers `ok`,
/// reporting 40.
struct SteerCutProvider {
    steering: Steering,
    calls: std::cell::Cell<usize>,
}

#[async_trait::async_trait(?Send)]
impl Provider for SteerCutProvider {
    async fn stream(
        &self,
        _request: &CompletionRequest,
        deltas: &mut dyn DeltaSink,
    ) -> Result<Completion, ProviderError> {
        let call = self.calls.get();
        self.calls.set(call + 1);
        if call == 0 {
            deltas
                .text_delta("half an ans")
                .map_err(ProviderError::Sink)?;
            assert!(self.steering.offer(1, "change tone".to_string()));
            return std::future::pending().await;
        }
        reporting(final_step("ok"), Some(40))
    }
}

#[tokio::test]
async fn a_request_a_steer_cut_short_counts_what_it_streamed() {
    // The steer's own boundary (`append_immediate_steering_after_cancel`,
    // `orchestrator.zig:240-261`) ends the attempt as a stop does, and upstream
    // settles every attempt that ends without usage at its estimate
    // (`finishTokenRequestWithoutUsage`, `telemetry.zig:149-155`): the cut
    // answer's 3 and then the continued step's 40.
    let tree = Tree::new();
    let steering = Steering::new();
    steering.open();
    let provider = SteerCutProvider {
        steering: steering.clone(),
        calls: std::cell::Cell::new(0),
    };
    let mut sink = Counted::default();
    let mut journal = RecordingJournal::default();
    xfx::agent::TurnMachine::new(turn("hi", context(&tree)))
        .with_steering(steering)
        .run_journaled(&provider, &mut sink, &mut journal)
        .await
        .expect("the turn completes");
    assert_eq!(sink.progress, [Progress::Settled(3), Progress::Settled(43)]);
    let SessionEvent::UsageRecorded {
        turn_output_tokens,
        output_tokens,
        ..
    } = usage_frame(&journal)
    else {
        unreachable!()
    };
    assert_eq!((*turn_output_tokens, *output_tokens), (Some(43), Some(40)));
}

#[tokio::test]
async fn a_command_the_person_stopped_leaves_the_request_that_asked_for_it_counted() {
    // The stop reaches a running command at once (`terminal::supervise`), and
    // the turn still settles: the request that asked for the command is in the
    // turn's frame, for `/usage`, at what it reported.
    let tree = Tree::new();
    let cancel = CancelToken::new();
    let prompter = ScriptedAmender::new(vec![Amendment::plain()], cancel.clone());
    let provider = StreamingProvider::new(vec![(
        vec![Chunk::ToolInput("{\"command\":\"sleep 30\"}")],
        reporting(
            calls_step(vec![ToolCall {
                id: "c1".to_string(),
                name: "terminal".to_string(),
                input: json!({ "action": "exec", "command": "sleep 30" }),
            }]),
            Some(40),
        ),
    )]);
    // From another thread: the command holds this one while it runs.
    let stopper = cancel.clone();
    let stop = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(300));
        stopper.cancel();
    });
    let mut sink = Counted::default();
    let mut journal = RecordingJournal::default();
    let started = std::time::Instant::now();
    let _ = run_turn_saved(
        asking_turn(
            "run it",
            // The tools hold the turn's token, as the TUI's conversation does.
            asking_context(&tree, ToolLimits::default(), prompter).with_cancel(cancel.clone()),
            cancel,
        ),
        ProjectContext::none(),
        &provider,
        &mut sink,
        &mut journal,
    )
    .await;
    stop.join().expect("the stopper");
    assert!(
        started.elapsed() < std::time::Duration::from_secs(10),
        "the command ran on"
    );
    assert_eq!(sink.progress.last(), Some(&Progress::Settled(40)));
    let SessionEvent::UsageRecorded {
        turn_output_tokens,
        requests,
        ..
    } = usage_frame(&journal)
    else {
        unreachable!()
    };
    assert_eq!(*turn_output_tokens, Some(40));
    assert_eq!(
        requests
            .iter()
            .map(|request| request.output_tokens)
            .collect::<Vec<_>>(),
        [40]
    );
}

#[tokio::test]
async fn a_lost_or_stopped_request_counts_what_it_streamed() {
    // A stream that broke, and one the user stopped, each after part of an
    // answer: no usage will ever come, and the estimate stands for it
    // (`finishTokenRequestWithoutUsage`, `telemetry.zig:149-155`). The turn
    // fails, and its frame keeps what its summary would have said.
    for lost in [
        ProviderError::Transport {
            subject: "the Gateway",
            detail: "the connection reset".to_string(),
        },
        ProviderError::Cancelled,
    ] {
        let tree = Tree::new();
        let provider = StreamingProvider::new(vec![(vec![Chunk::Text("half an ans")], Err(lost))]);
        let (progress, turn, answering) = counted(turn("hi", context(&tree)), &provider).await;
        assert_eq!(progress, [Progress::Settled(3)]);
        assert_eq!((turn, answering), (Some(3), None));
    }
}
