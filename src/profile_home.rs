//! The profile home, `~/.xfx`: the one way xfx makes it, and the one rule an
//! existing one is held to.
//!
//! Every writer that keeps a file in the profile -- the session store, the
//! settings writers, the MCP profile, the prompt history, `/skills create` and
//! `/skills add` -- can be the first thing on a machine to need `~/.xfx`, and
//! each used to make it its own way. One made it with the umask's mode, `0755`,
//! and the session store, which refuses a `~/.xfx` that group or other can
//! reach, then refused the home that writer left until the user ran `chmod`.
//! [`ensure`] and [`ensure_content`] are now how every one of them makes a
//! directory there.
//!
//! One writer makes its own, the stronger way: the usage cache -- a library
//! with no caller yet on this tree (`docs/parity.md`'s usage cache row) --
//! (`crate::usage::cache`'s `fs` half, `src/usage/cache.rs`) makes `~/.xfx` and
//! `usage-cache` with `mkdirat` `0700` on the home's descriptor, opens each with
//! `O_NOFOLLOW | O_DIRECTORY` (`open_dir_in`), `fchmod`s one only when it made
//! it, and refuses -- never repairs -- one that is a link or not private
//! (`open_or_create_profile`, `open_or_create_dir`), in its own words. It holds
//! `~/.xfx` to the same rule as everything here, and refuses a missing home as
//! this module does (`open_home`).
//!
//! - **Made private.** A missing `~/.xfx`, and each missing directory below it
//!   the writer needs, is made `0700` from its creation, one level at a time
//!   (`DirBuilder::mode`), so no level is ever the umask's for a moment.
//! - **No home, no profile.** A home directory that is not there is refused
//!   (`NotFound`), never made: a missing `$HOME` is a misconfiguration -- an
//!   unmounted home, a stale environment, a typo -- and upstream refuses it too
//!   (`Store.initFromHome`, `src/core/config/settings_store.zig:343-347@481104fd`).
//! - **Checked, never repaired.** A `~/.xfx` that is there must be a plain
//!   directory, owned by this user, with nothing granted to group or other
//!   ([`problem`], the session store's rule). One that is not is refused, in
//!   the session store's words ([`refusal`]), and never `chmod`ed: silently
//!   changing a directory xfx may not own would be a worse answer than
//!   stopping. Upstream repairs one instead (`openOrCreateVerifiedPrivateChild`,
//!   `src/core/shared/io.zig:712-742@481104fd`); `docs/parity.md` says why xfx
//!   does not.
//! - **Private state and user content below it.** The two kinds of level
//!   below `~/.xfx` are told apart as upstream tells them apart:
//!   - *Private state* -- `.skill-install-locks/` through [`ensure`], and
//!     `sessions/` and `sessions.staged/` through the session store's own
//!     check -- is held to `~/.xfx`'s rule when it is there: a link, something
//!     that is not a directory, another account's or an open one is refused
//!     (upstream opens it with `follow_symlinks = false` and answers
//!     `DurablePathUnsafe`, `openOrCreateVerifiedPrivateChild`).
//!   - *User content* -- the managed skills root `skills/` and each
//!     `skills/<name>/`, through [`ensure_content`] -- is followed when it is a
//!     link to a directory, so a skills directory linked into dotfiles keeps
//!     working, as upstream's `makeDirRecursive` follows one
//!     (`createSkillTemplate`, `src/builtins/skills.zig:247-252@481104fd`).
//! - **A trusted home, stable ancestors.** Every check here is on a path:
//!   `~/.xfx` is looked at with `lstat`, and each writer then reaches it again
//!   by the same path. That is sound while `$HOME` and the directories above it
//!   are not replaced between the check and the write, which takes write access
//!   to `$HOME` -- this user or root, who hold the profile already, or anyone
//!   at all when `$HOME` is writable by others without the sticky bit, the
//!   user's own misconfiguration. Two things are named and not done here: an
//!   opener that pins the directory it checked (`O_NOFOLLOW | O_DIRECTORY`,
//!   `fstat`, then `*at` calls, as the usage cache's `open_dir_in`,
//!   `src/usage/cache.rs`, already does for its own two directories), and
//!   access-control lists, which the mode bits do not show on macOS and which
//!   are not read.
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
    /// The session store's clause for this problem at `path`. An open
    /// directory -- the one problem this user can fix, it being theirs --
    /// names the fix.
    pub fn detail(&self, path: &Path) -> String {
        match *self {
            Self::Link => "it is a symbolic link, and xfx will not follow one to decide where \
                           session state lives"
                .to_string(),
            Self::NotDirectory => "it is not a directory".to_string(),
            Self::Owner { owner, current } => {
                format!("it is owned by uid {owner}, not by uid {current} running xfx")
            }
            Self::Open { mode } => format!(
                "it is mode {mode:o}; session state must not be reachable by group or other \
                 (`chmod 700 {}` makes it private)",
                shell_word(path)
            ),
        }
    }
}

/// `path` as one shell word naming the same directory, so the `chmod` a
/// refusal names can be pasted as it is: an absolute path whose every byte is
/// one a shell leaves alone is as it is; any other is single-quoted, and a
/// relative one -- a relative `HOME` gives one -- is said from `./`, so a
/// leading `~` or `=` is not expanded and a leading `-` is not an option.
fn shell_word(path: &Path) -> String {
    let text = path.to_string_lossy();
    let absolute = text.starts_with('/');
    let plain = absolute
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"/._-+@%:,=~".contains(&byte));
    if plain {
        return text.into_owned();
    }
    let text = if absolute {
        text.into_owned()
    } else {
        format!("./{text}")
    };
    format!("'{}'", text.replace('\'', "'\\''"))
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
        f.write_str(&refusal(&self.path, &self.problem.detail(&self.path)))
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

/// Makes sure `dir` -- the profile home `profile` itself, or a directory of
/// **private state** below it -- is there.
///
/// `profile` is checked when it is there ([`problem`]) and refused, never
/// repaired, when it is not fit; when it is missing it is made `0700`, as long
/// as the home directory it goes in is there. Then each level from `profile`
/// down to `dir` is made `0700` when it is missing, and held to `profile`'s
/// rule when it is there: a link, something that is not a directory, another
/// account's or an open one is refused, never followed.
///
/// `dir` must be `profile` or name a directory under it in plain components
/// (no `..`), so nothing this makes is ever outside the profile.
pub fn ensure(profile: &Path, dir: &Path) -> io::Result<()> {
    ensure_levels(profile, dir, Kind::Private)
}

/// [`ensure`] for a directory of **user content** below the profile home --
/// the managed skills root and a skill's directory in it: `profile` is checked
/// and made exactly as [`ensure`] checks and makes it, and each missing level
/// below it is made `0700`, but a level that is there is used as it is when it
/// is a directory **or a link to one**, which is followed, as upstream follows
/// one (`makeDirRecursive`, `src/core/shared/io.zig:1077-1093@481104fd`).
pub fn ensure_content(profile: &Path, dir: &Path) -> io::Result<()> {
    ensure_levels(profile, dir, Kind::Content)
}

/// What a level below the profile home holds, which decides how one that is
/// already there is taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// Held to the profile home's rule; a link is refused.
    Private,
    /// The user's: a link to a directory is followed.
    Content,
}

fn ensure_levels(profile: &Path, dir: &Path, kind: Kind) -> io::Result<()> {
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
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => match kind {
                Kind::Private => refuse_unfit(&level)?,
                Kind::Content if level.is_dir() => {}
                Kind::Content => return Err(err),
            },
            Err(err) => return Err(err),
        }
    }
    Ok(())
}

/// `Err` with the [`Refused`] for a directory at `path` that [`problem`]
/// finds unfit; `Ok` for a fit one.
fn refuse_unfit(path: &Path) -> io::Result<()> {
    match problem(path)? {
        None => Ok(()),
        Some(problem) => Err(Refused {
            path: path.to_path_buf(),
            problem,
        }
        .into_io()),
    }
}

fn outside(dir: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("{} is not inside the profile home", dir.display()),
    )
}

/// The profile home, checked when it is there and made `0700` when it is not;
/// a home directory that is not there is `NotFound`, and nothing is made.
fn ensure_profile(profile: &Path) -> io::Result<()> {
    match fs::symlink_metadata(profile) {
        Ok(_) => {}
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            if let Some(home) = profile.parent() {
                if !home.as_os_str().is_empty() && !home.is_dir() {
                    return Err(io::Error::new(
                        io::ErrorKind::NotFound,
                        format!(
                            "the home directory {} is not there, and xfx does not make one",
                            home.display()
                        ),
                    ));
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
    refuse_unfit(profile)
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
