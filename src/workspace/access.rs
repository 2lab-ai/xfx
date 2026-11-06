//! The directories a session may work in besides its workspace, and the
//! `/workspace` command that keeps them.
//!
//! Upstream's `/workspace [list|add PATH|remove PATH|clear]`
//! (`vercel-labs/fx@c1db9191 src/builtins/commands.zig:496`): an added
//! directory is **saved** -- `workspaces["<primary>"].additional_directories`
//! in the profile, a list of canonical paths (`settings_store.zig:1308-1450`)
//! -- and **active** while it is still the directory it was, and every active
//! one is a root of the session's [`AccessScope`] beside the workspace itself.
//!
//! # What an added root may be, and what it grants
//!
//! A directory that exists, named by any spelling -- relative to the
//! workspace, through `..`, through a symbolic link -- **saved as the real
//! path it resolves to** (`canonicalExistingDirectory`,
//! `workspace_access.zig:566-604`). That is the whole rule upstream has: the
//! workspace itself is refused, a path that is not a directory is refused, `~`
//! is a name like any other (relative, so `<workspace>/~`), and the home
//! directory or `/` are directories like any other. Each is admitted here as
//! it is there, and what it grants is what it grants there: reads and writes
//! inside it, as inside the workspace (`crate::permission::TargetScope`), with
//! auto still refusing the files that decide what runs later
//! (`crate::permission::MutationPlan::sensitive_auto_write_target`) and every
//! mode still refusing `.git` and `.xfx` (`crate::workspace::path`). Nothing
//! here widens a root past the directory the user named: it is resolved once,
//! when it is added, and **never followed again** -- not by a later turn
//! ([`AccessScope::with_observed_roots`]) and not by a later launch
//! ([`WorkspaceAccess::load`]): a saved path that has since become a link to
//! somewhere else is unavailable rather than the somewhere else authorized.
//! At launch that is narrower than upstream, which resolves a saved spelling
//! again (`resolveSavedDirectory`, `workspace_access.zig:534-561`); a
//! hand-written spelling that is not a real path (`/tmp/x` on macOS) is
//! therefore listed `Unavailable` here where upstream would follow it --
//! until `/workspace add` names that directory, which rewrites the entry to
//! its real path.
//!
//! # What is not upstream's
//!
//! Upstream also has launch-only roots (`--add-dir` on its interactive
//! command line) and a switch that suppresses the saved ones
//! (`--no-additional-dirs`, `cli_surface.zig:439-443`). xfx's shells have
//! neither -- `--add-dir` belongs to `xfx ask` alone (`crate::cli`) -- so every
//! entry here is saved, none is launch-only, and nothing is suppressed; the
//! snapshot says `saved_suppressed=false`, `command_line=false` and
//! `launch_flag_can_restore=false` because each is true of this product, not
//! because it was not asked.

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use serde_json::{Map, Value};

use super::path::{AccessScope, PathError};

/// The most directories a workspace may add (`max_additional_directories`,
/// `vercel-labs/fx@c1db9191 src/core/workspace/workspace_access.zig:7`).
pub const MAX_ADDITIONAL_DIRECTORIES: usize = 16;

/// What every `/workspace` notice is filed under.
pub const TOPIC: &str = "workspace";

/// `/workspace`'s refusal, upstream's words exactly
/// (`handleWorkspaceCommand`, `app_commands.zig:245-252`) -- which leave out
/// `list` although `list` is accepted.
pub const USAGE: &str = "usage: /workspace [add PATH|remove PATH|clear]";

/// What a change asked for while anything is running or waiting says
/// (`app_commands.zig:263-270`), as a neutral notice.
pub const BUSY: &str =
    "Workspace changes are unavailable until the active and queued work finishes.";

/// The settings object that holds one entry per workspace root.
pub const WORKSPACES_KEY: &str = "workspaces";

/// The key under a workspace's entry that holds its saved directories.
pub const ADDITIONAL_DIRECTORIES_KEY: &str = "additional_directories";

/// The longest path a command or the profile may name (`std.fs.max_path_bytes`).
const MAX_PATH_BYTES: usize = 4096;

// ---------------------------------------------------------------------------
// the command
// ---------------------------------------------------------------------------

/// One change `/workspace` can make.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Save a directory, given as typed.
    Add(String),
    /// Forget a saved directory, given as typed.
    Remove(String),
    /// Forget every saved directory.
    Clear,
}

impl Action {
    /// The word the snapshot's change line starts with.
    fn label(&self) -> &'static str {
        match self {
            Self::Add(_) => "add",
            Self::Remove(_) => "remove",
            Self::Clear => "clear",
        }
    }

    /// The path as the user typed it, which is what the change line echoes:
    /// `add ../extra`, while the list names the directory it resolved to.
    fn typed(&self) -> Option<&str> {
        match self {
            Self::Add(path) | Self::Remove(path) => Some(path),
            Self::Clear => None,
        }
    }
}

/// The rest of a `/workspace` line, read (`parseWorkspaceCommand`,
/// `app_commands.zig:198-216`): `Ok(None)` for the snapshot -- nothing, or
/// `list` -- `Ok(Some(_))` for a change, and `Err(())` for anything else,
/// which is answered with [`USAGE`]. The word is exact and case-sensitive,
/// and `add` or `remove` needs a path after a blank.
#[allow(clippy::result_unit_err)]
pub fn parse(rest: &str) -> Result<Option<Action>, ()> {
    let trimmed = rest.trim_matches([' ', '\t']);
    if trimmed.is_empty() || trimmed == "list" {
        return Ok(None);
    }
    if trimmed == "clear" {
        return Ok(Some(Action::Clear));
    }
    for (word, make) in [
        ("add", Action::Add as fn(String) -> Action),
        ("remove", Action::Remove as fn(String) -> Action),
    ] {
        let Some(after) = trimmed.strip_prefix(word) else {
            continue;
        };
        if !after.starts_with(|c: char| c.is_ascii_whitespace()) {
            continue;
        }
        let path = after.trim_matches([' ', '\t']);
        if path.is_empty() {
            return Err(());
        }
        return Ok(Some(make(path.to_string())));
    }
    Err(())
}

// ---------------------------------------------------------------------------
// the directories
// ---------------------------------------------------------------------------

/// One saved directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The real path it resolved to when it was saved -- or, for one that did
    /// not exist when it was read, the path it would have, through the
    /// nearest directory that did.
    pub path: PathBuf,
    /// Whether it is still a directory at that real path. A saved directory
    /// is active exactly when it is available: xfx suppresses none.
    pub available: bool,
}

impl Entry {
    /// What the picker says beside it (`formatWorkspaceEntryStatus`,
    /// `compact_command_menu_presentation.zig:1140-1158`): `Active · Saved`, or
    /// `Unavailable · Saved` while it is not there.
    pub fn status(&self) -> &'static str {
        if self.available {
            "Active \u{b7} Saved"
        } else {
            "Unavailable \u{b7} Saved"
        }
    }
}

/// The saved directories of one workspace, in the order they were saved.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkspaceAccess {
    entries: Vec<Entry>,
}

impl WorkspaceAccess {
    /// The directories `saved` names for the workspace at `primary`, as a
    /// launch reads them (`WorkspaceAccess.init`, `workspace_access.zig:97-157`).
    ///
    /// Each is **observed, not resolved** ([`observe_saved`]): what this
    /// product saves is a real path, so a saved path whose real path is no
    /// longer itself -- a link now, or a spelling somebody wrote by hand -- is
    /// listed as written and unavailable, rather than whatever it now leads to
    /// being authorized. One that is not there is kept, unavailable, rather
    /// than dropped: it is still saved, and `/workspace remove` still has to be
    /// able to forget it. A spelling that cannot be a directory at all (the
    /// workspace itself, a relative path) is left out rather than failing the
    /// launch over a line in a file.
    pub fn load(primary: &Path, saved: &[String]) -> Self {
        let mut entries: Vec<Entry> = Vec::new();
        for source in saved {
            let Ok((path, available)) = observe_saved(primary, source) else {
                continue;
            };
            match entries.iter_mut().find(|entry| entry.path == path) {
                Some(entry) => entry.available |= available,
                None => entries.push(Entry { path, available }),
            }
        }
        Self { entries }
    }

    /// Every saved directory, in order.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// The saved directories as the profile keeps them.
    pub fn saved(&self) -> Vec<String> {
        self.entries
            .iter()
            .map(|entry| entry.path.to_string_lossy().into_owned())
            .collect()
    }

    /// The directories a turn may use: the available ones.
    pub fn active_roots(&self) -> impl Iterator<Item = &Path> {
        self.entries
            .iter()
            .filter(|entry| entry.available)
            .map(|entry| entry.path.as_path())
    }

    /// The same directories looked at again (`stageAvailabilityRefresh`,
    /// `workspace_access.zig:208-276`): each is available while its real path
    /// is still itself and a directory.
    ///
    /// **Observed, not re-resolved**, which is upstream's rule for a root it
    /// has already seen (`resolveObservedDirectory`, `:615-633`), applied here
    /// to one that was missing as well: a saved directory that comes back as a
    /// symbolic link is unavailable rather than the link's target.
    pub fn refreshed(&self) -> Self {
        Self {
            entries: self
                .entries
                .iter()
                .map(|entry| Entry {
                    path: entry.path.clone(),
                    available: observed(&entry.path),
                })
                .collect(),
        }
    }

    /// The scope a turn runs in: the workspace and every active directory,
    /// each kept only while it is still the directory that was saved
    /// ([`AccessScope::with_observed_roots`]).
    pub fn scope(&self, primary: &Path) -> Result<AccessScope, PathError> {
        AccessScope::with_observed_roots(primary, self.active_roots())
    }
}

/// Whether `path` is still a directory whose real path is itself.
fn observed(path: &Path) -> bool {
    fs::canonicalize(path).is_ok_and(|now| now == path)
        && fs::symlink_metadata(path).is_ok_and(|meta| meta.is_dir())
}

// ---------------------------------------------------------------------------
// why a change was refused
// ---------------------------------------------------------------------------

/// Why a `/workspace` change did not happen, in upstream's words where
/// upstream has the case (`workspaceErrorMessage`,
/// `output_contracts.zig:207-221`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    InvalidPath,
    PathNotFound,
    NotDirectory,
    UnknownAdditionalDirectory,
    PrimaryDirectory,
    TooManyDirectories,
    HomeNotSet,
    /// The profile could not be read or written; the words are the
    /// operating system's or the settings file's.
    Settings(String),
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidPath => "path is invalid",
            Self::PathNotFound => "directory does not exist",
            Self::NotDirectory => "path is not a directory",
            Self::UnknownAdditionalDirectory => {
                "directory is not configured as an additional workspace"
            }
            Self::PrimaryDirectory => "the primary workspace cannot be added or removed",
            Self::TooManyDirectories => "additional directory limit reached",
            Self::HomeNotSet => "HOME is not set",
            Self::Settings(reason) => reason,
        })
    }
}

impl From<io::Error> for Refusal {
    fn from(err: io::Error) -> Self {
        Self::Settings(err.to_string())
    }
}

/// How far a change got before it was refused, which is what the notice's
/// first words say (`app_commands.zig:285-297`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Nothing was written: the path itself was refused.
    Stage,
    /// The profile was not changed.
    Commit,
    /// The profile was written and could not be read back.
    Reconcile,
}

/// A change that did not happen, and the sentence that says so.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub phase: Phase,
    pub refusal: Refusal,
}

impl Failure {
    fn at(phase: Phase) -> impl Fn(Refusal) -> Self {
        move |refusal| Self { phase, refusal }
    }

    /// `Workspace update rejected: directory does not exist`, and its kind.
    pub fn message(&self) -> String {
        let prefix = match self.phase {
            Phase::Stage => "Workspace update rejected",
            Phase::Commit => "Workspace settings were not changed",
            Phase::Reconcile => "Workspace settings are uncertain and could not be reloaded",
        };
        format!("{prefix}: {}", self.refusal)
    }
}

// ---------------------------------------------------------------------------
// a change
// ---------------------------------------------------------------------------

/// What a change did, for the snapshot's change line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mutation {
    action: &'static str,
    typed: Option<String>,
    /// The profile was written.
    pub saved_changed: bool,
    /// The directories in force are not the ones they were.
    pub runtime_changed: bool,
}

/// A change that happened: the directories now in force, read back from the
/// profile, and what the change line says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Updated {
    pub access: WorkspaceAccess,
    pub mutation: Mutation,
}

/// What reaches the profile, once the path is known.
enum Commit {
    Add(String),
    Remove(PathBuf),
    Clear,
}

/// Makes one change (`workspace_commands.execute`,
/// `vercel-labs/fx@c1db9191 src/core/workspace/workspace_commands.zig:83-181`):
/// the path is resolved, the profile at `settings` is changed under its lock,
/// and the directories in force are **read back from what was written**
/// rather than assumed from what was asked, so another process's change to the
/// same list is adopted rather than overwritten.
///
/// Durable or nothing, as upstream's is: a change the profile did not take
/// is not put in force for this process either. The caller must not run a
/// turn while this runs -- the change is to the scope that turn would use.
pub fn execute(
    primary: &Path,
    settings: Option<&Path>,
    current: &WorkspaceAccess,
    action: &Action,
) -> Result<Updated, Failure> {
    let commit = match action {
        Action::Add(typed) => Commit::Add(
            canonical_existing_directory(primary, typed)
                .and_then(utf8)
                .map_err(Failure::at(Phase::Stage))?,
        ),
        Action::Remove(typed) => Commit::Remove(
            removal_identity(current, primary, typed).map_err(Failure::at(Phase::Stage))?,
        ),
        Action::Clear => Commit::Clear,
    };
    let settings = settings.ok_or(Failure {
        phase: Phase::Commit,
        refusal: Refusal::HomeNotSet,
    })?;
    let key = primary.to_str().ok_or(Failure {
        phase: Phase::Commit,
        refusal: Refusal::InvalidPath,
    })?;
    let saved_changed = crate::provider::profile::edit_document(settings, |document| {
        apply(document, primary, key, &commit)
    })
    .map_err(Failure::at(Phase::Commit))?;
    let document = crate::provider::profile::read_existing(settings)
        .map_err(Refusal::from)
        .map_err(Failure::at(Phase::Reconcile))?;
    let saved = saved_in(&document, primary)
        .map_err(Refusal::Settings)
        .map_err(Failure::at(Phase::Reconcile))?;
    let access = WorkspaceAccess::load(primary, &saved);
    Ok(Updated {
        mutation: Mutation {
            action: action.label(),
            typed: action.typed().map(str::to_string),
            saved_changed,
            runtime_changed: *current != access,
        },
        access,
    })
}

/// The path as a string, which is what the profile can hold.
fn utf8(path: PathBuf) -> Result<String, Refusal> {
    path.into_os_string()
        .into_string()
        .map_err(|_| Refusal::InvalidPath)
}

/// One change to the profile's list for `primary` (`applyWorkspaceDirectoryMutationToRoot`,
/// `settings_store.zig:1308-1450`), and whether it changed anything.
///
/// An entry the list already holds -- by its own spelling, or by the real path
/// that spelling resolves to -- is not added twice. A list that comes out
/// empty leaves the file, and so does a workspace entry and a `workspaces`
/// object it leaves empty (`removeWorkspaceIfEmpty`, `:1498-1505`).
fn apply(
    document: &mut Map<String, Value>,
    primary: &Path,
    key: &str,
    commit: &Commit,
) -> Result<bool, Refusal> {
    let malformed = |what: &str| Refusal::Settings(format!("the settings file's {what}"));
    let workspaces = document
        .entry(WORKSPACES_KEY)
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| malformed("`workspaces` is not an object"))?;
    let entry = workspaces
        .entry(key)
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| malformed("workspace entry is not an object"))?;
    let mut list: Vec<String> = match entry.get(ADDITIONAL_DIRECTORIES_KEY) {
        None => Vec::new(),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| item.as_str().map(str::to_string))
            .collect::<Option<_>>()
            .ok_or_else(|| {
                malformed("`additional_directories` holds a value that is not a path")
            })?,
        Some(_) => return Err(malformed("`additional_directories` is not a list")),
    };
    let names = |source: &str, path: &Path| {
        Path::new(source) == path
            || resolve_saved(primary, source).is_ok_and(|(resolved, _)| resolved == path)
    };
    let changed = match commit {
        Commit::Add(path) => {
            // A spelling of the same directory that is not its real path --
            // written by hand, say -- becomes the real path, which is what a
            // launch puts in force (`WorkspaceAccess::load`); upstream
            // rewrites a matched entry to its identity the same way
            // (`settings_store.zig:1352-1357`). Only the entry this add names:
            // a saved directory that has become a link elsewhere is never
            // "repaired" into the link's target by a change about another.
            let mut found = false;
            let mut rewritten = false;
            for source in list.iter_mut() {
                if !names(source, Path::new(path)) {
                    continue;
                }
                if !found && source != path {
                    *source = path.clone();
                    rewritten = true;
                }
                found = true;
            }
            if found {
                let mut seen = std::collections::HashSet::new();
                let before = list.len();
                list.retain(|source| seen.insert(source.clone()));
                rewritten || list.len() != before
            } else if list.len() >= MAX_ADDITIONAL_DIRECTORIES {
                return Err(Refusal::TooManyDirectories);
            } else {
                list.push(path.clone());
                true
            }
        }
        Commit::Remove(path) => {
            let before = list.len();
            list.retain(|source| !names(source, path));
            list.len() != before
        }
        Commit::Clear => {
            let had = !list.is_empty() || entry.contains_key(ADDITIONAL_DIRECTORIES_KEY);
            list.clear();
            had
        }
    };
    // `shift_remove`, not `remove`: the profile keeps document order, and a
    // swap would move the last key into the hole -- a reordering of somebody
    // else's settings, `permission`'s ordered rows among them, that nothing
    // asked for.
    if list.is_empty() {
        entry.shift_remove(ADDITIONAL_DIRECTORIES_KEY);
    } else {
        entry.insert(
            ADDITIONAL_DIRECTORIES_KEY.to_string(),
            Value::Array(list.into_iter().map(Value::String).collect()),
        );
    }
    if entry.is_empty() {
        workspaces.shift_remove(key);
    }
    if workspaces.is_empty() {
        document.shift_remove(WORKSPACES_KEY);
    }
    Ok(changed)
}

// ---------------------------------------------------------------------------
// what the profile says
// ---------------------------------------------------------------------------

/// The saved directories of the workspace at `primary` in a settings
/// document, or why they cannot be read.
pub fn saved_in(document: &Map<String, Value>, primary: &Path) -> Result<Vec<String>, String> {
    let Some(workspaces) = document.get(WORKSPACES_KEY) else {
        return Ok(Vec::new());
    };
    let Some(workspaces) = workspaces.as_object() else {
        return Err("the settings file's `workspaces` is not an object".to_string());
    };
    let Some(entry) = primary.to_str().and_then(|key| workspaces.get(key)) else {
        return Ok(Vec::new());
    };
    let Some(entry) = entry.as_object() else {
        return Err("the settings file's workspace entry is not an object".to_string());
    };
    saved_from_entry(entry)
}

/// The saved directories one workspace entry holds, held to upstream's rule
/// for them (`settings_store.zig:1996-2009`): a list of at most
/// [`MAX_ADDITIONAL_DIRECTORIES`] absolute paths, none twice.
pub fn saved_from_entry(entry: &Map<String, Value>) -> Result<Vec<String>, String> {
    let Some(value) = entry.get(ADDITIONAL_DIRECTORIES_KEY) else {
        return Ok(Vec::new());
    };
    let rule = || {
        format!(
            "`{ADDITIONAL_DIRECTORIES_KEY}` must be a list of at most \
             {MAX_ADDITIONAL_DIRECTORIES} unique absolute directory paths"
        )
    };
    let Some(items) = value.as_array() else {
        return Err(rule());
    };
    if items.len() > MAX_ADDITIONAL_DIRECTORIES {
        return Err(rule());
    }
    let mut paths: Vec<String> = Vec::with_capacity(items.len());
    for item in items {
        let Some(path) = item.as_str() else {
            return Err(rule());
        };
        if valid_stored(path).is_err() || paths.iter().any(|seen| seen == path) {
            return Err(rule());
        }
        paths.push(path.to_string());
    }
    Ok(paths)
}

// ---------------------------------------------------------------------------
// resolution
// ---------------------------------------------------------------------------

/// A path the user typed, held to what any path must be: not empty, not
/// longer than a path can be, and with no NUL in it.
fn valid_input(input: &str) -> Result<(), Refusal> {
    if input.is_empty() || input.len() > MAX_PATH_BYTES || input.contains('\0') {
        return Err(Refusal::InvalidPath);
    }
    Ok(())
}

/// A path the profile holds: as above, and absolute.
fn valid_stored(path: &str) -> Result<(), Refusal> {
    valid_input(path)?;
    if !Path::new(path).is_absolute() {
        return Err(Refusal::InvalidPath);
    }
    Ok(())
}

/// `input` as an absolute path, its `.` and `..` folded **by the letters**,
/// against `primary` when it is relative (`std.fs.path.resolve`, which
/// upstream's `resolveAbsoluteInput` is, `workspace_access.zig:510-522`).
fn lexical_absolute(primary: &Path, input: &str) -> Result<PathBuf, Refusal> {
    valid_input(input)?;
    let joined = if Path::new(input).is_absolute() {
        PathBuf::from(input)
    } else {
        primary.join(input)
    };
    let mut out = PathBuf::new();
    for component in joined.components() {
        match component {
            Component::Prefix(prefix) => out.push(prefix.as_os_str()),
            Component::RootDir => out.push(Component::RootDir.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(name) => out.push(name),
        }
    }
    Ok(out)
}

/// The real path of the directory `input` names (`canonicalExistingDirectory`,
/// `workspace_access.zig:566-604`).
///
/// A relative path is anchored at the workspace and folded by the letters
/// first, as upstream's is; then the whole of it is resolved by the
/// filesystem, links and all, so what is saved is the directory reached and
/// not the name it was reached by. The workspace itself is refused: it is not
/// an *additional* directory, and adding it would grant nothing.
fn canonical_existing_directory(primary: &Path, input: &str) -> Result<PathBuf, Refusal> {
    valid_input(input)?;
    let absolute = if Path::new(input).is_absolute() {
        PathBuf::from(input)
    } else {
        lexical_absolute(primary, input)?
    };
    let canonical = fs::canonicalize(&absolute).map_err(|err| resolution_refusal(&err))?;
    let metadata = fs::symlink_metadata(&canonical).map_err(|err| match err.kind() {
        io::ErrorKind::NotFound => Refusal::PathNotFound,
        _ => Refusal::InvalidPath,
    })?;
    if !metadata.is_dir() {
        return Err(Refusal::NotDirectory);
    }
    if canonical == primary {
        return Err(Refusal::PrimaryDirectory);
    }
    Ok(canonical)
}

/// What a failed resolution means to the user (`workspace_access.zig:584-595`):
/// nothing reachable there -- absent, forbidden, a loop, a name too long, an
/// I/O error -- is "does not exist"; a file where a directory was needed on
/// the way is "not a directory"; anything else is "invalid".
fn resolution_refusal(err: &io::Error) -> Refusal {
    match err.kind() {
        io::ErrorKind::NotFound | io::ErrorKind::PermissionDenied => Refusal::PathNotFound,
        io::ErrorKind::NotADirectory => Refusal::NotDirectory,
        _ => {
            #[cfg(unix)]
            if let Some(code) = err.raw_os_error() {
                if [libc::ELOOP, libc::ENAMETOOLONG, libc::EIO].contains(&code) {
                    return Refusal::PathNotFound;
                }
            }
            Refusal::InvalidPath
        }
    }
}

/// A saved spelling, resolved (`resolveSavedDirectory`,
/// `workspace_access.zig:534-561`): the real directory and `true` when there
/// is one; otherwise the path it would have, through the nearest directory on
/// the way that does exist, and `false`.
fn resolve_saved(primary: &Path, source: &str) -> Result<(PathBuf, bool), Refusal> {
    valid_stored(source)?;
    match canonical_existing_directory(primary, source) {
        Ok(path) => Ok((path, true)),
        Err(Refusal::PathNotFound | Refusal::NotDirectory) => {
            let normalized = lexical_absolute(primary, source)?;
            if normalized == primary {
                return Err(Refusal::PrimaryDirectory);
            }
            Ok((through_nearest(&normalized)?, false))
        }
        Err(refusal) => Err(refusal),
    }
}

/// A saved spelling, **observed** for a launch: the directory and `true` while
/// its real path is the spelling itself (folded by its letters); otherwise
/// the spelling as written and `false` -- or, when nothing is there,
/// [`resolve_saved`]'s answer for a missing one.
///
/// The difference from [`resolve_saved`] is the one that matters for
/// authority: a saved path that leads somewhere else now does not take the
/// session there. `remove` and the add's duplicate check still resolve, so a
/// link to a saved directory still names it.
fn observe_saved(primary: &Path, source: &str) -> Result<(PathBuf, bool), Refusal> {
    valid_stored(source)?;
    let written = lexical_absolute(primary, source)?;
    if written == primary {
        return Err(Refusal::PrimaryDirectory);
    }
    match canonical_existing_directory(primary, source) {
        Ok(real) if real == written => Ok((real, true)),
        Ok(_) | Err(Refusal::PrimaryDirectory) => Ok((written, false)),
        Err(_) => resolve_saved(primary, source),
    }
}

/// `path` with its longest existing head replaced by that head's real path.
fn through_nearest(path: &Path) -> Result<PathBuf, Refusal> {
    let mut head = path.to_path_buf();
    let mut tail: Vec<std::ffi::OsString> = Vec::new();
    loop {
        if let Ok(real) = fs::canonicalize(&head) {
            let mut out = real;
            for name in tail.iter().rev() {
                out.push(name);
            }
            return Ok(out);
        }
        let Some(name) = head.file_name().map(|name| name.to_os_string()) else {
            return Err(Refusal::InvalidPath);
        };
        tail.push(name);
        if !head.pop() {
            return Err(Refusal::InvalidPath);
        }
    }
}

/// Which saved directory `input` means, for `remove`
/// (`removalIdentityAlloc`, `workspace_access.zig:443-469`): one whose path
/// is what `input` spells once folded, or else the directory `input` resolves
/// to. Upstream's second step reads `input` as a *saved* spelling, so a
/// relative path that is not a saved one by its letters is "invalid" there,
/// and here.
fn removal_identity(
    current: &WorkspaceAccess,
    primary: &Path,
    input: &str,
) -> Result<PathBuf, Refusal> {
    let folded = lexical_absolute(primary, input)?;
    if let Some(entry) = current.entries.iter().find(|entry| entry.path == folded) {
        return Ok(entry.path.clone());
    }
    let (resolved, _) = resolve_saved(primary, input)?;
    current
        .entries
        .iter()
        .find(|entry| entry.path == resolved)
        .map(|entry| entry.path.clone())
        .ok_or(Refusal::UnknownAdditionalDirectory)
}

// ---------------------------------------------------------------------------
// what it says
// ---------------------------------------------------------------------------

/// `text` with every character a terminal would obey written as its code
/// point instead (`\u{001B}`), as upstream encodes a path before it is shown
/// (`encodeTerminalSafe`, `text_utils.zig:343-385`). A directory's name is
/// the filesystem's, not this product's.
pub fn terminal_safe(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        if character.is_control() {
            out.push_str(&crate::permission::scalar_token(character));
        } else {
            out.push(character);
        }
    }
    out
}

/// The snapshot `/workspace list` and every change answer with
/// (`renderInteractiveBody`, `vercel-labs/fx@c1db9191
/// src/core/output/output_contracts.zig:285-322`), one fact per row:
///
/// ```text
/// primary=/w
/// saved_suppressed=false limit=16
/// add ../extra saved_changed=true runtime_changed=true launch_flag_can_restore=false
/// additional directories:
///  - /extra saved=true command_line=false available=true active=true
/// ```
pub fn snapshot(primary: &Path, access: &WorkspaceAccess, mutation: Option<&Mutation>) -> String {
    let mut out = format!(
        "primary={}\nsaved_suppressed=false limit={MAX_ADDITIONAL_DIRECTORIES}\n",
        terminal_safe(&primary.to_string_lossy())
    );
    if let Some(mutation) = mutation {
        out.push_str(mutation.action);
        if let Some(typed) = &mutation.typed {
            out.push(' ');
            out.push_str(&terminal_safe(typed));
        }
        out.push_str(&format!(
            " saved_changed={} runtime_changed={} launch_flag_can_restore=false\n",
            mutation.saved_changed, mutation.runtime_changed
        ));
    }
    if access.entries.is_empty() {
        out.push_str("additional directories: (none)");
        return out;
    }
    out.push_str("additional directories:");
    for entry in &access.entries {
        out.push_str(&format!(
            "\n - {} saved=true command_line=false available={} active={}",
            terminal_safe(&entry.path.to_string_lossy()),
            entry.available,
            entry.available
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A primary workspace and a sibling directory beside it, both real
    /// paths, with the profile's settings file in a home of its own.
    struct Tree {
        _dir: tempfile::TempDir,
        base: PathBuf,
        primary: PathBuf,
        settings: PathBuf,
    }

    impl Tree {
        fn new() -> Self {
            let dir = tempfile::tempdir().expect("a tree");
            let base = dir.path().canonicalize().expect("canonical");
            let primary = base.join("work");
            fs::create_dir(&primary).expect("work");
            fs::create_dir(base.join("home")).expect("home");
            let settings = base.join("home/.xfx/settings.json");
            Self {
                _dir: dir,
                base,
                primary,
                settings,
            }
        }

        fn dir(&self, name: &str) -> PathBuf {
            let path = self.base.join(name);
            fs::create_dir_all(&path).expect("dir");
            path
        }

        fn run(&self, current: &WorkspaceAccess, line: &str) -> Result<Updated, Failure> {
            let action = parse(line).expect("a change").expect("not a snapshot");
            execute(&self.primary, Some(&self.settings), current, &action)
        }

        fn document(&self) -> Value {
            serde_json::from_str(&fs::read_to_string(&self.settings).expect("settings"))
                .expect("json")
        }

        fn saved(&self) -> Vec<String> {
            saved_in(
                self.document().as_object().expect("an object"),
                &self.primary,
            )
            .expect("readable")
        }
    }

    fn path(text: &Path) -> String {
        text.to_string_lossy().into_owned()
    }

    #[test]
    fn the_command_reads_upstreams_four_forms_and_refuses_the_rest() {
        assert_eq!(parse(""), Ok(None));
        assert_eq!(parse("list"), Ok(None));
        assert_eq!(parse("  list \t"), Ok(None));
        assert_eq!(parse("clear"), Ok(Some(Action::Clear)));
        assert_eq!(parse("add ../x"), Ok(Some(Action::Add("../x".into()))));
        assert_eq!(parse("add\t /a b \t"), Ok(Some(Action::Add("/a b".into()))));
        assert_eq!(parse("remove /x"), Ok(Some(Action::Remove("/x".into()))));
        for refused in [
            "add", "add  ", "remove", "bogus", "List", "CLEAR", "addx /a", "list x", "clear x",
        ] {
            assert_eq!(parse(refused), Err(()), "{refused:?}");
        }
        // Upstream's usage line leaves `list` out although it takes it.
        assert_eq!(USAGE, "usage: /workspace [add PATH|remove PATH|clear]");
    }

    #[test]
    fn an_added_directory_is_saved_as_the_real_path_it_names_under_the_workspace_key() {
        let tree = Tree::new();
        let extra = tree.dir("extra");
        let updated = tree
            .run(&WorkspaceAccess::default(), "add ../extra")
            .expect("added");
        assert_eq!(
            updated.access.entries(),
            [Entry {
                path: extra.clone(),
                available: true
            }]
        );
        assert!(updated.mutation.saved_changed && updated.mutation.runtime_changed);
        assert_eq!(
            tree.document(),
            serde_json::json!({"workspaces": {path(&tree.primary): {"additional_directories": [path(&extra)]}}})
        );
        // The change line echoes the path as typed; the list names the
        // directory it resolved to.
        assert_eq!(
            snapshot(&tree.primary, &updated.access, Some(&updated.mutation)),
            format!(
                "primary={}\nsaved_suppressed=false limit=16\n\
                 add ../extra saved_changed=true runtime_changed=true launch_flag_can_restore=false\n\
                 additional directories:\n - {} saved=true command_line=false available=true active=true",
                tree.primary.display(),
                extra.display()
            )
        );
    }

    #[test]
    fn the_empty_snapshot_is_upstreams() {
        let tree = Tree::new();
        assert_eq!(
            snapshot(&tree.primary, &WorkspaceAccess::default(), None),
            format!(
                "primary={}\nsaved_suppressed=false limit=16\nadditional directories: (none)",
                tree.primary.display()
            )
        );
    }

    #[test]
    fn adding_what_is_already_saved_changes_nothing_and_says_so() {
        let tree = Tree::new();
        let extra = tree.dir("extra");
        let first = tree
            .run(
                &WorkspaceAccess::default(),
                &format!("add {}", extra.display()),
            )
            .expect("added");
        let before = fs::read(&tree.settings).expect("settings");
        let again = tree.run(&first.access, "add ../extra").expect("re-added");
        assert!(!again.mutation.saved_changed);
        assert!(!again.mutation.runtime_changed);
        assert_eq!(again.access, first.access);
        assert_eq!(fs::read(&tree.settings).expect("settings"), before);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_adds_the_directory_it_points_at_and_never_the_link() {
        let tree = Tree::new();
        let target = tree.dir("target");
        std::os::unix::fs::symlink(&target, tree.base.join("link")).expect("link");
        let updated = tree
            .run(&WorkspaceAccess::default(), "add ../link")
            .expect("added");
        assert_eq!(updated.access.saved(), [path(&target)]);
        // The link is then pointed somewhere else: what was authorized is the
        // directory, so the link's new target gains nothing.
        let elsewhere = tree.dir("elsewhere");
        fs::remove_file(tree.base.join("link")).expect("unlink");
        std::os::unix::fs::symlink(&elsewhere, tree.base.join("link")).expect("relink");
        let scope = updated.access.scope(&tree.primary).expect("scope");
        assert_eq!(scope.additional_roots(), std::slice::from_ref(&target));
        // And the saved directory itself swapped for a link to elsewhere is
        // unavailable, not elsewhere.
        fs::remove_dir(&target).expect("remove");
        std::os::unix::fs::symlink(&elsewhere, &target).expect("swap");
        let refreshed = updated.access.refreshed();
        assert_eq!(
            refreshed.entries(),
            [Entry {
                path: target.clone(),
                available: false
            }]
        );
        assert!(refreshed
            .scope(&tree.primary)
            .expect("scope")
            .additional_roots()
            .is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn a_saved_directory_swapped_for_a_link_is_unavailable_at_the_next_launch_too() {
        // What a launch reads is what was saved -- a real path -- and it is
        // observed, not followed: the link's target is not in the scope.
        let tree = Tree::new();
        let extra = tree.dir("extra");
        let elsewhere = tree.dir("elsewhere");
        tree.run(&WorkspaceAccess::default(), "add ../extra")
            .expect("added");
        fs::remove_dir(&extra).expect("remove");
        std::os::unix::fs::symlink(&elsewhere, &extra).expect("swap");

        let loaded = WorkspaceAccess::load(&tree.primary, &tree.saved());
        assert_eq!(
            loaded.entries(),
            [Entry {
                path: extra.clone(),
                available: false
            }]
        );
        assert!(loaded
            .scope(&tree.primary)
            .expect("scope")
            .additional_roots()
            .is_empty());
        // And it can still be forgotten by the name it was saved under.
        let removed = tree
            .run(&loaded, &format!("remove {}", extra.display()))
            .expect("removed");
        assert!(removed.access.entries().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn a_saved_spelling_that_is_not_a_real_path_is_unavailable_rather_than_followed() {
        // xfx saves real paths only; a hand-written spelling that resolves
        // somewhere else -- through a link on the way -- is listed as written
        // and grants nothing. Upstream would follow it at launch
        // (`resolveSavedDirectory`, `workspace_access.zig:534-561`); this is
        // the narrower answer.
        let tree = Tree::new();
        let target = tree.dir("target");
        std::os::unix::fs::symlink(&target, tree.base.join("link")).expect("link");
        let spelled = path(&tree.base.join("link"));
        let loaded = WorkspaceAccess::load(&tree.primary, std::slice::from_ref(&spelled));
        assert_eq!(
            loaded.entries(),
            [Entry {
                path: tree.base.join("link"),
                available: false
            }]
        );
        assert_eq!(loaded.active_roots().count(), 0);
    }

    #[cfg(unix)]
    #[test]
    fn a_hand_written_spelling_is_rewritten_to_the_real_path_on_add() {
        // A spelling a launch lists `Unavailable` because it is not a real
        // path is made one by adding the directory it names: the entry is
        // rewritten to the real path, as upstream rewrites a matched entry to
        // its identity (`settings_store.zig:1352-1357`), so it is in force.
        let tree = Tree::new();
        let target = tree.dir("target");
        std::os::unix::fs::symlink(&target, tree.base.join("link")).expect("link");
        crate::profile_home::made(tree.settings.parent().unwrap());
        fs::write(
            &tree.settings,
            serde_json::json!({"workspaces": {path(&tree.primary): {"additional_directories": [path(&tree.base.join("link"))]}}})
                .to_string(),
        )
        .expect("hand-written");
        let loaded = WorkspaceAccess::load(&tree.primary, &tree.saved());
        assert_eq!(loaded.active_roots().count(), 0);

        let updated = tree.run(&loaded, "add ../link").expect("added");
        assert!(updated.mutation.saved_changed, "{:?}", updated.mutation);
        assert!(updated.mutation.runtime_changed, "{:?}", updated.mutation);
        assert_eq!(tree.saved(), [path(&target)]);
        assert_eq!(
            updated.access.entries(),
            [Entry {
                path: target,
                available: true
            }]
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_change_never_rewrites_a_saved_directory_it_was_not_asked_about() {
        // A saved directory swapped for a link is unavailable; adding another
        // one must not "repair" it into the link's target.
        let tree = Tree::new();
        let extra = tree.dir("extra");
        let elsewhere = tree.dir("elsewhere");
        let other = tree.dir("other");
        let access = tree
            .run(&WorkspaceAccess::default(), "add ../extra")
            .expect("added")
            .access;
        fs::remove_dir(&extra).expect("remove");
        std::os::unix::fs::symlink(&elsewhere, &extra).expect("swap");
        let loaded = WorkspaceAccess::load(&tree.primary, &tree.saved());
        let updated = tree.run(&loaded, "add ../other").expect("added");
        assert_eq!(tree.saved(), [path(&extra), path(&other)]);
        assert_eq!(
            updated.access.active_roots().collect::<Vec<_>>(),
            [other.as_path()]
        );
        let _ = access;
    }

    #[test]
    fn dot_dot_is_folded_by_the_letters_and_the_result_resolved() {
        let tree = Tree::new();
        let extra = tree.dir("extra");
        tree.dir("work/sub");
        let updated = tree
            .run(&WorkspaceAccess::default(), "add sub/../../extra/.")
            .expect("added");
        assert_eq!(updated.access.saved(), [path(&extra)]);
        // `..` alone is the workspace's parent, a directory like any other:
        // upstream admits it, and so does this. It contains the workspace,
        // which stays the root a path inside it is resolved against.
        let parent = tree.run(&updated.access, "add ..").expect("parent");
        assert_eq!(parent.access.saved(), [path(&extra), path(&tree.base)]);
        let scope = parent.access.scope(&tree.primary).expect("scope");
        let inside = scope
            .resolve_existing(&tree.primary.join("sub").to_string_lossy())
            .expect("inside");
        assert_eq!(inside.root(), tree.primary);
    }

    #[test]
    fn home_and_the_filesystem_root_are_admitted_as_upstream_admits_them() {
        // Upstream refuses neither (`canonicalExistingDirectory` has no such
        // rule, nor does `--add-dir`'s parser, `cli_surface.zig:432-438`), and
        // neither does this. What they grant is bounded the same way any root
        // is: `.git`/`.xfx` refused in every mode, and auto refusing the files
        // that decide what runs later -- `tests/permissions.rs`
        // `auto_mode_will_not_rewrite_a_login_file_in_an_additional_root`.
        let tree = Tree::new();
        let home = tree.base.join("home");
        let updated = tree
            .run(
                &WorkspaceAccess::default(),
                &format!("add {}", home.display()),
            )
            .expect("home");
        let updated = tree.run(&updated.access, "add /").expect("root");
        assert_eq!(updated.access.saved(), [path(&home), "/".to_string()]);
        // `~` is a name, not the home directory: relative, so under the
        // workspace, where there is no such directory.
        let refusal = tree.run(&updated.access, "add ~").expect_err("tilde");
        assert_eq!(
            refusal.message(),
            "Workspace update rejected: directory does not exist"
        );
    }

    #[test]
    fn what_cannot_be_an_added_directory_is_refused_in_upstreams_words() {
        let tree = Tree::new();
        fs::write(tree.base.join("file.txt"), "x").expect("file");
        let none = WorkspaceAccess::default();
        for (line, message) in [
            (
                "add ../missing",
                "Workspace update rejected: directory does not exist",
            ),
            (
                "add ../file.txt",
                "Workspace update rejected: path is not a directory",
            ),
            (
                "add ../file.txt/below",
                "Workspace update rejected: path is not a directory",
            ),
            (
                "add .",
                "Workspace update rejected: the primary workspace cannot be added or removed",
            ),
            (
                "add ../work",
                "Workspace update rejected: the primary workspace cannot be added or removed",
            ),
            (
                "remove /nowhere/at/all",
                "Workspace update rejected: directory is not configured as an additional workspace",
            ),
            // Upstream reads a remove's path as a saved spelling once it is
            // not one by its letters, and a saved spelling is absolute.
            (
                "remove ../elsewhere",
                "Workspace update rejected: path is invalid",
            ),
            (
                "remove ../work",
                "Workspace update rejected: path is invalid",
            ),
        ] {
            let refusal = tree.run(&none, line).expect_err(line);
            assert_eq!(refusal.message(), message, "{line}");
        }
        let refusal = tree
            .run(&none, &format!("remove {}", tree.primary.display()))
            .expect_err("the workspace");
        assert_eq!(
            refusal.message(),
            "Workspace update rejected: the primary workspace cannot be added or removed"
        );
        let nul = execute(
            &tree.primary,
            Some(&tree.settings),
            &none,
            &Action::Add("a\0b".into()),
        )
        .expect_err("nul");
        assert_eq!(nul.refusal, Refusal::InvalidPath);
        assert!(!tree.settings.exists(), "a refusal wrote the profile");
    }

    #[test]
    fn a_directory_inside_the_workspace_is_saved_and_grants_nothing_more() {
        // Upstream admits it -- only the workspace itself is refused -- and the
        // scope has no use for it, so it is listed and the roots are unchanged.
        let tree = Tree::new();
        tree.dir("work/inner");
        let updated = tree
            .run(&WorkspaceAccess::default(), "add inner")
            .expect("added");
        assert_eq!(updated.access.entries().len(), 1);
        assert!(updated
            .access
            .scope(&tree.primary)
            .expect("scope")
            .additional_roots()
            .is_empty());
    }

    #[test]
    fn the_seventeenth_directory_is_refused_and_the_profile_keeps_sixteen() {
        let tree = Tree::new();
        let mut access = WorkspaceAccess::default();
        for index in 0..MAX_ADDITIONAL_DIRECTORIES {
            tree.dir(&format!("d{index}"));
            access = tree
                .run(&access, &format!("add ../d{index}"))
                .expect("under the cap")
                .access;
        }
        tree.dir("d16");
        let refusal = tree.run(&access, "add ../d16").expect_err("over the cap");
        assert_eq!(
            refusal.message(),
            "Workspace settings were not changed: additional directory limit reached"
        );
        assert_eq!(tree.saved().len(), MAX_ADDITIONAL_DIRECTORIES);
    }

    #[test]
    fn remove_and_clear_forget_and_leave_no_empty_object_behind() {
        let tree = Tree::new();
        let a = tree.dir("a");
        let b = tree.dir("b");
        // Another key in the same file survives every change.
        crate::profile_home::made(tree.settings.parent().unwrap());
        fs::write(&tree.settings, "{\"model\":\"m\"}\n").expect("seed");
        let access = tree
            .run(&WorkspaceAccess::default(), "add ../a")
            .expect("a")
            .access;
        let access = tree.run(&access, "add ../b").expect("b").access;

        let removed = tree.run(&access, "remove ../a").expect("removed");
        assert_eq!(removed.access.saved(), [path(&b)]);
        assert!(removed.mutation.saved_changed && removed.mutation.runtime_changed);
        assert!(
            snapshot(&tree.primary, &removed.access, Some(&removed.mutation))
                .contains("\nremove ../a saved_changed=true runtime_changed=true")
        );

        let cleared = tree.run(&removed.access, "clear").expect("cleared");
        assert!(cleared.access.entries().is_empty());
        assert_eq!(tree.document(), serde_json::json!({"model": "m"}));
        assert!(
            snapshot(&tree.primary, &cleared.access, Some(&cleared.mutation)).ends_with(
                "clear saved_changed=true runtime_changed=true launch_flag_can_restore=false\n\
                 additional directories: (none)"
            )
        );
        let again = tree.run(&cleared.access, "clear").expect("cleared twice");
        assert!(!again.mutation.saved_changed && !again.mutation.runtime_changed);
        let _ = a;
    }

    #[test]
    fn removing_the_last_directory_keeps_every_other_key_in_its_place() {
        // The profile's keys keep document order (`preserve_order`), and a
        // removal must not move the last key into the hole it leaves: the
        // `permission` table's rows are ordered and the last match wins.
        let tree = Tree::new();
        tree.dir("extra");
        let key = path(&tree.primary);
        crate::profile_home::made(tree.settings.parent().unwrap());
        fs::write(
            &tree.settings,
            format!(
                "{{\"model\":\"m\",\"workspaces\":{{\"/other\":{{\"a\":1}},\"{key}\":{{\"x\":1,\"additional_directories\":[],\"y\":2,\"z\":3}},\"/later\":{{\"b\":2}}}},\"permission\":{{}},\"zz\":true}}"
            ),
        )
        .expect("seed");
        let access = tree
            .run(&WorkspaceAccess::default(), "add ../extra")
            .expect("added")
            .access;
        tree.run(&access, "clear").expect("cleared");
        let document = tree.document();
        let top: Vec<&String> = document.as_object().unwrap().keys().collect();
        assert_eq!(top, ["model", "workspaces", "permission", "zz"]);
        let workspaces: Vec<&String> = document["workspaces"].as_object().unwrap().keys().collect();
        assert_eq!(workspaces, ["/other", key.as_str(), "/later"]);
        let entry: Vec<&String> = document["workspaces"][&key]
            .as_object()
            .unwrap()
            .keys()
            .collect();
        assert_eq!(entry, ["x", "y", "z"]);

        // And an emptied entry leaves without reordering its neighbours.
        fs::write(
            &tree.settings,
            format!(
                "{{\"workspaces\":{{\"/other\":{{}},\"{key}\":{{}},\"/later\":{{}},\"/last\":{{}}}},\"zz\":true}}"
            ),
        )
        .expect("seed");
        let access = tree
            .run(&WorkspaceAccess::default(), "add ../extra")
            .expect("added")
            .access;
        tree.run(&access, "clear").expect("cleared");
        let document = tree.document();
        let workspaces: Vec<&String> = document["workspaces"].as_object().unwrap().keys().collect();
        assert_eq!(workspaces, ["/other", "/later", "/last"]);
    }

    #[test]
    fn a_saved_directory_that_is_gone_is_listed_unavailable_and_can_still_be_removed() {
        let tree = Tree::new();
        let gone = tree.dir("gone");
        let access = tree
            .run(&WorkspaceAccess::default(), "add ../gone")
            .expect("added")
            .access;
        fs::remove_dir(&gone).expect("remove");
        let loaded = WorkspaceAccess::load(&tree.primary, &tree.saved());
        assert_eq!(
            loaded.entries(),
            [Entry {
                path: gone.clone(),
                available: false
            }]
        );
        assert_eq!(loaded.entries()[0].status(), "Unavailable \u{b7} Saved");
        assert!(snapshot(&tree.primary, &loaded, None)
            .ends_with("saved=true command_line=false available=false active=false"));
        assert_eq!(loaded.active_roots().count(), 0);
        // A scope is still made, of the workspace alone.
        assert!(loaded
            .scope(&tree.primary)
            .expect("scope")
            .additional_roots()
            .is_empty());
        let removed = tree
            .run(&access, &format!("remove {}", gone.display()))
            .expect("removed");
        assert!(removed.access.entries().is_empty());
    }

    #[test]
    fn a_change_adopts_what_another_process_saved_meanwhile() {
        let tree = Tree::new();
        let mine = tree.dir("mine");
        let theirs = tree.dir("theirs");
        crate::profile_home::made(tree.settings.parent().unwrap());
        fs::write(
            &tree.settings,
            serde_json::json!({"workspaces": {path(&tree.primary): {"additional_directories": [path(&theirs)]}}})
                .to_string(),
        )
        .expect("theirs");
        let updated = tree
            .run(&WorkspaceAccess::default(), "add ../mine")
            .expect("added");
        assert_eq!(updated.access.saved(), [path(&theirs), path(&mine)]);
    }

    #[test]
    fn no_home_is_a_commit_refusal_and_an_unreadable_profile_another() {
        let tree = Tree::new();
        tree.dir("extra");
        let action = Action::Add("../extra".into());
        let refusal =
            execute(&tree.primary, None, &WorkspaceAccess::default(), &action).expect_err("home");
        assert_eq!(
            refusal.message(),
            "Workspace settings were not changed: HOME is not set"
        );
        fs::create_dir_all(tree.settings.parent().unwrap()).expect("profile");
        fs::write(&tree.settings, "not json").expect("broken");
        let refusal = execute(
            &tree.primary,
            Some(&tree.settings),
            &WorkspaceAccess::default(),
            &action,
        )
        .expect_err("broken");
        assert_eq!(refusal.phase, Phase::Commit);
        assert_eq!(fs::read_to_string(&tree.settings).unwrap(), "not json");
    }

    #[test]
    fn the_profile_list_is_held_to_upstreams_rule() {
        let entry = |value: Value| {
            let mut map = Map::new();
            map.insert(ADDITIONAL_DIRECTORIES_KEY.to_string(), value);
            saved_from_entry(&map)
        };
        assert_eq!(
            entry(serde_json::json!(["/a", "/b"])),
            Ok(vec!["/a".into(), "/b".into()])
        );
        for bad in [
            serde_json::json!("/a"),
            serde_json::json!(["relative"]),
            serde_json::json!(["/a", "/a"]),
            serde_json::json!([1]),
            serde_json::json!((0..17).map(|n| format!("/d{n}")).collect::<Vec<_>>()),
        ] {
            assert!(entry(bad.clone()).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_path_is_shown_with_its_control_characters_spelled_out() {
        assert_eq!(terminal_safe("/a\u{1b}[2Jb"), "/a\\u{001B}[2Jb");
        assert_eq!(terminal_safe("/plain/path"), "/plain/path");
    }
}
