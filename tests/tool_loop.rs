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

use serde_json::{json, Value};
use tempfile::TempDir;

use xfx::agent::{run_turn, TurnError, TurnRequest};
use xfx::gateway::protocol::{
    Completion, CompletionRequest, FinishReason, ToolCall, ToolChoice, Usage,
};
use xfx::gateway::{CancelToken, DeltaSink, Provider, ProviderError};
use xfx::output::{Event, RecordingSink};
use xfx::tools::{Registry, ToolContext, ToolLimits, ToolResult, ADVERTISED_TOOLS};
use xfx::workspace::{AccessScope, PathError};

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
            assert!(
                matches!(
                    property["type"].as_str(),
                    Some("string" | "integer" | "boolean")
                ),
                "`{name}` in {schema} has an unbounded type"
            );
            assert!(
                property["description"]
                    .as_str()
                    .is_some_and(|d| !d.is_empty()),
                "`{name}` in {schema} has no description"
            );
        }
        for required in input["required"].as_array().unwrap_or(&Vec::new()) {
            let name = required.as_str().expect("a required name is a string");
            assert!(
                properties.contains_key(name),
                "`{name}` is required by {schema} but is not a property"
            );
        }
    }
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
        "ask_user_question",
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
    assert!(
        result.output.contains("it is not a regular file"),
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
        },
    }
}

fn turn(prompt: &str, tools: ToolContext) -> TurnRequest {
    TurnRequest {
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
