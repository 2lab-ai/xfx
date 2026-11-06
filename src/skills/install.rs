//! `/skills add` and `install`, **from a local directory only**.
//!
//! Upstream's install (`vercel-labs/fx@c1db9191 src/builtins/skills.zig:199-840`)
//! reads the forms people paste -- `npx skills add <source> --skill <name>`,
//! `skills.sh/<owner>/<repo>/<skill>`, `<owner>/<repo>@<skill>` -- tries the
//! source as a local directory, and otherwise clones it from GitHub. xfx keeps
//! every form and the local half whole: a `SKILL.md` at the directory's top is
//! installed under the directory's name, every other `SKILL.md` under the name
//! of the directory holding it, each copied into the managed root through a
//! staged directory and two renames under a per-skill lock, so a reader never
//! sees half a skill and a failed swap puts the old one back.
//!
//! The clone is **not** run. A source that is not an existing directory is
//! answered as upstream answers a clone that failed, and nothing reaches the
//! network: installing from GitHub is a network fetch and an execution of
//! somebody else's instructions, and that is a decision this command does not
//! get to make on its own (`docs/parity.md`, the `/skills` row).

use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

use super::contract;

/// What a failed install says (`installCommandResult`, `skills.zig:156-171`).
pub const FAILED: &str = "Failed to install. Check the source path or URL and try again.";

/// The directory the per-skill locks are kept in, beside the managed root
/// (`install_lock_dir_name`, `skills.zig:640`).
const LOCK_DIR: &str = ".skill-install-locks";

/// How long an install waits for another one installing the same name
/// (`install_lock_deadline_ms`, `:639`).
const LOCK_DEADLINE: std::time::Duration = std::time::Duration::from_millis(2_000);

/// How many fresh names a staging directory is tried under
/// (`install_transaction_attempts`, `:638`).
const TRANSACTION_ATTEMPTS: usize = 8;

/// Installs what `source` names into `managed`, and says how it went
/// (`installCommandResult` and the `.installed` arm of
/// `applySkillsCommandResult`, `app_commands.zig:1919-1940`).
pub fn install(managed: &Path, source: &str, filter: Option<&str>, cwd: &Path) -> String {
    match installed(managed, source, filter, cwd) {
        Ok(said) | Err(said) => said,
    }
}

/// [`install`], split by whether anything was installed: `Ok` is the
/// `Installed: <name>` lines upstream says once the list it reloads lands
/// (`queueSkillsNoticeAfterRefresh`, `app_commands.zig:1928-1951`), `Err` the
/// refusal it says at once.
pub fn installed(
    managed: &Path,
    source: &str,
    filter: Option<&str>,
    cwd: &Path,
) -> Result<String, String> {
    let Ok((source, filter)) = normalize(source, filter) else {
        return Err(FAILED.to_string());
    };
    let Some(directory) = local_directory(&source, cwd) else {
        // Upstream would clone `https://github.com/<source>.git` here; xfx
        // answers as a clone that failed would be answered.
        return Err(FAILED.to_string());
    };
    let fallback = directory
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    match install_directory(managed, &directory, &fallback, filter.as_deref()) {
        // A profile home xfx will not write in is said as the session store
        // says it: "check the source path" would send the user the wrong way.
        Err(err) => Err(crate::profile_home::Refused::of(&err).map_or_else(
            || FAILED.to_string(),
            |refused| format!("Failed to install: {refused}."),
        )),
        Ok(installed) if installed.is_empty() => Err(match filter {
            Some(filter) => format!("Skill '{filter}' not found in the repository."),
            None => "No skills found (no SKILL.md files).".to_string(),
        }),
        Ok(installed) => Ok(installed
            .iter()
            .map(|name| format!("Installed: {name}"))
            .collect::<Vec<_>>()
            .join("\n")),
    }
}

/// The source and filter a pasted install names (`normalizeInstallRequest`,
/// `skills.zig:212-228`).
fn normalize(raw: &str, explicit: Option<&str>) -> Result<(String, Option<String>), ()> {
    let trimmed = raw.trim_matches([' ', '\t', '\r', '\n']);
    if trimmed.is_empty() {
        return Err(());
    }
    let (command_source, command_filter) = npx_source(trimmed).unwrap_or((trimmed, None));
    let (inline_source, inline_filter) = inline_source(command_source);
    let mut merged: Option<&str> = None;
    for filter in [command_filter, inline_filter, explicit]
        .into_iter()
        .flatten()
    {
        if filter.is_empty() {
            continue;
        }
        match merged {
            None => merged = Some(filter),
            Some(kept) if kept == filter => {}
            Some(_) => return Err(()),
        }
    }
    Ok((inline_source.to_string(), merged.map(str::to_string)))
}

/// `npx skills add <source> [--skill <name>]`, and `bunx` the same
/// (`parseNpxInstallSource`, `:453-480`).
fn npx_source(input: &str) -> Option<(&str, Option<&str>)> {
    if !((input.starts_with("npx ") || input.starts_with("bunx ")) && input.contains("skills add"))
    {
        return None;
    }
    let mut source = None;
    let mut filter = None;
    let mut tokens = input
        .split([' ', '\t', '\r', '\n'])
        .filter(|token| !token.is_empty());
    while let Some(token) = tokens.next() {
        if matches!(
            token,
            "npx" | "bunx" | "skills" | "add" | "-g" | "-y" | "--yes"
        ) {
            continue;
        }
        if token == "--skill" {
            filter = Some(tokens.next()?);
            continue;
        }
        if let Some(value) = token.strip_prefix("--skill=") {
            filter = Some(value);
            continue;
        }
        if token.starts_with('-') {
            continue;
        }
        if source.is_none() {
            source = Some(token);
        }
    }
    source.map(|source| (source, filter))
}

/// `skills.sh/<owner>/<repo>/<skill>` and `<owner>/<repo>@<skill>`
/// (`parseInlineInstallSource`, `:486-534`).
fn inline_source(input: &str) -> (&str, Option<&str>) {
    if let Some(marker) = input.find("skills.sh/") {
        let rest = &input[marker + "skills.sh/".len()..];
        let mut parts = rest.split('/').filter(|part| !part.is_empty());
        if let (Some(owner), Some(repo)) = (parts.next(), parts.next()) {
            let skill = parts.next();
            return (&rest[..owner.len() + 1 + repo.len()], skill);
        }
    }
    let repo_skill = !(input.starts_with("http://")
        || input.starts_with("https://")
        || input.starts_with("git@")
        || input.starts_with('/')
        || input.starts_with("./")
        || input.starts_with("../")
        || input.starts_with("~/"));
    if repo_skill {
        if let Some(at) = input.rfind('@') {
            if input[..at].rfind('/').is_some_and(|slash| slash > 0) && at + 1 < input.len() {
                return (&input[..at], Some(&input[at + 1..]));
            }
        }
    }
    (input, None)
}

/// The directory `source` names when it is one: as written when absolute,
/// resolved against `cwd` otherwise (`resolveInstallDirectory`, `:438-451`).
fn local_directory(source: &str, cwd: &Path) -> Option<PathBuf> {
    let path = Path::new(source);
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        fs::canonicalize(cwd.join(path)).ok()?
    };
    fs::metadata(&resolved)
        .is_ok_and(|meta| meta.is_dir())
        .then_some(resolved)
}

/// Installs every skill `source` holds (`installFromDirectoryWithMetadataReader`,
/// `:314-421`), and answers the names installed in the order they were.
fn install_directory(
    managed: &Path,
    source: &Path,
    fallback: &str,
    filter: Option<&str>,
) -> io::Result<Vec<String>> {
    ensure_managed(managed, managed)?;
    let mut installed = Vec::new();
    // The directory itself, when it is one skill.
    if let Some(metadata) = read_metadata(&source.join("SKILL.md"), fallback) {
        let wanted = filter.is_none_or(|filter| filter == metadata.name || filter == fallback);
        if contract::is_managed_name(fallback) && wanted {
            copy_skill(source, managed, fallback)?;
            installed.push(metadata.name);
        }
    }
    // Then every `SKILL.md` below it, named by the directory holding it.
    for skill_file in skill_files(source)? {
        let Some(parent) = skill_file.parent() else {
            continue;
        };
        if parent == source {
            continue;
        }
        let Some(dir_name) = parent.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if dir_name.starts_with('.') || !contract::is_managed_name(dir_name) {
            continue;
        }
        let Some(metadata) = read_metadata(&skill_file, dir_name) else {
            continue;
        };
        if let Some(filter) = filter {
            if filter != dir_name && filter != metadata.name {
                continue;
            }
        }
        copy_skill(parent, managed, dir_name)?;
        installed.push(metadata.name);
    }
    Ok(installed)
}

/// A `SKILL.md`'s metadata, when it is a regular file whose head parses.
fn read_metadata(path: &Path, fallback: &str) -> Option<contract::Metadata> {
    let meta = fs::symlink_metadata(path).ok()?;
    if !meta.is_file() {
        return None;
    }
    let mut file = super::discovery::open_no_follow(path).ok()?;
    let content = contract::read_metadata_prefix(&mut file, meta.len()).ok()??;
    contract::resolve(&contract::parse(&content), fallback).ok()
}

/// Every regular file named `SKILL.md` under `root`, below its top level,
/// links not followed, in byte order of their paths.
///
/// Upstream walks the source in the order its file system lists it; sorted
/// here, so two machines install a many-skill source in the same order.
fn skill_files(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            if kind.is_dir() {
                pending.push(entry.path());
            } else if kind.is_file() && entry.file_name() == "SKILL.md" && dir != root {
                found.push(entry.path());
            }
        }
    }
    found.sort_by(|left, right| {
        left.as_os_str()
            .as_encoded_bytes()
            .cmp(right.as_os_str().as_encoded_bytes())
    });
    Ok(found)
}

/// The profile home the managed root `~/.xfx/skills` is in: its parent.
fn profile_of(managed: &Path) -> io::Result<&Path> {
    managed
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "InvalidSkillsDirectory"))
}

/// Makes `dir` -- the managed root, or a directory in it -- if it is not there,
/// the profile home above it as every writer in the profile makes it: each
/// missing level `0700`, an existing `~/.xfx` that is not private refused
/// rather than repaired ([`crate::profile_home::ensure`]). Upstream makes these
/// with the default mode (`makeDirRecursive`, `skills.zig:247`, `:252`,
/// `ensureDir`, `:786-798@481104fd`); `docs/parity.md` says why xfx does not.
pub(super) fn ensure_managed(managed: &Path, dir: &Path) -> io::Result<()> {
    crate::profile_home::ensure(profile_of(managed)?, dir)
}

/// Copies `source` into `managed/<name>` whole, or not at all
/// (`copySkillDirWithOptions`, `:690-735`): staged beside the destination,
/// then the old one moved aside, the new one moved in, and the old one put
/// back when the second move fails -- all under the name's lock.
fn copy_skill(source: &Path, managed: &Path, name: &str) -> io::Result<()> {
    if !contract::is_managed_name(name) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid skill name",
        ));
    }
    ensure_managed(managed, managed)?;
    let destination = managed.join(name);
    let transaction = transaction_root(managed)?;
    let result = (|| {
        let staged = transaction.join("staged");
        let backup = transaction.join("backup");
        copy_tree(source, &staged)?;
        let _lock = lock(managed, name)?;
        let moved_existing = match fs::rename(&destination, &backup) {
            Ok(()) => true,
            Err(error) if error.kind() == io::ErrorKind::NotFound => false,
            Err(error) => return Err(error),
        };
        if let Err(error) = fs::rename(&staged, &destination) {
            if moved_existing {
                fs::rename(&backup, &destination)?;
            }
            return Err(error);
        }
        Ok(())
    })();
    let _ = fs::remove_dir_all(&transaction);
    result
}

/// A fresh `managed/.skill-install-<32 hex>` directory (`createInstallTransaction`).
fn transaction_root(managed: &Path) -> io::Result<PathBuf> {
    for _ in 0..TRANSACTION_ATTEMPTS {
        let mut token = [0u8; 16];
        fill_random(&mut token);
        let hex: String = token.iter().map(|byte| format!("{byte:02x}")).collect();
        let root = managed.join(format!(".skill-install-{hex}"));
        match crate::profile_home::make_new(&root) {
            Ok(()) => return Ok(root),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "SkillInstallTransactionCollision",
    ))
}

/// Sixteen bytes nobody else picked: the clock, the process and a counter
/// hashed together. Not a secret -- a name that two installs must not share.
fn fill_random(token: &mut [u8; 16]) {
    use sha2::{Digest, Sha256};
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let mut hash = Sha256::new();
    hash.update(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            .to_le_bytes(),
    );
    hash.update(std::process::id().to_le_bytes());
    hash.update(COUNTER.fetch_add(1, Ordering::Relaxed).to_le_bytes());
    token.copy_from_slice(&hash.finalize()[..16]);
}

/// The per-name lock, `<managed's parent>/.skill-install-locks/<name>`, held
/// until it is dropped (`acquireSkillInstallLock`, `:660-688`).
#[cfg(unix)]
fn lock(managed: &Path, name: &str) -> io::Result<File> {
    use rustix::fs::{flock, FlockOperation};
    use std::os::unix::fs::OpenOptionsExt;
    let parent = profile_of(managed)?;
    let dir = parent.join(LOCK_DIR);
    crate::profile_home::ensure(parent, &dir)?;
    if !fs::symlink_metadata(&dir)?.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the lock directory is not a directory",
        ));
    }
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(dir.join(name))?;
    let started = std::time::Instant::now();
    loop {
        match flock(&file, FlockOperation::NonBlockingLockExclusive) {
            Ok(()) => return Ok(file),
            Err(rustix::io::Errno::WOULDBLOCK) if started.elapsed() < LOCK_DEADLINE => {
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            Err(rustix::io::Errno::WOULDBLOCK) => {
                return Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "SkillInstallLockBusy",
                ))
            }
            Err(error) => return Err(io::Error::from(error)),
        }
    }
}

#[cfg(not(unix))]
fn lock(_managed: &Path, _name: &str) -> io::Result<File> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "SkillInstallLockUnsupported",
    ))
}

/// A copy of `source` at `destination`: directories and regular files, links
/// and anything named `.git…` left behind (`copyDirRecursive`, `:737-760`).
///
/// `destination` is new -- the stage in a fresh transaction, or a directory in
/// it this copy has not made yet -- and each is made owner-only, as every
/// directory in the profile is (`crate::profile_home`).
fn copy_tree(source: &Path, destination: &Path) -> io::Result<()> {
    crate::profile_home::make_new(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let name = entry.file_name();
        if name.as_encoded_bytes().starts_with(b".git") {
            continue;
        }
        let kind = entry.file_type()?;
        let to = destination.join(&name);
        if kind.is_dir() {
            copy_tree(&entry.path(), &to)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), &to)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    #[test]
    fn a_local_directory_of_skills_is_installed_under_each_skills_directory() {
        let base = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(base.path()).unwrap();
        let source = root.join("work/skill-src");
        write(
            &source.join("foo/SKILL.md"),
            "---\nname: foo\ndescription: A skill installed from a local directory\n---\n\n# foo\n\nFoo body.\n",
        );
        write(&source.join("foo/.git/HEAD"), "ref");
        write(&source.join("foo/notes/extra.md"), "extra");
        let managed = root.join("home/.xfx/skills");
        let said = install(&managed, "./skill-src", None, &root.join("work"));
        assert_eq!(said, "Installed: foo");
        assert_eq!(
            fs::read_to_string(managed.join("foo/SKILL.md")).unwrap(),
            "---\nname: foo\ndescription: A skill installed from a local directory\n---\n\n# foo\n\nFoo body.\n"
        );
        assert!(managed.join("foo/notes/extra.md").exists());
        assert!(!managed.join("foo/.git").exists());
        assert!(root.join("home/.xfx/.skill-install-locks/foo").exists());
        let leftovers: Vec<_> = fs::read_dir(&managed)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(leftovers, ["foo"]);
    }

    #[test]
    fn a_source_that_is_not_a_directory_is_a_failed_install_and_no_fetch() {
        let base = tempfile::tempdir().unwrap();
        let managed = base.path().join("home/.xfx/skills");
        for source in [
            "owner/repo",
            "https://github.com/a/b",
            "npx skills add a/b",
            "./missing",
        ] {
            assert_eq!(
                install(&managed, source, None, base.path()),
                FAILED,
                "{source}"
            );
        }
        assert!(!managed.join("repo").exists());
    }

    #[test]
    fn a_filter_picks_one_skill_and_a_missing_one_is_said() {
        let base = tempfile::tempdir().unwrap();
        let source = base.path().join("src");
        write(&source.join("a/SKILL.md"), "---\nname: a\n---\n");
        write(&source.join("b/SKILL.md"), "---\nname: b\n---\n");
        let managed = base.path().join("home/.xfx/skills");
        let absolute = source.to_string_lossy().into_owned();
        assert_eq!(
            install(&managed, &absolute, Some("b"), base.path()),
            "Installed: b"
        );
        assert!(!managed.join("a").exists());
        assert_eq!(
            install(&managed, &absolute, Some("c"), base.path()),
            "Skill 'c' not found in the repository."
        );
        assert_eq!(
            install(&managed, &format!("{absolute}/a"), None, base.path()),
            "Installed: a"
        );
        let empty = base.path().join("empty");
        fs::create_dir_all(&empty).unwrap();
        assert_eq!(
            install(&managed, &empty.to_string_lossy(), None, base.path()),
            "No skills found (no SKILL.md files)."
        );
    }

    #[test]
    fn pasted_install_forms_are_read_as_upstreams() {
        assert_eq!(
            normalize("npx skills add owner/repo --skill one", None),
            Ok(("owner/repo".to_string(), Some("one".to_string())))
        );
        assert_eq!(
            normalize("https://skills.sh/owner/repo/two", None),
            Ok(("owner/repo".to_string(), Some("two".to_string())))
        );
        assert_eq!(
            normalize("owner/repo@three", None),
            Ok(("owner/repo".to_string(), Some("three".to_string())))
        );
        assert_eq!(normalize("owner/repo@a", Some("b")), Err(()));
        assert_eq!(normalize("  ", None), Err(()));
        assert_eq!(
            normalize("./local@x", None),
            Ok(("./local@x".to_string(), None))
        );
    }
}
