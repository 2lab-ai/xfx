//! Whether a write lands where a protected entry's links lead.
//!
//! [`super::MutationPlan::sensitive_auto_write_target`] names a file auto does
//! not change by the components of its own path. A link defeats that: with
//! `~/.zshenv` a link to `~/dotfiles/zshenv`, or `~/.vim/pack/dev/start/x` a
//! link to a plugin under development, the written path names nothing
//! protected and the next shell or editor runs exactly the bytes written. So
//! auto also asks whether any protected entry *leads* to the target.
//!
//! # Identities, not spellings
//!
//! Two paths to one file need not be one string: a non-ASCII case variant or
//! a decomposed name on a volume that folds them, macOS's
//! `/System/Volumes/Data` spelling of the same directory, a bind mount. So
//! nothing here compares paths. What is compared is the object each path
//! opens -- its device and inode, as `stat` reports them, following links:
//!
//! - every protected entry below every base (the home directory, every
//!   authorized root, and `$XDG_CONFIG_HOME` / `$ZDOTDIR` when the front end
//!   names them) is opened and recorded by identity, and so is every file
//!   git's global configuration (`~/.gitconfig`, `~/.config/git/config`,
//!   `$XDG_CONFIG_HOME/git/config`) includes, recursively
//!   ([`super::gitconfig`]) -- an included file is that configuration, and a
//!   `filter.<driver>.clean` written there runs on the user's next `git
//!   status`;
//! - every protected entry that is a directory is listed, recursively, and
//!   every directory and every link target found in it is recorded too -- a
//!   link to a directory is listed in its turn, and a directory once;
//! - the write is held when the target, or any directory above it that
//!   exists, is one of the recorded objects.
//!
//! A protected path that is not there yet -- `~/.config/fish` with `~/.config`
//! a link to `repo/config`, or a link that dangles -- has no identity. It is
//! recorded as its nearest directory that exists, by identity, plus the names
//! still missing below it, and the write is held when it would create those
//! names in that directory. Only those names are compared as strings, folded
//! as a case-insensitive volume folds them ([`crate::workspace::fold_case`]),
//! and only when they are ASCII: a missing name with anything else in it
//! cannot be compared without the volume's own normalization, so the check
//! asks.
//!
//! A plain file found inside a protected directory is not recorded on its
//! own (its directory is): the write tools replace a file by renaming a new
//! one over it, so a write cannot reach a protected file through a hard link
//! to it, nor onto a file bind mount (the rename fails).
//!
//! # Failing closed
//!
//! Only a lookup that answers *not there* (`ENOENT`, or `ENOTDIR` below a
//! file) clears anything. A directory that cannot be searched or listed, a
//! link that cannot be read, a loop (`ELOOP`, or more than [`MAX_LINK_HOPS`]
//! dangling links in a row), more than [`MAX_WALK_ENTRIES`] entries listed in
//! one check, or a target whose own directories cannot be inspected: xfx
//! cannot say, and auto asks.
//!
//! # The bound, and why there is no cache
//!
//! The check runs when auto is about to admit the write, every time, so
//! nothing it finds can be stale. [`MAX_WALK_ENTRIES`] is per check, across
//! every base and entry: 65,536 directory entries, about a tenth of a second
//! to list on an APFS volume (53,000 entries measured at 80 ms). A home whose
//! protected directories hold more -- an Emacs with every package's checkout
//! under `~/.emacs.d` -- makes auto ask for every write, which is the safe
//! direction. A cache would be cheaper and would have to know when a link was
//! added below `~/.vim`, which no single `mtime` says.

use std::collections::{HashSet, VecDeque};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use super::authority::SENSITIVE_AUTO_WRITE_SEQUENCES;

/// The most directory entries one check lists before it gives up and asks.
pub(crate) const MAX_WALK_ENTRIES: usize = 65_536;

/// How many dangling links in a row one resolution follows; a chain that
/// resolves is followed by the kernel, which stops at its own bound (`ELOOP`).
pub(crate) const MAX_LINK_HOPS: u8 = 32;

/// Why auto holds a write to `target`, as a clause that finishes "because",
/// when the target is inside a git directory, when a protected entry or a
/// configuration file the user's git reads leads to it, or when xfx could
/// not tell; `None` when every one was looked at and none does.
pub(crate) fn link_hold(target: &Path, check: &LinkCheck) -> Option<String> {
    hold_within(target, check, MAX_WALK_ENTRIES)
}

/// Where a write is checked against: the bases protected entries are looked
/// for below, the authorized roots git is asked about, and what the user's
/// git is told about its configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LinkCheck {
    pub bases: Vec<Base>,
    pub roots: Vec<PathBuf>,
    pub git: super::GitEnvironment,
}

/// A directory protected entries are looked for below.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Base {
    /// The home directory: every entry, as listed, and what git's global
    /// configuration there (`.gitconfig`, `.config/git/config`) includes.
    Home(PathBuf),
    /// A root, or `$ZDOTDIR`: every entry, as listed.
    Root(PathBuf),
    /// A directory standing for `~/.config` -- `$XDG_CONFIG_HOME` -- where
    /// the entries under `.config` are found without that component, and
    /// what `git/config` there includes.
    Config(PathBuf),
}

impl Base {
    /// The bases the process's environment names besides the home
    /// directory: `$XDG_CONFIG_HOME`, which fish, git, neovim and tmux read in
    /// place of `~/.config`, and `$ZDOTDIR`, where zsh reads its startup files
    /// in place of `~`. Each only when it is an absolute path. For a front end
    /// to hand its tools ([`crate::tools::ToolContext::with_link_bases`]).
    pub fn from_environment() -> Vec<Self> {
        let absolute = |name: &str| {
            std::env::var_os(name)
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
        };
        absolute("XDG_CONFIG_HOME")
            .map(Self::Config)
            .into_iter()
            .chain(absolute("ZDOTDIR").map(Self::Root))
            .collect()
    }

    /// Every protected entry below this base, and whether it is a `.git`.
    fn entries(&self) -> Vec<(PathBuf, bool)> {
        let (base, config) = match self {
            Self::Home(base) | Self::Root(base) => (base, false),
            Self::Config(base) => (base, true),
        };
        SENSITIVE_AUTO_WRITE_SEQUENCES
            .iter()
            .filter_map(|sequence| {
                let parts = if config {
                    sequence.strip_prefix(&[".config"][..])?
                } else {
                    sequence
                };
                Some((
                    parts
                        .iter()
                        .fold(base.clone(), |path, part| path.join(part)),
                    parts == [".git"],
                ))
            })
            .collect()
    }
}

/// Directories inside a git directory that hold data git never runs or
/// reads as configuration -- objects, references, logs, large-file stores --
/// and are not listed for links, so that a real repository's size does not
/// count against [`MAX_WALK_ENTRIES`].
const GIT_DATA_DIRECTORIES: &[&str] = &["objects", "refs", "logs", "lfs", "annex", "rr-cache"];

fn hold_within(target: &Path, check: &LinkCheck, limit: usize) -> Option<String> {
    let chain = match Chain::of(target) {
        Ok(chain) => chain,
        Err(why) => {
            return Some(format!(
                "xfx could not check whether a protected entry leads to it: {why}"
            ))
        }
    };
    if let Some(git_dir) = chain.git_dir.clone() {
        return Some(format!(
            "it is inside the git directory `{}`, whose config, hooks and attributes decide what git runs",
            git_dir.display()
        ));
    }
    if let Some(directory) = chain.completes.clone() {
        return Some(format!(
            "it would make `{}` a git directory, whose config, hooks and attributes decide what git runs",
            directory.display()
        ));
    }
    let mut walk = Walk {
        chain,
        limit,
        left: limit,
        queue: VecDeque::new(),
        listed: HashSet::new(),
    };
    walk.run(check).err()
}

/// A filesystem object as the volume names it: device and inode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Identity {
    device: u64,
    inode: u64,
}

impl Identity {
    fn of(metadata: &std::fs::Metadata) -> Self {
        use std::os::unix::fs::MetadataExt;
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
        }
    }
}

/// Where a protected path leads.
#[derive(Debug, PartialEq, Eq)]
enum Lead {
    /// Nothing can be there: the path is below a file.
    Absent,
    /// An object that is there, and a path that opens it.
    Node {
        identity: Identity,
        directory: bool,
        path: PathBuf,
    },
    /// Names not there yet, folded, below the directory `parent`.
    Missing {
        parent: Identity,
        names: Vec<String>,
    },
}

/// The target's directories that exist, each with the target's names below
/// it, folded; and the first of them that is a git directory, whatever it is
/// called -- a bare repository, `.git/modules/<name>`, the directory a `.git`
/// file names -- where every file decides what git runs.
struct Chain {
    links: Vec<(Identity, Vec<String>)>,
    git_dir: Option<PathBuf>,
    /// The directory the write would make a git directory, if it would.
    completes: Option<PathBuf>,
}

impl Chain {
    fn of(target: &Path) -> Result<Self, String> {
        let parts: Vec<&std::ffi::OsStr> = target
            .components()
            .filter_map(|component| match component {
                std::path::Component::Normal(name) => Some(name),
                _ => None,
            })
            .collect();
        let names: Vec<String> = parts
            .iter()
            .map(|name| fold(&name.to_string_lossy()))
            .collect();
        let mut links = Vec::new();
        let mut git_dir = None;
        let mut completes = None;
        let mut path = PathBuf::from("/");
        for depth in 0..=parts.len() {
            if depth > 0 {
                path.push(parts[depth - 1]);
            }
            match std::fs::metadata(&path) {
                Ok(metadata) => {
                    links.push((Identity::of(&metadata), names[depth..].to_vec()));
                    if git_dir.is_none()
                        && metadata.is_dir()
                        && super::git_attributes::is_git_dir(&path)?
                    {
                        git_dir = Some(path.clone());
                    }
                }
                Err(err)
                    if matches!(err.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) =>
                {
                    // `names[depth - 1]` is the first entry the write
                    // creates, in the directory that exists above it.
                    if let Some(parent) = path.parent().filter(|_| depth > 0) {
                        if completes_git_dir(parent, &names[depth - 1])? {
                            completes = Some(parent.to_path_buf());
                        }
                    }
                    break;
                }
                Err(err) => return Err(format!("`{}` cannot be inspected: {err}", path.display())),
            }
        }
        Ok(Self {
            links,
            git_dir,
            completes,
        })
    }

    /// Whether `lead` is the target or a directory above it, or names the
    /// target among what it would create.
    fn reached_by(&self, lead: &Lead) -> bool {
        match lead {
            Lead::Absent => false,
            Lead::Node { identity, .. } => self.links.iter().any(|(object, _)| object == identity),
            Lead::Missing { parent, names } => self
                .links
                .iter()
                .any(|(object, below)| object == parent && below.starts_with(names)),
        }
    }
}

struct Walk {
    chain: Chain,
    limit: usize,
    left: usize,
    /// Directories still to list, each with the protected entry it is part
    /// of and whether it is inside a git directory.
    queue: VecDeque<(PathBuf, String, bool)>,
    listed: HashSet<Identity>,
}

impl Walk {
    fn run(&mut self, check: &LinkCheck) -> Result<(), String> {
        for base in &check.bases {
            for (entry, git) in base.entries() {
                let name = entry.display().to_string();
                let lead =
                    resolve(&entry, MAX_LINK_HOPS).map_err(|why| cannot_check(&name, &why))?;
                self.reach(lead, &name, &name, git)?;
            }
        }
        // Every file the user's git reads as configuration
        // ([`super::gitconfig`]).
        let home = check.bases.iter().find_map(|base| match base {
            Base::Home(home) => Some(home.as_path()),
            _ => None,
        });
        let xdg = check.bases.iter().find_map(|base| match base {
            Base::Config(config) => Some(config.as_path()),
            _ => None,
        });
        let configuration = "the user's git configuration";
        let files = super::gitconfig::configuration_files(&check.roots, home, xdg, &check.git)
            .map_err(|why| cannot_check(configuration, &why))?;
        for file in files {
            let via = file.display().to_string();
            let lead = resolve(&file, MAX_LINK_HOPS)
                .map_err(|why| cannot_check(configuration, &format!("`{via}`: {why}")))?;
            self.reach(lead, configuration, &via, false)?;
        }
        while let Some((directory, entry, git)) = self.queue.pop_front() {
            self.list(&directory, &entry, git)?;
        }
        Ok(())
    }

    /// `via`, part of the protected `entry`, leads to `lead`: the write is
    /// held when that reaches the target, and a directory is queued to be
    /// listed for links of its own.
    fn reach(&mut self, lead: Lead, entry: &str, via: &str, git: bool) -> Result<(), String> {
        if self.chain.reached_by(&lead) {
            return Err(if via == entry {
                format!("`{entry}` leads to it and decides what runs later")
            } else {
                format!("`{via}` leads to it, and `{entry}` decides what runs later")
            });
        }
        if let Lead::Node {
            identity,
            directory: true,
            path,
        } = lead
        {
            if self.listed.insert(identity) {
                self.queue.push_back((path, entry.to_string(), git));
            }
        }
        Ok(())
    }

    fn list(&mut self, directory: &Path, entry: &str, git: bool) -> Result<(), String> {
        let unlistable = |err: std::io::Error| {
            cannot_check(
                entry,
                &format!("`{}` cannot be listed: {err}", directory.display()),
            )
        };
        for item in std::fs::read_dir(directory).map_err(unlistable)? {
            let item = item.map_err(unlistable)?;
            if self.left == 0 {
                return Err(cannot_check(
                    entry,
                    &format!(
                        "listing its directories took more than {} entries",
                        self.limit
                    ),
                ));
            }
            self.left -= 1;
            let path = item.path();
            let uninspectable = |err: std::io::Error| {
                cannot_check(
                    entry,
                    &format!("`{}` cannot be inspected: {err}", path.display()),
                )
            };
            let kind = item.file_type().map_err(uninspectable)?;
            if kind.is_symlink() {
                let via = path.display().to_string();
                let lead = resolve(&path, MAX_LINK_HOPS)
                    .map_err(|why| cannot_check(entry, &format!("`{via}`: {why}")))?;
                self.reach(lead, entry, &via, git)?;
            } else if kind.is_dir() {
                if git
                    && GIT_DATA_DIRECTORIES
                        .iter()
                        .any(|data| item.file_name() == *data)
                {
                    continue;
                }
                // Recorded too: a bind mount of a directory inside a protected
                // one is that directory, under another name.
                let metadata = item.metadata().map_err(uninspectable)?;
                let lead = Lead::Node {
                    identity: Identity::of(&metadata),
                    directory: true,
                    path: path.clone(),
                };
                self.reach(lead, entry, &path.display().to_string(), git)?;
            }
        }
        Ok(())
    }
}

/// Whether creating `name` (folded) in `directory` would complete a git
/// directory by the test [`super::git_attributes::is_git_dir`] makes -- a
/// `HEAD` beside `objects` or `commondir`, or either of those beside a
/// `HEAD`.
///
/// Each piece alone is an ordinary write, so a directory can be assembled one
/// write at a time -- `config` with a hook in it, `refs/`, `objects/`, then
/// `HEAD` -- and once it is whole, the user's git reads that `config` as a
/// repository's whenever it runs in or below it. Git's own test
/// (`is_git_directory`) needs `HEAD`, `objects` and `refs`, so this one is met
/// no later than git's; and a write inside a directory that already meets it
/// asks anyway ([`Chain::git_dir`]). Which piece comes last does not matter:
/// the write that adds it is this one.
fn completes_git_dir(directory: &Path, name: &str) -> Result<bool, String> {
    // Not followed: a link that dangles is a piece too -- git reads `HEAD`
    // without following it -- and counting one only ever asks more.
    let has = |entry: &str| match std::fs::symlink_metadata(directory.join(entry)) {
        Ok(_) => Ok(true),
        Err(err) if matches!(err.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) => {
            Ok(false)
        }
        Err(err) => Err(format!(
            "`{}` cannot be inspected: {err}",
            directory.join(entry).display()
        )),
    };
    match name {
        "head" => Ok(has("objects")? || has("commondir")?),
        "objects" | "commondir" => has("HEAD"),
        _ => Ok(false),
    }
}

fn cannot_check(entry: &str, why: &str) -> String {
    format!("xfx could not check whether `{entry}` leads to it: {why}")
}

/// `name` as a case-insensitive volume compares it with an ASCII name.
fn fold(name: &str) -> String {
    crate::workspace::fold_case(name).to_ascii_lowercase()
}

/// Where `path` leads, or why that cannot be said.
fn resolve(path: &Path, hops: u8) -> Result<Lead, String> {
    match std::fs::metadata(path) {
        Ok(metadata) => Ok(Lead::Node {
            identity: Identity::of(&metadata),
            directory: metadata.is_dir(),
            path: path.to_path_buf(),
        }),
        Err(err) if err.kind() == ErrorKind::NotFound => missing(path, hops),
        // Below a file: nothing can be there, and nothing can be created.
        Err(err) if err.kind() == ErrorKind::NotADirectory => Ok(Lead::Absent),
        Err(err) => Err(format!("`{}` cannot be inspected: {err}", path.display())),
    }
}

/// `path` opens nothing: it is a link that dangles, or a name not there yet
/// -- whose nearest directory that exists decides where it would be.
fn missing(path: &Path, hops: u8) -> Result<Lead, String> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            if hops == 0 {
                return Err(format!("more than {MAX_LINK_HOPS} dangling links in a row"));
            }
            let link = std::fs::read_link(path)
                .map_err(|err| format!("`{}` cannot be read: {err}", path.display()))?;
            // Joined, not normalized: the kernel resolves a `..` in the part
            // that exists, through any link in it, and a `..` in the part that
            // does not is refused below.
            let pointed = match path.parent() {
                Some(parent) if link.is_relative() => parent.join(link),
                _ => link,
            };
            resolve(&pointed, hops - 1)
        }
        Ok(_) => Err(format!(
            "`{}` changed while it was inspected",
            path.display()
        )),
        Err(err) if err.kind() == ErrorKind::NotFound => {
            let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
                return Err(format!("`{}` names no entry", path.display()));
            };
            let name = fold(&name.to_string_lossy());
            if !name.is_ascii() {
                return Err(format!(
                    "`{}` is not there yet, and a name that is not ASCII cannot be compared without the volume",
                    path.display()
                ));
            }
            match resolve(parent, hops)? {
                Lead::Absent
                | Lead::Node {
                    directory: false, ..
                } => Ok(Lead::Absent),
                Lead::Node { identity, .. } => Ok(Lead::Missing {
                    parent: identity,
                    names: vec![name],
                }),
                Lead::Missing { parent, mut names } => {
                    names.push(name);
                    Ok(Lead::Missing { parent, names })
                }
            }
        }
        Err(err) if err.kind() == ErrorKind::NotADirectory => Ok(Lead::Absent),
        Err(err) => Err(format!("`{}` cannot be inspected: {err}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::TempDir::new().expect("a tree");
        let root = dir.path().canonicalize().expect("canonical");
        (dir, root)
    }

    /// A check with these bases, no roots, and no git environment.
    fn check(bases: Vec<Base>) -> LinkCheck {
        LinkCheck {
            bases,
            ..LinkCheck::default()
        }
    }

    fn identity(path: &Path) -> Identity {
        Identity::of(&std::fs::metadata(path).expect("there"))
    }

    #[test]
    fn a_walk_past_its_bound_asks_rather_than_clearing_the_write() {
        let (_home_dir, home) = tree();
        let (_repo_dir, repo) = tree();
        std::fs::create_dir_all(home.join(".vim/plugin")).expect(".vim");
        for name in ["a.vim", "b.vim", "c.vim", "d.vim"] {
            std::fs::write(home.join(".vim/plugin").join(name), "").expect("a plugin");
        }
        let target = repo.join("notes.txt");
        let bases = check(vec![Base::Home(home.clone())]);
        // Under the bound, a tree with no link in it is no reason to ask.
        assert_eq!(hold_within(&target, &bases, 16), None);
        let hold = hold_within(&target, &bases, 3).expect("held");
        assert!(hold.contains("could not check"), "{hold}");
        assert!(hold.contains("more than 3 entries"), "{hold}");
    }

    #[test]
    fn a_dangling_link_leads_where_it_points_through_missing_directories() {
        let (_home_dir, home) = tree();
        let (_repo_dir, repo) = tree();
        std::os::unix::fs::symlink(repo.join("a/b/zshenv"), home.join(".zshenv")).expect("link");
        assert_eq!(
            resolve(&home.join(".zshenv"), MAX_LINK_HOPS),
            Ok(Lead::Missing {
                parent: identity(&repo),
                names: vec!["a".into(), "b".into(), "zshenv".into()],
            })
        );
        let bases = check(vec![Base::Home(home.clone())]);
        let hold = hold_within(&repo.join("a/b/zshenv"), &bases, 64).expect("held");
        assert!(hold.contains("decides what runs later"), "{hold}");
        // Spelled as the link spells it, but folded as the volume would.
        assert!(hold_within(&repo.join("A/B/ZSHENV"), &bases, 64).is_some());
        assert_eq!(hold_within(&repo.join("a/zshenv"), &bases, 64), None);
        // Below a file, nothing can be created, so nothing is there.
        std::fs::write(home.join(".config"), "").expect("a file");
        assert_eq!(
            resolve(&home.join(".config/fish"), MAX_LINK_HOPS),
            Ok(Lead::Absent)
        );
    }

    #[test]
    fn a_missing_name_that_is_not_ascii_is_not_guessed_at() {
        let (_home_dir, home) = tree();
        let (_repo_dir, repo) = tree();
        std::os::unix::fs::symlink(repo.join("\u{AC1C}\u{BC1C}/zshenv"), home.join(".zshenv"))
            .expect("link");
        let hold =
            hold_within(&repo.join("notes.txt"), &check(vec![Base::Home(home)]), 64).expect("held");
        assert!(hold.contains("not ASCII"), "{hold}");
    }

    #[test]
    fn a_git_configuration_xfx_cannot_read_for_its_includes_asks() {
        let (_home_dir, home) = tree();
        let (_repo_dir, repo) = tree();
        let target = repo.join("notes.txt");
        let bases = check(vec![Base::Home(home.clone())]);
        for config in [
            "[include]\n\tpath = \"open\n",
            "[include]\n\tpath = ~someone/x\n",
            "[include]\n\tpath = %(prefix)/etc/x\n",
        ] {
            std::fs::write(home.join(".gitconfig"), config).expect("a config");
            let hold = hold_within(&target, &bases, 64).expect("held");
            assert!(hold.contains("could not check"), "{config:?}: {hold}");
        }
        // Two files that include each other are read once each.
        std::fs::write(home.join(".gitconfig"), "[include]\n\tpath = ~/b\n").expect("a");
        std::fs::write(home.join("b"), "[include]\n\tpath = ~/.gitconfig\n").expect("b");
        assert_eq!(hold_within(&target, &bases, 64), None);
        assert!(hold_within(&home.join("b"), &bases, 64).is_some());
    }

    #[test]
    fn the_config_base_stands_for_dot_config() {
        let (_xdg_dir, xdg) = tree();
        let (_repo_dir, repo) = tree();
        std::fs::create_dir_all(repo.join("fish")).expect("fish");
        std::os::unix::fs::symlink(repo.join("fish"), xdg.join("fish")).expect("link");
        let hold = hold_within(
            &repo.join("fish/config.fish"),
            &check(vec![Base::Config(xdg.clone())]),
            64,
        )
        .expect("held");
        assert!(hold.contains("fish` leads to it"), "{hold}");
        // `.zshrc` is not below `.config`, so a config base has no such entry.
        std::os::unix::fs::symlink(repo.join("zshrc"), xdg.join(".zshrc")).expect("link");
        assert_eq!(
            hold_within(&repo.join("zshrc"), &check(vec![Base::Config(xdg)]), 64),
            None
        );
    }

    #[test]
    fn the_target_is_matched_by_what_the_volume_opens() {
        let (_home_dir, home) = tree();
        let (_repo_dir, repo) = tree();
        std::fs::create_dir_all(repo.join("Dotfiles")).expect("dotfiles");
        std::fs::write(repo.join("Dotfiles/VimRC"), "").expect("vimrc");
        std::fs::create_dir_all(home.join(".vim")).expect(".vim");
        std::os::unix::fs::symlink(repo.join("Dotfiles/VimRC"), home.join(".vim/vimrc"))
            .expect("link");
        let bases = check(vec![Base::Home(home.clone())]);
        // The link's own spelling.
        let hold = hold_within(&repo.join("Dotfiles/VimRC"), &bases, 64).expect("held");
        assert!(hold.contains("vimrc` leads to it"), "{hold}");
        // Another spelling of the same file, where the volume folds case.
        if repo.join("dotfiles").exists() {
            assert!(hold_within(&repo.join("dotfiles/vimrc"), &bases, 64).is_some());
        }
        // A file beside it is another object.
        assert_eq!(hold_within(&repo.join("Dotfiles/other"), &bases, 64), None);
    }
}
