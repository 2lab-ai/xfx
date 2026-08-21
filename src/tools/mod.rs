//! The closed tool registry.
//!
//! The registry is a `static` table of four read-only specs. It is not
//! configurable, not extensible at runtime, and not merged with anything: what
//! fxr advertises to a model is this list, in this order, and the code that runs
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
pub mod read;
pub mod spec;
pub mod terminal;

use std::fmt;

use serde_json::Value;

use crate::gateway::protocol::ToolCall;

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
];

/// A tool call fxr never offered.
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
