//! The closed tool registry.
//!
//! The registry is a `static` table of four read-only specs. It is not
//! configurable, not extensible at runtime, and not merged with anything: what
//! xfx advertises to a model is this list, in this order, and the code that runs
//! is the same object that was advertised
//! (`vercel-labs/fx@580a0c5d src/builtins/tools.zig:1351-1380`).
//!
//! Two consequences are the point:
//!
//! - **Advertisement is a promise.** [`ADVERTISED_TOOLS`] is reconciled against
//!   `docs/parity.md` by `scripts/check-no-stubs.sh`, so a tool cannot reach a
//!   model schema before its parity row says `implemented`.
//! - **A call for anything else is not answered.** [`Registry::execute`] returns
//!   [`UnadvertisedTool`] rather than inventing a result, and the turn ends
//!   rather than continuing on a false premise.

pub mod mutate;
pub mod question;
pub mod read;
pub mod spec;
pub mod terminal;

use std::fmt;

use serde_json::Value;

use crate::gateway::protocol::ToolCall;

pub use question::QuestionRequester;
pub use spec::{
    InputSchema, PermissionKind, Property, PropertyKind, RaceInterlude, ToolContext, ToolDecoder,
    ToolExecutor, ToolInput, ToolLimits, ToolResult, ToolSession, ToolSpec, ToolValidator,
};

/// Every tool name this build advertises, in registry order.
///
/// `scripts/check-no-stubs.sh` reads this declaration textually and requires an
/// `implemented` row in `docs/parity.md` for each name.
pub const ADVERTISED_TOOLS: &[&str] = &[
    "list_files",
    "glob_files",
    "grep_files",
    "read_file",
    "write_file",
    "edit_file",
    "create_folder",
    "terminal",
    "ask_user_question",
];

/// The specs themselves, in upstream's order (`tools.zig:1352-1367`): the read
/// group, then the mutation group, then the terminal.
static BUILTIN_TOOLS: &[ToolSpec] = &[
    read::LIST_FILES,
    read::GLOB_FILES,
    read::GREP_FILES,
    read::READ_FILE,
    mutate::WRITE_FILE,
    mutate::EDIT_FILE,
    mutate::CREATE_FOLDER,
    terminal::TERMINAL,
    question::ASK_USER_QUESTION,
];

/// A tool call xfx never offered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnadvertisedTool {
    pub name: String,
}

impl fmt::Display for UnadvertisedTool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "`{}` is not a tool this build advertises; the advertised tools are {}",
            self.name,
            ADVERTISED_TOOLS.join(", ")
        )
    }
}

impl std::error::Error for UnadvertisedTool {}

/// The closed set of tools a turn may run.
#[derive(Debug, Clone, Copy)]
pub struct Registry {
    specs: &'static [ToolSpec],
}

impl Registry {
    /// The one registry this build has.
    pub const fn builtin() -> Self {
        Self {
            specs: BUILTIN_TOOLS,
        }
    }

    pub fn specs(&self) -> &'static [ToolSpec] {
        self.specs
    }

    /// The advertised names, in order.
    pub fn names(&self) -> Vec<&'static str> {
        self.specs.iter().map(ToolSpec::name).collect()
    }

    pub fn spec(&self, name: &str) -> Option<&'static ToolSpec> {
        self.specs.iter().find(|spec| spec.name() == name)
    }

    /// The `tools` array of a Gateway request: one closed schema per spec.
    pub fn advertisement(&self) -> Vec<Value> {
        self.specs.iter().map(ToolSpec::advertisement).collect()
    }

    /// Runs one model tool call.
    ///
    /// Ok(result) covers both "it worked" and "it refused": a refusal is
    /// something the model can act on, so it travels back as a correlated tool
    /// result. `Err` is reserved for the one case the turn cannot represent --
    /// a tool that was never offered.
    ///
    /// Permission admission is *not* here. It is inside the executors that need
    /// it, because a decision has to be made about a prepared plan -- an exact
    /// target with its exact preimage, an exact argv with its exact cwd -- and
    /// only the executor can produce one. A gate at this level would have to
    /// decide from the raw arguments, which is the mistake this design exists to
    /// avoid: it would judge a path the model wrote rather than the file that
    /// path currently resolves to.
    ///
    /// What is asserted here instead is that every [`PermissionKind`] that
    /// requires an authority belongs to a spec whose executor mints one; see
    /// `every_mutating_spec_goes_through_a_permission_decision`.
    pub fn execute(
        &self,
        call: &ToolCall,
        context: &ToolContext,
    ) -> Result<ToolResult, UnadvertisedTool> {
        let Some(spec) = self.spec(&call.name) else {
            return Err(UnadvertisedTool {
                name: call.name.clone(),
            });
        };
        let mut result = spec.run(&call.input, context);
        // A backstop, not the bound. Each executor caps its own output; this
        // guarantees the property for all of them in one place, and says so
        // rather than truncating quietly.
        if let Some(clipped) = spec::clip(&result.output, context.limits().max_output_bytes) {
            result.output = format!(
                "{clipped}\n... [tool output truncated at {} bytes]",
                context.limits().max_output_bytes
            );
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::AccessScope;
    use serde_json::json;

    fn context() -> (tempfile::TempDir, ToolContext) {
        let dir = tempfile::tempdir().expect("temporary workspace");
        let scope = AccessScope::primary_only(dir.path()).expect("scope");
        (dir, ToolContext::new(scope))
    }

    #[test]
    fn the_eight_original_schemas_are_byte_identical() {
        // Captured at the tip before the array extension. A byte that moves here is
        // a change to what every model already sees.
        const PINNED: &[(&str, &str)] = &[
            (
                "list_files",
                r#"{"description":"List the entries of one directory, one level deep, without reading file contents. Paths are relative to the workspace root, or absolute inside an authorized root; anything else is refused. Directories end in /, symlinks in @. Entries named .git, node_modules, dist, build, coverage, .next, zig-out, or .zig-cache are always omitted; everything else, including dotfiles, is listed. When to use: inspect a known folder, confirm a name, or choose the next path to read. When NOT to use: recursive discovery, content search, or a shell ls.","inputSchema":{"additionalProperties":false,"properties":{"path":{"description":"Directory to list. Defaults to the workspace root.","type":"string"}},"type":"object"},"name":"list_files","type":"function"}"#,
            ),
            (
                "glob_files",
                r#"{"description":"Find file paths matching a glob pattern below one directory, with mode=count for an exact count without listing. Paths are relative to the workspace root, or absolute inside an authorized root. The search does not see everything: symlinks are not followed, build directories such as .git and node_modules are pruned, paths excluded by .gitignore are skipped, and hidden dot-paths are skipped unless the pattern itself names one (for example .github/**/*.yml). Results are sorted, and a capped or incomplete search says so on its own ... line. When to use: locate files by name, extension, or directory shape. When NOT to use: search file contents, read a file, or count non-file things.","inputSchema":{"additionalProperties":false,"properties":{"mode":{"description":"Use matches to list paths, or count for an exact count without listing.","enum":["matches","count"],"type":"string"},"path":{"description":"Directory to search below. Defaults to the workspace root.","type":"string"},"pattern":{"description":"Glob pattern to match, such as src/**/*.rs or *.md.","type":"string"}},"required":["pattern"],"type":"object"},"name":"glob_files","type":"function"}"#,
            ),
            (
                "grep_files",
                r#"{"description":"Search text files for a literal substring, optionally narrowed by path and include glob, with modes for matching lines, files with matches, or counts, plus head_limit/offset paging and bounded context_lines. Paths are relative to the workspace root, or absolute inside an authorized root. Regular expressions are not supported: the pattern is matched literally. The search does not see every file: symlinks are not followed, build directories such as .git and node_modules are pruned, paths excluded by .gitignore are skipped, hidden dot-paths are skipped unless the include glob itself names one, and files that are not UTF-8 text or are above the size cap are not searched. Any file skipped for the last two reasons is counted on a ... skipped line, so no matches means no matches among the files actually searched. When to use: find an exact symbol, string, or usage site. When NOT to use: filename lookup, reading a known path, or regex search.","inputSchema":{"additionalProperties":false,"properties":{"case_insensitive":{"description":"Match without regard to case.","type":"boolean"},"context_lines":{"description":"Lines to show before and after each match. Bounded by the tool.","type":"integer"},"head_limit":{"description":"Positive maximum results to return. Defaults to the output cap.","type":"integer"},"include":{"description":"Glob applied to candidate paths before any file is read, such as *.rs.","type":"string"},"mode":{"description":"Use matches for lines, files_with_matches for paths, or count for exact counts.","enum":["matches","files_with_matches","count"],"type":"string"},"offset":{"description":"Zero-based result offset for paging. Defaults to 0.","type":"integer"},"path":{"description":"Directory to search below. Defaults to the workspace root.","type":"string"},"pattern":{"description":"Literal substring to search for. Not a regular expression.","type":"string"}},"required":["pattern"],"type":"object"},"name":"grep_files","type":"function"}"#,
            ),
            (
                "read_file",
                r#"{"description":"Read one UTF-8 text file as bounded, line-numbered output, with an optional start_line/line_count range. Paths are relative to the workspace root, or absolute inside an authorized root. Output states how many of the file's lines it showed, so a partial read is never mistaken for the whole file. When to use: inspect an exact known path. When NOT to use: list a directory, search many files, or read binary data.","inputSchema":{"additionalProperties":false,"properties":{"line_count":{"description":"Positive number of lines to return. Defaults to the read cap.","type":"integer"},"path":{"description":"Path relative to the workspace root, or an absolute path inside an authorized root.","type":"string"},"start_line":{"description":"1-based first line to return. Defaults to 1.","type":"integer"}},"required":["path"],"type":"object"},"name":"read_file","type":"function"}"#,
            ),
            (
                "write_file",
                r#"{"description":"Create a file, or replace an existing file's entire contents. An existing file must have been read in full with read_file first, and must not have changed since that read; otherwise the call is refused so that unseen content is never discarded. The replacement is staged in the same directory and renamed into place, so a reader never sees a half-written file, and the previous permission bits are preserved. When to use: add a new file, or intentionally rewrite a small or generated one. When NOT to use: a focused change to an existing file (use edit_file), creating directories (use create_folder), or deleting anything.","inputSchema":{"additionalProperties":false,"properties":{"content":{"description":"The complete new contents of the file.","type":"string"},"path":{"description":"Path to change, relative to the workspace root or absolute inside an authorized root. Every component is opened without following symbolic links, so a path through a link is refused rather than redirected. `..` components are refused; name the path from the workspace root instead.","type":"string"}},"required":["path","content"],"type":"object"},"name":"write_file","type":"function"}"#,
            ),
            (
                "edit_file",
                r#"{"description":"Replace exactly one occurrence of old_string with new_string in an existing UTF-8 text file. The file must have been read in full with read_file first and must not have changed since. old_string must appear exactly once: if it appears zero times or more than once the call is refused rather than guessing, so include enough surrounding text to make it unique. An edit whose result equals the current contents changes nothing and says so. When to use: a focused patch after reading the file. When NOT to use: whole-file rewrites (use write_file), ambiguous repeated text, or files you have not read.","inputSchema":{"additionalProperties":false,"properties":{"new_string":{"description":"The text to put in its place.","type":"string"},"old_string":{"description":"The exact text to replace. Must occur exactly once in the file.","type":"string"},"path":{"description":"Path to change, relative to the workspace root or absolute inside an authorized root. Every component is opened without following symbolic links, so a path through a link is refused rather than redirected. `..` components are refused; name the path from the workspace root instead.","type":"string"}},"required":["path","old_string","new_string"],"type":"object"},"name":"edit_file","type":"function"}"#,
            ),
            (
                "create_folder",
                r#"{"description":"Create a directory, including any missing parent directories. Existing directories are left alone and reported as already present. When to use: prepare a location for files you are about to write. When NOT to use: create files, inspect a directory (use list_files), or build speculative structure the task did not ask for.","inputSchema":{"additionalProperties":false,"properties":{"path":{"description":"Path to change, relative to the workspace root or absolute inside an authorized root. Every component is opened without following symbolic links, so a path through a link is refused rather than redirected. `..` components are refused; name the path from the workspace root instead.","type":"string"}},"required":["path"],"type":"object"},"name":"create_folder","type":"function"}"#,
            ),
            (
                "terminal",
                r#"{"description":"Run one command in the workspace and return its captured result: exit status, standard output, and standard error. Set action to exec. A recognized read-only command runs as an exact argument list with no shell, so quoting, globbing, variable substitution, redirection, and operators such as |, &&, ;, and > are not expanded and take the command off the automatic route. Commands that compile or run project code always need approval even though the automatic mode may have written the files they would compile; this includes cargo test, build, check, clippy, bench, run, and fmt, because a cargo alias in .cargo/config.toml can redirect any subcommand that is not a cargo built-in. The automatic cargo surface is cargo --version, cargo -V, cargo --list, and cargo metadata --no-deps. Operands must be relative, must not contain .., and must not resolve outside the authorized roots. Anything else needs an explicit approval before it runs. Output is captured, not streamed, and is truncated past a fixed size; the command is killed if it outruns its time limit. There is no sandbox: an approved command runs with the invoking user's privileges. When to use: build, test, lint, or inspect version control state. When NOT to use: reading or searching files (use the file tools), long-lived or interactive processes, or anything that publishes, installs, deletes, or reaches the network.","inputSchema":{"additionalProperties":false,"properties":{"action":{"description":"The only supported action is exec: run one command and capture its result.","enum":["exec"],"type":"string"},"command":{"description":"The command to run, as one line.","type":"string"},"cwd":{"description":"Directory to run in, inside an authorized root. Defaults to the workspace root.","type":"string"}},"required":["action","command"],"type":"object"},"name":"terminal","type":"function"}"#,
            ),
        ];
        let registry = Registry::builtin();
        assert_eq!(PINNED.len(), 8);
        for (name, expected) in PINNED {
            let spec = registry.spec(name).expect("an advertised tool");
            assert_eq!(
                &serde_json::to_string(&spec.advertisement()).unwrap(),
                expected,
                "`{name}`'s advertised schema moved"
            );
        }
    }

    #[test]
    fn the_declared_inventory_is_the_registry() {
        // The textual inventory `scripts/check-no-stubs.sh` reads and the table
        // the product actually runs cannot drift apart.
        assert_eq!(Registry::builtin().names(), ADVERTISED_TOOLS);
    }

    #[test]
    fn every_spec_declares_the_authority_its_effects_need() {
        // The registry's contract in one table: reading is free, changing a file
        // or starting a process is not. A new spec that forgets to declare its
        // kind fails here rather than at a user's expense.
        let expected = [
            ("list_files", PermissionKind::ReadOnly),
            ("glob_files", PermissionKind::ReadOnly),
            ("grep_files", PermissionKind::ReadOnly),
            ("read_file", PermissionKind::ReadOnly),
            ("write_file", PermissionKind::MutateFile),
            ("edit_file", PermissionKind::MutateFile),
            ("create_folder", PermissionKind::MutateFile),
            ("terminal", PermissionKind::RunCommand),
            ("ask_user_question", PermissionKind::Interaction),
        ];
        let actual: Vec<(&str, PermissionKind)> = Registry::builtin()
            .specs()
            .iter()
            .map(|spec| (spec.name(), spec.permission()))
            .collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn every_mutating_spec_goes_through_a_permission_decision() {
        // A context whose session is `ask` with no approval channel can admit
        // nothing. Every spec that declares it needs an authority must therefore
        // refuse, which is what proves the executor consults policy rather than
        // declaring a kind and ignoring it.
        let (_dir, context) = context();
        let arguments = [
            ("write_file", json!({ "path": "a.txt", "content": "x" })),
            (
                "edit_file",
                json!({ "path": "a.txt", "old_string": "a", "new_string": "b" }),
            ),
            ("create_folder", json!({ "path": "made" })),
            ("terminal", json!({ "action": "exec", "command": "pwd" })),
        ];
        for (name, input) in arguments {
            let spec = Registry::builtin().spec(name).expect("advertised");
            assert!(spec.permission().requires_authority(), "{name}");
            let result = Registry::builtin()
                .execute(
                    &ToolCall {
                        id: "c1".to_string(),
                        name: name.to_string(),
                        input,
                    },
                    &context,
                )
                .expect("the tool is advertised");
            assert!(!result.ok, "{name} ran without an authority: {result:?}");
        }
    }

    #[test]
    fn no_two_specs_share_a_name() {
        let mut names = Registry::builtin().names();
        names.sort();
        let unique = names.len();
        names.dedup();
        assert_eq!(names.len(), unique);
    }

    #[test]
    fn every_required_field_is_a_declared_property() {
        for spec in Registry::builtin().specs() {
            let schema = spec.input_schema();
            for required in schema.required {
                assert!(
                    schema
                        .properties
                        .iter()
                        .any(|property| property.name == *required),
                    "{} requires an undeclared `{required}`",
                    spec.name()
                );
            }
        }
    }

    #[test]
    fn a_call_for_an_unadvertised_tool_is_not_answered() {
        let (_dir, context) = context();
        let err = Registry::builtin()
            .execute(
                &ToolCall {
                    id: "c1".to_string(),
                    // Genuinely deferred: `delete_file` is an upstream tool
                    // with a `deferred` parity row, so this stays a real case
                    // rather than one that expires the moment a tool lands.
                    name: "delete_file".to_string(),
                    input: json!({}),
                },
                &context,
            )
            .expect_err("delete_file is not advertised");
        assert_eq!(err.name, "delete_file");
        let message = err.to_string();
        assert!(message.contains("delete_file"), "{message}");
        assert!(message.contains("read_file"), "{message}");
    }

    #[test]
    fn no_advertised_schema_nests_deeper_than_two_array_edges() {
        // `depth` counts **object levels**, so the question tool's own schema --
        // outer object, question object, option object -- is 3, which is two array
        // edges. The bound is on the edges; naming it 2 while counting objects
        // would fail on the very schema this task exists to allow.
        fn depth(schema: InputSchema) -> usize {
            1 + schema
                .properties
                .iter()
                .filter_map(|property| property.array)
                .map(|array| depth(*array.items))
                .max()
                .unwrap_or(0)
        }
        for spec in Registry::builtin().specs() {
            assert!(
                depth(spec.input_schema()) <= 3,
                "`{}` nests too deep",
                spec.name()
            );
        }
        // And the eight scalar-only tools stay flat, so the bound is not vacuous.
        for name in ["list_files", "read_file", "write_file", "terminal"] {
            assert_eq!(
                depth(Registry::builtin().spec(name).unwrap().input_schema()),
                1
            );
        }
    }

    #[test]
    fn an_oversized_output_is_truncated_with_a_sentence_that_says_so() {
        // `list_files` bounds its entry *count*, not its byte count, so a
        // directory of long names is exactly the case the registry's backstop
        // exists for.
        let dir = tempfile::tempdir().expect("temporary workspace");
        for index in 0..20 {
            std::fs::write(dir.path().join(format!("entry-{index:02}.txt")), "x").expect("write");
        }
        let scope = AccessScope::primary_only(dir.path()).expect("scope");
        let context = ToolContext::with_limits(
            scope,
            ToolLimits {
                max_output_bytes: 60,
                ..ToolLimits::default()
            },
        );
        let result = Registry::builtin()
            .execute(
                &ToolCall {
                    id: "c1".to_string(),
                    name: "list_files".to_string(),
                    input: json!({}),
                },
                &context,
            )
            .expect("list_files is advertised");
        assert!(result.ok);
        assert!(
            result
                .output
                .contains("[tool output truncated at 60 bytes]"),
            "{}",
            result.output
        );
        // The kept prefix is bounded; only the sentence that explains it is not.
        let (kept, _) = result
            .output
            .split_once("\n... [tool output truncated")
            .expect("the sentinel is on its own line");
        assert!(kept.len() <= 60, "{}", kept.len());
    }
}
