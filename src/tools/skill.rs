//! `skill`: the model loads a skill it was told about, or one text file of it.
//!
//! Upstream's tool (`vercel-labs/fx@c1db9191 src/builtins/tools.zig:542-572`,
//! `src/tools/skills/skill.zig`), its description and schema word for word: a
//! `location` copied from the turn's `<available_skills>` block -- or the path
//! a skill's own content named it by -- and an optional `resource` inside it.
//! The answer is the resource whole in a `<skill_content … complete="true">`
//! element, or upstream's sentence for why not
//! ([`crate::skills::invocation::tool_load`]).
//!
//! Read-only, as upstream's is: it reads files under a skill directory the
//! user put there and changes nothing. A configured rule over the `skill`
//! category still decides it ([`crate::permission::rules::SKILL`]), judged on
//! the skill directory's name, as upstream keys a skill grant by name.
//!
//! **Not here**: upstream's legacy `name`/`offset` form, which pages a skill in
//! chunks. The advertised schema is closed and names neither field, so a model
//! cannot send them; xfx decodes exactly what it advertised.

use serde_json::Value;

use super::spec::{
    object, optional_string, required_string, Activity, InputSchema, PermissionKind, Presentation,
    Property, PropertyKind, ToolContext, ToolInput, ToolResult, ToolSpec,
};

/// Unreachable through [`ToolSpec::run`], which pairs each decoder with its
/// own executor, and stated rather than `unreachable!()` for `read`'s reason.
fn mismatched(tool: &str) -> String {
    format!("{tool} received arguments that belong to another tool")
}

/// Upstream's description (`skill_description`, `tools.zig:159-160`).
const DESCRIPTION: &str = "Load an installed skill or one required relative text resource completely. Copy the exact advertised location. Resolve paths mentioned in skill instructions from the selected skill directory, not the workspace. Read referenced text with the same location and its relative resource path. When to use: the user explicitly invokes a listed skill or the task clearly matches one. When NOT to use: installing a missing skill.";

/// The row a call that reads a resource other than `SKILL.md` is shown with
/// (`skill.zig` `presentation`, `:110-127`).
pub const RESOURCE_PRESENTATION: Presentation = Presentation::new(
    Activity::Read,
    "Reading skill resource",
    "Read skill resource",
    "SKILL.md",
);

pub const SKILL: ToolSpec = ToolSpec::new(
    "skill",
    DESCRIPTION,
    PermissionKind::ReadOnly,
    // `action_label`, `completed_action_label` and `label_arg_default`
    // (`tools.zig:560-563`).
    Presentation::new(Activity::Read, "Loading skill", "Loaded skill", "skill"),
    InputSchema {
        properties: &[
            Property {
                name: "location",
                kind: PropertyKind::String,
                description: "The exact advertised location of the selected skill.",
                allowed: &[],
                array: None,
            },
            Property {
                name: "resource",
                kind: PropertyKind::String,
                description: "Optional relative text resource within the selected skill. Omit or pass an empty string to read SKILL.md.",
                allowed: &[],
                array: None,
            },
        ],
        required: &["location"],
    },
    decode_skill,
    validate_skill,
    execute_skill,
);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillInput {
    pub location: String,
    pub resource: Option<String>,
}

impl SkillInput {
    /// What the row names: the resource when it is not the skill's own file,
    /// otherwise the skill's directory -- the leaf of its location, which is
    /// its name unless the directory was renamed.
    pub fn target(&self) -> &str {
        if let Some(resource) = self
            .resource
            .as_deref()
            .filter(|resource| !reads_skill_file(resource))
        {
            return resource;
        }
        self.location
            .rsplit('/')
            .next()
            .filter(|leaf| !leaf.is_empty())
            .unwrap_or(&self.location)
    }

    /// The row's verbs when the call reads another resource.
    pub fn presentation(&self) -> Option<Presentation> {
        self.resource
            .as_deref()
            .filter(|resource| !reads_skill_file(resource))
            .map(|_| RESOURCE_PRESENTATION)
    }
}

/// Whether `resource` is the skill's own file, as an empty one is.
fn reads_skill_file(resource: &str) -> bool {
    resource.is_empty() || resource == "SKILL.md"
}

fn decode_skill(input: &Value) -> Result<ToolInput, String> {
    let object = object("skill", input)?;
    Ok(ToolInput::Skill(SkillInput {
        location: required_string("skill", object, "location")?,
        resource: optional_string("skill", object, "resource")?,
    }))
}

fn validate_skill(input: &ToolInput) -> Result<(), String> {
    let ToolInput::Skill(_) = input else {
        return Err(mismatched("skill"));
    };
    Ok(())
}

fn execute_skill(input: &ToolInput, context: &ToolContext) -> ToolResult {
    let ToolInput::Skill(input) = input else {
        return ToolResult::failure(mismatched("skill"));
    };
    let target = input.target();
    if let Err(reason) = context.permissions().judge_read("skill", target.as_bytes()) {
        return ToolResult::refused("skill", &reason);
    }
    let loaded = crate::skills::invocation::tool_load(
        context.skills(),
        context.scope().primary(),
        context.home(),
        &input.location,
        input.resource.as_deref(),
        context.limits().max_output_bytes,
    );
    if loaded.ok {
        ToolResult::success(loaded.output, format!("loaded skill {target}"))
    } else {
        ToolResult::failure(loaded.output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::AccessScope;
    use serde_json::json;
    use std::sync::Arc;

    #[test]
    fn the_schema_is_upstreams() {
        let advertised = serde_json::to_string(&SKILL.advertisement()).unwrap();
        assert_eq!(
            advertised,
            r#"{"description":"Load an installed skill or one required relative text resource completely. Copy the exact advertised location. Resolve paths mentioned in skill instructions from the selected skill directory, not the workspace. Read referenced text with the same location and its relative resource path. When to use: the user explicitly invokes a listed skill or the task clearly matches one. When NOT to use: installing a missing skill.","inputSchema":{"additionalProperties":false,"properties":{"location":{"description":"The exact advertised location of the selected skill.","type":"string"},"resource":{"description":"Optional relative text resource within the selected skill. Omit or pass an empty string to read SKILL.md.","type":"string"}},"required":["location"],"type":"object"},"name":"skill","type":"function"}"#
        );
    }

    #[test]
    fn a_call_loads_the_turns_advertised_skill_and_names_it_on_its_row() {
        let base = tempfile::tempdir().unwrap();
        let work = std::fs::canonicalize(base.path()).unwrap();
        let dir = work.join(".xfx/skills/demo");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("SKILL.md"), "---\nname: demo\n---\nSay DEMO.\n").unwrap();
        let found = crate::skills::discovery::discover(&work, None);
        let catalog = crate::skills::catalog::build(&found.skills, &found.diagnostics);
        let location = catalog.locations.of(&found.skills[0]).unwrap();
        let context = ToolContext::new(AccessScope::primary_only(&work).unwrap())
            .with_skills(Some(Arc::new(catalog.locations.clone())), None);
        let arguments = json!({ "location": location });
        let shown = SKILL.shown(&arguments);
        assert_eq!(shown.target.as_deref(), Some("demo"));
        assert_eq!(shown.presentation.running, "Loading skill");
        let result = SKILL.run(&arguments, &context);
        assert!(result.ok, "{result:?}");
        assert!(result.output.contains("Say DEMO."), "{}", result.output);

        let resource = json!({ "location": location, "resource": "notes.md" });
        assert_eq!(SKILL.shown(&resource).presentation, RESOURCE_PRESENTATION);
        assert_eq!(SKILL.shown(&resource).target.as_deref(), Some("notes.md"));
        let missing = SKILL.run(&resource, &context);
        assert!(!missing.ok);
        assert_eq!(missing.output, "skill failed: FileNotFound");
    }
}
