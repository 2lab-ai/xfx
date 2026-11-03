//! The attribute file auto's own `git` still reads.
//!
//! A `filter`, `diff` or `merge` driver is a command git runs, configured
//! anywhere git reads configuration -- an included file auto can write among
//! them -- and selected by an attribute. The hardened argv
//! (`super::authority::GIT_CONFIG_OVERRIDES`) closes every attribute source git
//! offers a switch for: the work tree's `.gitattributes` (`--attr-source`, an
//! empty tree), `core.attributesFile` (`/dev/null`), and the system file
//! (`GIT_ATTR_NOSYSTEM` in the plan's environment). One is left:
//! `$GIT_DIR/info/attributes`, which no option skips. The file tools never
//! write below `.git`, but a repository can already carry one, and a macro
//! defined there can select a driver without naming it on the line that uses
//! it.
//!
//! So before auto runs `git`, every git directory the command could use is
//! found and its `info/attributes` read: a line that names a driver
//! (`filter=`, `diff=`, `merge=` -- a macro's definition included) makes the
//! command ask, and so does a file that cannot be read.
//!
//! # Which git directories
//!
//! More than git itself would pick, never fewer: walking from the working
//! directory to `/`, every `.git` directory, every `gitdir:` file's target,
//! and every directory that is itself a git directory (`HEAD` and `objects`),
//! not only the nearest -- git's discovery stops at the first one that is
//! valid, and a guess at "valid" is a check that could be wrong in the unsafe
//! direction. For each, its `commondir` (a linked worktree reads the main
//! repository's `info/attributes`) and every submodule's below `modules/` (a
//! `git status` runs one in each).

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// The most bytes of one `info/attributes` read; past it, the command asks.
const MAX_ATTRIBUTES_BYTES: u64 = 1024 * 1024;

/// The most entries listed below one `modules/` directory; past it, the
/// command asks. A module's git directory is listed as well as its own
/// `modules/` -- hooks, info and the rest, about two dozen entries for a
/// fresh one -- so this leaves room for several hundred submodules.
const MAX_MODULE_ENTRIES: usize = 16_384;

/// Why auto does not run `git` in `cwd`, or `None` when no attribute file a
/// git directory of it holds selects a driver.
pub(crate) fn driver_hold(cwd: &Path) -> Option<String> {
    check(cwd).err()
}

fn check(cwd: &Path) -> Result<(), String> {
    for dir in repository_dirs(cwd)? {
        attributes(&dir.join("info").join("attributes"))?;
    }
    Ok(())
}

/// Every git directory a git command run in `cwd` can read: each one from
/// `cwd` up, a linked worktree's common directory, and every submodule's
/// below `modules/`.
pub(crate) fn repository_dirs(cwd: &Path) -> Result<Vec<PathBuf>, String> {
    let mut found: Vec<PathBuf> = Vec::new();
    for git_dir in git_dirs(cwd)? {
        let mut dirs = vec![git_dir.clone()];
        if let Some(common) = common_dir(&git_dir)? {
            dirs.push(common);
        }
        for dir in dirs {
            if !found.contains(&dir) {
                found.push(dir.clone());
            }
            modules(&dir.join("modules"), &mut found)?;
        }
    }
    Ok(found)
}

/// Every git directory from `cwd` up to `/`.
fn git_dirs(cwd: &Path) -> Result<Vec<PathBuf>, String> {
    let mut found = Vec::new();
    for dir in cwd.ancestors() {
        let dot_git = dir.join(".git");
        match std::fs::metadata(&dot_git) {
            Ok(metadata) if metadata.is_dir() => found.push(dot_git),
            Ok(_) => found.push(git_file_target(&dot_git, dir)?),
            Err(err) if absent(&err) => {}
            Err(err) => return Err(uninspectable(&dot_git, &err)),
        }
        if is_git_dir(dir)? {
            found.push(dir.to_path_buf());
        }
    }
    Ok(found)
}

/// The directory a `.git` file names (`gitdir: <path>`), relative to `dir`.
fn git_file_target(file: &Path, dir: &Path) -> Result<PathBuf, String> {
    let text = read_bounded(file)?;
    let target = text
        .lines()
        .next()
        .and_then(|line| line.strip_prefix("gitdir:"))
        .map(str::trim)
        .filter(|target| !target.is_empty())
        .ok_or_else(|| format!("`{}` is not a `gitdir:` file", file.display()))?;
    Ok(dir.join(target))
}

/// Whether `dir` is itself a git directory, whatever it is called: a `HEAD`
/// file, and an `objects` directory or -- a linked worktree's -- a
/// `commondir` file.
///
/// A link that dangles counts as the piece it is named for: git takes a
/// `HEAD` link to `refs/heads/main` for a symbolic ref whether or not the
/// branch exists yet, and counting a dangling `objects` too only asks more.
pub(crate) fn is_git_dir(dir: &Path) -> Result<bool, String> {
    let is = |path: PathBuf, directory: bool| match std::fs::symlink_metadata(&path) {
        Ok(link) if link.file_type().is_symlink() => match std::fs::metadata(&path) {
            Ok(metadata) => Ok(metadata.is_dir() == directory),
            Err(err) if absent(&err) => Ok(true),
            Err(err) => Err(uninspectable(&path, &err)),
        },
        Ok(metadata) => Ok(metadata.is_dir() == directory),
        Err(err) if absent(&err) => Ok(false),
        Err(err) => Err(uninspectable(&path, &err)),
    };
    Ok(is(dir.join("HEAD"), false)?
        && (is(dir.join("objects"), true)? || is(dir.join("commondir"), false)?))
}

/// The main repository's git directory, for a linked worktree's.
fn common_dir(git_dir: &Path) -> Result<Option<PathBuf>, String> {
    let file = git_dir.join("commondir");
    match std::fs::metadata(&file) {
        Ok(_) => {
            let text = read_bounded(&file)?;
            let common = text.lines().next().map(str::trim).unwrap_or_default();
            if common.is_empty() {
                return Err(format!("`{}` names no directory", file.display()));
            }
            Ok(Some(git_dir.join(common)))
        }
        Err(err) if absent(&err) => Ok(None),
        Err(err) => Err(uninspectable(&file, &err)),
    }
}

/// Every submodule git directory below `modules`. A submodule's name may
/// hold `/`, so a directory that is not a git directory is a step of a name
/// and is descended. One that is, is recorded and descended too, except for
/// its objects, refs, logs and other data ([`super::links::GIT_DATA_DIRECTORIES`]),
/// which would count a real repository's size against the bound: what passes
/// the git-directory test -- a dangling `objects` link is enough -- may hold
/// another module's directory under a name of its own. A linked directory is
/// followed.
fn modules(modules: &Path, found: &mut Vec<PathBuf>) -> Result<(), String> {
    // Each directory to list, and whether it is a git directory.
    let mut queue = vec![(modules.to_path_buf(), false)];
    let mut left = MAX_MODULE_ENTRIES;
    while let Some((dir, git_dir)) = queue.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(err) if absent(&err) => continue,
            Err(err) => return Err(uninspectable(&dir, &err)),
        };
        for entry in entries {
            let entry = entry.map_err(|err| uninspectable(&dir, &err))?;
            left = left.checked_sub(1).ok_or_else(|| {
                format!(
                    "`{}` holds more than {MAX_MODULE_ENTRIES} entries",
                    modules.display()
                )
            })?;
            if git_dir
                && super::links::GIT_DATA_DIRECTORIES
                    .iter()
                    .any(|data| entry.file_name() == *data)
            {
                continue;
            }
            let path = entry.path();
            let directory = match std::fs::metadata(&path) {
                Ok(metadata) => metadata.is_dir(),
                Err(err) if absent(&err) => false,
                Err(err) => return Err(uninspectable(&path, &err)),
            };
            if !directory {
                continue;
            }
            let is = is_git_dir(&path)?;
            if is && !found.contains(&path) {
                found.push(path.clone());
            }
            queue.push((path, is));
        }
    }
    Ok(())
}

/// Refuses an attribute file that names a driver or cannot be read.
fn attributes(file: &Path) -> Result<(), String> {
    let text = match std::fs::metadata(file) {
        Ok(_) => read_bounded(file)?,
        Err(err) if absent(&err) => return Ok(()),
        Err(err) => return Err(uninspectable(file, &err)),
    };
    let selects = text.lines().map(str::trim).any(|line| {
        !line.is_empty()
            && !line.starts_with('#')
            && ["filter=", "diff=", "merge="]
                .iter()
                .any(|driver| line.contains(driver))
    });
    if selects {
        return Err(format!(
            "`{}` selects a filter, diff or merge driver",
            file.display()
        ));
    }
    Ok(())
}

fn read_bounded(file: &Path) -> Result<String, String> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(file)
        .and_then(|opened| {
            opened
                .take(MAX_ATTRIBUTES_BYTES + 1)
                .read_to_end(&mut bytes)
        })
        .map_err(|err| uninspectable(file, &err))?;
    if bytes.len() as u64 > MAX_ATTRIBUTES_BYTES {
        return Err(format!(
            "`{}` is larger than {MAX_ATTRIBUTES_BYTES} bytes",
            file.display()
        ));
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn absent(err: &std::io::Error) -> bool {
    matches!(err.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory)
}

fn uninspectable(path: &Path, err: &std::io::Error) -> String {
    format!("`{}` cannot be read: {err}", path.display())
}
