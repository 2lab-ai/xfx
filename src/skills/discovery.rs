//! Which skills a session can see, found the way upstream finds them, in
//! **xfx's own roots only**.
//!
//! Upstream's discovery (`vercel-labs/fx@c1db9191
//! src/core/skills/skill_runtime.zig:336-1046`, root policy
//! `src/builtins/skills.zig:16-38`) scans, in precedence order, the workspace
//! and each of its ancestors short of the home directory, then the managed
//! install root, then the home directory's compatibility roots. Its product
//! roots are `.fx/skills` and `skills/`; xfx's are `.xfx/skills` and `skills/`,
//! and its managed root is `~/.xfx/skills`.
//!
//! **What is not here, on purpose.** Upstream also reads other agents' skill
//! roots -- `.opencode/skills`, `.codex/skills`, `.claude/skills`,
//! `.agents/skills`, `.claw/skills`, in the workspace and in the home
//! directory. xfx does not: on a machine where another agent keeps its skills,
//! reading them would put that agent's instructions into xfx's prompt without
//! anyone having asked for it. That is a decided default, recorded in
//! `docs/parity.md` and `.prd/tui-phase4/ssot.md`, not an omission.
//!
//! Everything else is upstream's: the candidates of a root in byte order, a
//! candidate that is a directory with a regular `SKILL.md` in it, a link
//! followed only where the root has an authority to contain it, the same
//! directory reached twice counted once, and every candidate that could not be
//! read kept as a diagnostic rather than dropped in silence.

use std::collections::HashSet;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

use super::contract::{self, InvalidCause};

/// The workspace root of xfx's own skills (`.fx/skills` upstream).
pub const WORKSPACE_ROOT: &str = ".xfx/skills";

/// The workspace root shared by every agent that reads `skills/`.
pub const SHARED_ROOT: &str = "skills";

/// The managed install root, under the home directory.
pub const MANAGED_ROOT: &str = ".xfx/skills";

/// Where a skill was found (upstream's `SkillSource`, cut to xfx's roots).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Source {
    /// `<workspace or ancestor>/.xfx/skills`.
    WorkspaceXfx,
    /// `<workspace or ancestor>/skills`.
    WorkspaceShared,
    /// `~/.xfx/skills`, where `/skills create` and `add` write.
    Managed,
}

impl Source {
    /// The long label a refusal names a root by (`skillSourceLabel`,
    /// `skill_runtime.zig:1138-1154`).
    pub fn label(self) -> &'static str {
        match self {
            Self::WorkspaceXfx => "workspace .xfx/skills",
            Self::WorkspaceShared => "workspace skills/",
            Self::Managed => "global ~/.xfx/skills",
        }
    }

    /// The short label the menu search matches (`skillSourceShortLabel`,
    /// `:1156-1172`).
    pub fn short_label(self) -> &'static str {
        match self {
            Self::WorkspaceXfx => "workspace .xfx",
            Self::WorkspaceShared => "workspace skills/",
            Self::Managed => "global .xfx",
        }
    }

    /// Which group a listing files it under, and in what order
    /// (`skillGroupLabel`, `skillGroupRank`, `:1098-1135`).
    pub fn group(self) -> (usize, &'static str) {
        match self {
            Self::Managed => (0, "Managed installs"),
            Self::WorkspaceXfx | Self::WorkspaceShared => (1, "Workspace skills"),
        }
    }

    /// Whether `/skills remove` may delete it (`isManagedInstallSkill`).
    pub fn is_managed(self) -> bool {
        self == Self::Managed
    }
}

/// One skill: what it is called, what it says it is for, the directory it is,
/// and the root it came from (upstream's `Skill`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skill {
    pub name: String,
    pub description: String,
    /// The candidate directory as the root names it -- the root joined with
    /// the entry's name, not resolved -- which is the identity a location and
    /// a binding carry.
    pub path: PathBuf,
    pub source: Source,
}

/// How much of a root a diagnostic is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// The whole root could not be read: an unknown number of skills are missing.
    Root,
    /// One candidate was skipped.
    Candidate,
}

/// Why something was skipped (`SkillDiagnosticCause`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cause {
    InvalidMetadata(InvalidCause),
    LinkedCandidateUnavailable,
    Unreadable,
    Oversized,
}

/// One thing discovery could not use, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub path: PathBuf,
    pub source: Source,
    pub scope: Scope,
    pub cause: Cause,
}

/// What one discovery found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Discovery {
    pub skills: Vec<Skill>,
    pub diagnostics: Vec<Diagnostic>,
}

impl Discovery {
    /// The skill `/skills show` and `remove` mean by a bare name: the managed
    /// one when there is one, else the first found (`findSkillForProvider`,
    /// `vercel-labs/fx@c1db9191 src/core/app/app_commands.zig:1855-1865`).
    pub fn named(&self, name: &str) -> Option<&Skill> {
        let mut fallback = None;
        for skill in self.skills.iter().filter(|skill| skill.name == name) {
            if skill.source.is_managed() {
                return Some(skill);
            }
            fallback.get_or_insert(skill);
        }
        fallback
    }
}

/// One directory discovery reads, and what may contain a link inside it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Root {
    path: PathBuf,
    source: Source,
    /// The directory a linked candidate must resolve inside: the workspace
    /// ancestor the root hangs off. The managed root has none, so a link in it
    /// is never followed (`appendDupeRoot`, `skill_runtime.zig:534-536`).
    authority: Option<PathBuf>,
}

/// The managed install root for `home`.
pub fn managed_root(home: &Path) -> PathBuf {
    home.join(MANAGED_ROOT)
}

/// Every root, in precedence order (`appendConfiguredSkillRoots`,
/// `skill_runtime.zig:384-409`).
///
/// The workspace and each ancestor **short of the home directory** -- a
/// workspace that is not under home is walked to `/`, as upstream's is -- then
/// the managed root. A root reached twice is read once.
fn roots(workspace: &Path, home: Option<&Path>) -> Vec<Root> {
    let mut roots: Vec<Root> = Vec::new();
    let mut push = |root: Root| {
        if !roots.iter().any(|kept| kept.path == root.path) {
            roots.push(root);
        }
    };
    for dir in workspace.ancestors() {
        if home.is_some_and(|home| home == dir) {
            break;
        }
        for (relative, source) in [
            (WORKSPACE_ROOT, Source::WorkspaceXfx),
            (SHARED_ROOT, Source::WorkspaceShared),
        ] {
            push(Root {
                path: dir.join(relative),
                source,
                authority: Some(dir.to_path_buf()),
            });
        }
    }
    if let Some(home) = home {
        push(Root {
            path: managed_root(home),
            source: Source::Managed,
            authority: None,
        });
    }
    roots
}

/// Every skill visible from `workspace` (`loadVisibleSkills`,
/// `skill_runtime.zig:336-382`), and every candidate that could not be used.
///
/// `home` is the home directory the managed root is under; without one there
/// is no managed root and the ancestor walk goes to `/`.
pub fn discover(workspace: &Path, home: Option<&Path>) -> Discovery {
    let mut found = Discovery::default();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    for root in roots(workspace, home) {
        read_root(&root, &mut found, &mut seen);
    }
    found
}

/// Every candidate of one root, in byte order (`appendSkillsFromDir`,
/// `:703-728`).
fn read_root(root: &Root, found: &mut Discovery, seen: &mut HashSet<PathBuf>) {
    let entries = match root_entries(root) {
        Ok(entries) => entries,
        Err(_) if root_is_missing(&root.path) => return,
        Err(_) => {
            found.diagnostics.push(Diagnostic {
                path: root.path.clone(),
                source: root.source,
                scope: Scope::Root,
                cause: Cause::Unreadable,
            });
            return;
        }
    };
    for (name, linked) in entries {
        read_candidate(root, &name, linked, found, seen);
    }
}

/// The root's directory entries and links -- links only where the root can
/// contain them -- sorted by name (`collectSkillEntries`, `:283-311`).
fn root_entries(root: &Root) -> io::Result<Vec<(std::ffi::OsString, bool)>> {
    let dir = match &root.authority {
        Some(authority) => contained(&root.path, authority)?,
        None => {
            let meta = fs::symlink_metadata(&root.path)?;
            if !meta.is_dir() {
                return Err(io::Error::new(
                    io::ErrorKind::NotADirectory,
                    "the managed root is not a directory",
                ));
            }
            root.path.clone()
        }
    };
    let mut entries = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let linked = kind.is_symlink();
        if !(kind.is_dir() || linked && root.authority.is_some()) {
            continue;
        }
        entries.push((entry.file_name(), linked));
    }
    entries.sort_by(|left, right| left.0.as_encoded_bytes().cmp(right.0.as_encoded_bytes()));
    Ok(entries)
}

/// `path` resolved, when it resolves inside `authority`
/// (`openContainedDir`, `:560-572`).
fn contained(path: &Path, authority: &Path) -> io::Result<PathBuf> {
    let canonical = fs::canonicalize(path)?;
    let inside = canonical.starts_with(authority)
        || fs::canonicalize(authority).is_ok_and(|resolved| canonical.starts_with(resolved));
    if !inside {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "the path resolves outside its authority",
        ));
    }
    Ok(canonical)
}

/// Whether a root that would not open is simply not there -- every component
/// a directory until one that does not exist -- rather than there and
/// unreadable (`rootPathIsMissing`, `:741-777`).
///
/// **A link to a directory counts as one on the way**, where upstream's walk
/// stops at any link: a home directory reached through one -- `/var` on macOS,
/// `/home` on some Linux systems -- would otherwise turn a managed root that
/// was never created into a warning on every turn.
fn root_is_missing(path: &Path) -> bool {
    let mut walked = PathBuf::new();
    for component in path.components() {
        walked.push(component);
        match fs::metadata(&walked) {
            Err(error) => return error.kind() == io::ErrorKind::NotFound,
            Ok(meta) if !meta.is_dir() => return false,
            Ok(_) => {}
        }
    }
    false
}

/// One candidate, appended or diagnosed (`appendSkillCandidate`, `:844-940`).
fn read_candidate(
    root: &Root,
    name: &std::ffi::OsStr,
    linked: bool,
    found: &mut Discovery,
    seen: &mut HashSet<PathBuf>,
) {
    let path = root.path.join(name);
    let diagnose = |found: &mut Discovery, cause: Cause| {
        found.diagnostics.push(Diagnostic {
            path: path.clone(),
            source: root.source,
            scope: Scope::Candidate,
            cause,
        });
    };
    let dir = if linked {
        let Some(authority) = &root.authority else {
            return;
        };
        match contained(&path, authority) {
            Ok(dir) if fs::metadata(&dir).is_ok_and(|meta| meta.is_dir()) => dir,
            _ => return diagnose(found, Cause::LinkedCandidateUnavailable),
        }
    } else {
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.is_dir() => path.clone(),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return,
            _ => return diagnose(found, Cause::Unreadable),
        }
    };
    let mut file = match open_skill_file(&dir, root.authority.as_deref()) {
        Opened::File(file) => file,
        Opened::Missing => return,
        Opened::Rejected => return diagnose(found, Cause::Unreadable),
    };
    // The same directory reached by a second name is the first one's.
    let canonical = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
    if !seen.insert(canonical) {
        return;
    }
    let fallback = name.to_string_lossy();
    match inspect(&mut file, &fallback) {
        Inspected::Valid(metadata) => found.skills.push(Skill {
            name: metadata.name,
            description: metadata.description,
            path,
            source: root.source,
        }),
        Inspected::Invalid(cause) => diagnose(found, Cause::InvalidMetadata(cause)),
        Inspected::Unreadable => diagnose(found, Cause::Unreadable),
        Inspected::Oversized => diagnose(found, Cause::Oversized),
    }
}

/// What opening a candidate's `SKILL.md` came to.
pub(crate) enum Opened {
    File(File),
    Missing,
    Rejected,
}

/// The candidate's `SKILL.md`: a regular file, or a link to one inside the
/// root's authority (`openPrimarySkillFile`, `:779-842`).
pub(crate) fn open_skill_file(dir: &Path, authority: Option<&Path>) -> Opened {
    let path = dir.join("SKILL.md");
    let meta = match fs::symlink_metadata(&path) {
        Ok(meta) => meta,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Opened::Missing,
        Err(_) => return Opened::Rejected,
    };
    if meta.file_type().is_symlink() {
        let Some(authority) = authority else {
            return Opened::Rejected;
        };
        let Ok(target) = contained(&path, authority) else {
            return Opened::Rejected;
        };
        if !fs::symlink_metadata(&target).is_ok_and(|meta| meta.is_file()) {
            return Opened::Rejected;
        }
        return match File::open(&target) {
            Ok(file) => Opened::File(file),
            Err(_) => Opened::Rejected,
        };
    }
    if !meta.is_file() {
        return Opened::Rejected;
    }
    match open_no_follow(&path) {
        Ok(file) => Opened::File(file),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Opened::Missing,
        Err(_) => Opened::Rejected,
    }
}

/// A file opened without following a link at its last component.
pub(crate) fn open_no_follow(path: &Path) -> io::Result<File> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    options.open(path)
}

enum Inspected {
    Valid(contract::Metadata),
    Invalid(InvalidCause),
    Unreadable,
    Oversized,
}

/// A candidate's metadata, from the head of its file
/// (`inspectSkillCandidateFile`, `:959-987`).
fn inspect(file: &mut File, fallback: &str) -> Inspected {
    let Ok(meta) = file.metadata() else {
        return Inspected::Unreadable;
    };
    if !meta.is_file() {
        return Inspected::Unreadable;
    }
    let content = match contract::read_metadata_prefix(file, meta.len()) {
        Ok(Some(content)) => content,
        Ok(None) => return Inspected::Oversized,
        Err(_) => return Inspected::Unreadable,
    };
    match contract::resolve(&contract::parse(&content), fallback) {
        Ok(metadata) => Inspected::Valid(metadata),
        Err(cause) => Inspected::Invalid(cause),
    }
}

/// The candidate `skill` names, opened and checked to still be the skill that
/// was advertised: its file a regular `SKILL.md` and its name unchanged
/// (`openValidatedSkillCandidate`, `:989-1046`).
pub(crate) fn reopen(skill: &Skill, authority: Option<&Path>) -> Result<File, Reopened> {
    let dir = match fs::symlink_metadata(&skill.path) {
        Ok(meta) if meta.is_dir() => skill.path.clone(),
        Ok(meta) if meta.file_type().is_symlink() => match authority {
            Some(authority) => contained(&skill.path, authority).map_err(|_| Reopened::Skipped)?,
            None => return Err(Reopened::Skipped),
        },
        Ok(_) => return Err(Reopened::Skipped),
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Err(Reopened::Missing),
        Err(_) => return Err(Reopened::Skipped),
    };
    let mut file = match open_skill_file(&dir, authority) {
        Opened::File(file) => file,
        Opened::Missing => return Err(Reopened::Missing),
        Opened::Rejected => return Err(Reopened::Skipped),
    };
    let fallback = skill
        .path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    match inspect(&mut file, &fallback) {
        Inspected::Valid(metadata) if metadata.name == skill.name => {
            use std::io::Seek;
            file.rewind().map_err(|_| Reopened::Skipped)?;
            Ok(file)
        }
        Inspected::Valid(_) => Err(Reopened::NameMismatch),
        _ => Err(Reopened::Skipped),
    }
}

/// Why an advertised candidate could not be opened again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reopened {
    Missing,
    NameMismatch,
    Skipped,
}

/// The authority a skill's root had, for opening it again: the workspace
/// ancestor its root hangs off, or none for a managed skill.
pub(crate) fn authority_of(skill: &Skill) -> Option<PathBuf> {
    match skill.source {
        Source::Managed => None,
        Source::WorkspaceXfx => skill
            .path
            .parent()?
            .parent()?
            .parent()
            .map(Path::to_path_buf),
        Source::WorkspaceShared => skill.path.parent()?.parent().map(Path::to_path_buf),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn skill(dir: &Path, name: &str, description: &str) {
        write(
            &dir.join(name).join("SKILL.md"),
            &format!("---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n"),
        );
    }

    #[test]
    fn roots_are_read_workspace_first_then_ancestors_then_the_managed_root() {
        let base = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(base.path()).unwrap();
        let home = base.join("home");
        let work = home.join("projects").join("work");
        fs::create_dir_all(&work).unwrap();
        skill(&work.join(".xfx/skills"), "demo", "Demo workspace skill");
        skill(&work.join("skills"), "release-notes", "Write release notes");
        skill(&home.join("projects/skills"), "parent", "From the parent");
        skill(&home.join(".xfx/skills"), "foo", "Managed");
        // Another agent's roots, in the workspace and at home: never read.
        skill(&work.join(".claude/skills"), "claude-only", "not xfx's");
        skill(&home.join(".codex/skills"), "codex-only", "not xfx's");
        // Home itself is where the walk stops: its `skills/` is not a root.
        skill(&home.join("skills"), "home-shared", "past the walk");

        let found = discover(&work, Some(&home));
        let names: Vec<(&str, Source)> = found
            .skills
            .iter()
            .map(|skill| (skill.name.as_str(), skill.source))
            .collect();
        assert_eq!(
            names,
            [
                ("demo", Source::WorkspaceXfx),
                ("release-notes", Source::WorkspaceShared),
                ("parent", Source::WorkspaceShared),
                ("foo", Source::Managed),
            ]
        );
        assert_eq!(found.skills[0].path, work.join(".xfx/skills/demo"));
        assert!(found.diagnostics.is_empty(), "{:?}", found.diagnostics);
    }

    #[test]
    fn candidates_are_in_byte_order_and_bad_ones_are_diagnosed() {
        let base = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(base.path()).unwrap();
        let root = base.join("work/.xfx/skills");
        skill(&root, "b", "second");
        skill(&root, "a", "first");
        write(&root.join("broken/SKILL.md"), "---\nname: x/y\n---\n");
        fs::create_dir_all(root.join("no-skill-file")).unwrap();
        fs::create_dir_all(root.join("dir-is-file/SKILL.md")).unwrap();
        write(&root.join("plain-file"), "not a candidate");

        let found = discover(&base.join("work"), Some(&base.join("home")));
        let names: Vec<&str> = found
            .skills
            .iter()
            .map(|skill| skill.name.as_str())
            .collect();
        assert_eq!(names, ["a", "b"]);
        assert_eq!(
            found.diagnostics,
            [
                Diagnostic {
                    path: root.join("broken"),
                    source: Source::WorkspaceXfx,
                    scope: Scope::Candidate,
                    cause: Cause::InvalidMetadata(InvalidCause::InvalidName),
                },
                Diagnostic {
                    path: root.join("dir-is-file"),
                    source: Source::WorkspaceXfx,
                    scope: Scope::Candidate,
                    cause: Cause::Unreadable,
                },
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_link_is_followed_inside_its_workspace_and_never_in_the_managed_root() {
        let base = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(base.path()).unwrap();
        let work = base.join("work");
        let home = base.join("home");
        skill(&work.join("elsewhere"), "linked", "through a link");
        skill(&base.join("outside"), "escaped", "outside the workspace");
        fs::create_dir_all(work.join(".xfx/skills")).unwrap();
        std::os::unix::fs::symlink(
            work.join("elsewhere/linked"),
            work.join(".xfx/skills/linked"),
        )
        .unwrap();
        std::os::unix::fs::symlink(
            base.join("outside/escaped"),
            work.join(".xfx/skills/escaped"),
        )
        .unwrap();
        skill(
            &base.join("managed-target"),
            "managed-link",
            "a link at home",
        );
        fs::create_dir_all(home.join(".xfx/skills")).unwrap();
        std::os::unix::fs::symlink(
            base.join("managed-target/managed-link"),
            home.join(".xfx/skills/managed-link"),
        )
        .unwrap();

        let found = discover(&work, Some(&home));
        let names: Vec<&str> = found
            .skills
            .iter()
            .map(|skill| skill.name.as_str())
            .collect();
        assert_eq!(names, ["linked"]);
        assert_eq!(
            found.diagnostics,
            [Diagnostic {
                path: work.join(".xfx/skills/escaped"),
                source: Source::WorkspaceXfx,
                scope: Scope::Candidate,
                cause: Cause::LinkedCandidateUnavailable,
            }]
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_missing_root_under_a_linked_home_is_not_a_warning() {
        let base = tempfile::tempdir().unwrap();
        let real = fs::canonicalize(base.path()).unwrap();
        fs::create_dir_all(real.join("real-home")).unwrap();
        fs::create_dir_all(real.join("work")).unwrap();
        std::os::unix::fs::symlink(real.join("real-home"), real.join("home")).unwrap();
        let found = discover(&real.join("work"), Some(&real.join("home")));
        assert!(found.diagnostics.is_empty(), "{:?}", found.diagnostics);
    }

    #[test]
    fn a_bare_name_prefers_the_managed_skill() {
        let make = |source| Skill {
            name: "dup".to_string(),
            description: String::new(),
            path: PathBuf::from(format!("/{source:?}/dup")),
            source,
        };
        let found = Discovery {
            skills: vec![make(Source::WorkspaceXfx), make(Source::Managed)],
            diagnostics: Vec::new(),
        };
        assert_eq!(found.named("dup").unwrap().source, Source::Managed);
        assert!(found.named("nope").is_none());
    }
}
