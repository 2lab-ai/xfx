//! The profile home, `~/.xfx`: the one way xfx makes it, and the one rule an
//! existing one is held to.
//!
//! Every writer that keeps a file in the profile -- the session store, the
//! settings writers, the MCP profile, the prompt history, `/skills create` and
//! `/skills add` -- can be the first thing on a machine to need `~/.xfx`, and
//! each used to make it its own way. One made it with the umask's mode, `0755`,
//! and the session store, which refuses a `~/.xfx` that group or other can
//! reach, then refused the home that writer left until the user ran `chmod`.
//! [`ensure`] is now how every one of them makes a directory there.
//!
//! - **Made private.** A missing `~/.xfx`, and each missing directory below it
//!   the writer needs, is made `0700` from its creation, one level at a time
//!   (`DirBuilder::mode`), so no level is ever the umask's for a moment. So is a
//!   missing home directory above it.
//! - **Checked, never repaired.** A `~/.xfx` that is there must be a plain
//!   directory, owned by this user, with nothing granted to group or other
//!   ([`problem`], the session store's rule). One that is not is refused, in
//!   the session store's words ([`refusal`]), and never `chmod`ed: silently
//!   changing a directory xfx may not own would be a worse answer than
//!   stopping. Upstream repairs one instead (`openOrCreateVerifiedPrivateChild`,
//!   `src/core/shared/io.zig:712-742@481104fd`); `docs/parity.md` says why xfx
//!   does not.
//!
//! Workspace `.xfx/` directories are not this module's: they live in the
//! project, nothing here makes them, and the write tools refuse any path through
//! one (`crate::workspace::path::PROTECTED_WRITE_DIRECTORY_NAMES`).

use std::fmt;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

/// The mode every directory [`ensure`] makes is made with.
pub const DIR_MODE: u32 = 0o700;

/// Why an existing directory may not hold the profile's private state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
    /// A symbolic link: whatever it points at would decide where state lives.
    Link,
    /// Something other than a directory.
    NotDirectory,
    /// Another account's directory.
    Owner { owner: u32, current: u32 },
    /// A directory granting something to group or other.
    Open { mode: u32 },
}

impl Problem {
    /// The session store's clause for this problem, word for word.
    pub fn detail(&self) -> String {
        match *self {
            Self::Link => "it is a symbolic link, and xfx will not follow one to decide where \
                           session state lives"
                .to_string(),
            Self::NotDirectory => "it is not a directory".to_string(),
            Self::Owner { owner, current } => {
                format!("it is owned by uid {owner}, not by uid {current} running xfx")
            }
            Self::Open { mode } => format!(
                "it is mode {mode:o}; session state must not be reachable by group or other"
            ),
        }
    }
}

/// The sentence a refused directory is said in: the session store's, so a
/// home one writer refuses reads the same as the store refusing it.
pub fn refusal(path: &Path, detail: &str) -> String {
    format!("{} cannot hold session state: {detail}", path.display())
}

/// A profile home [`ensure`] would not use, carried inside the `io::Error` it
/// answers so a caller that speaks in error names (the MCP profile) can tell
/// which it was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    pub path: PathBuf,
    pub problem: Problem,
}

impl fmt::Display for Refused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&refusal(&self.path, &self.problem.detail()))
    }
}

impl std::error::Error for Refused {}

impl Refused {
    /// The refusal inside `err`, when `err` is one.
    pub fn of(err: &io::Error) -> Option<&Self> {
        err.get_ref().and_then(|inner| inner.downcast_ref::<Self>())
    }

    fn into_io(self) -> io::Error {
        io::Error::new(io::ErrorKind::PermissionDenied, self)
    }
}

/// What is wrong with the directory at `path`, looked at without following a
/// link; `None` when it is fit to hold private state **or is not there at
/// all** -- a missing directory is the caller's to make.
#[cfg(unix)]
pub fn problem(path: &Path) -> io::Result<Option<Problem>> {
    use std::os::unix::fs::MetadataExt;
    use std::os::unix::fs::PermissionsExt;

    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err),
    };
    if metadata.file_type().is_symlink() {
        return Ok(Some(Problem::Link));
    }
    if !metadata.is_dir() {
        return Ok(Some(Problem::NotDirectory));
    }
    let owner = metadata.uid();
    let current = rustix::process::geteuid().as_raw();
    if owner != current {
        return Ok(Some(Problem::Owner { owner, current }));
    }
    let mode = metadata.permissions().mode() & 0o777;
    if mode & 0o077 != 0 {
        return Ok(Some(Problem::Open { mode }));
    }
    Ok(None)
}

#[cfg(not(unix))]
pub fn problem(path: &Path) -> io::Result<Option<Problem>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => Ok(None),
        Ok(_) => Ok(Some(Problem::NotDirectory)),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

/// Makes sure `dir` -- the profile home `profile` itself, or a directory below
/// it -- is there.
///
/// `profile` is checked when it is there ([`problem`]) and refused, never
/// repaired, when it is not fit; when it is missing it is made `0700`, and so
/// is any missing directory above it. Then each missing level from `profile`
/// down to `dir` is made `0700` by itself. A level below `profile` that is
/// already there is used as it is, as long as it is a directory: below a
/// private home only its owner could have put it there.
///
/// `dir` must be `profile` or name a directory under it in plain components
/// (no `..`), so nothing this makes is ever outside the profile.
pub fn ensure(profile: &Path, dir: &Path) -> io::Result<()> {
    let below = dir.strip_prefix(profile).map_err(|_| outside(dir))?;
    // Every level named before anything is made: a `..` refused after the
    // profile home was made would have made it for nothing.
    let mut levels = Vec::new();
    for component in below.components() {
        let Component::Normal(name) = component else {
            return Err(outside(dir));
        };
        levels.push(name);
    }
    ensure_profile(profile)?;
    let mut level = profile.to_path_buf();
    for name in levels {
        level.push(name);
        match make_new(&level) {
            Ok(()) => {}
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists && level.is_dir() => {}
            Err(err) => return Err(err),
        }
    }
    Ok(())
}

fn outside(dir: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("{} is not inside the profile home", dir.display()),
    )
}

/// The profile home, checked when it is there and made `0700` when it is not.
fn ensure_profile(profile: &Path) -> io::Result<()> {
    match fs::symlink_metadata(profile) {
        Ok(_) => {}
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            if let Some(parent) = profile.parent() {
                if !parent.as_os_str().is_empty() && !parent.is_dir() {
                    make_all(parent)?;
                }
            }
            match make_new(profile) {
                Ok(()) => {}
                // Lost a race with another writer making it: it is checked
                // below all the same.
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {}
                Err(err) => return Err(err),
            }
        }
        Err(err) => return Err(err),
    }
    match problem(profile)? {
        None => Ok(()),
        Some(problem) => Err(Refused {
            path: profile.to_path_buf(),
            problem,
        }
        .into_io()),
    }
}

/// Makes one new directory, owner-only from the moment it exists; one already
/// there is `AlreadyExists`, for a writer that needs a directory nobody else
/// made (`/skills add`'s transaction) to answer. Its parent must be there.
#[cfg(unix)]
pub fn make_new(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new().mode(DIR_MODE).create(path)
}

#[cfg(not(unix))]
pub fn make_new(path: &Path) -> io::Result<()> {
    fs::create_dir(path)
}

/// Makes `path` and each missing directory above it, every one owner-only:
/// `DirBuilder`'s recursive create gives each level it makes the builder's mode.
#[cfg(unix)]
fn make_all(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(DIR_MODE)
        .create(path)
}

#[cfg(not(unix))]
fn make_all(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)
}

/// A profile home for a test's fixture: `profile` made as [`ensure`] makes it,
/// so a fixture is the home xfx would have made rather than the umask's.
#[cfg(test)]
pub(crate) fn made(profile: &Path) -> &Path {
    ensure(profile, profile).expect("a private profile home");
    profile
}

/// A temporary directory for a test to use **as** a profile home, `0700` as
/// xfx makes one: `tempfile`'s own are made with the umask's mode.
#[cfg(test)]
pub(crate) fn temp_home() -> tempfile::TempDir {
    use std::os::unix::fs::PermissionsExt;
    tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(DIR_MODE))
        .tempdir()
        .expect("a private temporary directory")
}

#[cfg(test)]
mod tests;
