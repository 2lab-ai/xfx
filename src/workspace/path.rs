//! Canonical paths and the roots a turn is allowed to touch.
//!
//! Every filesystem access a tool makes goes through [`AccessScope`], and the
//! scope answers exactly one question: *is this real path inside a root the
//! user authorized?* Two properties make that answer trustworthy:
//!
//! - **The scope is canonical.** Roots are resolved with
//!   [`std::fs::canonicalize`] when the scope is built, and every request is
//!   canonicalized before it is compared. Comparing unresolved strings would let
//!   `a/../../b`, a `/var` -> `/private/var` alias, or a symlink name a path the
//!   prefix test would then approve.
//! - **Containment is decided before the file is opened.** `canonicalize` walks
//!   the symlink chain with `realpath(3)`; it does not read the target. So a
//!   symlink that points outside the scope is refused while its contents are
//!   still on disk and unread, rather than after they are in memory
//!   (`vercel-labs/fx@580a0c5d src/core/workspace/workspace_access.zig:79-95`).
//!
//! A scope is `primary + explicitly configured additional roots`. There is no
//! implicit root: not the home directory, not the parent of the workspace, not
//! `/tmp`. Upstream draws the same line
//! (`workspace_access.zig:53-77`).
//!
//! # What the proof does not cover
//!
//! [`ResolvedPath`] is a proof about the filesystem *as it was when
//! `canonicalize` returned*, not a handle to the object it named. Resolution and
//! the subsequent `open`/`read_dir` are two separate syscall sequences over a
//! mutable namespace, so between them a component of the path can be replaced --
//! classically, a directory swapped for a symlink pointing outside the scope.
//! An attacker who can write inside an authorized root at the same moment a tool
//! call is running could therefore win that race and have the read follow the
//! new target. Every check here is correct; the assumption it rests on is **no
//! hostile concurrent namespace mutation between resolution and I/O**.
//!
//! That assumption is reasonable for v0.1, whose threat model is a mistaken or
//! manipulated *model* rather than a hostile local process: a model can only ask
//! for a path, and it cannot create the symlink it would need to win the race.
//! It is not a defence against a local attacker.
//!
//! Closing it durably needs the path proof and the I/O to be the same operation
//! rather than two: resolve component by component from a directory descriptor
//! with `openat(..., O_NOFOLLOW)` so no component can be substituted after it is
//! checked, and/or revalidate the identity (`st_dev`/`st_ino`) of the opened
//! file against the identity the proof recorded before any bytes are used. Both
//! are real work with real platform differences, and neither belongs in the same
//! change as the first read tools; they are recorded as a residual in the Task 3
//! report rather than half-built here. The mutation slice, which opens files for
//! writing and already needs identity revalidation for its own staged-rename
//! race, is where this should land.

use std::fmt;
use std::path::{Path, PathBuf};

/// Directory names that are never listed, walked, or searched.
///
/// Upstream's set, verbatim
/// (`vercel-labs/fx@580a0c5d src/core/tooling/tool_dispatch.zig:36-45`). It is a
/// backstop rather than the whole rule: the walkers also apply `.gitignore`, so
/// a Rust `target/` or a Python `__pycache__/` is excluded by the project's own
/// declaration rather than by a name xfx had to guess.
pub const IGNORED_DIRECTORY_NAMES: &[&str] = &[
    ".git",
    ".zig-cache",
    "zig-out",
    "node_modules",
    ".next",
    "dist",
    "build",
    "coverage",
];

/// Whether `name` is one of the always-ignored directory names.
pub fn is_ignored_directory(name: &str) -> bool {
    IGNORED_DIRECTORY_NAMES.contains(&name)
}

/// Directory names the typed file tools never write into, at any depth.
///
/// These are not "files that matter"; they are the places where *the authority
/// itself* is configured. `.git/config` is executable input to git -- a
/// `core.fsmonitor`, a `diff.external`, or a `diff.<driver>.textconv` entry is a
/// command that `git status` or `git diff` will run through a shell -- so a mode
/// that admits a bounded workspace write would otherwise admit, one call later,
/// an arbitrary command on the direct read-only route. `.xfx` is xfx's own
/// profile home, which holds the session log an approval is recorded in.
///
/// The set is explicit rather than "every dot directory" on purpose: `.github`,
/// `.cargo`, and `.vscode` are ordinary project content that a coding agent is
/// expected to edit, and refusing them would make the tool useless to buy a
/// guarantee that is not about them.
///
/// The comparison is on the components *below an authorized root*. A user who
/// points xfx at a directory that is itself inside a `.git` has said that this
/// is the workspace, and the invocation already rests on that decision.
pub const PROTECTED_WRITE_DIRECTORY_NAMES: &[&str] = &[".git", ".xfx"];

/// Whether `name` is a directory the file tools refuse to write into or through.
///
/// Case-insensitive, and that is not politeness. macOS ships a case-insensitive
/// APFS volume by default, so `.GIT/hooks/pre-commit` names the same directory
/// `openat` reaches for `.git/hooks/pre-commit`; an exact comparison would be a
/// check the caller could spell its way around on one of the two platforms xfx
/// supports. The cost on a case-sensitive filesystem is that a directory
/// genuinely called `.GIT` is refused as well, which has no plausible legitimate
/// instance and is the safe direction to be wrong in.
///
/// ASCII folding only: both names are ASCII, and Unicode case folding would put
/// locale-dependent behaviour inside a security check for nothing.
pub fn is_protected_write_directory(name: &str) -> bool {
    PROTECTED_WRITE_DIRECTORY_NAMES
        .iter()
        .any(|protected| protected.eq_ignore_ascii_case(name))
}

/// Why a path could not be used.
///
/// The `Display` text reaches the model as a tool result, so each variant says
/// what the model should do differently. It names the path the model *asked
/// for*, never the path it resolved to: reporting the resolved target of a
/// refused symlink would hand back a fact about the filesystem outside the
/// scope, which is the thing the refusal exists to withhold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathError {
    /// The path was empty or only whitespace.
    Empty,
    /// Nothing exists at the requested path.
    NotFound { requested: String },
    /// The path exists but resolves outside every authorized root.
    OutsideScope { requested: String },
    /// The path could not be resolved for a reason other than absence.
    Unresolvable { requested: String, detail: String },
    /// A configured root is not a usable directory.
    RootUnavailable { path: String, detail: String },
}

impl fmt::Display for PathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "the path must not be empty"),
            Self::NotFound { requested } => write!(f, "no such path: `{requested}`"),
            Self::OutsideScope { requested } => write!(
                f,
                "`{requested}` resolves outside the authorized workspace roots"
            ),
            Self::Unresolvable { requested, detail } => {
                write!(f, "cannot resolve `{requested}`: {detail}")
            }
            Self::RootUnavailable { path, detail } => {
                write!(f, "`{path}` is not a usable directory: {detail}")
            }
        }
    }
}

impl std::error::Error for PathError {}

/// A real path proven to sit inside a named root.
///
/// It can only be produced by [`AccessScope::resolve_existing`], so a value of
/// this type is itself the proof; a caller cannot construct one for a path the
/// scope never approved.
///
/// The proof is about the moment it was made. It is not a file handle, and it
/// does not survive a hostile concurrent rename or symlink swap between
/// resolution and the read that follows -- see the module docs, "What the proof
/// does not cover".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPath {
    absolute: PathBuf,
    root: PathBuf,
}

impl ResolvedPath {
    /// The canonical target.
    pub fn absolute(&self) -> &Path {
        &self.absolute
    }

    /// The authorized root that contains it.
    pub fn root(&self) -> &Path {
        &self.root
    }
}

/// The roots a turn may read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessScope {
    primary: PathBuf,
    additional: Vec<PathBuf>,
}

impl AccessScope {
    /// A scope containing only the workspace itself.
    pub fn primary_only(primary: impl AsRef<Path>) -> Result<Self, PathError> {
        Self::new(primary, std::iter::empty::<&Path>())
    }

    /// A scope containing the workspace and the roots the user named.
    ///
    /// Every root is canonicalized and required to be a directory here, once,
    /// so a later resolution compares canonical against canonical. A root that
    /// is already inside another is dropped rather than kept: it grants nothing,
    /// and keeping it would make `roots()` imply an authority that does not
    /// exist.
    pub fn new(
        primary: impl AsRef<Path>,
        additional: impl IntoIterator<Item = impl AsRef<Path>>,
    ) -> Result<Self, PathError> {
        let primary = canonical_directory(primary.as_ref())?;
        let mut roots: Vec<PathBuf> = Vec::new();
        for candidate in additional {
            let root = canonical_directory(candidate.as_ref())?;
            if path_inside(&primary, &root) || roots.iter().any(|kept| path_inside(kept, &root)) {
                continue;
            }
            roots.push(root);
        }
        Ok(Self {
            primary,
            additional: roots,
        })
    }

    /// The workspace root. Relative paths are resolved against it.
    pub fn primary(&self) -> &Path {
        &self.primary
    }

    /// The extra roots, in the order the user named them.
    pub fn additional_roots(&self) -> &[PathBuf] {
        &self.additional
    }

    /// Every authorized root, primary first.
    pub fn roots(&self) -> impl Iterator<Item = &Path> {
        std::iter::once(self.primary.as_path()).chain(self.additional.iter().map(PathBuf::as_path))
    }

    /// The root containing `canonical`, when one does.
    ///
    /// `canonical` must already be canonical; this is a comparison, not a
    /// resolution, and it is not the security boundary on its own.
    pub fn root_for(&self, canonical: &Path) -> Option<&Path> {
        self.roots().find(|root| path_inside(root, canonical))
    }

    /// Resolves an existing path and proves it is in scope.
    ///
    /// This is the only way into the filesystem for a tool. The order matters:
    /// trim, anchor, canonicalize, *then* test containment. Testing containment
    /// on the unresolved string would approve a symlink whose target is outside.
    ///
    /// The result describes the namespace as of this call. It is not proof
    /// against a path component being replaced before the caller opens the file;
    /// see the module docs, "What the proof does not cover".
    pub fn resolve_existing(&self, requested: &str) -> Result<ResolvedPath, PathError> {
        let trimmed = requested.trim();
        if trimmed.is_empty() {
            return Err(PathError::Empty);
        }
        let candidate = Path::new(trimmed);
        let anchored = if candidate.is_absolute() {
            candidate.to_path_buf()
        } else {
            self.primary.join(candidate)
        };

        let canonical = std::fs::canonicalize(&anchored).map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                PathError::NotFound {
                    requested: trimmed.to_string(),
                }
            } else {
                PathError::Unresolvable {
                    requested: trimmed.to_string(),
                    detail: err.to_string(),
                }
            }
        })?;

        match self.root_for(&canonical) {
            Some(root) => Ok(ResolvedPath {
                root: root.to_path_buf(),
                absolute: canonical,
            }),
            None => Err(PathError::OutsideScope {
                requested: trimmed.to_string(),
            }),
        }
    }

    /// How a canonical path is shown to the model.
    ///
    /// Inside the workspace it is workspace-relative, because that is the name
    /// the user and the model both use. The workspace root itself is `.`.
    /// Anywhere else it stays absolute: an additional root has no meaningful
    /// position relative to the workspace, and a `../../elsewhere` rendering
    /// would invite the model to reuse a path that does not resolve.
    pub fn display_path(&self, canonical: &Path) -> String {
        if canonical == self.primary {
            return ".".to_string();
        }
        match canonical.strip_prefix(&self.primary) {
            Ok(relative) => relative.to_string_lossy().into_owned(),
            Err(_) => canonical.to_string_lossy().into_owned(),
        }
    }
}

/// Canonicalizes `path` and requires it to be a directory.
fn canonical_directory(path: &Path) -> Result<PathBuf, PathError> {
    let canonical = std::fs::canonicalize(path).map_err(|err| PathError::RootUnavailable {
        path: path.to_string_lossy().into_owned(),
        detail: err.to_string(),
    })?;
    let metadata = std::fs::metadata(&canonical).map_err(|err| PathError::RootUnavailable {
        path: path.to_string_lossy().into_owned(),
        detail: err.to_string(),
    })?;
    if !metadata.is_dir() {
        return Err(PathError::RootUnavailable {
            path: path.to_string_lossy().into_owned(),
            detail: "not a directory".to_string(),
        });
    }
    Ok(canonical)
}

/// Whether `candidate` is `root` or lives beneath it.
///
/// Component-wise rather than string-prefix, so `/work` does not contain
/// `/work-evil` (`vercel-labs/fx@580a0c5d src/core/workspace/pathing.zig:1125-1132`).
fn path_inside(root: &Path, candidate: &Path) -> bool {
    candidate == root || candidate.starts_with(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> tempfile::TempDir {
        tempfile::tempdir().expect("temporary tree")
    }

    fn canonical(dir: &tempfile::TempDir) -> PathBuf {
        dir.path().canonicalize().expect("canonicalize")
    }

    #[test]
    fn containment_is_component_wise_not_a_string_prefix() {
        assert!(path_inside(Path::new("/work"), Path::new("/work")));
        assert!(path_inside(Path::new("/work"), Path::new("/work/src/a.rs")));
        assert!(!path_inside(
            Path::new("/work"),
            Path::new("/work-evil/a.rs")
        ));
        assert!(!path_inside(Path::new("/work/src"), Path::new("/work")));
    }

    #[test]
    fn the_ignored_directory_set_is_upstreams() {
        // Upstream's list, in upstream's order (`tool_dispatch.zig:36-45`).
        assert_eq!(
            IGNORED_DIRECTORY_NAMES,
            [
                ".git",
                ".zig-cache",
                "zig-out",
                "node_modules",
                ".next",
                "dist",
                "build",
                "coverage"
            ]
        );
        assert!(is_ignored_directory(".git"));
        assert!(!is_ignored_directory("src"));
    }

    #[test]
    fn the_write_protected_set_covers_the_authorities_and_not_ordinary_dot_directories() {
        assert_eq!(PROTECTED_WRITE_DIRECTORY_NAMES, [".git", ".xfx"]);
        assert!(is_protected_write_directory(".git"));
        assert!(is_protected_write_directory(".xfx"));
        // macOS's default volume is case-insensitive, so on one of the two
        // supported platforms these name the very same directory.
        for spelled in [".GIT", ".Git", ".gIt", ".XFX", ".Xfx"] {
            assert!(
                is_protected_write_directory(spelled),
                "`{spelled}` reaches the same directory on a case-insensitive filesystem"
            );
        }
        // The whole reason the set is a list and not "starts with a dot": these
        // are project content a coding agent is expected to change.
        for ordinary in [".github", ".cargo", ".vscode", ".config", "git", "src"] {
            assert!(
                !is_protected_write_directory(ordinary),
                "`{ordinary}` must stay writable"
            );
        }
    }

    #[test]
    fn an_additional_root_already_inside_the_primary_is_dropped() {
        let dir = tree();
        let root = canonical(&dir);
        std::fs::create_dir(root.join("inner")).expect("create inner");
        let scope = AccessScope::new(&root, [root.join("inner")]).expect("scope");
        assert!(scope.additional_roots().is_empty());
        assert_eq!(scope.roots().count(), 1);
    }

    #[test]
    fn a_duplicate_additional_root_is_kept_once() {
        let dir = tree();
        let extra = tree();
        let scope = AccessScope::new(canonical(&dir), [canonical(&extra), canonical(&extra)])
            .expect("scope");
        assert_eq!(scope.additional_roots().len(), 1);
    }

    #[test]
    fn the_workspace_root_itself_displays_as_a_single_dot() {
        let dir = tree();
        let root = canonical(&dir);
        let scope = AccessScope::primary_only(&root).expect("scope");
        assert_eq!(scope.display_path(&root), ".");
        assert_eq!(scope.display_path(&root.join("a/b.txt")), "a/b.txt");
        assert_eq!(
            scope.display_path(Path::new("/elsewhere/x.txt")),
            "/elsewhere/x.txt"
        );
    }

    #[test]
    fn a_resolution_error_names_the_requested_path_and_not_the_resolved_one() {
        let message = PathError::OutsideScope {
            requested: "escape.txt".to_string(),
        }
        .to_string();
        assert!(message.contains("escape.txt"), "{message}");
        assert!(message.contains("outside"), "{message}");
    }
}
