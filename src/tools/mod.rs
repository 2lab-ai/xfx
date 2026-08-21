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

pub mod read;
pub mod spec;

use std::fmt;

use serde_json::Value;

use crate::gateway::protocol::ToolCall;

pub use spec::{
    InputSchema, PermissionKind, Property, PropertyKind, ToolContext, ToolDecoder, ToolExecutor,
    ToolInput, ToolLimits, ToolResult, ToolSpec, ToolValidator,
};

/// Every tool name this build advertises, in registry order.
///
/// `scripts/check-no-stubs.sh` reads this declaration textually and requires an
/// `implemented` row in `docs/parity.md` for each name.
pub const ADVERTISED_TOOLS: &[&str] = &["list_files", "glob_files", "grep_files", "read_file"];

/// The specs themselves, in upstream's order (`tools.zig:1352-1355`).
static BUILTIN_TOOLS: &[ToolSpec] = &[
    read::LIST_FILES,
    read::GLOB_FILES,
    read::GREP_FILES,
    read::READ_FILE,
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
    /// Every spec in this registry is [`PermissionKind::ReadOnly`], which every
    /// permission mode admits (design, "Permissions"). That invariant is
    /// asserted by a test rather than by a branch here, so the mutation slice
    /// has to add the approval channel in the same change as the first kind
    /// that needs one.
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
    fn every_spec_is_read_only() {
        // The admission rule in `execute` depends on this. When it stops holding
        // this test fails, which is the point at which an approval channel has
        // to exist.
        for spec in Registry::builtin().specs() {
            assert_eq!(
                spec.permission(),
                PermissionKind::ReadOnly,
                "{} is not read-only",
                spec.name()
            );
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
                    name: "terminal".to_string(),
                    input: json!({}),
                },
                &context,
            )
            .expect_err("terminal is not advertised");
        assert_eq!(err.name, "terminal");
        let message = err.to_string();
        assert!(message.contains("terminal"), "{message}");
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
