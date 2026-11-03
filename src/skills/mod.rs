//! Skills: task instructions a person keeps in a directory, which the model is
//! told about and loads when a prompt names one.
//!
//! A port of upstream's skill runtime (`vercel-labs/fx@c1db9191
//! src/core/skills/`, `src/builtins/skills.zig`, `src/tools/skills/skill.zig`)
//! over **xfx's own roots** -- `.xfx/skills` and `skills/` in the workspace and
//! its ancestors, and the managed root `~/.xfx/skills` -- and nobody else's
//! ([`discovery`]). What each turn gets is two system blocks, both upstream's
//! bytes: the catalog of every visible skill ([`catalog`]) and, when the prompt
//! names skills, their instructions whole ([`invocation`]). The model reads
//! more through the `skill` tool (`crate::tools::skill`); a person manages
//! them with `/skills` ([`command`], [`install`]).
//!
//! # What is decided rather than ported
//!
//! * Other agents' roots are not read (see [`discovery`]).
//! * `add`/`install` read a local directory and never clone (see [`install`]),
//!   and the `install_skill` tool is not advertised at all.

pub mod catalog;
pub mod command;
pub mod contract;
pub mod discovery;
pub mod install;
pub mod invocation;

use std::path::Path;

pub use catalog::{Catalog, Locations};
pub use discovery::{Diagnostic, Discovery, Skill, Source};
pub use invocation::{Binding, Explicit};

/// How many diagnostics a warning spells out before it counts the rest
/// (`diagnostic_notice_item_count`, `skill_runtime.zig:15`).
const DIAGNOSTIC_ITEMS: usize = 4;

/// The longest path a diagnostic quotes (`diagnostic_path_max_bytes`, `:16`).
const DIAGNOSTIC_PATH_BYTES: usize = 4 * 1024;

/// What discovery skipped, said to a person (`writeDiagnosticSummary`,
/// `skill_runtime.zig:112-154`).
///
/// Upstream ends the sentence by pointing at its trace log, or at the variable
/// that writes one; xfx keeps no such log, so the sentence ends where the
/// diagnostics do.
pub fn diagnostic_summary(diagnostics: &[Diagnostic]) -> String {
    use discovery::{Cause, Scope};
    let mut out = String::from("skill discovery warning: ");
    for (index, diagnostic) in diagnostics.iter().take(DIAGNOSTIC_ITEMS).enumerate() {
        if index > 0 {
            out.push_str("; ");
        }
        let path = bounded_path(&diagnostic.path);
        match diagnostic.scope {
            Scope::Root => out.push_str(&format!(
                "inventory incomplete because root \"{path}\" could not be read, so an unknown number of skills may be missing; fix access to the root and reload skills"
            )),
            Scope::Candidate => {
                out.push_str(&format!("candidate \"{path}\" was skipped because "));
                out.push_str(&match diagnostic.cause {
                    Cause::InvalidMetadata(cause) => format!(
                        "its metadata is invalid ({}); use one safe name and an optional inline description or a >, >-, or | block, then reload skills",
                        cause.tag()
                    ),
                    Cause::LinkedCandidateUnavailable => "its linked skill directory could not be resolved to an authorized readable directory; repair or remove the link, or authorize its external location, then reload skills".to_string(),
                    Cause::Unreadable => "SKILL.md is unreadable or not a regular file; fix the file type or access, then reload skills".to_string(),
                    Cause::Oversized => format!(
                        "its frontmatter exceeds the supported {}-byte metadata header; shorten the name/description header, then reload skills",
                        contract::MAX_FRONTMATTER_BYTES
                    ),
                });
            }
        }
    }
    if diagnostics.len() > DIAGNOSTIC_ITEMS {
        let more = diagnostics.len() - DIAGNOSTIC_ITEMS;
        out.push_str(&format!(
            "; {more} additional diagnostic{} omitted",
            if more == 1 { "" } else { "s" }
        ));
    }
    out
}

/// A path encoded as a model scalar and cut at the diagnostic cap with `...`.
fn bounded_path(path: &Path) -> String {
    let mut encoded = String::new();
    catalog::write_scalar(&mut encoded, &path.to_string_lossy());
    if encoded.len() <= DIAGNOSTIC_PATH_BYTES {
        return encoded;
    }
    let mut cut = DIAGNOSTIC_PATH_BYTES;
    while !encoded.is_char_boundary(cut) {
        cut -= 1;
    }
    if let Some(amp) = encoded[..cut].rfind('&') {
        if !encoded[amp..cut].contains(';') {
            cut = amp;
        }
    }
    format!("{}...", &encoded[..cut])
}

/// What one turn is told about skills: the catalog every request carries and,
/// when the prompt or the composer named skills, their instructions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TurnSkills {
    pub catalog: Catalog,
    pub explicit: Explicit,
}

impl TurnSkills {
    /// Discovers the skills visible from `workspace` now -- a skill added in
    /// another window is in the next prompt's catalog -- and builds both
    /// blocks for `prompt` (`prepareSkillCatalog` and
    /// `buildExplicitPromptSection`, `orchestrator.zig:5440-5458`, `:5616-5657`).
    ///
    /// `window` is the context window of the model the turn talks to, when
    /// one is known: the catalog's budget is sized by it
    /// ([`catalog::Budget::for_window`]).
    pub fn prepare(
        workspace: &Path,
        home: Option<&Path>,
        prompt: &str,
        bindings: &[Binding],
        window: Option<u64>,
    ) -> Self {
        let found = discovery::discover(workspace, home);
        Self::over(&found, prompt, bindings, window)
    }

    /// The same, over skills already discovered.
    pub fn over(
        found: &Discovery,
        prompt: &str,
        bindings: &[Binding],
        window: Option<u64>,
    ) -> Self {
        let catalog = catalog::build(&found.skills, &found.diagnostics, window);
        // Upstream builds the explicit block only when there is a catalog or a
        // binding (`orchestrator.zig:5621`).
        let explicit = if !catalog.text.is_empty() || !bindings.is_empty() {
            invocation::explicit(&found.skills, &found.diagnostics, prompt, bindings)
        } else {
            Explicit::default()
        };
        Self { catalog, explicit }
    }

    /// The catalog block, when there is a skill to tell of: the system
    /// message upstream puts in the stable prefix.
    pub fn catalog_block(&self) -> Option<&str> {
        Some(self.catalog.text.as_str()).filter(|text| !text.is_empty())
    }

    /// The explicit block, when the query named skills: the system message
    /// upstream puts in the per-request overlay.
    pub fn explicit_block(&self) -> Option<&str> {
        Some(self.explicit.text.as_str()).filter(|text| !text.is_empty())
    }
}

/// Every visible skill, grouped as upstream's listing groups them
/// (`listSkillsSummary`, `skill_runtime.zig:2964-3004`): the surface that has
/// no menu -- the line-oriented shell -- says this for `/skills list`.
pub fn summary(skills: &[Skill]) -> String {
    if skills.is_empty() {
        return "No skills available.\n".to_string();
    }
    let mut out = format!("Visible skills ({}):\n", skills.len());
    for group in ["Managed installs", "Workspace skills"] {
        let members: Vec<&Skill> = skills
            .iter()
            .filter(|skill| skill.source.group().1 == group)
            .collect();
        if members.is_empty() {
            continue;
        }
        out.push_str(&format!("\n{group} ({}):\n", members.len()));
        for skill in members {
            if skill.description.is_empty() {
                out.push_str(&format!("  - {} ", skill.name));
            } else {
                out.push_str(&format!("  - {}: {} ", skill.name, skill.description));
            }
            out.push_str(&format!("[{}]\n", skill.source.label()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn a_warning_names_each_skip_and_counts_the_rest() {
        let diagnostic = |path: &str, scope, cause| Diagnostic {
            path: PathBuf::from(path),
            source: Source::WorkspaceXfx,
            scope,
            cause,
        };
        let one = diagnostic_summary(&[diagnostic(
            "/w/.xfx/skills/bad",
            discovery::Scope::Candidate,
            discovery::Cause::InvalidMetadata(contract::InvalidCause::MissingName),
        )]);
        assert_eq!(
            one,
            "skill discovery warning: candidate \"/w/.xfx/skills/bad\" was skipped because its metadata is invalid (missing_name); use one safe name and an optional inline description or a >, >-, or | block, then reload skills"
        );
        let many: Vec<Diagnostic> = (0..6)
            .map(|index| {
                diagnostic(
                    &format!("/r{index}"),
                    discovery::Scope::Root,
                    discovery::Cause::Unreadable,
                )
            })
            .collect();
        assert!(diagnostic_summary(&many).ends_with("; 2 additional diagnostics omitted"));
    }

    #[test]
    fn the_listing_groups_managed_installs_first() {
        let skills = vec![
            Skill {
                name: "demo".to_string(),
                description: "Demo".to_string(),
                path: PathBuf::from("/w/.xfx/skills/demo"),
                source: Source::WorkspaceXfx,
            },
            Skill {
                name: "foo".to_string(),
                description: String::new(),
                path: PathBuf::from("/h/.xfx/skills/foo"),
                source: Source::Managed,
            },
        ];
        assert_eq!(
            summary(&skills),
            "Visible skills (2):\n\nManaged installs (1):\n  - foo [global ~/.xfx/skills]\n\nWorkspace skills (1):\n  - demo: Demo [workspace .xfx/skills]\n"
        );
        assert_eq!(summary(&[]), "No skills available.\n");
    }
}
