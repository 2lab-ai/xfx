//! `/skills [list|add|install|show|create|remove|path] [name|url|path]`: the
//! grammar and what each form does, as upstream's
//! (`vercel-labs/fx@c1db9191 src/builtins/skills.zig:93-197`,
//! `src/core/app/app_commands.zig:1739-1945`).
//!
//! Every answer is upstream's sentence with this product's names in it --
//! `.xfx/skills`, `~/.xfx/skills`, "the xfx managed install root" -- and two
//! are narrower on purpose:
//!
//! * **`path` says nothing about compatibility roots**, because xfx reads none
//!   ([`super::discovery`]); upstream's third line would be a claim about
//!   directories this product never opens.
//! * **`add` and `install` take a local directory only.** Upstream tries one
//!   and then runs `git clone --depth 1 https://github.com/<source>.git`
//!   (`installFromGitHub`, `skills.zig:265-280`). xfx answers a source that is
//!   not an existing directory exactly as upstream answers a clone that
//!   failed -- `Failed to install. Check the source path or URL and try
//!   again.` -- and reaches no network to get there. A decided default, in
//!   `docs/parity.md`.

use std::path::{Path, PathBuf};

use super::discovery::Discovery;
use super::install;

/// Upstream's usage line, which the command answers anything else with.
pub const USAGE: &str = "usage: /skills [list|add|install|show|create|remove|path] [name|url|path]";

/// The topic every answer is said under.
pub const TOPIC: &str = "skills";

/// The template `create` writes (`createSkillTemplate`, `skills.zig:242-262`).
fn template(name: &str) -> String {
    format!(
        "---\nname: {name}\ndescription: Describe when this skill should activate\n---\n\n# {name}\n\nInstructions for this skill...\n"
    )
}

/// One `/skills` line, read (`parseCommand`, `skills.zig:93-104`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Bare, or `list`: the menu.
    List,
    Show(String),
    /// `add` or `install`, with what to install from and an optional
    /// `--skill` filter.
    Install {
        source: String,
        filter: Option<String>,
    },
    Create(String),
    Remove(String),
    Path,
    Usage,
}

/// Reads the rest of a `/skills` line: spaces and tabs at either end do not
/// count, and a word matches only with what upstream requires after it.
pub fn parse(rest: &str) -> Command {
    let trimmed = rest.trim_matches([' ', '\t']);
    if trimmed.is_empty() || trimmed == "list" {
        return Command::List;
    }
    if let Some(name) = trimmed.strip_prefix("show ") {
        return Command::Show(name.trim_matches([' ', '\t']).to_string());
    }
    if let Some(arguments) = trimmed
        .strip_prefix("add ")
        .or_else(|| trimmed.strip_prefix("install "))
    {
        let arguments = arguments.trim_matches([' ', '\t']);
        let (source, filter) = match arguments
            .find("--skill ")
            .map(|at| (at, "--skill ".len()))
            .or_else(|| arguments.find("--skill=").map(|at| (at, "--skill=".len())))
        {
            Some((at, len)) => (
                arguments[..at].trim_matches([' ', '\t']),
                Some(arguments[at + len..].trim_matches([' ', '\t']).to_string()),
            ),
            None => (arguments, None),
        };
        return Command::Install {
            source: source.to_string(),
            filter,
        };
    }
    if let Some(name) = trimmed.strip_prefix("create ") {
        return Command::Create(name.trim_matches([' ', '\t']).to_string());
    }
    if let Some(name) = trimmed.strip_prefix("remove ") {
        return Command::Remove(name.trim_matches([' ', '\t']).to_string());
    }
    if trimmed == "path" {
        return Command::Path;
    }
    Command::Usage
}

/// What a `/skills` line comes to on a surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Open the menu (`.open_menu`).
    OpenMenu,
    /// Open it on this skill, under its own tab (`.focus_menu`).
    Focus(String),
    /// A name more than one skill answers to: the menu, searching for it,
    /// with the name put in the composer as the search
    /// (`executeSkillsCommand`'s `.ambiguous_name` arm, `app_commands.zig:1826-1833`).
    Search(String),
    /// Sentences, said in order as neutral notices under [`TOPIC`] -- the usage
    /// line under none. Several when a command says what it is about to do
    /// before it does it (`Installing from …`).
    Said(Vec<String>),
    /// The managed root changed: `said` at once, then `after` once the list
    /// read again holds the change (upstream's `reload` notices and its
    /// `.installed` result, `queueSkillsNoticeAfterRefresh`,
    /// `app_commands.zig:1903-1951`). A surface with no list to read again
    /// says both in order.
    Changed { said: Vec<String>, after: String },
}

/// The topic one of the answers is said under: none for the usage line,
/// which names its command already (`noticeTopicForBody`, `:1851-1853`).
pub fn topic_for(body: &str) -> &'static str {
    if body.starts_with("usage:") {
        ""
    } else {
        TOPIC
    }
}

/// Runs one `/skills` line against what was discovered and the managed root
/// (`executeCommand`, `skills.zig:126-210`).
///
/// `cwd` is what a relative `add` source is read against: the process's
/// working directory, as upstream's `realpathAlloc` reads it.
pub fn execute(command: &Command, found: &Discovery, managed: &Path, cwd: &Path) -> Outcome {
    match command {
        Command::List => Outcome::OpenMenu,
        Command::Show(name) => {
            let matching = found.skills.iter().filter(|skill| skill.name == *name).count();
            match matching {
                0 => Outcome::Said(vec![format!("Skill '{name}' not found.")]),
                1 => Outcome::Focus(name.clone()),
                _ => Outcome::Search(name.clone()),
            }
        }
        Command::Path => Outcome::Said(vec![format!(
            "xfx workspace roots are auto-discovered from .xfx/skills and skills/.\nxfx managed install root: {}",
            managed.display()
        )]),
        Command::Usage => Outcome::Said(vec![USAGE.to_string()]),
        Command::Create(name) => changed(Vec::new(), create(managed, name)),
        Command::Remove(name) => changed(Vec::new(), remove(found, managed, name)),
        Command::Install { source, filter } => {
            let said = vec![format!("Installing from {source}...")];
            changed(
                said,
                install::installed(managed, source, filter.as_deref(), cwd),
            )
        }
    }
}

/// `said`, then a sentence that is said after the reload when the command
/// changed something and with the rest when it did not.
fn changed(mut said: Vec<String>, result: Result<String, String>) -> Outcome {
    match result {
        Ok(after) => Outcome::Changed { said, after },
        Err(refusal) => {
            said.push(refusal);
            Outcome::Said(said)
        }
    }
}

/// `create <name>` (`createCommandResult`, `skills.zig:173-180`).
fn create(managed: &Path, name: &str) -> Result<String, String> {
    if !super::contract::is_managed_name(name) {
        return Err(
            "Invalid skill name. Use a single directory name without '/' or '\\'.".to_string(),
        );
    }
    let dir = managed.join(name);
    let file = dir.join("SKILL.md");
    // The managed root's directories are the profile's: `~/.xfx` and each
    // missing level below it made owner-only, an existing `~/.xfx` that is not
    // private refused (`crate::profile_home::ensure`).
    let written = super::install::ensure_managed(managed, &dir)
        .and_then(|()| std::fs::write(&file, template(name)));
    match written {
        Ok(()) => Ok(format!("Created {}", file.display())),
        // Upstream's `create` has no sentence of its own for a write that
        // failed -- the error ends the command -- so the failure is said in
        // the shape of its other refusals.
        Err(error) => Err(format!("Failed to create skill '{name}': {error}.")),
    }
}

/// `remove <name>`: only what the managed root holds (`removeCommandResult`,
/// `skills.zig:182-196`).
fn remove(found: &Discovery, managed: &Path, name: &str) -> Result<String, String> {
    let Some(skill) = found.named(name) else {
        return Err(format!("Skill '{name}' not found."));
    };
    if !skill.source.is_managed() {
        return Err(format!(
            "Skill '{name}' comes from {}, not the xfx managed install root. Remove it from {}.",
            skill.source.label(),
            skill.path.display()
        ));
    }
    let Some(leaf) = skill.path.file_name().and_then(|leaf| leaf.to_str()) else {
        return Err(format!("Failed to remove skill '{name}'."));
    };
    if !super::contract::is_managed_name(leaf) {
        return Err(format!("Failed to remove skill '{name}'."));
    }
    let target: PathBuf = managed.join(leaf);
    // Never through a link: `deleteTree` on the managed root removes the
    // entry, and a link planted there is removed as a link.
    let removed = match std::fs::symlink_metadata(&target) {
        Ok(meta) if meta.is_dir() => std::fs::remove_dir_all(&target),
        Ok(_) => std::fs::remove_file(&target),
        Err(error) => Err(error),
    };
    match removed {
        Ok(()) => Ok(format!("Removed skill '{name}'.")),
        Err(_) => Err(format!("Failed to remove skill '{name}'.")),
    }
}

#[cfg(test)]
mod tests {
    use super::super::discovery::{discover, Source};
    use super::*;
    use std::fs;

    #[test]
    fn the_grammar_is_upstreams() {
        assert_eq!(parse(""), Command::List);
        assert_eq!(parse("  list "), Command::List);
        assert_eq!(parse("show demo"), Command::Show("demo".to_string()));
        assert_eq!(
            parse("add ./src --skill foo"),
            Command::Install {
                source: "./src".to_string(),
                filter: Some("foo".to_string())
            }
        );
        assert_eq!(
            parse("install owner/repo --skill=bar"),
            Command::Install {
                source: "owner/repo".to_string(),
                filter: Some("bar".to_string())
            }
        );
        assert_eq!(parse("create made"), Command::Create("made".to_string()));
        assert_eq!(parse("remove made"), Command::Remove("made".to_string()));
        assert_eq!(parse("path"), Command::Path);
        for usage in ["bogus", "show", "listx", "path now", "add"] {
            assert_eq!(parse(usage), Command::Usage, "{usage}");
        }
    }

    fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let base = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(base.path()).unwrap();
        let work = root.join("work");
        let home = root.join("home");
        fs::create_dir_all(work.join(".xfx/skills/demo")).unwrap();
        fs::write(
            work.join(".xfx/skills/demo/SKILL.md"),
            "---\nname: demo\ndescription: Demo\n---\n",
        )
        .unwrap();
        fs::create_dir_all(&home).unwrap();
        (base, work, home)
    }

    #[test]
    fn create_writes_upstreams_template_and_remove_takes_only_managed_skills() {
        let (_base, work, home) = fixture();
        let managed = home.join(".xfx/skills");
        let said = execute(
            &parse("create made"),
            &discover(&work, Some(&home)),
            &managed,
            &work,
        );
        let file = managed.join("made/SKILL.md");
        assert_eq!(
            said,
            Outcome::Changed {
                said: Vec::new(),
                after: format!("Created {}", file.display())
            }
        );
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            "---\nname: made\ndescription: Describe when this skill should activate\n---\n\n# made\n\nInstructions for this skill...\n"
        );
        assert_eq!(
            execute(
                &parse("create ../escape"),
                &Discovery::default(),
                &managed,
                &work
            ),
            Outcome::Said(vec![
                "Invalid skill name. Use a single directory name without '/' or '\\'.".to_string()
            ])
        );

        let found = discover(&work, Some(&home));
        assert_eq!(found.named("made").unwrap().source, Source::Managed);
        assert_eq!(
            execute(&parse("remove demo"), &found, &managed, &work),
            Outcome::Said(vec![format!(
                "Skill 'demo' comes from workspace .xfx/skills, not the xfx managed install root. Remove it from {}.",
                work.join(".xfx/skills/demo").display()
            )])
        );
        assert!(work.join(".xfx/skills/demo/SKILL.md").exists());
        assert_eq!(
            execute(&parse("remove made"), &found, &managed, &work),
            Outcome::Changed {
                said: Vec::new(),
                after: "Removed skill 'made'.".to_string()
            }
        );
        assert!(!managed.join("made").exists());
        assert_eq!(
            execute(
                &parse("remove made"),
                &discover(&work, Some(&home)),
                &managed,
                &work
            ),
            Outcome::Said(vec!["Skill 'made' not found.".to_string()])
        );
    }

    #[test]
    fn show_focuses_one_skill_and_path_names_xfx_roots_only() {
        let (_base, work, home) = fixture();
        let managed = home.join(".xfx/skills");
        let found = discover(&work, Some(&home));
        assert_eq!(
            execute(&parse("show demo"), &found, &managed, &work),
            Outcome::Focus("demo".to_string())
        );
        assert_eq!(
            execute(&parse("show nope"), &found, &managed, &work),
            Outcome::Said(vec!["Skill 'nope' not found.".to_string()])
        );
        let Outcome::Said(path) = execute(&Command::Path, &found, &managed, &work) else {
            panic!("path says something");
        };
        assert_eq!(
            path,
            [format!(
                "xfx workspace roots are auto-discovered from .xfx/skills and skills/.\nxfx managed install root: {}",
                managed.display()
            )]
        );
        assert!(!path[0].contains(".claude"));
    }
}
