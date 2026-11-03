//! `skill`: the model loads a skill it was told about, or one text file of it.
//!
//! Upstream's tool (`vercel-labs/fx@c1db9191 src/builtins/tools.zig:542-572`,
//! `src/tools/skills/skill.zig`), its description and schema word for word: a
//! `location` copied from the turn's `<available_skills>` block -- or the path
//! a skill's own content named it by -- and an optional `resource` inside it.
//! The answer is the resource whole in a `<skill_content … complete="true">`
//! element, or upstream's sentence for why not
//! ([`crate::skills::invocation::tool_call`]).
//!
//! Read-only, as upstream's is: it reads files under a skill directory the
//! user put there and changes nothing. A configured rule over the `skill`
//! category still decides it ([`crate::permission::rules::SKILL`]), judged on
//! the skill directory's name, as upstream keys a skill grant by name.
//!
//! The decoder also takes upstream's **legacy named form** (`decode`,
//! `skill.zig:28-84`), which the advertised schema does not name: `name`, with
//! or without a `location`, and an `offset` to page from. Such a call loads
//! the resource a chunk at a time -- 20 KiB cut back to a line break, in a
//! `<skill_content … offset="…" next_offset="…">` element that says where the
//! next one takes up ([`crate::skills::invocation::load_chunk`]) -- and a name
//! with no location is looked up among the skills discovered afresh. The
//! refusals are upstream's sentences.

use serde_json::Value;

use super::spec::{
    Activity, InputSchema, PermissionKind, Presentation, Property, PropertyKind, ToolContext,
    ToolInput, ToolResult, ToolSpec,
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
    /// The legacy form's skill name.
    pub name: Option<String>,
    pub location: Option<String>,
    pub resource: Option<String>,
    /// Where the legacy form's chunk starts; 0 otherwise.
    pub offset: usize,
}

impl SkillInput {
    /// What the row names: the resource when it is not the skill's own file,
    /// otherwise the skill -- by the name the call gave, or the leaf of its
    /// location, which is its name unless the directory was renamed.
    pub fn target(&self) -> &str {
        if let Some(resource) = self
            .resource
            .as_deref()
            .filter(|resource| !reads_skill_file(resource))
        {
            return resource;
        }
        if self.offset > 0 {
            return "SKILL.md";
        }
        if let Some(name) = self.name.as_deref() {
            return name;
        }
        let location = self.location.as_deref().unwrap_or_default();
        location
            .rsplit('/')
            .next()
            .filter(|leaf| !leaf.is_empty())
            .unwrap_or(location)
    }

    /// The row's verbs when the call reads another resource, or pages its
    /// own file past its start (`presentation`, `skill.zig:96-111`).
    pub fn presentation(&self) -> Option<Presentation> {
        let other = self
            .resource
            .as_deref()
            .is_some_and(|resource| !reads_skill_file(resource));
        (other || self.offset > 0).then_some(RESOURCE_PRESENTATION)
    }
}

/// Whether `resource` is the skill's own file, as an empty one is.
fn reads_skill_file(resource: &str) -> bool {
    resource.is_empty() || resource == "SKILL.md"
}

/// Upstream's decoder, sentence for sentence (`decode`, `skill.zig:28-84`):
/// the advertised `location` form, or the legacy `name` form, which alone
/// takes an `offset`.
fn decode_skill(input: &Value) -> Result<ToolInput, String> {
    let object = input
        .as_object()
        .ok_or_else(|| "skill arguments must be an object".to_string())?;
    let string = |key: &str| -> Result<Option<String>, String> {
        match object.get(key) {
            None => Ok(None),
            Some(Value::String(value)) => Ok(Some(value.clone())),
            Some(_) => Err(format!("skill field \"{key}\" must be a string")),
        }
    };
    let name = string("name")?;
    if name.is_none() && !object.contains_key("location") {
        return Err("skill requires an advertised location".to_string());
    }
    let location = string("location")?;
    let resource = string("resource")?;
    let offset = match object.get("offset") {
        None => 0,
        Some(_) if name.is_none() => {
            return Err("skill offset requires the legacy named resource form".to_string())
        }
        Some(value) => value
            .as_u64()
            .and_then(|offset| usize::try_from(offset).ok())
            .ok_or_else(|| "skill field \"offset\" must be a non-negative integer".to_string())?,
    };
    Ok(ToolInput::Skill(SkillInput {
        name,
        location,
        resource,
        offset,
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
    // Judged before it got here, under `skill` at the skill it loads
    // (`super::targets_of`, in `super::Registry::execute`).
    let target = input.target();
    let loaded = crate::skills::invocation::tool_call(
        context.skills(),
        context.scope().primary(),
        context.home(),
        &crate::skills::invocation::Call {
            name: input.name.as_deref(),
            location: input.location.as_deref(),
            resource: input.resource.as_deref(),
            offset: input.offset,
        },
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
        let catalog = crate::skills::catalog::build(&found.skills, &found.diagnostics, None);
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

    #[test]
    fn the_decoder_refuses_in_upstreams_sentences() {
        // `expectDecodeFailure`'s cases (`skill.zig:398-404`) and the offset
        // rules (`:58-66`).
        for (arguments, refusal) in [
            (json!([1]), "skill arguments must be an object"),
            (json!({}), "skill requires an advertised location"),
            (json!({"name": 1}), "skill field \"name\" must be a string"),
            (
                json!({"name": "workflow", "location": 1}),
                "skill field \"location\" must be a string",
            ),
            (
                json!({"location": "x", "resource": 2}),
                "skill field \"resource\" must be a string",
            ),
            (
                json!({"location": "x", "offset": 4}),
                "skill offset requires the legacy named resource form",
            ),
            (
                json!({"name": "workflow", "offset": -1}),
                "skill field \"offset\" must be a non-negative integer",
            ),
            (
                json!({"name": "workflow", "offset": "4"}),
                "skill field \"offset\" must be a non-negative integer",
            ),
        ] {
            assert_eq!(
                decode_skill(&arguments),
                Err(refusal.to_string()),
                "{arguments}"
            );
        }
        assert_eq!(
            decode_skill(&json!({"name": "workflow", "location": "/tmp/skills/workflow"})),
            Ok(ToolInput::Skill(SkillInput {
                name: Some("workflow".to_string()),
                location: Some("/tmp/skills/workflow".to_string()),
                resource: None,
                offset: 0,
            }))
        );
    }

    #[test]
    fn the_legacy_named_form_pages_a_resource_and_says_where_the_next_chunk_starts() {
        // `presentation`'s cases (`skill.zig:450-470`).
        let shown = |arguments: Value| SKILL.shown(&arguments);
        assert_eq!(
            shown(json!({"name": "demo"})).presentation.running,
            "Loading skill"
        );
        assert_eq!(
            shown(json!({"name": "demo"})).target.as_deref(),
            Some("demo")
        );
        assert_eq!(
            shown(json!({"name": "demo", "resource": "SKILL.md", "offset": 0}))
                .presentation
                .running,
            "Loading skill"
        );
        assert_eq!(
            shown(json!({"name": "demo", "offset": 128})).presentation,
            RESOURCE_PRESENTATION
        );
        assert_eq!(
            shown(json!({"name": "demo", "offset": 128}))
                .target
                .as_deref(),
            Some("SKILL.md")
        );

        let base = tempfile::tempdir().unwrap();
        let work = std::fs::canonicalize(base.path()).unwrap();
        let dir = work.join(".xfx/skills/demo");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("SKILL.md"), "---\nname: demo\n---\nSay DEMO.\n").unwrap();
        // A resource past one chunk: lines of 100 bytes.
        let line = format!("{}\n", "x".repeat(99));
        let big = line.repeat(300);
        std::fs::write(dir.join("big.md"), &big).unwrap();
        // No location: the skills are discovered afresh, by name.
        let context = ToolContext::new(AccessScope::primary_only(&work).unwrap());
        let whole = SKILL.run(&json!({"name": "demo"}), &context);
        assert!(whole.ok, "{whole:?}");
        let skill_file = "---\nname: demo\n---\nSay DEMO.\n";
        assert_eq!(
            whole.output,
            format!(
                "<skill_content name=\"demo\" resource=\"SKILL.md\" offset=\"0\" next_offset=\"{}\">\n{skill_file}\n</skill_content>",
                skill_file.len()
            )
        );
        let first = SKILL.run(&json!({"name": "demo", "resource": "big.md"}), &context);
        assert!(first.ok, "{first:?}");
        let chunk = crate::skills::invocation::CHUNK_LIMIT / 100 * 100;
        assert!(first.output.starts_with(&format!(
            "<skill_content name=\"demo\" resource=\"big.md\" offset=\"0\" next_offset=\"{chunk}\">\n"
        )));
        assert!(first.output.ends_with(&format!(
            "\n<context_limit name=\"skill_chunk_bytes\" action=\"truncated\" observed_bytes=\"{}\" effective_bytes=\"20480\" source=\"compiled default\" next_offset=\"{chunk}\" override=\"--context-limit skill_chunk_bytes=BYTES|off\" />\n</skill_content>",
            big.len()
        )));
        let rest = SKILL.run(
            &json!({"name": "demo", "resource": "big.md", "offset": chunk}),
            &context,
        );
        assert!(rest.ok, "{rest:?}");
        assert!(rest.output.contains(&format!(
            "offset=\"{chunk}\" next_offset=\"{}\">",
            (2 * chunk).min(big.len())
        )));
        // An offset inside a character, or past the end, is refused.
        std::fs::write(dir.join("wide.md"), "\u{e9}\n").unwrap();
        let inside = SKILL.run(
            &json!({"name": "demo", "resource": "wide.md", "offset": 1}),
            &context,
        );
        assert!(!inside.ok);
        assert_eq!(
            inside.output,
            "skill offset must be at a valid UTF-8 boundary within the selected resource"
        );
        // A name nobody has, in upstream's words.
        let missing = SKILL.run(&json!({"name": "nobody"}), &context);
        assert!(!missing.ok);
        assert_eq!(
            missing.output,
            "Skill \"nobody\" not found. Refresh available skills and retry with an advertised name."
        );
    }
}
