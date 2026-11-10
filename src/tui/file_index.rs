//! What the `@` file picker can offer: the workspace's files and directories,
//! found on a worker thread, and the rows one query makes of them.
//!
//! Upstream's index (`vercel-labs/fx@c1db9191`,
//! `src/core/workspace/file_index.zig`), cut down to the half a picker reads.
//! Three things are its rules and are kept exactly:
//!
//! * **What is in it** (`discoverScopeCandidates`, `file_index.zig:907-993`).
//!   Inside a git work tree the files are what `git ls-files --cached --others
//!   --exclude-standard` lists -- tracked files, and untracked ones the
//!   project's ignore rules admit -- and the directories are every directory
//!   under the root that `git ls-files --others --ignored --directory` does not
//!   name as ignored (`workspace_files.zig:180-192`). Outside one, both are a
//!   walk that skips the always-ignored names (`ignored_dirs.zig:3-12`, the same
//!   list as [`crate::workspace::path::IGNORED_DIRECTORY_NAMES`]) and does not
//!   descend a symlink. Hidden names are in either way, `.git` never is, a
//!   tracked file that no longer exists is dropped, and so is any path that
//!   could not be painted as it stands ([`is_terminal_safe`]).
//! * **How much** (`file_index.zig:25-26@481104fd`): at most [`MOST_INDEXED`]
//!   entries, none longer than [`MOST_PATH_BYTES`], files and directories merged
//!   in byte order. A query shows at most [`MOST_RESULTS`] of them
//!   (`file_completion_state.zig:7@481104fd`).
//! * **How a query ranks them** (`choosePlacement` and `scoreBetter`,
//!   `file_index.zig:1776-1819`, `:1884-1890@481104fd`): a case-blind
//!   subsequence, placed where fzf's v2 path score is highest -- inside the
//!   name, or from the folder holding it into the name, or, only failing
//!   both, anywhere -- and ranked by four facts in a fixed order: the query
//!   *is* the path, the name or the name before a dot; the placement ends in
//!   the name; its score; where it starts. Then the shorter path, then byte
//!   order. The bold letters are that placement. An empty query is the
//!   index's own order.
//!
//! **The walk never runs on the thread that paints** ([`Files`]): a
//! `git ls-files` on a large tree or a slow disk would freeze the band. The
//! index is built on a worker thread, which discovers every entry and then
//! publishes them a prefix at a time, in their final order (`loadGeneration`
//! and `fillProgressive`, `file_index.zig:790-842` and `:172-230`). The first
//! index is searched as it grows -- rows fill in while it is published, and a
//! query nothing published yet matches says the picker is indexing (`count > 0`
//! or else the source's state, `input_completion_runtime.zig:838-842`). Once an
//! index has landed, a refresh stays private until it is whole, and the
//! previous one is searched meanwhile (`searchableGeneration`,
//! `file_index.zig:647-654@481104fd`) -- upstream's "keeps the completed result visible
//! through rapid large-index refreshes" (`tests/e2e/tui-file-picker.test.ts:471`).
//! A directory listing for a path the user typed (`@src/`, `@~/`, `@../`) is
//! read on a worker thread too (`directory_completion_job.zig`), once per
//! directory, and the query is ranked against it here.

use std::cmp::Ordering;
use std::io::Read;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering as Atomic};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::Arc;

use unicode_segmentation::UnicodeSegmentation;

use crate::workspace::path::is_ignored_directory;

/// The most entries an index holds (`max_indexed_files`, `file_index.zig:25@481104fd`).
pub(crate) const MOST_INDEXED: usize = 100_000;

/// The longest path an index holds (`max_path_len`, `file_index.zig:26@481104fd`).
pub(crate) const MOST_PATH_BYTES: usize = 2048;

/// The most rows one query yields (`capacity`, `file_completion_state.zig:7@481104fd`).
pub(crate) const MOST_RESULTS: usize = 32;

/// The most a `git ls-files` may print before it is not believed
/// (`default_git_stdout_limit`, `workspace_files.zig:11`).
const MOST_GIT_BYTES: usize = 32 * 1024 * 1024;

/// Where a git that is trusted to list files lives (`trustedGitExecutable`,
/// `workspace_files.zig:258-279`): a fixed list rather than `PATH`, so a
/// workspace cannot put a `git` of its own in front of the one the user meant.
const TRUSTED_GIT: &[&str] = &[
    "/usr/bin/git",
    "/bin/git",
    "/usr/local/bin/git",
    "/opt/homebrew/bin/git",
    "/opt/local/bin/git",
    "/run/current-system/sw/bin/git",
];

/// Whether an entry is a file or a directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Kind {
    File,
    Directory,
}

/// One entry of the index: a path relative to the workspace, `/`-separated,
/// with no trailing separator on a directory -- and what upstream works out
/// for it once, as it builds the index, rather than on every keystroke
/// (`fillProgressive`, `file_index.zig:209-222@481104fd`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Candidate {
    /// Private: the facts below are this path's, so nothing outside this
    /// module may change it (no other module reads it either).
    path: String,
    pub(crate) kind: Kind,
    /// The path's bytes, ASCII letters lower-cased (`lower_buf`).
    lower: Box<[u8]>,
    /// The name's first byte, after the last `/` (`basename_starts`).
    name: usize,
    /// [`letters`] of the path, and [`NON_ASCII`] if it has a byte past
    /// ASCII (`char_masks`).
    mask: u32,
    /// [`letters`] of the name, and of the folder holding it and the name:
    /// where the search inside the name and the one from its folder into it
    /// look. Not upstream's: they spare those two searches a path whose
    /// part lacks one of the query's letters (an ASCII path's only).
    name_mask: u32,
    folder_mask: u32,
}

impl Candidate {
    /// The only way to make one, so its facts are always its own path's.
    pub(crate) fn new(path: String, kind: Kind) -> Self {
        let lower: Box<[u8]> = path.bytes().map(|byte| byte.to_ascii_lowercase()).collect();
        let mask = letters(&lower) | if path.is_ascii() { 0 } else { NON_ASCII };
        let name = basename_start(&path);
        // The folder holding the name starts after the `/` before it, as
        // `choose_placement` finds it.
        let folder = name
            .checked_sub(1)
            .and_then(|slash| u8::rfind(&lower, slash, b'/'))
            .map_or(0, |slash| slash + 1);
        Self {
            name_mask: letters(&lower[name..]),
            folder_mask: letters(&lower[folder..]),
            name,
            path,
            kind,
            lower,
            mask,
        }
    }
}

/// One row a query found: the path the picker shows and inserts, and the byte
/// ranges of it the query matched, which are painted bold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Found {
    pub(crate) path: String,
    pub(crate) kind: Kind,
    pub(crate) spans: Vec<Range<usize>>,
}

/// What one lookup came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Lookup {
    /// Rows, possibly none.
    Rows(Vec<Found>),
    /// Nothing to search yet: the first index, or the directory, is still
    /// being read.
    Loading,
    /// The index could not be built, or the directory could not be read.
    Unavailable,
}

/// Whether `text` can be painted as it stands (`isTerminalSafe`,
/// `text_utils.zig:411-421` and `:620-626`): no C0 or C1 control, no DEL, no
/// zero-width or direction-changing format character, no byte-order mark.
pub(crate) fn is_terminal_safe(text: &str) -> bool {
    text.chars().all(|scalar| {
        let code = u32::from(scalar);
        !(code < 0x20
            || code == 0x7f
            || (0x80..=0x9f).contains(&code)
            || (0x200b..=0x200f).contains(&code)
            || (0x2028..=0x202e).contains(&code)
            || (0x2060..=0x206f).contains(&code)
            || code == 0xfeff)
    })
}

/// Whether `query` names a path to browse rather than words to look up in the
/// index (`queryMode`, `path_completion.zig:25-27`): `~`, `.` and `..` on their
/// own, or anything with a `/` in it.
pub(crate) fn is_explicit(query: &str) -> bool {
    explicit(query).is_some()
}

/// A browsed path, split (`parseExplicitQuery`, `path_completion.zig:156-176`).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Explicit<'a> {
    /// The directory to list, as typed.
    parent: &'a str,
    /// What every row begins with: the query up to and including its last `/`.
    prefix: String,
    /// What the entries' names are ranked against.
    name: &'a str,
}

fn explicit(query: &str) -> Option<Explicit<'_>> {
    if matches!(query, "~" | "." | "..") {
        return Some(Explicit {
            parent: query,
            prefix: format!("{query}/"),
            name: "",
        });
    }
    let separator = query.rfind('/')?;
    let parent = if separator == 0 {
        &query[..1]
    } else {
        &query[..separator]
    };
    Some(Explicit {
        parent,
        prefix: query[..=separator].to_string(),
        name: &query[separator + 1..],
    })
}

/// The directory a browsed parent names: the home directory for `~`, itself
/// when absolute, and the workspace's otherwise
/// (`resolve_workspace_or_external_literal_path_with_home`, `pathing.zig:237-244`).
fn resolve(root: &Path, home: Option<&Path>, parent: &str) -> Option<PathBuf> {
    if parent == "~" {
        return home.map(Path::to_path_buf);
    }
    if let Some(rest) = parent.strip_prefix("~/") {
        return home.map(|home| home.join(rest));
    }
    if parent.starts_with('/') {
        return Some(PathBuf::from(parent));
    }
    Some(root.join(parent))
}

// ---------------------------------------------------------------------------
// discovery
// ---------------------------------------------------------------------------

/// The trusted git on this machine, if there is one.
pub(crate) fn trusted_git() -> Option<PathBuf> {
    TRUSTED_GIT
        .iter()
        .map(PathBuf::from)
        .find(|candidate| candidate.is_file())
}

/// Every entry the picker can offer under `root`, in byte order.
///
/// `git` is the executable to ask, or `None` to walk. `Err` is a work tree git
/// could not list -- the files of a repository are git's to say, and a walk in
/// their place would offer what the project ignores
/// (`git_worktree_is_authoritative`, `workspace_files.zig:100-109`) -- or a
/// stop that arrived first.
pub(crate) fn discover(
    root: &Path,
    git: Option<&Path>,
    stop: &AtomicBool,
) -> Result<Vec<Candidate>, ()> {
    let listed = match git {
        Some(git) => match ls_files(
            root,
            git,
            &["--cached", "--others", "--exclude-standard"],
            stop,
        ) {
            Some(raw) => Some((git, raw)),
            None if stop.load(Atomic::Relaxed) => return Err(()),
            None if has_git_metadata(root) => return Err(()),
            None => None,
        },
        None => None,
    };
    let (files, directories) = match listed {
        Some((git, raw)) => {
            let mut files = Vec::new();
            for path in raw.split('\0') {
                if files.len() >= MOST_INDEXED {
                    break;
                }
                if path.is_empty()
                    || path.split('/').any(|part| part == ".git")
                    || !admissible(path)
                {
                    continue;
                }
                files.push(path.to_string());
            }
            files.sort_unstable();
            // A tracked file the work tree no longer has is not something to
            // offer (`file_index.zig:960-966`).
            files.retain(|path| {
                std::fs::symlink_metadata(root.join(path))
                    .is_ok_and(|meta| meta.is_file() || meta.file_type().is_symlink())
            });
            let ignored = ls_files(
                root,
                git,
                &["--others", "--ignored", "--directory", "--exclude-standard"],
                stop,
            )
            .unwrap_or_default();
            let ignored: std::collections::HashSet<&str> = ignored
                .split('\0')
                .filter_map(|entry| entry.strip_suffix('/'))
                .map(|entry| entry.trim_end_matches('/'))
                .filter(|entry| !entry.is_empty())
                .collect();
            let mut directories = Vec::new();
            walk(root, stop, &mut |relative, is_directory| {
                if !is_directory {
                    return Visit::Skip;
                }
                if relative.rsplit('/').next() == Some(".git") || ignored.contains(relative) {
                    return Visit::Skip;
                }
                directories.push(relative.to_string());
                Visit::Descend
            })?;
            (files, directories)
        }
        None => {
            let mut files = Vec::new();
            let mut directories = Vec::new();
            walk(root, stop, &mut |relative, is_directory| {
                let name = relative.rsplit('/').next().unwrap_or(relative);
                if is_ignored_directory(name) {
                    return Visit::Skip;
                }
                if is_directory {
                    directories.push(relative.to_string());
                    Visit::Descend
                } else {
                    files.push(relative.to_string());
                    Visit::Skip
                }
            })?;
            files.retain(|path| admissible(path));
            files.sort_unstable();
            (files, directories)
        }
    };
    let mut directories: Vec<String> = directories
        .into_iter()
        .filter(|path| admissible(path))
        .collect();
    directories.sort_unstable();
    Ok(merged(files, directories))
}

/// Files and directories in one byte order, a file first where the two are
/// the same path (`nextSortedDiscoveredCandidate`, `file_index.zig:996-1011`),
/// and no more than [`MOST_INDEXED`] of them.
fn merged(files: Vec<String>, directories: Vec<String>) -> Vec<Candidate> {
    let mut out = Vec::with_capacity((files.len() + directories.len()).min(MOST_INDEXED));
    let mut files = files.into_iter().peekable();
    let mut directories = directories.into_iter().peekable();
    while out.len() < MOST_INDEXED {
        let take_file = match (files.peek(), directories.peek()) {
            (None, None) => break,
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (Some(file), Some(directory)) => file.as_bytes() <= directory.as_bytes(),
        };
        let (path, kind) = if take_file {
            (files.next(), Kind::File)
        } else {
            (directories.next(), Kind::Directory)
        };
        if let Some(path) = path {
            out.push(Candidate::new(path, kind));
        }
    }
    out
}

/// Whether a path may be an entry at all (`acceptedCandidate`,
/// `file_index.zig:1071-1078`).
fn admissible(path: &str) -> bool {
    !path.is_empty() && path.len() <= MOST_PATH_BYTES && is_terminal_safe(path)
}

/// What a walk does with one entry.
enum Visit {
    Descend,
    Skip,
}

/// Every entry under `root`, depth first, handed to `visit` with its path
/// relative to `root` and whether it is a directory. A symlink is never
/// descended (`walkWorkspacePaths`, `workspace_files.zig:476-566`); a
/// directory that cannot be read is passed over.
fn walk(
    root: &Path,
    stop: &AtomicBool,
    visit: &mut dyn FnMut(&str, bool) -> Visit,
) -> Result<(), ()> {
    let mut stack: Vec<(PathBuf, String)> = vec![(root.to_path_buf(), String::new())];
    let mut seen = 0usize;
    while let Some((directory, prefix)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        let mut below = Vec::new();
        for entry in entries.flatten() {
            if stop.load(Atomic::Relaxed) {
                return Err(());
            }
            // A bound on the reading as well as on what is kept: a workspace
            // rooted somewhere enormous -- a home directory -- is cut off
            // after this many entries rather than read to the end.
            seen += 1;
            if seen > MOST_INDEXED * 4 {
                return Ok(());
            }
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            let relative = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            if relative.len() > MOST_PATH_BYTES {
                continue;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let is_directory = kind.is_dir();
            if !is_directory && !kind.is_file() && !kind.is_symlink() {
                continue;
            }
            if let Visit::Descend = visit(&relative, is_directory) {
                below.push((entry.path(), relative));
            }
        }
        // Reversed onto the stack, so the walk takes them in the order they
        // were read; the caller sorts what it keeps anyway.
        stack.extend(below.into_iter().rev());
    }
    Ok(())
}

/// `git ls-files -z <args>` run in `root`, or `None` when git said no, said
/// too much, or was stopped.
fn ls_files(root: &Path, git: &Path, args: &[&str], stop: &AtomicBool) -> Option<String> {
    // `core.fsmonitor` is a command `ls-files` runs from the repository's
    // configuration when it reads the index; the listing does not need it.
    let mut command = crate::process::command(git);
    command
        .args(["-c", "core.fsmonitor=false"])
        .arg("--no-optional-locks")
        .arg("ls-files")
        .arg("-z")
        .args(args)
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = crate::process::spawn(&mut command).ok()?;
    let mut raw = Vec::new();
    let read = child
        .stdout
        .take()
        .map(|stdout| stdout.take(MOST_GIT_BYTES as u64 + 1).read_to_end(&mut raw));
    let status = child.wait().ok()?;
    if stop.load(Atomic::Relaxed)
        || !status.success()
        || raw.len() > MOST_GIT_BYTES
        || !matches!(read, Some(Ok(_)))
    {
        return None;
    }
    String::from_utf8(raw).ok()
}

/// Whether `root` or a directory above it holds a `.git` (`hasGitMetadata`,
/// `workspace_files.zig:157-176`).
fn has_git_metadata(root: &Path) -> bool {
    root.ancestors()
        .any(|directory| std::fs::symlink_metadata(directory.join(".git")).is_ok())
}

/// The entries of the directory a browsed path names, as `(name, kind)`, or
/// nothing once `stop` is set.
///
/// Every entry whose name can be painted: hidden ones included, nothing
/// ignored -- the user typed the path (`completeCancellable`,
/// `path_completion.zig:44-120`). A symlink is what it points at, and one that
/// points nowhere is left out.
fn list_directory(directory: &Path, stop: &AtomicBool) -> Result<Vec<(String, Kind)>, ()> {
    list_directory_until(directory, || stop.load(Atomic::Relaxed))
}

/// [`list_directory`], asking `stopped` where fx's listing checks its cancel
/// flag (`checkCancellation`, `path_completion.zig:43-146@481104fd`): before
/// the folder is opened (`:75`), and for each entry before the advance
/// (`:89`), after it (`:91`) and before the entry's `stat` (`:95`).
/// Cooperative, as fx's is: a filesystem call already in flight -- the
/// `read_dir`, an advance, an entry's `metadata` -- runs to its end, and the
/// listing stops at the next question.
fn list_directory_until(
    directory: &Path,
    mut stopped: impl FnMut() -> bool,
) -> Result<Vec<(String, Kind)>, ()> {
    if stopped() {
        return Err(());
    }
    let entries = std::fs::read_dir(directory).map_err(|_| ())?;
    list_entries(
        entries.map(|entry| entry.map(|entry| (entry.file_name(), entry.path()))),
        stopped,
    )
}

/// The entries `entries` yields -- each a name and the path that reaches it,
/// or the error its advance failed with -- as [`list_directory_until`] takes
/// them: an iterator rather than the `read_dir` itself, so a case can make an
/// advance fail after entries that read.
fn list_entries(
    mut entries: impl Iterator<Item = std::io::Result<(std::ffi::OsString, PathBuf)>>,
    mut stopped: impl FnMut() -> bool,
) -> Result<Vec<(String, Kind)>, ()> {
    let mut out = Vec::new();
    loop {
        if stopped() {
            return Err(());
        }
        // An advance that fails fails the listing, as fx's does
        // (`iterator.next(io) catch return error.PathUnavailable`, `:90`):
        // the folder is unavailable, not a shorter one.
        let (name, path) = match entries.next() {
            None => break,
            Some(Ok(entry)) => entry,
            Some(Err(_)) => return Err(()),
        };
        if stopped() {
            return Err(());
        }
        let Ok(name) = name.into_string() else {
            continue;
        };
        if !is_terminal_safe(&name) {
            continue;
        }
        if stopped() {
            return Err(());
        }
        let Ok(meta) = std::fs::metadata(&path) else {
            continue;
        };
        let kind = if meta.is_dir() {
            Kind::Directory
        } else if meta.is_file() {
            Kind::File
        } else {
            continue;
        };
        out.push((name, kind));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// ranking
// ---------------------------------------------------------------------------

/// What a match is ranked on (`MatchScore`, `file_index.zig:1254-1265@481104fd`),
/// in the order [`score_order`] compares them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Score {
    /// The query is the path, the name, or the name up to a dot (`exactFit`).
    exact: bool,
    /// The name tier: the placement is inside the name or runs from the
    /// folder holding it into the name. Off for the last search over the
    /// whole path, which ranks below every name match even where that
    /// placement happens to end in the name (`ab/x/cd.ts` for `abcd`).
    name: bool,
    /// The placement's score, [`NAME_BONUS`] in it for one inside the name.
    quality: i32,
    /// Its first scalar, counted from where its search began.
    first: usize,
}

/// Whether `left` is the better match (`scoreBetter`, `file_index.zig:1884-1890`).
fn score_order(left: &Score, right: &Score) -> Ordering {
    right
        .exact
        .cmp(&left.exact)
        .then(right.name.cmp(&left.name))
        .then(right.quality.cmp(&left.quality))
        .then(left.first.cmp(&right.first))
}

/// The whole order: the score, then the shorter path, then bytes
/// (`rankedCandidateBetter`, `file_index.zig:1892-1899`; `NameQuery.better`,
/// `:1302-1309`, which skips the first two for an empty query).
fn better(left: (&Score, &str), right: (&Score, &str), query_empty: bool) -> Ordering {
    let ranked = if query_empty {
        Ordering::Equal
    } else {
        score_order(left.0, right.0).then(left.1.len().cmp(&right.1.len()))
    };
    ranked.then_with(|| left.1.as_bytes().cmp(right.1.as_bytes()))
}

// fzf's v2 path-scheme score, as fx keeps it (`file_index.zig:1410-1423`).
const MOST_ALIGN_CELLS: usize = 4096;
const RUN_BONUS: [i32; 4] = [4, 7, 8, 9];
const SCORE_MATCH: i32 = 16;
const GAP_START: i32 = -3;
const GAP_EXTENSION: i32 = -1;
const BONUS_BOUNDARY: i32 = SCORE_MATCH / 2;
const BONUS_NON_WORD: i32 = SCORE_MATCH / 2;
const BONUS_CAMEL_123: i32 = BONUS_BOUNDARY + GAP_EXTENSION;
const BONUS_CONSECUTIVE: i32 = -(GAP_START + GAP_EXTENSION);
const FIRST_CHAR_MULTIPLIER: i32 = 2;
const BONUS_BOUNDARY_WHITE: i32 = BONUS_BOUNDARY;
const BONUS_BOUNDARY_DELIMITER: i32 = BONUS_BOUNDARY + 1;
const INVALID: i32 = i32::MIN;
/// What a placement inside the name earns over one that starts in the
/// folder holding it (`name_bonus`, `file_index.zig:1722-1727`).
const NAME_BONUS: i32 = 4;

/// [`Candidate::mask`]'s bit for a path with a byte past ASCII
/// (`non_ascii_mask`, `file_index.zig:1175`).
const NON_ASCII: u32 = 1 << 31;

/// Bit `n` for each letter `'a' + n` in `bytes`, either case (`alphaMask`,
/// `file_index.zig:1164-1173`).
fn letters(bytes: &[u8]) -> u32 {
    bytes
        .iter()
        .map(u8::to_ascii_lowercase)
        .filter(u8::is_ascii_lowercase)
        .fold(0, |mask, letter| mask | 1 << (letter - b'a'))
}

/// Each byte's own offset: an ASCII path's scalars are its bytes, where
/// upstream passes no offsets at all (`bonusAt`, `file_index.zig:1477-1481`).
static IDENTITY: [usize; MOST_PATH_BYTES + 1] = {
    let mut offsets = [0; MOST_PATH_BYTES + 1];
    let mut at = 0;
    while at < offsets.len() {
        offsets[at] = at;
        at += 1;
    }
    offsets
};

/// The placements an entry may still need (`Needed`, `file_index.zig:1760-1768`).
#[derive(Clone, Copy)]
struct Needed {
    /// Those that end in a folder.
    folder: bool,
    /// Those that start in the folder holding the name and end in the name.
    crossing: bool,
}

/// Every placement: what a search outside [`rank_top`] places.
const EVERY: Needed = Needed {
    folder: true,
    crossing: true,
};

/// Whether the name, and the folder holding it with the name, may hold the
/// query: false only where an ASCII path's part lacks one of its letters
/// ([`Candidate::name_mask`]), so the search there would find nothing.
#[derive(Clone, Copy)]
struct Holds {
    name: bool,
    folder: bool,
}

/// Every part may hold the query: a path with a byte past ASCII, or a name.
const ANY: Holds = Holds {
    name: true,
    folder: true,
};

/// A byte's class (`CharClass`, `charClassOf`, `file_index.zig:1445-1457`).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Class {
    White,
    NonWord,
    Delimiter,
    Lower,
    Upper,
    Letter,
    Number,
}

fn class(byte: u8) -> Class {
    match byte {
        b'a'..=b'z' => Class::Lower,
        b'A'..=b'Z' => Class::Upper,
        b'0'..=b'9' => Class::Number,
        b' ' | b'\t' | b'\n' | b'\r' => Class::White,
        b'/' => Class::Delimiter,
        0x80..=0xff => Class::Letter,
        _ => Class::NonWord,
    }
}

/// What a character after `prev` earns (`bonusFor`, `file_index.zig:1459-1475`).
fn bonus_for(prev: Class, class: Class) -> i32 {
    use Class::*;
    if matches!(class, Lower | Upper | Letter | Number) {
        match prev {
            White => return BONUS_BOUNDARY_WHITE,
            Delimiter => return BONUS_BOUNDARY_DELIMITER,
            NonWord => return BONUS_BOUNDARY,
            _ => {}
        }
    }
    if (prev == Lower && class == Upper) || (prev != Number && class == Number) {
        return BONUS_CAMEL_123;
    }
    match class {
        NonWord | Delimiter => BONUS_NON_WORD,
        White => BONUS_BOUNDARY_WHITE,
        _ => 0,
    }
}

/// The bonus of scalar `position`, read from the path's own bytes: the case
/// as written, and the start a separator (`bonusAt`, `file_index.zig:1477-1481`).
fn bonus_at(path: &[u8], offsets: &[usize], position: usize) -> i32 {
    let at = offsets[position];
    let prev = if at == 0 {
        Class::Delimiter
    } else {
        class(path[at - 1])
    };
    bonus_for(prev, class(path[at]))
}

fn run_slot(run: i32) -> usize {
    match run.max(BONUS_CONSECUTIVE) {
        4 => 0,
        7 => 1,
        8 => 2,
        9 => 3,
        _ => unreachable!("a bonus is 0, 7, 8 or 9"),
    }
}

/// A character in a run: its bonus and the run's (`consecutiveBonus`,
/// `file_index.zig:1493-1496`).
fn consecutive_bonus(bonus: i32, run: i32) -> (i32, i32) {
    if bonus >= BONUS_BOUNDARY && bonus > run {
        return (bonus, bonus);
    }
    (bonus.max(run.max(BONUS_CONSECUTIVE)), run)
}

/// The score of one placement and its first scalar from `range_start`
/// (`scoreAlignment`, `file_index.zig:1498-1521`).
fn score_alignment(
    path: &[u8],
    offsets: &[usize],
    range_start: usize,
    positions: &[usize],
) -> (i32, usize) {
    let (mut total, mut run) = (0, 0);
    for (index, &position) in positions.iter().enumerate() {
        let bonus = bonus_at(path, offsets, position);
        if index == 0 {
            (total, run) = (SCORE_MATCH + bonus * FIRST_CHAR_MULTIPLIER, bonus);
        } else if position == positions[index - 1] + 1 {
            let (bonus, next) = consecutive_bonus(bonus, run);
            (total, run) = (total + SCORE_MATCH + bonus, next);
        } else {
            let gap = (position - positions[index - 1] - 1) as i32;
            total += SCORE_MATCH + bonus + GAP_START + (gap - 1) * GAP_EXTENSION;
            run = bonus;
        }
    }
    (total, positions[0] - range_start)
}

fn better_candidate(score: i32, start: u16, best: i32, best_start: u16) -> bool {
    score > best || (score == best && start < best_start)
}

/// What a placement is searched over: an ASCII path's lower-cased bytes, or
/// a path's folded scalars. Every scan of a search is one of these two, so
/// bytes are found with `memchr` -- the scans were half a keystroke's cost
/// over 100,000 entries -- and scalars one at a time.
trait Unit: Copy + PartialEq + From<u8> {
    /// The first of `text[from..]` that is `wanted`.
    fn find(text: &[Self], from: usize, wanted: Self) -> Option<usize>;
    /// The last of `text[..to]` that is `wanted`.
    fn rfind(text: &[Self], to: usize, wanted: Self) -> Option<usize>;
}

impl Unit for u8 {
    fn find(text: &[u8], from: usize, wanted: u8) -> Option<usize> {
        memchr::memchr(wanted, text.get(from..)?).map(|at| from + at)
    }

    fn rfind(text: &[u8], to: usize, wanted: u8) -> Option<usize> {
        memchr::memrchr(wanted, &text[..to])
    }
}

impl Unit for char {
    fn find(text: &[char], from: usize, wanted: char) -> Option<usize> {
        let at = text.get(from..)?.iter().position(|c| *c == wanted)?;
        Some(from + at)
    }

    fn rfind(text: &[char], to: usize, wanted: char) -> Option<usize> {
        text[..to].iter().rposition(|c| *c == wanted)
    }
}

/// One search's text and alignment workspace, reused for every path.
#[derive(Default)]
struct Scratch {
    /// The path's scalars, case-folded.
    text: Vec<char>,
    /// Each scalar's first byte, then the path's length.
    offsets: Vec<usize>,
    align: Align,
}

#[derive(Default)]
struct Align {
    /// Four run slots a cell, at upstream's widths (`AlignScratch`,
    /// `file_index.zig:1427-1438`). They grow to the largest table yet and
    /// are never cleared: a cell's scores are reset when the program reaches
    /// it (`file_index.zig:1634`), and a start or a predecessor is read only behind a
    /// score that was written with it -- any offer beats `INVALID` alone.
    scores: Vec<i32>,
    starts: Vec<u16>,
    pred_entries: Vec<u16>,
    pred_slots: Vec<u8>,
    columns: Vec<u16>,
    row_offsets: Vec<usize>,
    forward: Vec<usize>,
    backward: Vec<usize>,
    /// The placement found last.
    positions: Vec<usize>,
    /// How many searches [`align_best`] has run with this workspace, for a
    /// case to count the work a keystroke does.
    #[cfg(test)]
    searches: usize,
}

impl Scratch {
    fn load(&mut self, path: &str) {
        self.text.clear();
        self.offsets.clear();
        for (at, scalar) in path.char_indices() {
            self.text.push(fold(scalar));
            self.offsets.push(at);
        }
        self.offsets.push(path.len());
    }
}

impl Align {
    /// Keeps `score` in slot `at` if it beats what is there (`betterCandidate`,
    /// `file_index.zig:1523-1525`): higher, or as high and starting earlier.
    fn offer(&mut self, at: usize, score: i32, start: u16, entry: u16, from: u8) {
        if better_candidate(score, start, self.scores[at], self.starts[at]) {
            self.scores[at] = score;
            self.starts[at] = start;
            self.pred_entries[at] = entry;
            self.pred_slots[at] = from;
        }
    }
}

/// The best placement of `query` in `text[range_start..]` whose last scalar is
/// at or after `end_min`: its score and first scalar, the highest score and
/// then the earliest start, its positions in `align.positions`; past
/// [`MOST_ALIGN_CELLS`], the leftmost one (`alignBest`, `file_index.zig:1527-1720`).
/// `text` is an ASCII path's lower-cased bytes or a path's folded scalars.
fn align_best<T: Unit>(
    text: &[T],
    path: &[u8],
    offsets: &[usize],
    range_start: usize,
    end_min: usize,
    query: &[T],
    s: &mut Align,
) -> Option<(i32, usize)> {
    #[cfg(test)]
    {
        s.searches += 1;
    }
    let m = query.len();
    if m == 0 || range_start > text.len() || m > text.len() - range_start {
        return None;
    }
    s.positions.clear();
    if m == 1 && text.len() - end_min <= MOST_ALIGN_CELLS {
        // The earliest scalar with the highest bonus (`:1546-1564`).
        let (mut best, mut from): (Option<(usize, i32)>, usize) = (None, end_min);
        while let Some(j) = T::find(text, from, query[0]) {
            let bonus = bonus_at(path, offsets, j);
            if best.is_none_or(|(_, top)| bonus > top) {
                best = Some((j, bonus));
                if bonus == BONUS_BOUNDARY_DELIMITER {
                    break;
                }
            }
            from = j + 1;
        }
        let (only, bonus) = best?;
        s.positions.push(only);
        return Some((
            SCORE_MATCH + bonus * FIRST_CHAR_MULTIPLIER,
            only - range_start,
        ));
    }
    // The leftmost placement from `range_start`, and the rightmost.
    s.forward.clear();
    let mut from = range_start;
    for wanted in query {
        let at = T::find(text, from, *wanted)?;
        s.forward.push(at);
        from = at + 1;
    }
    s.backward.clear();
    s.backward.resize(m, 0);
    let mut to = text.len();
    for row in (0..m).rev() {
        to = T::rfind(text, to, query[row])?;
        s.backward[row] = to;
    }
    if s.backward[m - 1] < end_min {
        return None;
    }
    if s.forward[m - 1] < end_min {
        s.forward[m - 1] = T::find(text, end_min, query[m - 1])?;
    }
    let cells: usize = s
        .forward
        .iter()
        .zip(&s.backward)
        .map(|(a, b)| b - a + 1)
        .sum();
    if s.forward == s.backward || cells > MOST_ALIGN_CELLS {
        // One placement, or too many cells: the leftmost (`:1591-1604`).
        s.positions.extend_from_slice(&s.forward);
        return Some(score_alignment(path, offsets, range_start, &s.positions));
    }
    // One cell for each place `query[row]` matches between its two greedy
    // positions, four run slots each.
    s.columns.clear();
    s.row_offsets.clear();
    for (row, wanted) in query.iter().enumerate() {
        s.row_offsets.push(s.columns.len());
        let (first, last) = (s.forward[row], s.backward[row]);
        // A position fits u16: no indexed path passes `MOST_PATH_BYTES`.
        debug_assert!(last <= usize::from(u16::MAX));
        s.columns.extend(
            (first..=last)
                .filter(|at| text[*at] == *wanted)
                .map(|at| at as u16),
        );
    }
    s.row_offsets.push(s.columns.len());
    let slots = s.columns.len() * 4;
    if s.scores.len() < slots {
        s.scores.resize(slots, INVALID);
        s.starts.resize(slots, 0);
        s.pred_entries.resize(slots, 0);
        s.pred_slots.resize(slots, 0);
    }
    for row in 0..m {
        let prev = if row > 0 {
            s.row_offsets[row - 1]..s.row_offsets[row]
        } else {
            0..0
        };
        let (mut gap, mut gap_start, mut gap_entry, mut gap_slot) = (INVALID, 0, 0, 0);
        let (mut next_gap, mut diag) = (prev.start, prev.start);
        for entry in s.row_offsets[row]..s.row_offsets[row + 1] {
            let (j, cell) = (usize::from(s.columns[entry]), entry * 4);
            s.scores[cell..cell + 4].fill(INVALID);
            let bonus = bonus_at(path, offsets, j);
            if row == 0 {
                let slot = run_slot(bonus);
                s.scores[cell + slot] = SCORE_MATCH + bonus * FIRST_CHAR_MULTIPLIER;
                s.starts[cell + slot] = s.columns[entry];
                continue;
            }
            // Every predecessor at least two positions back.
            while next_gap < prev.end && usize::from(s.columns[next_gap]) + 2 <= j {
                for slot in 0..4 {
                    let (value, start) =
                        (s.scores[next_gap * 4 + slot], s.starts[next_gap * 4 + slot]);
                    if value == INVALID {
                        continue;
                    }
                    let shifted = value + i32::from(s.columns[next_gap]);
                    if better_candidate(shifted, start, gap, gap_start) {
                        (gap, gap_start, gap_entry, gap_slot) =
                            (shifted, start, next_gap as u16, slot as u8);
                    }
                }
                next_gap += 1;
            }
            while diag < prev.end && usize::from(s.columns[diag]) + 1 < j {
                diag += 1;
            }
            if diag < prev.end && usize::from(s.columns[diag]) + 1 == j {
                for (from, run) in RUN_BONUS.into_iter().enumerate() {
                    let (value, start) = (s.scores[diag * 4 + from], s.starts[diag * 4 + from]);
                    if value != INVALID {
                        let (step, run) = consecutive_bonus(bonus, run);
                        s.offer(
                            cell + run_slot(run),
                            value + SCORE_MATCH + step,
                            start,
                            diag as u16,
                            from as u8,
                        );
                    }
                }
            }
            if gap != INVALID {
                // A gap of `j - p - 1`: `GAP_START`, then `GAP_EXTENSION`
                // for each position past its first (`:1676-1685`).
                let score = gap - j as i32 - 1 + SCORE_MATCH + bonus;
                s.offer(
                    cell + run_slot(bonus),
                    score,
                    gap_start,
                    gap_entry,
                    gap_slot,
                );
            }
        }
    }
    let (mut best, mut best_start, mut at) = (INVALID, 0, 0);
    for cell in s.row_offsets[m - 1] * 4..s.row_offsets[m] * 4 {
        let (value, start) = (s.scores[cell], s.starts[cell]);
        if value != INVALID && better_candidate(value, start, best, best_start) {
            (best, best_start, at) = (value, start, cell);
        }
    }
    if best == INVALID {
        return None;
    }
    s.positions.resize(m, 0);
    for row in (0..m).rev() {
        s.positions[row] = usize::from(s.columns[at / 4]);
        at = usize::from(s.pred_entries[at]) * 4 + usize::from(s.pred_slots[at]);
    }
    Some((best, usize::from(best_start) - range_start))
}

/// The highest score a placement that starts above the name and ends in it
/// can reach (`maxCrossingQuality`, `file_index.zig:1736-1743`).
fn max_crossing_quality<T: PartialEq + From<u8>>(query: &[T]) -> i32 {
    let m = query.len() as i32;
    let ceiling = SCORE_MATCH * m + BONUS_BOUNDARY_DELIMITER * (m + 1);
    if query.contains(&T::from(b'/')) {
        ceiling
    } else {
        ceiling + GAP_START
    }
}

/// The placement a path is ranked by, and the search that found it
/// (`choosePlacement`, `file_index.zig:1776-1819`): the best inside the name
/// plus [`NAME_BONUS`], or a better one from the folder holding the name into
/// it; failing both, the best anywhere, ranked in the folder tier below every
/// name match whether or not it ends in the name. A kind of placement that
/// `needed` turns off is not searched for.
// fx's seven arguments, and what the masks rule out.
#[allow(clippy::too_many_arguments)]
fn choose_placement<T: Unit>(
    text: &[T],
    path: &[u8],
    offsets: &[usize],
    base: usize,
    query: &[T],
    needed: Needed,
    holds: Holds,
    s: &mut Align,
) -> Option<(Score, usize, usize)> {
    let place = |quality: i32, first: usize| Score {
        quality,
        first,
        name: true,
        ..Score::default()
    };
    let mut best = holds
        .name
        .then(|| align_best(text, path, offsets, base, base, query, s))
        .flatten()
        .map(|(quality, first)| (place(quality + NAME_BONUS, first), base, base));
    // With no placement in the name, a path that does not hold the letters
    // in order has none anywhere: one scan settles it (`file_index.zig:1802-1804`).
    // Only worth it where the search over the whole path would follow: a
    // crossing search settles a path that holds no crossing alone.
    if best.is_none() && query.len() > 1 && needed.folder && !is_subsequence(text, query) {
        return None;
    }
    let can_win = best.is_none_or(|(inside, ..)| inside.quality < max_crossing_quality(query));
    if needed.crossing && holds.folder && base > 0 && query.len() > 1 && can_win {
        let folder = T::rfind(text, base - 1, T::from(b'/')).map_or(0, |slash| slash + 1);
        if let Some((quality, first)) = align_best(text, path, offsets, folder, base, query, s) {
            if best.is_none_or(|(inside, ..)| quality > inside.quality) {
                best = Some((place(quality, first), folder, base));
            }
        }
    }
    if best.is_some() || !needed.folder {
        return best;
    }
    let (quality, first) = align_best(text, path, offsets, 0, 0, query, s)?;
    let score = Score {
        quality,
        first,
        ..Score::default()
    };
    Some((score, 0, 0))
}

/// Whether `text` holds `query` in order (`isSubsequence`, `file_index.zig:1821-1829`).
fn is_subsequence<T: Unit>(text: &[T], query: &[T]) -> bool {
    let mut from = 0;
    query.iter().all(|wanted| {
        T::find(text, from, *wanted).is_some_and(|at| {
            from = at + 1;
            true
        })
    })
}

/// A query, case-folded once (`prepareQuery`, `file_index.zig:1208-1215`):
/// its scalars; its bytes, when every one is ASCII; and their [`letters`].
struct Folded {
    chars: Vec<char>,
    ascii: Option<Vec<u8>>,
    mask: u32,
}

impl Folded {
    fn new(query: &str) -> Self {
        let chars: Vec<char> = query.chars().map(fold).collect();
        let ascii: Option<Vec<u8>> = chars
            .iter()
            .map(|c| c.is_ascii().then_some(*c as u8))
            .collect();
        let mask = ascii.as_deref().map_or(0, letters);
        Self { chars, ascii, mask }
    }
}

/// One scalar's case fold: its lower case when that is one scalar, itself
/// otherwise. Not upstream's `unicode_simple_fold`: that maps `ς` to `σ`,
/// which lower-casing leaves apart, so `σigma` finds `ςigma` in fx and not
/// here (`file_index.zig:2909-2944@481104fd`). Unit CASE-FOLD-SIMPLE, queued.
fn fold(scalar: char) -> char {
    if scalar.is_ascii() {
        return scalar.to_ascii_lowercase();
    }
    let mut lower = scalar.to_lowercase();
    match (lower.next(), lower.next()) {
        (Some(one), None) => one,
        _ => scalar,
    }
}

/// Upstream's O(1) reject (`file_index.zig:1371-1373`): an ASCII path can
/// hold the query only if the query is ASCII too and every letter of it is
/// among the path's. A path with any other byte is always scored.
fn admits(entry: &Candidate, query: &Folded) -> bool {
    entry.mask & NON_ASCII != 0 || (query.ascii.is_some() && entry.mask & query.mask == query.mask)
}

/// The query is the path, the name, or the name before a dot (`exactFit`,
/// `file_index.zig:1753-1758`).
fn exact_fit<T: PartialEq + From<u8>>(whole: &[T], name: &[T], query: &[T]) -> bool {
    whole == query
        || name == query
        || (name.len() > query.len()
            && name[query.len()] == T::from(b'.')
            && name.starts_with(query))
}

/// The basename's first byte: after the last `/`.
fn basename_start(path: &str) -> usize {
    path.rfind('/').map_or(0, |slash| slash + 1)
}

/// `path` loaded into `s`, and its name's first scalar.
fn loaded(path: &str, s: &mut Scratch) -> usize {
    s.load(path);
    let base = basename_start(path);
    s.offsets
        .iter()
        .position(|at| *at >= base)
        .unwrap_or(s.text.len())
}

/// How well `query` matches `path`, its scalars folded here
/// (`scoreFoldedMatchWith`, `file_index.zig:1866-1882`): an entry with a byte
/// past ASCII, and a browsed folder's names. Past [`MOST_PATH_BYTES`], none,
/// as upstream scores no longer name (`NameQuery.score`, `file_index.zig:1287`).
fn score(path: &str, query: &Folded, needed: Needed, s: &mut Scratch) -> Option<Score> {
    let query = &query.chars[..];
    if query.is_empty() || path.is_empty() || path.len() > MOST_PATH_BYTES {
        return None;
    }
    let base = loaded(path, s);
    let Scratch {
        text,
        offsets,
        align,
    } = s;
    let exact = exact_fit(text, &text[base..], query);
    // An exact match always enters, so it is always placed in full.
    let needed = if exact { EVERY } else { needed };
    let (score, ..) = choose_placement(
        text,
        path.as_bytes(),
        offsets,
        base,
        query,
        needed,
        ANY,
        align,
    )?;
    Some(Score { exact, ..score })
}

/// One entry's score in [`rank_top`] (`file_index.zig:1371-1387`): an ASCII
/// path is placed on its lower-cased bytes, any other folded scalar by
/// scalar. Past `limit` bytes, only an exact match is placed.
fn score_entry(
    entry: &Candidate,
    query: &Folded,
    needed: Needed,
    limit: Option<usize>,
    s: &mut Scratch,
) -> Option<Score> {
    if !admits(entry, query) {
        return None;
    }
    if entry.mask & NON_ASCII != 0 {
        return score(&entry.path, query, needed, s);
    }
    let (lower, ascii) = (&entry.lower[..], query.ascii.as_deref()?);
    if lower.is_empty() || lower.len() > MOST_PATH_BYTES {
        return None;
    }
    let exact = exact_fit(lower, &lower[entry.name..], ascii);
    if !exact && limit.is_some_and(|limit| lower.len() > limit) {
        return None;
    }
    let needed = if exact { EVERY } else { needed };
    let holds = Holds {
        name: entry.name_mask & query.mask == query.mask,
        folder: entry.folder_mask & query.mask == query.mask,
    };
    let (bytes, offsets) = (entry.path.as_bytes(), &IDENTITY[..=lower.len()]);
    let (score, ..) = choose_placement(
        lower,
        bytes,
        offsets,
        entry.name,
        ascii,
        needed,
        holds,
        &mut s.align,
    )?;
    Some(Score { exact, ..score })
}

/// The byte ranges of `path` the query matched -- the placement that set its
/// rank -- whole grapheme clusters, adjacent ones merged
/// (`reconstructMatchSpans` and `spansFromMatchedOffsets`, `file_index.zig:1921-1983`).
fn spans(path: &str, query: &Folded, s: &mut Scratch) -> Vec<Range<usize>> {
    let query = &query.chars[..];
    if query.is_empty() || path.is_empty() {
        return Vec::new();
    }
    let base = loaded(path, s);
    let (text, offsets, bytes) = (&s.text[..], &s.offsets[..], path.as_bytes());
    let Some((_, range_start, end_min)) =
        choose_placement(text, bytes, offsets, base, query, EVERY, ANY, &mut s.align)
    else {
        return Vec::new();
    };
    // The chosen search again: a later one in the choice reused the positions.
    if align_best(
        text,
        bytes,
        offsets,
        range_start,
        end_min,
        query,
        &mut s.align,
    )
    .is_none()
    {
        return Vec::new();
    }
    let offsets: Vec<usize> = s
        .align
        .positions
        .iter()
        .map(|position| s.offsets[*position])
        .collect();
    let mut out: Vec<Range<usize>> = Vec::new();
    let mut next = offsets.iter().peekable();
    for (start, cluster) in path.grapheme_indices(true) {
        let end = start + cluster.len();
        let mut hit = false;
        while next.peek().is_some_and(|offset| **offset < end) {
            next.next();
            hit = true;
        }
        if !hit {
            continue;
        }
        match out.last_mut() {
            Some(last) if last.end == start => last.end = end,
            _ => out.push(start..end),
        }
    }
    out
}

/// The [`MOST_RESULTS`] best of `entries`, best first (`rankTopN`,
/// `file_index.zig:1330-1403@481104fd`). Upstream keeps `min(out.len, 64)`
/// (`file_index.zig:617`), and the picker's `out` holds 32
/// (`file_picker_completion_cap`, `input_completion_runtime.zig:44`;
/// `file_completion_state.zig:7`).
/// Once the set is full, an entry is placed only as far as it could still
/// displace the worst kept: no folder placement once that one is a name
/// match, no crossing one once it beats [`max_crossing_quality`], and past
/// its length once it is the best a name can do -- the rows stay those of
/// placing every entry in full. A path listed twice ties on everything but
/// its place in the index, and the earlier is kept.
fn rank_top(entries: &[Candidate], query: &Folded, s: &mut Scratch) -> Vec<(Score, usize)> {
    let order = |left: &(Score, usize), right: &(Score, usize)| {
        better(
            (&left.0, &entries[left.1].path),
            (&right.0, &entries[right.1].path),
            false,
        )
        .then(left.1.cmp(&right.1))
    };
    let m = query.chars.len() as i32;
    let max_crossing = max_crossing_quality(&query.chars);
    // `maxNameQuality` (`file_index.zig:1745-1751`): the query as a run from the name's
    // first letter, after its `/`.
    let max_name = SCORE_MATCH * m + BONUS_BOUNDARY_DELIMITER * (m + 1) + NAME_BONUS;
    let mut kept: Vec<(Score, usize)> = Vec::with_capacity(MOST_RESULTS);
    let mut worst = 0;
    for (index, entry) in entries.iter().enumerate() {
        let (mut needed, mut limit) = (EVERY, None);
        if kept.len() == MOST_RESULTS {
            let (top, at) = kept[worst];
            needed = Needed {
                folder: !(top.exact || top.name),
                crossing: !(top.exact || (top.name && top.quality > max_crossing)),
            };
            if top.exact {
                limit = Some(0);
            } else if top.name && top.quality == max_name && top.first == 0 {
                limit = Some(entries[at].path.len());
            }
        }
        let Some(score) = score_entry(entry, query, needed, limit, s) else {
            continue;
        };
        if kept.len() < MOST_RESULTS {
            kept.push((score, index));
        } else if order(&(score, index), &kept[worst]).is_lt() {
            kept[worst] = (score, index);
        } else {
            continue;
        }
        if kept.len() == MOST_RESULTS {
            worst = (0..MOST_RESULTS)
                .max_by(|a, b| order(&kept[*a], &kept[*b]))
                .unwrap_or(0);
        }
    }
    kept.sort_by(order);
    kept
}

/// The [`MOST_RESULTS`] best of `entries` for `query`, best first
/// (`searchAtRevision`, `file_index.zig:585-645@481104fd`).
///
/// An empty query is the first entries in the index's own order and matches
/// nothing in particular, so no row has a bold run.
pub(crate) fn search(entries: &[Candidate], query: &str) -> Vec<Found> {
    if query.is_empty() {
        return entries
            .iter()
            .take(MOST_RESULTS)
            .map(|candidate| Found {
                path: candidate.path.clone(),
                kind: candidate.kind,
                spans: Vec::new(),
            })
            .collect();
    }
    let folded = Folded::new(query);
    let mut scratch = Scratch::default();
    rank_top(entries, &folded, &mut scratch)
        .into_iter()
        .map(|(_, index)| {
            let candidate = &entries[index];
            Found {
                path: candidate.path.clone(),
                kind: candidate.kind,
                spans: spans(&candidate.path, &folded, &mut scratch),
            }
        })
        .collect()
}

/// The folder a browsed path lists the contents of (`listedFolder`,
/// `path_completion.zig:133-143@481104fd`): `src/` lists `src` and `~` lists
/// `~`, and none for a partial name, the root or a doubled `/`. One that
/// cannot be painted as it stands, or written as an `@` word, has no row.
fn listed_folder<'a>(explicit: &'a Explicit<'_>) -> Option<&'a str> {
    if !explicit.name.is_empty() || explicit.prefix.len() < 2 {
        return None;
    }
    let folder = explicit.prefix.strip_suffix('/')?;
    if folder.ends_with('/') || !is_terminal_safe(folder) {
        return None;
    }
    Some(folder)
}

/// The rows a browsed path makes of its directory's `entries`
/// (`completeCancellable`, `path_completion.zig:44-131@481104fd`): each
/// entry's name ranked against what was typed after the last `/`, shown
/// behind the prefix the user typed, with the matched letters bold -- and,
/// when the path names a folder whose contents it lists, that folder first,
/// so it can be chosen and not only opened (7c8ef4a3; `:79-84`, `:124-128`).
/// The folder's row takes one of the list's [`MOST_RESULTS`], as fx's takes
/// the first slot of its `capacity` (`file_completion_state.zig:7`).
fn rank_listing(entries: &[(String, Kind)], explicit: &Explicit<'_>) -> Vec<Found> {
    let folded = Folded::new(explicit.name);
    let empty = folded.chars.is_empty();
    let mut scratch = Scratch::default();
    let mut ranked: Vec<(Score, String, Kind)> = entries
        .iter()
        .filter_map(|(name, kind)| {
            let score = if empty {
                Score::default()
            } else {
                score(name, &folded, EVERY, &mut scratch)?
            };
            let path = format!("{}{name}", explicit.prefix);
            (path.len() <= MOST_PATH_BYTES && is_terminal_safe(&path))
                .then_some((score, path, *kind))
        })
        .collect();
    ranked.sort_by(|left, right| better((&left.0, &left.1), (&right.0, &right.1), empty));
    let folder = listed_folder(explicit).map(|folder| Found {
        path: folder.to_string(),
        kind: Kind::Directory,
        spans: Vec::new(),
    });
    ranked.truncate(MOST_RESULTS - usize::from(folder.is_some()));
    folder
        .into_iter()
        .chain(ranked.into_iter().map(|(_, path, kind)| {
            let name = &path[explicit.prefix.len()..];
            let spans = spans(name, &folded, &mut scratch)
                .into_iter()
                .map(|span| span.start + explicit.prefix.len()..span.end + explicit.prefix.len())
                .collect();
            Found { path, kind, spans }
        }))
        .collect()
}

// ---------------------------------------------------------------------------
// the worker
// ---------------------------------------------------------------------------

/// How many entries one publication carries. Upstream publishes each entry as
/// it is stored (`ready_count`, `file_index.zig:225`); a message per entry
/// would be a hundred thousand of them for a large tree, and a run of entries
/// is a prefix all the same.
const PUBLISHED_AT_ONCE: usize = 512;

/// What a walk's worker says.
#[derive(Debug)]
pub(crate) enum Published {
    /// The next entries, in the index's order, after those already published.
    More(Vec<Candidate>),
    /// Every entry has been published.
    Ready,
    /// The index could not be built, or the walk was stopped.
    Failed,
}

/// A walk in flight, the scope it walks, and the prefix of its index
/// published so far.
struct Walk {
    rx: Receiver<Published>,
    stop: Arc<AtomicBool>,
    /// The scope's epoch when it started (`loading.scope_epoch =
    /// self.scope_epoch`, `file_index.zig:386@481104fd`).
    epoch: u64,
    published: Vec<Candidate>,
}

/// Every entry under the workspace and then under each added directory, in
/// that order (`discoverScopeCandidates`, `file_index.zig:902-1030`).
///
/// The workspace's are named relative to it, as they always were; an added
/// directory's are named by their **absolute** path, because that is the only
/// name that reaches them from the workspace (`appendDiscoveredCandidate`,
/// `:1032-1051`: `display = if (root_index == 0) relative else absolute`). One
/// cap for all of them, taken in that order, and a path that one root already
/// listed -- an added directory that holds the workspace, say -- is listed
/// once, under the first. An added directory that cannot be read is left out
/// and the others are kept (`file index root discovery omitted`, `:944`); the
/// workspace that cannot be is the index failing, as before.
pub(crate) fn discover_roots(
    root: &Path,
    extra: &[PathBuf],
    git: Option<&Path>,
    stop: &AtomicBool,
) -> Result<Vec<Candidate>, ()> {
    let mut entries = discover(root, git, stop)?;
    if extra.is_empty() {
        return Ok(entries);
    }
    let mut seen: std::collections::HashSet<PathBuf> = entries
        .iter()
        .map(|candidate| root.join(&candidate.path))
        .collect();
    for added in extra {
        if entries.len() >= MOST_INDEXED || stop.load(Atomic::Relaxed) {
            break;
        }
        let Ok(found) = discover(added, git, stop) else {
            if stop.load(Atomic::Relaxed) {
                return Err(());
            }
            continue;
        };
        for candidate in found {
            if entries.len() >= MOST_INDEXED {
                break;
            }
            let absolute = added.join(&candidate.path);
            let Some(path) = absolute.to_str().map(str::to_string) else {
                continue;
            };
            if !admissible(&path) || !seen.insert(absolute) {
                continue;
            }
            // Its facts are its absolute path's, not the relative one's.
            entries.push(Candidate::new(path, candidate.kind));
        }
    }
    Ok(entries)
}

/// The worker's whole job: discovery, then publication a prefix at a time
/// (`loadGeneration`, `file_index.zig:790-842`). A stop is looked at between
/// publications as upstream looks at it between entries (`file_index.zig:189`).
fn load(
    root: &Path,
    extra: &[PathBuf],
    git: Option<&Path>,
    stop: &AtomicBool,
    tx: &mpsc::Sender<Published>,
) {
    let Ok(entries) = discover_roots(root, extra, git, stop) else {
        let _ = tx.send(Published::Failed);
        return;
    };
    let mut entries = entries.into_iter();
    loop {
        if stop.load(Atomic::Relaxed) {
            let _ = tx.send(Published::Failed);
            return;
        }
        let run: Vec<Candidate> = entries.by_ref().take(PUBLISHED_AT_ONCE).collect();
        if run.is_empty() {
            break;
        }
        if tx.send(Published::More(run)).is_err() {
            return;
        }
    }
    let _ = tx.send(Published::Ready);
}

/// The last directory listed, and what it held -- or `Err` when it could not
/// be read.
type Listed = (PathBuf, Result<Vec<(String, Kind)>, ()>);

/// A directory listing in flight, for the directory it was asked about.
struct Listing {
    directory: PathBuf,
    rx: Receiver<Result<Vec<(String, Kind)>, ()>>,
    stop: Arc<AtomicBool>,
}

/// The picker's source of rows: the workspace index and the last directory
/// listed, each read on a thread of its own.
///
/// Nothing here blocks. [`Self::poll`] adopts what a worker has finished and
/// says whether anything changed, which is how the session knows a frame is
/// owed; [`Self::lookup`] answers from what is in hand and starts whatever is
/// missing.
pub(crate) struct Files {
    root: PathBuf,
    /// The directories `/workspace` added, active ones only, whose entries
    /// follow the workspace's ([`discover_roots`]).
    extra: Vec<PathBuf>,
    home: Option<PathBuf>,
    git: Option<PathBuf>,
    /// The last index a walk finished, kept until the next one is whole --
    /// a change of scope included, as fx keeps its adopted generation
    /// (`active_generation`, replaced only on adoption, `file_index.zig:425`,
    /// `:677@481104fd`). A walk in flight searches its own published prefix
    /// only while this is `None` (`searchableGeneration`, `:647-654`).
    index: Option<Arc<Vec<Candidate>>>,
    /// The epoch of the scope [`Self::index`] was walked for (the adopted
    /// generation's `scope_epoch`, `readableRevision`, `:572-583`).
    index_epoch: u64,
    /// The workspace's scope epoch: moved by every change of the access
    /// ([`Self::change_scope`]; `invalidateScope`, `app_workspace_runtime.
    /// zig:98-104@481104fd`). An indexed lookup answers only from an index
    /// or a prefix of this epoch, and is unavailable otherwise
    /// (`prepareFilePicker`, `input_completion_runtime.zig:887-888`).
    epoch: u64,
    /// Whether the last walk failed and there is no index to fall back on.
    failed: bool,
    walk: Option<Walk>,
    /// Whether a refresh was asked for while a walk ran, to start when that
    /// walk ends, whatever it ends in (fx's `pending_scope`,
    /// `replacePendingRoots` and `joinThreadIfDone`, `file_index.zig:443-451`,
    /// `:494-523@481104fd`). One however many were asked for: fx coalesces
    /// to the latest scope, and a walk reads the roots as they are when it
    /// starts. Any walk that starts takes it: it reads the current roots,
    /// which is all a request asks for.
    pending_refresh: bool,
    /// Whether the session's first indexed episode has asked for the
    /// index's refresh (fx's `file_picker_episode_seen`, `picker_state.zig:
    /// 171@481104fd`: set once, never reset). That first request is skipped
    /// while no index has been whole yet ([`Self::refresh_index_for_list`]).
    episode_seen: bool,
    /// The last directory listed and what it held, or `Err` when it could not
    /// be read.
    listed: Option<Listed>,
    listing: Option<Listing>,
    /// How many walks and listings this source has started, for a case to
    /// count the directory work a session asked for.
    #[cfg(test)]
    started: usize,
    /// How many of them were walks.
    #[cfg(test)]
    walks: usize,
    /// How many lookups the session asked for.
    #[cfg(test)]
    lookups: usize,
}

impl Files {
    /// A source for the workspace at `root`, with `home` for `~`. Nothing is
    /// read until the launch's walk ([`Self::start_index`]), the first
    /// [`Self::refresh_index`] or a [`Self::lookup`] of a browsed path.
    pub(crate) fn new(root: PathBuf, home: Option<PathBuf>) -> Self {
        Self {
            root,
            extra: Vec::new(),
            home,
            git: trusted_git(),
            index: None,
            index_epoch: 0,
            epoch: 0,
            failed: false,
            walk: None,
            pending_refresh: false,
            episode_seen: false,
            listed: None,
            listing: None,
            #[cfg(test)]
            started: 0,
            #[cfg(test)]
            walks: 0,
            #[cfg(test)]
            lookups: 0,
        }
    }

    /// The launch's walk (`startFileIndex` -> `ensureScopeEpoch`,
    /// `app_workspace_runtime.zig:145-148@481104fd`): none for an empty root
    /// (`:146`; `file_index.zig:358`), and none unless nothing has been
    /// walked -- no index, no walk, no failure (`currentState() != .idle`,
    /// `:357`, `:333-341`). Otherwise the scope as it stands, with its epoch
    /// now (`:361`), which the walk keeps whatever changes after it
    /// (`startLoad`, `:386`): a change of the access before it is whole
    /// finds it of the old epoch ([`Self::lookup`]).
    pub(crate) fn start_index(&mut self) {
        if self.index.is_some() || self.walk.is_some() || self.failed {
            return;
        }
        if self.root.as_os_str().is_empty() {
            return;
        }
        self.start_walk();
    }

    /// A picker has just opened: the directory listing is forgotten, so a
    /// browsed path is read fresh. Whether the index is walked again is the
    /// list's refresh request's ([`Self::refresh_index`]).
    pub(crate) fn begin_episode(&mut self) {
        self.listed = None;
    }

    /// The refresh an indexed list asks for (`prepareFilePicker`,
    /// `input_completion_runtime.zig:854-866@481104fd`): the session's first
    /// one is skipped while no index has been whole yet and a walk runs --
    /// the walk that builds it is the refresh it would have asked for
    /// (`file_picker_episode_seen`, `isFileIndexLoading`, `:857-861`) -- and
    /// every later one is [`Self::refresh_index`]. The walk it meets is the
    /// launch's ([`Self::start_index`]), or one a `/workspace` change queued
    /// behind it ([`super::shell`]'s `adopt_workspace`).
    ///
    /// fx's `currentState` reads the loading generation's own state, so a
    /// loader that has finished is `.ready` or `.failed` before the join
    /// takes it (`file_index.zig:336-340@481104fd`). xfx's walk says so on
    /// its channel, which [`Self::poll`] reads after the keys are routed; so
    /// the decision first takes whatever the walk has said, as the poll
    /// would ([`Self::take_walk`]), and a walk that has ended is not loading.
    pub(crate) fn refresh_index_for_list(&mut self) {
        if !self.episode_seen {
            self.episode_seen = true;
            self.take_walk();
            if self.loading() {
                return;
            }
        }
        self.refresh_index();
    }

    /// Whether the session's first index is still being built: none has
    /// been whole yet and a walk is running. fx's `isFileIndexLoading`
    /// (`main.zig:2125-2127@481104fd`) is `currentState() == .loading`
    /// (`file_index.zig:333-341`), which is `.ready` once a generation has
    /// been adopted -- kept through any change of scope, as [`Self::index`]
    /// is -- and otherwise the state of the one loading: a failed first index
    /// with nothing walking is `.failed`, not loading, and a walk started
    /// after it is loading again (`startLoad` clears the failure, `:389`).
    /// A walk of the old roots is not stopped by a change of them, so it is
    /// loading until it ends, as fx's is.
    fn loading(&self) -> bool {
        self.index.is_none() && self.walk.is_some()
    }

    /// Walks the index again (`refreshFileIndex`, `app_workspace_runtime.zig:
    /// 150-152@481104fd`), or -- a walk already running -- once that walk
    /// ends ([`Self::pending_refresh`]; `refreshOwnedRoots`,
    /// `file_index.zig:488-509`).
    pub(crate) fn refresh_index(&mut self) {
        if self.walk.is_some() {
            self.pending_refresh = true;
        } else {
            self.start_walk();
        }
    }

    /// The workspace's access changed, and the directories added to it are
    /// now `extra` -- the same ones or not: the scope's epoch moves
    /// (`invalidateScope`, `app_workspace_runtime.zig:98-104@481104fd`, on
    /// every change `install` sees, `:134-140`). The walk the change asks for
    /// is the caller's ([`super::shell`]'s `adopt_workspace`; `install`'s
    /// `refreshFileIndex`, `:140`).
    ///
    /// Nothing is stopped or dropped. A walk of the old scope runs on, and
    /// the new one is queued behind it (`refreshOwnedRoots`,
    /// `file_index.zig:488-497`); the index there is stays searchable with
    /// its old epoch, and so an indexed lookup says it is unavailable until
    /// a walk of this scope is whole ([`Self::lookup`]; `readableRevision`,
    /// `:572-583`; `prepareFilePicker`, `input_completion_runtime.zig:
    /// 887-888`).
    pub(crate) fn change_scope(&mut self, extra: Vec<PathBuf>) {
        self.epoch = self.epoch.wrapping_add(1);
        self.extra = extra;
    }

    fn start_walk(&mut self) {
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let root = self.root.clone();
        let extra = self.extra.clone();
        let git = self.git.clone();
        let flag = Arc::clone(&stop);
        let spawned = super::signals::spawn_unsignalled("xfx-file-index", move || {
            load(&root, &extra, git.as_deref(), &flag, &tx)
        });
        // A walk of the current roots is what any queued refresh asks for.
        self.pending_refresh = false;
        if spawned.is_ok() {
            #[cfg(test)]
            {
                self.started += 1;
                self.walks += 1;
            }
            self.walk = Some(Walk {
                rx,
                stop,
                epoch: self.epoch,
                published: Vec::new(),
            });
        } else if self.index.is_none() {
            self.failed = true;
        }
    }

    /// Adopts whatever a worker has finished, and says whether that changed
    /// what a lookup would answer.
    pub(crate) fn poll(&mut self) -> bool {
        let mut changed = self.take_walk();
        // The walk ended -- ready, failed or gone -- with a refresh asked for
        // meanwhile: it starts now (`joinThreadIfDone`, `file_index.zig:
        // 443-451@481104fd`). After a failed first index the list then says
        // it is indexing, as fx's does once `startLoad` clears the failure
        // (`:389`): a lookup reads the walk in flight before the failure.
        if self.walk.is_none() && self.pending_refresh {
            self.start_walk();
            changed = true;
        }
        if let Some(listing) = &self.listing {
            match listing.rx.try_recv() {
                Ok(listed) => {
                    self.listed = Some((listing.directory.clone(), listed));
                    self.listing = None;
                    changed = true;
                }
                Err(TryRecvError::Disconnected) => {
                    self.listed = Some((listing.directory.clone(), Err(())));
                    self.listing = None;
                    changed = true;
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        changed
    }

    /// Takes in what the walk has said so far -- its published runs, and its
    /// end, whole or failed -- and says whether that changed what a lookup
    /// would answer.
    fn take_walk(&mut self) -> bool {
        let mut changed = false;
        // Everything published so far, not one publication a turn: a reader
        // takes the whole prefix there is (`ready_count.load(.acquire)`,
        // `file_index.zig:153`).
        while let Some(walk) = &mut self.walk {
            match walk.rx.try_recv() {
                Ok(Published::More(run)) => {
                    walk.published.extend(run);
                    // A refresh is private until it is whole, so only a first
                    // index growing changes what a lookup answers.
                    changed |= self.index.is_none();
                }
                Ok(Published::Ready) => {
                    // Adopted whole, of the scope it walked, whatever the
                    // scope is now (`joinThreadIfDone`, `file_index.zig:
                    // 425@481104fd`).
                    let entries = std::mem::take(&mut walk.published);
                    self.index = Some(Arc::new(entries));
                    self.index_epoch = walk.epoch;
                    self.failed = false;
                    self.walk = None;
                    changed = true;
                }
                Ok(Published::Failed) | Err(TryRecvError::Disconnected) => {
                    // A failed refresh keeps the index it had: what was true a
                    // moment ago is a better answer than none. A first index
                    // that fails drops what it had published, as upstream stops
                    // searching a failed generation (`file_index.zig:647-649`).
                    self.failed = self.index.is_none();
                    self.walk = None;
                    changed = true;
                }
                Err(TryRecvError::Empty) => break,
            }
        }
        changed
    }

    /// The walks and listings this source has started.
    #[cfg(test)]
    pub(crate) fn started_for_test(&self) -> usize {
        self.started
    }

    /// The walks this source has started.
    #[cfg(test)]
    pub(crate) fn walks_for_test(&self) -> usize {
        self.walks
    }

    /// The scope's epoch now, and the epoch of the walk in flight, if any.
    #[cfg(test)]
    pub(crate) fn epochs_for_test(&self) -> (u64, Option<u64>) {
        (self.epoch, self.walk.as_ref().map(|walk| walk.epoch))
    }

    /// The lookups the session has asked of this source.
    #[cfg(test)]
    pub(crate) fn lookups_for_test(&self) -> usize {
        self.lookups
    }

    /// Whether a directory listing is in flight.
    #[cfg(test)]
    pub(crate) fn listing_in_flight_for_test(&self) -> bool {
        self.listing.is_some()
    }

    /// The stop flag of the listing in flight, for a case to watch.
    #[cfg(test)]
    pub(crate) fn listing_stop_for_test(&self) -> Option<Arc<AtomicBool>> {
        self.listing
            .as_ref()
            .map(|listing| Arc::clone(&listing.stop))
    }

    /// Whether a worker is still running.
    #[cfg(test)]
    pub(crate) fn busy(&self) -> bool {
        self.walk.is_some() || self.listing.is_some()
    }

    /// What `query` -- the decoded text after the `@` -- finds now.
    pub(crate) fn lookup(&mut self, query: &str) -> Lookup {
        #[cfg(test)]
        {
            self.lookups += 1;
        }
        if query.len() > MOST_PATH_BYTES {
            return Lookup::Unavailable;
        }
        let Some(explicit) = explicit(query) else {
            // What a reader gets is of the scope it was walked for; of
            // another, the list is unavailable until a walk of this one is
            // (`prepareFilePicker`, `input_completion_runtime.zig:887-888@
            // 481104fd`) -- adopted whole, or the first index published.
            return match (&self.index, &self.walk) {
                (Some(_), _) if self.index_epoch != self.epoch => Lookup::Unavailable,
                (Some(index), _) => Lookup::Rows(search(index, query)),
                (None, Some(walk)) if walk.epoch != self.epoch => Lookup::Unavailable,
                // The first index while it is published: what it has matched
                // so far, or -- nothing matched yet -- that it is still loading
                // (`input_completion_runtime.zig:838-842`).
                (None, Some(walk)) => match search(&walk.published, query) {
                    rows if rows.is_empty() => Lookup::Loading,
                    rows => Lookup::Rows(rows),
                },
                (None, None) if self.failed => Lookup::Unavailable,
                (None, None) => Lookup::Loading,
            };
        };
        let Some(directory) = resolve(&self.root, self.home.as_deref(), explicit.parent) else {
            return Lookup::Unavailable;
        };
        match &self.listed {
            Some((listed, Ok(entries))) if *listed == directory => {
                return Lookup::Rows(rank_listing(entries, &explicit));
            }
            Some((listed, Err(()))) if *listed == directory => return Lookup::Unavailable,
            _ => {}
        }
        if self
            .listing
            .as_ref()
            .is_some_and(|listing| listing.directory == directory)
        {
            return Lookup::Loading;
        }
        // Stopped and let go of together ([`Self::abandon_listing`]): its
        // answer, `Err` once stopped, is never taken.
        if let Some(previous) = self.listing.take() {
            previous.stop.store(true, Atomic::Relaxed);
        }
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let target = directory.clone();
        let flag = Arc::clone(&stop);
        let spawned = super::signals::spawn_unsignalled("xfx-file-listing", move || {
            let _ = tx.send(list_directory(&target, &flag));
        });
        if spawned.is_err() {
            return Lookup::Unavailable;
        }
        #[cfg(test)]
        {
            self.started += 1;
        }
        self.listing = Some(Listing {
            directory,
            rx,
            stop,
        });
        Lookup::Loading
    }

    /// Tells the directory listing in flight to stop -- its thread asks
    /// between entries ([`list_directory_until`]) -- and forgets it, so a
    /// late answer to it is never taken, and says whether there was one: the
    /// list it was for is not eligible any more (`reconcile` with `eligible`
    /// false, `directory_completion_job.zig:140-151@481104fd`: the task
    /// stopped, the request dropped; `Task.stop`, `:68-78`). The next lookup
    /// of that folder lists it again.
    ///
    /// **Every place that sets a listing's stop flag also lets go of its
    /// receiver** -- here, the superseded listing in [`Self::lookup`], and
    /// [`Drop`] -- so the `Err` a stopped listing answers with is never taken
    /// into `listed`: a cancel is never shown as a folder that could not be
    /// read, as fx's harvest drops an abandoned task's output
    /// (`directory_completion_job.zig:191-217`).
    pub(crate) fn abandon_listing(&mut self) -> bool {
        let Some(listing) = self.listing.take() else {
            return false;
        };
        listing.stop.store(true, Atomic::Relaxed);
        true
    }

    /// Forgets a folder that could not be read, so the next lookup reads it
    /// again: Tab on a picker that says it could not read (`retry`,
    /// `file_completion_state.zig:230-233@481104fd`). A failed index is
    /// walked again by the refresh the same retry asks for, and only for a
    /// query the index answers ([`Self::refresh_index`]).
    pub(crate) fn retry(&mut self) {
        if matches!(self.listed, Some((_, Err(())))) {
            self.listed = None;
        }
    }
}

impl Drop for Files {
    fn drop(&mut self) {
        // A walk of a large tree outliving the session that asked for it is
        // CPU spent on nobody; the walk's and the listing's threads check
        // this between entries.
        if let Some(walk) = &self.walk {
            walk.stop.store(true, Atomic::Relaxed);
        }
        // Stopped as the source and its receiver go together
        // ([`Self::abandon_listing`]).
        if let Some(listing) = &self.listing {
            listing.stop.store(true, Atomic::Relaxed);
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    use std::process::Command;
    use std::time::{Duration, Instant};

    /// One span, as a list.
    fn one(span: Range<usize>) -> Vec<Range<usize>> {
        vec![span]
    }

    fn write(root: &Path, path: &str) {
        let full = root.join(path);
        std::fs::create_dir_all(full.parent().expect("a parent")).expect("a directory");
        std::fs::write(full, "x").expect("a file");
    }

    fn paths(entries: &[Candidate]) -> Vec<String> {
        entries
            .iter()
            .map(|entry| match entry.kind {
                Kind::File => entry.path.clone(),
                Kind::Directory => format!("{}/", entry.path),
            })
            .collect()
    }

    pub(crate) fn candidates(paths: &[&str]) -> Vec<Candidate> {
        paths
            .iter()
            .map(|path| match path.strip_suffix('/') {
                Some(directory) => Candidate::new(directory.to_string(), Kind::Directory),
                None => Candidate::new((*path).to_string(), Kind::File),
            })
            .collect()
    }

    /// The workspace the research captures used (`ws2`).
    pub(crate) fn seeded() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("a temp dir");
        for path in [
            "Cargo.toml",
            "README.md",
            "docs/guide.md",
            "src/main.rs",
            "src/lib.rs",
            "src/deep/sched.rs",
        ] {
            write(dir.path(), path);
        }
        dir
    }

    #[test]
    fn an_added_directory_is_indexed_after_the_workspace_by_its_absolute_paths() {
        // `appendDiscoveredCandidate` (`file_index.zig:1032-1051`): the first
        // root's entries relative, every other root's absolute.
        let base = tempfile::tempdir().expect("a base");
        let base_path = base.path().canonicalize().expect("canonical");
        let work = base_path.join("work");
        let extra = base_path.join("extra");
        write(&work, "main.rs");
        write(&extra, "notes/plan.md");
        let found = discover_roots(
            &work,
            &[extra.clone(), base_path.join("missing")],
            None,
            &AtomicBool::new(false),
        )
        .expect("a walk");
        let extra_text = extra.to_string_lossy();
        assert_eq!(
            paths(&found),
            [
                "main.rs".to_string(),
                format!("{extra_text}/notes/"),
                format!("{extra_text}/notes/plan.md"),
            ]
        );
        // An added directory that holds the workspace lists the workspace's
        // entries once, under the workspace's own names.
        let found = discover_roots(
            &work,
            std::slice::from_ref(&base_path),
            None,
            &AtomicBool::new(false),
        )
        .expect("a walk");
        let listed = paths(&found);
        assert_eq!(
            listed
                .iter()
                .filter(|path| path.ends_with("main.rs"))
                .count(),
            1
        );
        assert!(
            listed.contains(&format!("{extra_text}/notes/plan.md")),
            "{listed:?}"
        );
    }

    #[test]
    fn a_change_of_scope_keeps_the_index_and_is_unavailable_until_its_walk_is_whole() {
        // fx keeps its adopted generation through a change of scope, with the
        // old epoch (`readableRevision`, `file_index.zig:572-583@481104fd`),
        // and an indexed list of another epoch is unavailable
        // (`prepareFilePicker`, `input_completion_runtime.zig:887-888`) until
        // a walk of the new scope is adopted -- the same roots again
        // included, since every change of the access moves the epoch
        // (`invalidateScope`, `app_workspace_runtime.zig:98-104`).
        let base = tempfile::tempdir().expect("a base");
        let base_path = base.path().canonicalize().expect("canonical");
        let work = base_path.join("work");
        let extra = base_path.join("extra");
        write(&work, "main.rs");
        write(&extra, "plan.md");
        let mut files = Files::new(work, None);
        files.git = None;
        files.refresh_index();
        settled(&mut files, |files| !files.busy());
        assert!(found_paths(files.lookup("plan")).is_empty());

        files.change_scope(vec![extra.clone()]);
        assert!(files.index.is_some(), "the old scope's index was dropped");
        assert_eq!(
            files.lookup("plan"),
            Lookup::Unavailable,
            "the old scope's index answered"
        );
        files.refresh_index();
        assert_eq!(
            files.lookup("plan"),
            Lookup::Unavailable,
            "the new walk's prefix answered while an index is held"
        );
        settled(&mut files, |files| !files.busy());
        assert_eq!(
            found_paths(files.lookup("plan")),
            [format!("{}/plan.md", extra.display())]
        );
        files.change_scope(vec![extra]);
        assert_eq!(files.lookup("plan"), Lookup::Unavailable);
        files.refresh_index();
        settled(&mut files, |files| !files.busy());
        assert!(matches!(files.lookup("plan"), Lookup::Rows(_)));
    }

    #[test]
    fn a_change_of_scope_lets_the_walk_run_on_and_its_prefix_answers_nothing() {
        // fx's change of scope stops no walk: `refreshOwnedRoots` queues the
        // new scope behind it (`file_index.zig:488-497@481104fd`), and the
        // walk keeps the epoch it started with (`:386`). Its prefix -- the
        // first index, searched as it is published -- is of the old scope,
        // so an indexed lookup is unavailable; a first walk of the new scope
        // is searched as it is published again.
        let mut files = Files::new(PathBuf::from("/nonexistent-root"), None);
        let old = driven(&mut files);
        old.send(Published::More(candidates(&["src/main.rs"])))
            .expect("the walk listens");
        files.poll();
        assert_eq!(found_paths(files.lookup("main")), ["src/main.rs"]);
        files.change_scope(Vec::new());
        let stop = Arc::clone(&files.walk.as_ref().expect("the walk").stop);
        assert!(!stop.load(Atomic::Relaxed), "the change stopped the walk");
        assert_eq!(files.lookup("main"), Lookup::Unavailable);
        old.send(Published::Ready).expect("the walk listens");
        assert!(files.poll());
        assert!(
            files.index.is_some(),
            "the old scope's walk was not adopted"
        );
        assert_eq!(files.lookup("main"), Lookup::Unavailable);

        // A change before any walk: the walk it starts is the first, of the
        // new epoch, and its prefix is searched. In a session the launch has
        // walked first (`app_entry_runtime.zig:334@481104fd`;
        // `Files::start_index`, since AT-LAUNCH-WALK), so this is the source
        // after a launch walk that failed (no searchable generation,
        // `file_index.zig:647-654`), as the shell's
        // `a_change_after_the_launch_walk_failed_lists_the_new_walks_prefix`
        // shows; a launch walk loading or adopted holds the old epoch
        // (`the_launch_walk_keeps_the_launch_epoch_through_a_change`, below).
        let mut files = Files::new(PathBuf::from("/nonexistent-root"), None);
        files.change_scope(Vec::new());
        let new = driven(&mut files);
        new.send(Published::More(candidates(&["src/main.rs"])))
            .expect("the walk listens");
        files.poll();
        assert_eq!(found_paths(files.lookup("main")), ["src/main.rs"]);
    }

    #[test]
    fn the_launch_walks_a_root_once_and_an_empty_root_never() {
        // fx's launch walk (`startFileIndex`, `app_workspace_runtime.zig:
        // 145-148@481104fd`): none for an empty workspace root (`:146`;
        // `ensureScopeEpoch`'s own check, `file_index.zig:358`), and none
        // unless the index is idle -- nothing adopted, loading or failed
        // (`currentState() != .idle`, `:357`; `:333-341`).
        let dir = seeded();
        let mut files = Files::new(dir.path().to_path_buf(), None);
        files.git = None;
        files.start_index();
        assert_eq!(files.walks_for_test(), 1, "the launch walked nothing");
        files.start_index();
        assert_eq!(files.walks_for_test(), 1, "a second walk while one ran");
        settled(&mut files, |files| !files.busy());
        assert!(files.index.is_some(), "the launch walk was not adopted");
        files.start_index();
        assert_eq!(files.walks_for_test(), 1, "a walk over an adopted index");

        let mut failed = Files::new(dir.path().to_path_buf(), None);
        failed.git = None;
        driven(&mut failed)
            .send(Published::Failed)
            .expect("the walk listens");
        failed.poll();
        failed.start_index();
        assert_eq!(failed.walks_for_test(), 0, "a walk after a failed index");

        let mut empty = Files::new(PathBuf::new(), None);
        empty.git = None;
        empty.start_index();
        assert_eq!(empty.walks_for_test(), 0, "an empty root was walked");
    }

    #[test]
    fn the_launch_walk_keeps_the_launch_epoch_through_a_change() {
        // The launch walk carries the scope's epoch when it starts
        // (`ensureScopeEpoch`, `file_index.zig:361@481104fd`; `startLoad`,
        // `:386`), and a change of the access does not touch it: its prefix,
        // and its index once adopted, are of the old epoch, unavailable to
        // an indexed lookup (`readableRevision`, `:572-583`;
        // `input_completion_runtime.zig:887-888`) until a walk of the new
        // scope is whole.
        let dir = seeded();
        let mut files = Files::new(dir.path().to_path_buf(), None);
        files.git = None;
        // The constructor's change, as the shell's makes it.
        files.change_scope(Vec::new());
        files.start_index();
        let launch = held(&mut files);
        launch
            .send(Published::More(candidates(&["src/main.rs"])))
            .expect("the walk listens");
        files.poll();
        assert_eq!(found_paths(files.lookup("main")), ["src/main.rs"]);
        files.change_scope(Vec::new());
        assert_eq!(
            files.lookup("main"),
            Lookup::Unavailable,
            "the launch walk's prefix answered after a change"
        );
        launch.send(Published::Ready).expect("the walk listens");
        files.poll();
        assert!(files.index.is_some(), "the launch walk was not adopted");
        assert_eq!(
            files.lookup("main"),
            Lookup::Unavailable,
            "the launch walk's index answered after a change"
        );
        files.refresh_index();
        settled(&mut files, |files| !files.busy());
        assert_eq!(found_paths(files.lookup("main"))[0], "src/main.rs");
    }

    #[test]
    fn a_walk_lists_files_and_directories_in_one_byte_order() {
        let dir = seeded();
        let found = discover(dir.path(), None, &AtomicBool::new(false)).expect("a walk");
        // `fx-composer-40x120-at.txt`: the first six rows of an empty query.
        assert_eq!(
            paths(&found),
            [
                "Cargo.toml",
                "README.md",
                "docs/",
                "docs/guide.md",
                "src/",
                "src/deep/",
                "src/deep/sched.rs",
                "src/lib.rs",
                "src/main.rs",
            ]
        );
    }

    #[test]
    fn a_walk_skips_the_always_ignored_names_and_keeps_hidden_ones() {
        let dir = tempfile::tempdir().expect("a temp dir");
        for path in [
            "node_modules/pkg/index.js",
            "build/out.o",
            ".git/config",
            ".hidden/kept.txt",
            "src/build",
            "keep.txt",
        ] {
            write(dir.path(), path);
        }
        let found = discover(dir.path(), None, &AtomicBool::new(false)).expect("a walk");
        // A file named like an ignored directory is skipped too
        // (`workspace_files.zig:530-531`).
        assert_eq!(
            paths(&found),
            [".hidden/", ".hidden/kept.txt", "keep.txt", "src/"]
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_walk_does_not_descend_a_symlink() {
        let dir = tempfile::tempdir().expect("a temp dir");
        write(dir.path(), "real/inside.txt");
        std::os::unix::fs::symlink(dir.path().join("real"), dir.path().join("link"))
            .expect("a link");
        let found = discover(dir.path(), None, &AtomicBool::new(false)).expect("a walk");
        assert_eq!(paths(&found), ["link", "real/", "real/inside.txt"]);
    }

    #[test]
    fn a_path_that_cannot_be_painted_is_not_an_entry() {
        assert!(is_terminal_safe("src/main.rs"));
        assert!(is_terminal_safe("caf\u{e9}/\u{6587}.txt"));
        for unsafe_name in [
            "a\u{1b}b",
            "a\nb",
            "a\u{7f}",
            "a\u{85}",
            "a\u{200b}b",
            "a\u{202e}b",
            "\u{feff}a",
        ] {
            assert!(!is_terminal_safe(unsafe_name), "{unsafe_name:?}");
        }
        let dir = tempfile::tempdir().expect("a temp dir");
        write(dir.path(), "ok.txt");
        write(dir.path(), "bad\u{1b}[31m.txt");
        let found = discover(dir.path(), None, &AtomicBool::new(false)).expect("a walk");
        assert_eq!(paths(&found), ["ok.txt"]);
    }

    #[test]
    fn a_work_tree_is_listed_by_git_and_its_ignore_rules_hold() {
        let Some(git) = trusted_git() else {
            // No trusted git on this machine: the walk is what runs, and the
            // walk's own rules are the cases above.
            return;
        };
        let dir = tempfile::tempdir().expect("a temp dir");
        let root = dir.path();
        let run = |args: &[&str]| {
            let status = crate::process::status(
                Command::new(&git)
                    .args(args)
                    .current_dir(root)
                    .env("GIT_CONFIG_GLOBAL", "/dev/null")
                    .env("GIT_CONFIG_NOSYSTEM", "1")
                    .stdout(Stdio::null())
                    .stderr(Stdio::null()),
            )
            .expect("git runs");
            assert!(status.success(), "git {args:?}");
        };
        run(&["init", "--quiet"]);
        std::fs::write(root.join(".gitignore"), "ignored.txt\nignored-dir/\n").expect("a file");
        for path in [
            "tracked.txt",
            "untracked.txt",
            "ignored.txt",
            "ignored-dir/item.txt",
            ".hidden/tracked.txt",
            "gone.txt",
        ] {
            write(root, path);
        }
        run(&[
            "add",
            ".gitignore",
            "tracked.txt",
            ".hidden/tracked.txt",
            "gone.txt",
        ]);
        std::fs::remove_file(root.join("gone.txt")).expect("removed");
        let found = discover(root, Some(&git), &AtomicBool::new(false)).expect("a listing");
        // `file_index.zig` test "production scope admits tracked untracked
        // hidden and direct directory candidates" (:2673-2733): tracked,
        // untracked and hidden in; ignored files and directories, `.git`, and a
        // tracked file the tree no longer has, out.
        assert_eq!(
            paths(&found),
            [
                ".gitignore",
                ".hidden/",
                ".hidden/tracked.txt",
                "tracked.txt",
                "untracked.txt"
            ]
        );
    }

    #[test]
    fn a_stop_ends_a_walk_without_an_index() {
        let dir = seeded();
        assert!(discover(dir.path(), None, &AtomicBool::new(true)).is_err());
    }

    #[test]
    fn the_index_is_capped() {
        let files: Vec<String> = (0..MOST_INDEXED + 5).map(|n| format!("f{n:06}")).collect();
        let directories = vec!["d".to_string()];
        let merged = merged(files, directories);
        assert_eq!(merged.len(), MOST_INDEXED);
    }

    fn rows(entries: &[Candidate], query: &str) -> Vec<String> {
        search(entries, query)
            .into_iter()
            .map(|found| match found.kind {
                Kind::File => found.path,
                Kind::Directory => format!("{}/", found.path),
            })
            .collect()
    }

    #[test]
    fn an_empty_query_is_the_index_in_its_own_order() {
        let entries = candidates(&["b", "a/", "c"]);
        assert_eq!(rows(&entries, ""), ["b", "a/", "c"]);
        let many: Vec<Candidate> = (0..40)
            .map(|n| candidates(&[&format!("f{n:02}")])[0].clone())
            .collect();
        assert_eq!(search(&many, "").len(), MOST_RESULTS);
    }

    #[test]
    fn a_query_is_a_case_blind_subsequence() {
        let dir = seeded();
        let entries = discover(dir.path(), None, &AtomicBool::new(false)).expect("a walk");
        // `fx-composer-40x120-at-sch.txt`: one row.
        assert_eq!(rows(&entries, "sch"), ["src/deep/sched.rs"]);
        assert_eq!(rows(&entries, "SCH"), ["src/deep/sched.rs"]);
        assert_eq!(rows(&entries, "gui"), ["docs/guide.md"]);
        assert!(rows(&entries, "zzz").is_empty());
    }

    #[test]
    fn an_exact_name_comes_first_and_a_clean_folder_and_name_match_beats_a_mid_name_one() {
        let entries = candidates(&["main/other.rs", "src/main.rs", "main.rs", "x/remain.rs"]);
        // `main.rs` is the query exactly; `src/main.rs`'s name is too, and is
        // longer. `main/other.rs` runs `main` from its own folder's start into
        // `.rs` (173), which fx ranks as a name match above `x/remain.rs`'s
        // run from the middle of its name (148, and the name bonus 4):
        // `choosePlacement` (`file_index.zig:1776-1819@481104fd`), and
        // `p5-at-rank`'s `main-rs` on fx.
        assert_eq!(
            rows(&entries, "main.rs"),
            ["main.rs", "src/main.rs", "main/other.rs", "x/remain.rs"]
        );
    }

    #[test]
    fn word_starts_outrank_a_tighter_match_elsewhere() {
        // `fm` starts two words of `file-map` and none of `farm`.
        let entries = candidates(&["farm", "file-map"]);
        assert_eq!(rows(&entries, "fm"), ["file-map", "farm"]);
        // camelCase is a word start too.
        let entries = candidates(&["xfilemap", "fileMap"]);
        assert_eq!(rows(&entries, "fm"), ["fileMap", "xfilemap"]);
    }

    #[test]
    fn ties_go_to_the_shorter_path_and_then_to_bytes() {
        let entries = candidates(&["b/ab", "a/ab", "c/d/ab"]);
        assert_eq!(rows(&entries, "ab"), ["a/ab", "b/ab", "c/d/ab"]);
    }

    #[test]
    fn the_matched_letters_are_whole_clusters_and_runs_are_merged() {
        let found = search(&candidates(&["src/deep/sched.rs"]), "sch");
        // `fx-composer-40x120-at-sch.ansi`: `src/deep/` then bold `sch`.
        assert_eq!(found[0].spans, one(9..12));
        // A combining mark stays with the letter it marks.
        let found = search(&candidates(&["cafe\u{301}.txt"]), "e");
        assert_eq!(found[0].spans, one(3..6));
        // A match that has to cross into the directory is spanned there.
        let found = search(&candidates(&["ab/cd"]), "ac");
        assert_eq!(found[0].spans, [0..1, 3..4]);
    }

    #[test]
    fn only_the_best_thirty_two_are_kept() {
        let entries: Vec<Candidate> = (0..100)
            .map(|n| candidates(&[&format!("dir/x{n:03}")])[0].clone())
            .collect();
        let found = search(&entries, "x");
        // fx's `capacity`, the lookup's buffer (`file_completion_state.zig:7`,
        // `input_completion_runtime.zig:895@481104fd`), as a number: a cap
        // written as the constant would follow the constant.
        assert_eq!(found.len(), 32);
        assert_eq!(found[0].path, "dir/x000");
        assert_eq!(found[31].path, "dir/x031");
        assert_eq!(search(&entries, "").len(), 32);
    }

    /// The first row `query` finds among `paths` (fx's `expectFirstSearchPath`,
    /// `file_index.zig:2045-2055@481104fd`).
    fn first_row(paths: &[&str], query: &str) -> String {
        rows(&candidates(paths), query)
            .into_iter()
            .next()
            .expect("a row")
    }

    /// Every placement of `query` in `lower[range_start..]` ending at or after
    /// `end_min`, scored: the best (highest, then earliest), and the highest
    /// of those that start before `end_min` (`bruteForceAlignment`,
    /// `file_index.zig:2633-2672@481104fd`).
    fn brute_force(
        path: &[u8],
        lower: &[char],
        range_start: usize,
        end_min: usize,
        query: &[char],
    ) -> (Option<(i32, usize)>, Option<i32>) {
        fn visit(
            at: (&[u8], &[char], usize, usize, &[char]),
            from: usize,
            positions: &mut Vec<usize>,
            out: &mut (Option<(i32, usize)>, Option<i32>),
        ) {
            let (path, lower, range_start, end_min, query) = at;
            if positions.len() == query.len() {
                if positions[query.len() - 1] < end_min {
                    return;
                }
                let offsets: Vec<usize> = (0..=path.len()).collect();
                let (quality, first) = score_alignment(path, &offsets, range_start, positions);
                if positions[0] < end_min {
                    out.1 = Some(out.1.map_or(quality, |peak| peak.max(quality)));
                }
                if out.0.is_none_or(|(best, start)| {
                    quality > best || (quality == best && first < start)
                }) {
                    out.0 = Some((quality, first));
                }
                return;
            }
            for (position, scalar) in lower.iter().enumerate().skip(from) {
                if *scalar == query[positions.len()] {
                    positions.push(position);
                    visit(at, position + 1, positions, out);
                    positions.pop();
                }
            }
        }
        let mut out = (None, None);
        visit(
            (path, lower, range_start, end_min, query),
            range_start,
            &mut Vec::new(),
            &mut out,
        );
        out
    }

    #[test]
    fn the_best_placement_is_what_a_brute_force_search_finds() {
        // "best alignment equals a brute-force search over a small alphabet"
        // (`file_index.zig:2674-2729@481104fd`): every text of one to four
        // of `aAb/-1`, every query of one to three of `ab/-1`, searched from
        // the start, the name, and the start into the name.
        let mut align = Align::default();
        let mut checked = 0;
        for text_len in 1..=4u32 {
            for code in 0..6usize.pow(text_len) {
                let path: Vec<u8> = (0..text_len)
                    .map(|digit| b"aAb/-1"[code / 6usize.pow(digit) % 6])
                    .collect();
                let lower: Vec<char> = path
                    .iter()
                    .map(|b| b.to_ascii_lowercase() as char)
                    .collect();
                let offsets: Vec<usize> = (0..=path.len()).collect();
                let base = path
                    .iter()
                    .rposition(|b| *b == b'/')
                    .map_or(0, |slash| slash + 1);
                for (range_start, end_min) in [(0, 0), (base, base), (0, base)] {
                    for query_len in 1..=3u32 {
                        for query_code in 0..5usize.pow(query_len) {
                            let query: Vec<char> = (0..query_len)
                                .map(|digit| {
                                    "ab/-1".as_bytes()[query_code / 5usize.pow(digit) % 5] as char
                                })
                                .collect();
                            let (expected, peak) =
                                brute_force(&path, &lower, range_start, end_min, &query);
                            if let Some(peak) = peak {
                                assert!(peak <= max_crossing_quality(&query));
                            }
                            let actual = align_best(
                                &lower,
                                &path,
                                &offsets,
                                range_start,
                                end_min,
                                &query,
                                &mut align,
                            );
                            checked += 1;
                            assert_eq!(
                                actual, expected,
                                "{path:?} {query:?} {range_start} {end_min}"
                            );
                            if actual.is_none() {
                                continue;
                            }
                            let positions = &align.positions;
                            assert!(positions[query.len() - 1] >= end_min);
                            assert_eq!(
                                score_alignment(&path, &offsets, range_start, positions),
                                actual.unwrap()
                            );
                            assert!(positions.iter().zip(&query).all(|(at, c)| lower[*at] == *c));
                        }
                    }
                }
            }
        }
        assert!(checked > 600_000, "{checked}");
    }

    #[test]
    fn past_the_cell_budget_the_leftmost_placement_is_scored() {
        // "alignment scores the greedy placement beyond the cell budget"
        // (`file_index.zig:2731-2740@481104fd`).
        let path = [b'a'; 2000];
        let text = ['a'; 2000];
        let offsets: Vec<usize> = (0..=path.len()).collect();
        let mut align = Align::default();
        let found =
            align_best(&text, &path, &offsets, 0, 0, &['a'; 100], &mut align).expect("a placement");
        assert_eq!(align.positions, (0..100).collect::<Vec<_>>());
        assert_eq!(found, score_alignment(&path, &offsets, 0, &align.positions));
    }

    #[test]
    fn the_first_letters_bonus_counts_twice() {
        // `bonus_first_char_multiplier` (`file_index.zig:1420@481104fd`) in
        // `scoreAlignment` (`:1498-1521`). For `ab`, `aab` places `a` at its
        // start (9 after the implied `/`, twice: 34) and `b` a gap later
        // (13): 47; `xaB` runs `aB` from no word start (16, then 16 and the
        // camel bonus 7): 39. Counted once, `aab` makes 38 and goes second.
        assert_eq!(rows(&candidates(&["xaB", "aab"]), "ab"), ["aab", "xaB"]);
    }

    #[test]
    fn the_bold_letters_are_the_names_placement_when_the_folders_loses() {
        // `reconstructMatchSpans` searches the chosen range again
        // (`file_index.zig:1933-1935@481104fd`): the search through the
        // folder ran last and left its own letters. For `abcd`, the name of
        // `ab/c-d-ab-cd.ts` holds one placement, `ab` and `cd` after their
        // `-` (101, and 4: 105), under `max_crossing_quality` (106), so the
        // folder's search runs: `ab` at the start and `c`, `d` after the `/`
        // and the `-` (102). The name's wins, and its letters are bold.
        let found = search(&candidates(&["ab/c-d-ab-cd.ts"]), "abcd");
        assert_eq!(found[0].spans, [7..9, 10..12]);
    }

    #[test]
    fn of_two_placements_as_good_and_as_early_the_dynamic_programs_order_picks_the_bold_letters() {
        // `alignBest` (`file_index.zig:1527-1720@481104fd`) keeps the first
        // of equal candidates it is offered (`betterCandidate`, strict):
        // a cell takes the run from the diagonal before the gap (`:1658-1686`),
        // and the last row's cells are read first to last (`:1689-1704`).
        // `BAbaab` for `abab`: `A b . a b` and `A b a . b` both score 69 from
        // the same `A`; the diagonal into the last `b` comes first, so the
        // bold letters are `Ab`, `ab`.
        let found = search(&candidates(&["BAbaab"]), "abab");
        assert_eq!(found[0].spans, [1..3, 4..6]);
        // `xab-b` for `ab`: `ab` as a run and `a`, then `b` after the `-`,
        // both 36 from the same `a`; the earlier last letter is read first.
        let found = search(&candidates(&["xab-b"]), "ab");
        assert_eq!(found[0].spans, one(1..3));
    }

    #[test]
    fn a_camel_start_and_a_matched_punctuation_earn_their_bonuses() {
        // `bonusFor` (`file_index.zig:1459-1475@481104fd`): an upper-case
        // letter after a lower-case one earns 7, so `axB` (54) ranks above
        // `aab` (47) for `ab`, where without it the two tie and bytes put
        // `aab` first.
        assert_eq!(rows(&candidates(&["aab", "axB"]), "ab"), ["axB", "aab"]);
        // A matched `-` earns 8 and starts a run of 8 (`consecutiveBonus`,
        // `file_index.zig:1493-1496`): `aa-b` scores 79 for `a-b`, `a-xb`
        // 72; without the 8, `aa-b` scores 71 and goes second.
        assert_eq!(
            rows(&candidates(&["a-xb", "aa-b"]), "a-b"),
            ["aa-b", "a-xb"]
        );
    }

    #[test]
    fn a_rows_window_ends_before_the_next_rows_last_place() {
        // The right-hand greedy scan (`file_index.zig:1575-1582@481104fd`)
        // places each letter at its last place before the next letter's,
        // not at its last place in the path. For `ab` over `a`, 2100 `x`,
        // `-ab`, 2000 `x` and a final `a`: row `a` runs to the `a` of `-ab`,
        // 2104 cells in all, so the program finds the run after the `-`.
        // Ended at the final `a`, the rows would be 4106 cells, past the
        // budget, and the leftmost placement would be scored instead.
        let text = format!("a{}-ab{}a", "x".repeat(2100), "x".repeat(2000));
        let (path, chars) = (text.as_bytes(), text.chars().collect::<Vec<_>>());
        let offsets: Vec<usize> = (0..=path.len()).collect();
        let mut align = Align::default();
        let found =
            align_best(&chars, path, &offsets, 0, 0, &['a', 'b'], &mut align).expect("a placement");
        assert_eq!(align.positions, [2102, 2103]);
        assert_eq!(found, score_alignment(path, &offsets, 0, &align.positions));
        let found = align_best(path, path, &offsets, 0, 0, b"ab", &mut align).expect("a placement");
        assert_eq!(align.positions, [2102, 2103], "the same over bytes");
        assert_eq!(found, score_alignment(path, &offsets, 0, &align.positions));
    }

    #[test]
    fn the_cell_budget_is_four_thousand_and_ninety_six_cells() {
        // `max_align_cells` (`file_index.zig:1410@481104fd`) and the check
        // past it (`:1601`). `a`, n `x`, `b`, 2000 `x`, `-ab`: row `a`'s
        // window runs from the first `a` to the last, row `b`'s from the
        // first `b` to the last, n + 2 * 2000 + 8 cells. The best placement
        // is the closing `ab` after its `-`; the leftmost is the first `a`
        // and the first `b`, a gap of n between them.
        let mut align = Align::default();
        for (n, cells, searched) in [(88, 4096, true), (89, 4097, false)] {
            let text = format!("a{}b{}-ab", "x".repeat(n), "x".repeat(2000));
            let (path, chars) = (text.as_bytes(), text.chars().collect::<Vec<_>>());
            let offsets: Vec<usize> = (0..=path.len()).collect();
            assert_eq!(n + 2 * 2000 + 8, cells);
            let found = align_best(&chars, path, &offsets, 0, 0, &['a', 'b'], &mut align)
                .expect("a placement");
            let last = chars.len() - 2;
            let expected = if searched {
                vec![last, last + 1]
            } else {
                vec![0, n + 1]
            };
            assert_eq!(align.positions, expected, "{cells} cells");
            assert_eq!(found, score_alignment(path, &offsets, 0, &align.positions));
        }
    }

    #[test]
    fn the_score_compares_its_tiers_before_its_quality_and_position() {
        // "score comparison ranks tiers before alignment quality and
        // position" (`file_index.zig:2531-2549@481104fd`).
        let base = Score {
            exact: false,
            name: false,
            quality: 10,
            first: 5,
        };
        let exact = Score {
            exact: true,
            quality: 0,
            ..base
        };
        let name = Score {
            name: true,
            quality: 0,
            ..base
        };
        let stronger = Score {
            quality: 11,
            first: 9,
            ..base
        };
        let earlier = Score { first: 4, ..base };
        for (left, right) in [
            (exact, name),
            (name, stronger),
            (stronger, earlier),
            (earlier, base),
        ] {
            assert_eq!(
                score_order(&left, &right),
                Ordering::Less,
                "{left:?} {right:?}"
            );
        }
        assert_eq!(score_order(&base, &base), Ordering::Equal);
    }

    #[test]
    fn the_kept_rows_are_the_best_of_every_path_scored() {
        // fx's "pruned ranking keeps ..." cases (`file_index.zig:2775-2827@
        // 481104fd`): more rows than the list keeps, and the one fx names first.
        let mut paths: Vec<String> = Vec::new();
        paths.extend((0..12).map(|i| format!("src/tools/alpha/tango/x{i}.zig")));
        paths.extend((0..80).map(|i| format!("pkg/{i}/st-at{i}.zig")));
        paths.extend((0..70).map(|i| format!("src/tools/alpha/tango/y{i}.zig")));
        paths.extend(["docs/stat.md", "stat", "src/STATUS.md"].map(String::from));
        let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
        assert_eq!(first_row(&refs, "stat"), "stat");
        let mut paths: Vec<String> = Vec::new();
        paths.extend((0..90).map(|i| format!("pkg/{}/st{i}.zig", i % 7)));
        paths.extend((0..40).map(|i| format!("deep/nested/folder/{i}/stage{i}.md")));
        paths.extend((0..30).flat_map(|i| [format!("a/sx{i}.ts"), format!("b/ts{i}.ts")]));
        paths.extend(
            [
                "zz/st.md",
                "q/s",
                "st",
                "x/s.zig",
                "pkg/0/st0.zig",
                "w/st1.zi",
            ]
            .map(String::from),
        );
        let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
        assert_eq!(first_row(&refs, "st"), "st");
        let mut paths: Vec<String> = (0..70).map(|i| format!("service/{i}/main.zig")).collect();
        paths.extend((0..30).map(|i| format!("x/mainframe{i}.zig")));
        paths.extend(["src/main.zig", "main.zig"].map(String::from));
        let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
        assert_eq!(first_row(&refs, "main"), "main.zig");
        let mut paths: Vec<String> = Vec::new();
        paths.extend((0..40).map(|i| format!("pkg/{i}/vercel-error{i}.ts")));
        paths.extend((0..30).map(|i| format!("docs/{i}/vercel-json/clean{i}.mdx")));
        paths.extend((0..30).map(|i| format!("vercel/cle/x{i}/z.md")));
        paths.extend((0..20).map(|i| format!("v/e/r/c/l/e{i}.txt")));
        paths.extend((0..70).map(|i| format!("lib/{i}/vercle{i}.ts")));
        paths.push("apps/vercle.ts".into());
        let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
        assert_eq!(first_row(&refs, "vercle"), "apps/vercle.ts");
    }

    #[test]
    fn the_rows_rank_as_fxs_ranking_table_says() {
        // "ranking signals follow the accepted descending influence"
        // (`file_index.zig:2444-2516@481104fd`), every case.
        for (paths, query, expected) in [
            (
                &["other/src/core/main.zig", "src/core/main.zig"][..],
                "src/core/main.zig",
                "src/core/main.zig",
            ),
            (
                &["src/main.zig.backup", "src/main.zig"],
                "main.zig",
                "src/main.zig",
            ),
            (
                &["core/guide.txt", "docs/core-guide.txt"],
                "core",
                "docs/core-guide.txt",
            ),
            (
                &["src/core/main.zig", "misc/scattered/memo.zig"],
                "scm",
                "misc/scattered/memo.zig",
            ),
            (
                &["src/xwidget.zig", "src/widgetx.zig"],
                "wid",
                "src/widgetx.zig",
            ),
            (&["a/x/b", "x/a/b"], "ab", "x/a/b"),
            (
                &["src/xxidxx.zig", "src/xidxxx.zig"],
                "id",
                "src/xidxxx.zig",
            ),
            (&["src/axbyc.zig", "src/abczz.zig"], "abc", "src/abczz.zig"),
            (
                &["src/axxxbxc.zig", "src/axbxczz.zig"],
                "abc",
                "src/axbxczz.zig",
            ),
            (
                &["src/abc-long-name.txt", "src/abc.txt"],
                "abc",
                "src/abc.txt",
            ),
        ] {
            assert_eq!(first_row(paths, query), expected, "{query} over {paths:?}");
        }
        // "subsequence scoring prefers basename prefix over mid-path match",
        // "... prefers earlier match position", "... returns no match when
        // query is absent", "... accepts longer query than basename via full
        // path" and "search returns best matches first" (`file_index.zig:2373-2413`).
        assert_eq!(
            rows(
                &candidates(&[
                    "src/wasm_term_main.zig",
                    "src/main.zig",
                    "README.md",
                    "src/core/shared/io.zig",
                    "benchmarks/startup.sh",
                ]),
                "main"
            )[..2],
            ["src/main.zig", "src/wasm_term_main.zig"]
        );
        assert_eq!(
            first_row(&["foo/bar/baz-main.zig", "src/main.zig"], "main"),
            "src/main.zig"
        );
        assert!(rows(&candidates(&["src/main.zig"]), "xyz").is_empty());
        assert_eq!(
            rows(&candidates(&["src/main.zig"]), "src/main"),
            ["src/main.zig"]
        );
    }

    #[test]
    fn a_name_prefix_or_a_whole_word_outranks_letters_scattered_through_a_name() {
        // "a contiguous name prefix ranks above letters scattered across word
        // breaks" (`file_index.zig:2551-2562@481104fd`): all five rows, the
        // first two named.
        let found = rows(
            &candidates(&[
                "spikes/noisy/clusters-a.toml",
                "spikes/noisy/results/conn-storm-pro-activity.csv",
                "spikes/noisy/results/conn-storm-free-activity.csv",
                "STATUS.md",
                "contracts/fixtures/stats_response.json",
            ]),
            "stat",
        );
        assert_eq!(found.len(), 5);
        assert_eq!(
            found[..2],
            ["STATUS.md", "contracts/fixtures/stats_response.json"]
        );
        // "a whole word inside a name ranks above scattered letters" (`file_index.zig:2564-2567`).
        assert_eq!(
            first_row(&["scripts/test-matrix.ts", "src/test-stat.zig"], "stat"),
            "src/test-stat.zig"
        );
        assert_eq!(
            first_row(
                &[
                    "src/ui/inline_dex_view.zig",
                    "src/core/workspace/file_index.zig"
                ],
                "index"
            ),
            "src/core/workspace/file_index.zig"
        );
        // "a query equal to a file name before an extension is an exact
        // match" (`file_index.zig:2569-2571`).
        assert_eq!(
            first_row(
                &[
                    "bench/file_index_bench.zig",
                    "src/core/workspace/file_index.zig"
                ],
                "file_index"
            ),
            "src/core/workspace/file_index.zig"
        );
    }

    #[test]
    fn a_match_may_start_in_the_names_own_folder_and_one_that_ends_in_a_folder_is_last() {
        // "a match across the file's folder and its name ranks above letters
        // scattered through a name" (`file_index.zig:2573-2575@481104fd`).
        assert_eq!(
            first_row(
                &[
                    "packages/vercel-error.ts",
                    "docs/vercel-json/clean-urls.mdx"
                ],
                "vercle"
            ),
            "docs/vercel-json/clean-urls.mdx"
        );
        // "a name match beats a match of the same shape through the folder"
        // (`file_index.zig:2577-2580`).
        assert_eq!(
            first_row(&["src/ab/cd.ts", "src/x/ab-cd.ts"], "abcd"),
            "src/x/ab-cd.ts"
        );
        // "a match that ends in a folder ranks below every name match"
        // (`file_index.zig:2582-2584`).
        assert_eq!(
            first_row(
                &["src/components/card.tsx", "src/use-comparison.ts"],
                "comp"
            ),
            "src/use-comparison.ts"
        );
        // "a match may start in the file's own folder but not further up"
        // (`file_index.zig:2586-2597`).
        assert_eq!(
            rows(&candidates(&["ab/x/cd.ts", "zz/x-a-b-c-d.ts"]), "abcd"),
            ["zz/x-a-b-c-d.ts", "ab/x/cd.ts"]
        );
    }

    #[test]
    fn the_bold_letters_are_the_placement_that_was_scored() {
        // "highlights of a folder-and-name match cover both parts"
        // (`file_index.zig:2599-2613@481104fd`): bytes 5..8 and 17..20.
        let found = search(&candidates(&["docs/vercel-json/clean-urls.mdx"]), "vercle");
        assert_eq!(found[0].spans, [5..8, 17..20]);
        // "highlights mark the placement that was scored" (`file_index.zig:2615-2630`).
        for found in search(&candidates(&["src/test-stat.zig", "STATUS.md"]), "stat") {
            let start = if found.path == "STATUS.md" { 0 } else { 9 };
            assert_eq!(found.spans, one(start..start + 4), "{}", found.path);
        }
        // "typed search supports abbreviated subsequences and caller-owned
        // spans" (`file_index.zig:2415-2442`): one row, its bold letters `fiidx`.
        let paths = [
            "src/core/workspace/",
            "src/core/workspace/file_index.zig",
            "src/core/workspace/workspace_files.zig",
        ];
        let found = search(&candidates(&paths), "FIIDX");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].path, paths[1]);
        let bold: String = found[0]
            .spans
            .iter()
            .map(|span| &found[0].path[span.clone()])
            .collect();
        assert_eq!(bold, "fiidx");
    }

    #[test]
    fn ties_go_to_the_shorter_path_then_bytes_whatever_the_order_or_the_kind() {
        // "ranking is independent of discovery order and resolves ties
        // bytewise" (`file_index.zig:2518-2529@481104fd`).
        assert_eq!(
            first_row(&["src/zeta.txt", "src/beta.txt"], "ta"),
            "src/beta.txt"
        );
        assert_eq!(
            first_row(&["src/beta.txt", "src/zeta.txt"], "ta"),
            "src/beta.txt"
        );
        // "file and directory candidates have no ranking priority" (`file_index.zig:2829-2850`).
        for paths in [
            ["src/zeta.txt", "src/beta.txt/"],
            ["src/zeta.txt/", "src/beta.txt"],
        ] {
            let found = search(&candidates(&paths), "ta");
            let found: Vec<&str> = found.iter().map(|found| found.path.as_str()).collect();
            assert_eq!(found, ["src/beta.txt", "src/zeta.txt"], "{paths:?}");
        }
    }

    #[test]
    fn case_is_folded_and_a_mixed_unicode_path_ranks_by_the_same_score() {
        // "search is case-insensitive" (`file_index.zig:2885-2907@481104fd`).
        let paths = ["README.md", "src/Main.zig"];
        assert_eq!(first_row(&paths, "README"), "README.md");
        assert_eq!(first_row(&paths, "readme"), "README.md");
        assert_eq!(first_row(&paths, "main"), "src/Main.zig");
        // "Unicode search compares mixed basename and full-path scores" (`file_index.zig:3124-3135`).
        assert_eq!(
            rows(
                &candidates(&["docs/\u{c4}rger.txt", "\u{c4}rger/docs.txt"]),
                "\u{e4}rger"
            ),
            ["docs/\u{c4}rger.txt", "\u{c4}rger/docs.txt"]
        );
        // "bitmap prefilter does not reject when query has no a-z letters" (`file_index.zig:3152-3164`).
        assert_eq!(
            first_row(&["docs/123.md", "src/main.zig"], "123"),
            "docs/123.md"
        );
    }

    #[test]
    fn an_added_directorys_entries_rank_by_the_same_score_on_their_absolute_paths() {
        // An added root's entries are absolute (`appendDiscoveredCandidate`,
        // `file_index.zig:1086-1105@481104fd`) and scored as any other
        // (`rankTopN`, `:1330-1403`). `p5-at-rank`'s `road-map` on fx: the
        // workspace's name before its extension, the added one's, then the
        // added one's other name.
        let paths = [
            "docs/road_map.md",
            "/run/extra/notes/road_map.md",
            "/run/extra/roadmaps/x_map.txt",
        ];
        assert_eq!(rows(&candidates(&paths), "road_map"), paths);
        // A match that ends in one of its folders ranks below every name
        // match (`file_index.zig:2582-2584`): `ext` is only in `extra`.
        assert_eq!(
            rows(
                &candidates(&["/run/extra/notes/a.md", "docs/text.md"]),
                "ext"
            ),
            ["docs/text.md", "/run/extra/notes/a.md"]
        );
    }

    #[test]
    fn a_folder_row_that_only_the_whole_path_matches_is_bold_through_the_whole_path() {
        // `workspace-index`'s `at-plan`: `@plan` matches the added folder
        // `<tmp>/fxcap.<run>/extra/notes` only through its whole path, the
        // capture's random run directory included (`choosePlacement`'s last
        // search, `file_index.zig:1815-1818@481104fd`). The head sweep's fx
        // run was `fxcap.qnVERa` and its xfx run `fxcap.YTZTDF`: the first
        // has an `n` close to the `a` before it, the second none. Here the
        // scratch root is a short stand-in with the same choice: the run
        // name's `n` wins for `qnVERa`, the `n` of `notes` for `YTZTDF`,
        // each the best placement in its own path, as the two captures bold.
        let bold = |run: &str| -> String {
            let path = format!("/srv/pr/cl/at/x/039/fxcap.{run}/extra/notes/");
            let found = search(&candidates(&[&path]), "plan");
            let path = &found[0].path;
            let around: Vec<&str> = found[0]
                .spans
                .iter()
                .map(|span| &path[span.start - 3..span.end])
                .collect();
            around.join(" ")
        };
        assert_eq!(bold("qnVERa"), "rv/p r/cl cl/a p.qn");
        assert_eq!(bold("YTZTDF"), "rv/p r/cl cl/a ra/n");
    }

    #[test]
    fn a_browsed_folder_ranks_its_names_by_the_same_score() {
        // The listing scores each name with `NameQuery` (`path_completion.zig:
        // 56`, `:93`, `:234@481104fd`): the name is the whole text, its
        // first character follows a separator (`bonusAt`,
        // `file_index.zig:1477-1481`), and the order is `better`'s
        // (`:1302-1309`). So a name ranks there as it does under a folder:
        // the whole word first (`file_index.zig:2564-2567`).
        let names = entries(&[
            "test-matrix.ts",
            "test-stat.zig",
            "inline_dex_view.zig",
            "file_index.zig",
        ]);
        assert_eq!(listed(&names, "src/stat")[0].0, "src/test-stat.zig");
        assert_eq!(listed(&names, "src/index")[0].0, "src/file_index.zig");
        let found = rank_listing(&names, &explicit("src/stat").expect("a path"));
        assert_eq!(found[0].spans, one(9..13), "the bold word, past the prefix");
        // "path completion fuzzy ranking retains the best bounded result"
        // (`path_completion.zig:497-516`).
        let names = entries(&["Desktop/", "ktop/", "OtherDesktop/"]);
        assert_eq!(listed(&names, "./ktop")[0].0, "./ktop");
        assert_eq!(listed(&names, "./dsktp")[0].0, "./Desktop");
    }

    #[test]
    fn the_p5_at_rank_workspace_ranks_as_fx_shows_it() {
        // The rows fx v0.0.16 shows for the scenario `p5-at-rank` (the AT-RANK
        // receipt's fx/), over the workspace it copies.
        let entries = discover(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/fxcap/scenarios/p5-at-rank-ws"),
            None,
            &AtomicBool::new(false),
        )
        .expect("a walk");
        let shown = |query: &str| -> Vec<String> {
            let mut found = rows(&entries, query);
            found.truncate(10);
            found
        };
        assert_eq!(
            shown("scm"),
            [
                "misc/scattered/memo.zig",
                "scripts/test-matrix.ts",
                "src/main.rs",
                "src/Main.zig",
                "src/components/",
                "src/use-comparison.ts",
                "docs/vercel-json/clean-urls.mdx",
                "src/core/main.zig",
                "src/components/card.tsx",
            ]
        );
        assert_eq!(
            shown("stat"),
            [
                "STATUS.md",
                "contracts/fixtures/stats_response.json",
                "src/test-stat.zig",
                "src/beta.txt",
                "src/zeta.txt",
                "scripts/test-matrix.ts",
                "src/components/card.tsx",
            ]
        );
        assert_eq!(
            shown("index"),
            [
                "bench/file_index_bench.zig",
                "src/core/workspace/file_index.zig",
                "src/ui/inline_dex_view.zig",
            ]
        );
        assert_eq!(
            shown("file_index"),
            [
                "src/core/workspace/file_index.zig",
                "bench/file_index_bench.zig"
            ]
        );
        assert_eq!(
            shown("main.rs"),
            ["main.rs", "src/main.rs", "main/other.rs", "x/remain.rs"]
        );
        assert_eq!(
            shown("vercle"),
            [
                "docs/vercel-json/clean-urls.mdx",
                "packages/vercel-error.ts"
            ]
        );
        assert_eq!(shown("abcd"), ["zz/x-a-b-c-d.ts", "ab/x/cd.ts"]);
        assert_eq!(
            shown("comp"),
            [
                "src/components/",
                "src/use-comparison.ts",
                "docs/road_map.md",
                "src/components/card.tsx",
            ]
        );
        assert_eq!(
            shown("ab"),
            [
                "ab/",
                "zz/x-a-b-c-d.ts",
                "x/a/b",
                "ab/x/",
                "ab/x/cd.ts",
                "a/x/b"
            ]
        );
        assert_eq!(
            shown("ta"),
            [
                "scripts/test-matrix.ts",
                "src/test-stat.zig",
                "STATUS.md",
                "contracts/fixtures/stats_response.json",
                "src/beta.txt",
                "src/zeta.txt",
                "contracts/",
                "src/components/card.tsx",
                "contracts/fixtures/",
            ]
        );
        assert_eq!(
            shown("MAIN"),
            [
                "main/",
                "main.rs",
                "src/main.rs",
                "src/Main.zig",
                "src/core/main.zig",
                "x/remain.rs",
                "src/use-comparison.ts",
                "main/other.rs",
            ]
        );
    }

    /// fx's benchmark inventory (its arrays and `generateInventory`,
    /// `benchmarks/file_index_bench.zig:34-70`, `:82-135@481104fd`): entry `i` a file
    /// when `i` is even and a directory when it is odd, the first four fixed.
    fn fx_inventory(count: usize) -> Vec<Candidate> {
        const SUBSYSTEMS: [&str; 14] = [
            "apps",
            "packages",
            "libs",
            "services",
            "tools",
            "plugins",
            "extensions",
            "modules",
            "infra",
            "vendor",
            "web",
            "mobile",
            "workers",
            "functions",
        ];
        const AREAS: [&str; 16] = [
            "core",
            "utils",
            "components",
            "hooks",
            "api",
            "db",
            "auth",
            "ui",
            "server",
            "client",
            "cli",
            "tests",
            "docs",
            "internal",
            "shared",
            "config",
        ];
        const SUBAREAS: [&str; 16] = [
            "common",
            "helpers",
            "types",
            "schemas",
            "models",
            "views",
            "routes",
            "handlers",
            "services",
            "stores",
            "context",
            "providers",
            "layouts",
            "pages",
            "forms",
            "fields",
        ];
        const NAMES: [&str; 32] = [
            "index", "main", "app", "Button", "Input", "Modal", "helper", "util", "types",
            "schema", "model", "view", "route", "handler", "service", "store", "hook", "context",
            "provider", "layout", "page", "widget", "icon", "form", "field", "label", "table",
            "list", "card", "dialog", "menu", "nav",
        ];
        const EXTENSIONS: [&str; 11] = [
            ".ts", ".tsx", ".js", ".jsx", ".zig", ".py", ".rs", ".go", ".md", ".json", ".yaml",
        ];
        (0..count)
            .map(|i| {
                let path = match i {
                    0 => "docs/\u{c4}rger-file.txt".to_string(),
                    1 => "docs/\u{212a}elvin".to_string(),
                    2 => "src/core/workspace/file_index.zig".to_string(),
                    3 => "apps/web/api-route-handler".to_string(),
                    _ => {
                        let mut path = format!(
                            "{}/{}/",
                            SUBSYSTEMS[i % SUBSYSTEMS.len()],
                            AREAS[(i / SUBSYSTEMS.len()) % AREAS.len()]
                        );
                        let depth = (i / 7) % 3;
                        if depth >= 1 {
                            path += SUBAREAS[(i / 17) % SUBAREAS.len()];
                            path.push('/');
                        }
                        if depth >= 2 {
                            path += SUBAREAS[(i / 23) % SUBAREAS.len()];
                            path.push('/');
                        }
                        path += &format!("{}{i}", NAMES[(i / 31) % NAMES.len()]);
                        if i % 2 == 0 {
                            path += EXTENSIONS[(i / 13) % EXTENSIONS.len()];
                        }
                        path
                    }
                };
                let kind = if i % 2 == 0 {
                    Kind::File
                } else {
                    Kind::Directory
                };
                Candidate::new(path, kind)
            })
            .collect()
    }

    /// Every entry placed in full by [`score`] -- no kept set, no cut, no
    /// mask, none of the facts an entry keeps, an ASCII path read scalar by
    /// scalar -- then the whole order (a stable sort, so a tie keeps the
    /// index's order), the best [`MOST_RESULTS`] and their bold letters:
    /// AT-RANK's search, which the pruned one must match.
    fn unpruned(entries: &[Candidate], query: &str) -> Vec<Found> {
        let (folded, mut s) = (Folded::new(query), Scratch::default());
        let mut ranked: Vec<(Score, &Candidate)> = entries
            .iter()
            .filter_map(|entry| Some((score(&entry.path, &folded, EVERY, &mut s)?, entry)))
            .collect();
        ranked.sort_by(|left, right| {
            better((&left.0, &left.1.path), (&right.0, &right.1.path), false)
        });
        ranked.truncate(MOST_RESULTS);
        ranked
            .into_iter()
            .map(|(_, entry)| Found {
                path: entry.path.clone(),
                kind: entry.kind,
                spans: spans(&entry.path, &folded, &mut s),
            })
            .collect()
    }

    /// `search` against [`unpruned`]: the same rows, kinds and bold letters.
    fn same_as_unpruned(entries: &[Candidate], query: &str) -> Vec<Found> {
        let found = search(entries, query);
        assert_eq!(found, unpruned(entries, query), "{query:?}");
        found
    }

    fn owned(paths: &[String]) -> Vec<Candidate> {
        candidates(&paths.iter().map(String::as_str).collect::<Vec<_>>())
    }

    #[test]
    fn each_entry_keeps_its_lower_case_its_name_and_its_letters() {
        // fx's `buildFromRaw` cases (`file_index.zig:2092-2128@481104fd`):
        // the lower-cased path, the name's start and the a-z mask, each
        // stored as the entry is.
        let entries = candidates(&[
            "a/b.zig",
            "CAMEL.md",
            "a/b/c.txt",
            "root.txt",
            "src/Main.zig",
            "README.md",
            "docs/\u{c4}rger",
        ]);
        assert_eq!(&*entries[1].lower, b"camel.md");
        assert_eq!(&entries[2].path[entries[2].name..], "c.txt");
        assert_eq!(&entries[3].path[entries[3].name..], "root.txt");
        assert_eq!(entries[4].mask, letters(b"src/main.zig"));
        assert_eq!(entries[5].mask, letters(b"readme.md"));
        assert_eq!(letters(b"az-AZ"), 1 | 1 << 25);
        // A byte past ASCII marks the path, and its letters never reject it.
        assert_eq!(entries[0].mask & NON_ASCII, 0);
        assert_eq!(entries[6].mask & NON_ASCII, NON_ASCII);
    }

    #[test]
    fn a_path_without_one_of_the_querys_letters_is_never_placed() {
        // `rankTopN`'s reject (`file_index.zig:1371-1373@481104fd`): an
        // ASCII path is placed only if it has every letter the query has,
        // and an ASCII query; one with another byte, always.
        let entry = |path: &str| Candidate::new(path.to_string(), Kind::File);
        assert!(admits(&entry("src/Main.rs"), &Folded::new("MaIn")));
        assert!(admits(&entry("src/main.rs"), &Folded::new("s/m.r")));
        assert!(!admits(&entry("src/main.rs"), &Folded::new("mz")));
        assert!(!admits(&entry("src/main.rs"), &Folded::new("zm")));
        assert!(!admits(&entry("src/main.rs"), &Folded::new("m\u{e4}")));
        assert!(admits(&entry("docs/\u{212a}elvin"), &Folded::new("klv")));
        assert!(admits(&entry("docs/\u{c4}rger"), &Folded::new("zz")));
    }

    #[test]
    fn the_pruned_search_keeps_the_rows_of_placing_every_path() {
        // fx's own cases (`file_index.zig:2775-2827@481104fd`), in their
        // order, which is not the index's byte order: a later entry has to
        // displace the worst one kept.
        let mut paths: Vec<String> = (0..12)
            .map(|i| format!("src/tools/alpha/tango/x{i}.zig"))
            .chain((0..80).map(|i| format!("pkg/{i}/st-at{i}.zig")))
            .chain((0..70).map(|i| format!("src/tools/alpha/tango/y{i}.zig")))
            .collect();
        paths.extend(["docs/stat.md", "stat", "src/STATUS.md"].map(String::from));
        assert_eq!(same_as_unpruned(&owned(&paths), "stat")[0].path, "stat");

        // Crowded names: more than the kept rows at the top score for `s`
        // and `st`, with equal-length ties, then shorter and exact paths.
        let mut paths: Vec<String> = (0..90)
            .map(|i| format!("pkg/{}/st{i}.zig", i % 7))
            .chain((0..40).map(|i| format!("deep/nested/folder/{i}/stage{i}.md")))
            .chain((0..30).flat_map(|i| [format!("a/sx{i}.ts"), format!("b/ts{i}.ts")]))
            .collect();
        paths.extend(["zz/st.md", "q/s", "st", "x/s.zig", "w/st1.zi"].map(String::from));
        let entries = owned(&paths);
        same_as_unpruned(&entries, "s");
        assert_eq!(same_as_unpruned(&entries, "st")[0].path, "st");

        // Once every kept row is exact, only an exact match is placed.
        let mut paths: Vec<String> = (0..70)
            .map(|i| format!("service/{i}/main.zig"))
            .chain((0..30).map(|i| format!("x/mainframe{i}.zig")))
            .collect();
        paths.extend(["src/main.zig", "main.zig"].map(String::from));
        assert_eq!(same_as_unpruned(&owned(&paths), "main")[0].path, "main.zig");

        // Names of every strength, matches through the own folder, matches
        // that end in a folder, and perfect names that end the crossing search.
        let mut paths: Vec<String> = (0..40)
            .map(|i| format!("pkg/{i}/vercel-error{i}.ts"))
            .chain((0..30).map(|i| format!("docs/{i}/vercel-json/clean{i}.mdx")))
            .chain((0..30).map(|i| format!("vercel/cle/x{i}/z.md")))
            .chain((0..20).map(|i| format!("v/e/r/c/l/e{i}.txt")))
            .collect();
        let cut = paths.len();
        paths.extend((0..70).map(|i| format!("lib/{i}/vercle{i}.ts")));
        paths.push("apps/vercle.ts".to_string());
        let found = same_as_unpruned(&owned(&paths), "vercle");
        assert_eq!(found[0].path, "apps/vercle.ts");
        // Without the perfect names the crossing search is never cut.
        same_as_unpruned(&owned(&paths[..cut]), "vercle");

        // Not in fx's cases: the cut is past the crossing ceiling, not at it
        // (`>`, `file_index.zig:1368@481104fd`). For `ab` each `w<i>/a/b`
        // crosses from `a` into `b` at the ceiling itself (34 and 22: 56),
        // so the worst kept is one; `a/b`, as good and shorter, comes last
        // and still enters, first.
        let mut paths: Vec<String> = (0..40).map(|i| format!("w{i}/a/b")).collect();
        paths.push("a/b".to_string());
        assert_eq!(max_crossing_quality(&['a', 'b']), 56);
        assert_eq!(same_as_unpruned(&owned(&paths), "ab")[0].path, "a/b");
    }

    #[test]
    fn the_pruned_search_finds_what_placing_every_path_finds() {
        // Every prefix of each query, over `p5-at-rank`'s workspace and over
        // a generated tree in no particular order: the first entries of fx's
        // benchmark inventory, and crowds that fill the kept rows with exact
        // names, perfect names of many lengths, crossings and folder
        // matches, mixed case, scalars past ASCII -- a final sigma and a
        // capital one among them, which the fold keeps apart and joins
        // (CASE-FOLD-SIMPLE) -- and absolute paths. Every path once; a path
        // listed twice is `a_path_listed_twice_keeps_its_place_in_the_index`.
        let workspace = discover(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/fxcap/scenarios/p5-at-rank-ws"),
            None,
            &AtomicBool::new(false),
        )
        .expect("a walk");
        let mut paths: Vec<String> = fx_inventory(2400)
            .into_iter()
            .map(|entry| match entry.kind {
                Kind::File => entry.path,
                Kind::Directory => entry.path + "/",
            })
            .collect();
        for i in 0..48 {
            paths.extend([
                format!("pkg/{}/st{i}.zig", i % 7),
                format!("deep/nested/{i}/stage{i}.md"),
                format!("service/{i}/main.zig"),
                format!("x/mainframe{i}.zig/"),
                format!("docs/{i}/vercel-json/clean{i}.mdx"),
                format!("vercel/cle/x{i}/z.md"),
                format!("lib/{i}/vercle{i}.ts"),
                format!("src/{i}/Main{i}.Zig"),
                format!("STATUS{i}.md"),
                format!("docs/{i}/\u{c4}rger-{i}.txt"),
                format!("docs/\u{212a}elvin{i}/"),
                format!("caf\u{e9}/R\u{e9}sum\u{e9}{i}.md"),
                format!("\u{65e5}\u{672c}/\u{30d5}\u{30a1}\u{30a4}\u{30eb}{i}.md"),
                format!("/run/extra/notes/road_map{i}.md"),
                format!("ab/x{i}/cd.ts"),
                format!("docs/{i}/\u{3c2}igma-final{i}.txt"),
                format!("\u{3a3}IGMA/{i}/\u{3c3}igma{i}.md"),
            ]);
        }
        paths.extend(
            [
                "main.zig",
                "stat",
                "st",
                "q/s",
                "apps/vercle.ts",
                "ab/c-d-ab-cd.ts",
            ]
            .map(String::from),
        );
        paths.sort();
        paths.dedup();
        // A fixed shuffle (an LCG), so a winner can arrive after the set is full.
        let mut seed: u64 = 0x2545_f491_4f6c_dd1d;
        for at in (1..paths.len()).rev() {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            paths.swap(at, (seed >> 33) as usize % (at + 1));
        }
        let generated = owned(&paths);
        let queries = [
            "src/core/workspace/file_index.zig",
            "scwfi",
            "ae",
            "ashr",
            "zzz-no-match-\u{3a9}",
            "api-route",
            "\u{e4}rf",
            "Klv",
            "stat",
            "MAIN.zig",
            "service/main",
            "vercle",
            "road_map",
            "abcd",
            "comp",
            "scm",
            "index",
            "R\u{e9}sum\u{e9}",
            "\u{30d5}\u{30a1}\u{30a4}\u{30eb}",
            "x/",
            "\u{3c3}igma",
            "\u{3a3}\u{3c2}",
        ];
        for entries in [&workspace, &generated] {
            for query in queries {
                for (end, _) in query.char_indices().skip(1).chain([(query.len(), ' ')]) {
                    same_as_unpruned(entries, &query[..end]);
                }
            }
        }
    }

    #[test]
    fn a_path_listed_twice_keeps_its_place_in_the_index() {
        // A file and a directory of one path, which `merged` keeps both of,
        // tie on everything fx compares (`rankedCandidateBetter`,
        // `file_index.zig:1892-1899@481104fd`). At the cut-off, the one
        // earlier in the index is kept, before the set is full or after.
        // fx keeps the one in the earlier slot (`findWorstSlot`, `:1901-1907`),
        // which a replacement can make the later one.
        let crowd: Vec<String> = (0..31).map(|i| format!("ab{i}")).collect();
        for (pair, kept) in [
            (["zz/ab-x/", "zz/ab-x"], Kind::Directory),
            (["zz/ab-x", "zz/ab-x/"], Kind::File),
        ] {
            for pair_first in [true, false] {
                let mut paths: Vec<String> = pair.iter().map(|p| p.to_string()).collect();
                if pair_first {
                    paths.extend(crowd.iter().cloned());
                } else {
                    paths.splice(0..0, crowd.iter().cloned());
                }
                let found = same_as_unpruned(&owned(&paths), "ab");
                assert_eq!(found.len(), MOST_RESULTS);
                assert_eq!((found[31].path.as_str(), found[31].kind), ("zz/ab-x", kept));
            }
        }
    }

    #[test]
    fn a_path_of_the_longest_length_is_placed_and_one_past_it_is_not() {
        // `MOST_PATH_BYTES` (`max_path_len`, `file_index.zig:26@481104fd`):
        // an ASCII path of 2048 bytes is placed on its kept bytes as any
        // other, and one of 2049, which no index holds, is placed by
        // neither search.
        let long = |n: usize| format!("{}/stat.md", "x".repeat(n - "/stat.md".len()));
        let paths = vec![
            long(MOST_PATH_BYTES),
            long(MOST_PATH_BYTES + 1),
            "a/stat".to_string(),
        ];
        assert_eq!(paths[0].len(), MOST_PATH_BYTES);
        let found = same_as_unpruned(&owned(&paths), "stat");
        assert_eq!(found.len(), 2);
        assert_eq!(found[1].path.len(), MOST_PATH_BYTES);
    }

    #[test]
    fn a_keystroke_places_no_more_than_its_cuts_allow() {
        // The cuts change no row, so only the work they save shows them:
        // the searches `align_best` runs for every prefix of fx's eight
        // benchmark queries over its 100,000-entry inventory, counted, not
        // timed, so the count holds on any machine and in a debug build. A
        // search that places a folder match, a crossing or a long path that
        // could no longer enter (`file_index.zig:1344-1370@481104fd`) passes
        // its query's ceiling.
        let entries = fx_inventory(MOST_INDEXED);
        // Each ceiling is the count at AT-RANK-PERF's round 3, whose masks
        // spare most searches a part of the path cannot hold. Without the
        // folder cut all but the fifth and the seventh go past theirs;
        // without the crossing cut the second and the sixth; without the
        // length limit all but the fifth, the seventh and the last (the
        // AT-RANK-PERF receipt, `r3-counts.txt`).
        let ceilings = [
            ("src/core/workspace/file_index.zig", 154_620),
            ("scwfi", 5_084),
            ("ae", 64_867),
            ("ashr", 96_795),
            ("zzz-no-match-\u{3a9}", 20_284),
            ("api-route", 13_989),
            ("\u{e4}rf", 7),
            ("Klv", 7_463),
        ];
        let mut over = Vec::new();
        for (query, ceiling) in ceilings {
            let searches: usize = query
                .char_indices()
                .skip(1)
                .map(|(end, _)| end)
                .chain([query.len()])
                .map(|end| {
                    let mut s = Scratch::default();
                    rank_top(&entries, &Folded::new(&query[..end]), &mut s);
                    s.align.searches
                })
                .sum();
            eprintln!("{query:36} {searches:>9} searches, ceiling {ceiling}");
            if searches > ceiling {
                over.push((query, searches, ceiling));
            }
        }
        assert!(over.is_empty(), "past the ceiling: {over:?}");
    }

    #[test]
    fn a_refresh_and_an_added_directory_are_searched_by_their_own_paths() {
        // fx works out a path's facts as it builds each index
        // (`fillProgressive`, `file_index.zig:209-222@481104fd`), so a
        // refresh's are its own, and an added directory's are its absolute
        // paths' (`appendDiscoveredCandidate`, `:1086-1105`).
        let own = |files: &Files| {
            let index = files.index.as_ref().expect("an index");
            assert!(index
                .iter()
                .all(|entry| *entry == Candidate::new(entry.path.clone(), entry.kind)));
        };
        let dir = seeded();
        let mut files = Files::new(dir.path().to_path_buf(), None);
        files.git = None;
        files.refresh_index();
        settled(&mut files, |files| !files.busy());
        own(&files);
        std::fs::rename(
            dir.path().join("src/deep/sched.rs"),
            dir.path().join("src/deep/Quartz.rs"),
        )
        .expect("a rename");
        files.refresh_index();
        settled(&mut files, |files| !files.busy());
        own(&files);
        match files.lookup("qrtz") {
            Lookup::Rows(found) => {
                assert_eq!(found[0].path, "src/deep/Quartz.rs");
                assert_eq!(found[0].spans, [9..10, 12..15]);
            }
            other => panic!("{other:?}"),
        }
        let extra = tempfile::tempdir().expect("a temp dir");
        write(extra.path(), "notes/Zebra.md");
        files.change_scope(vec![extra.path().to_path_buf()]);
        files.refresh_index();
        settled(&mut files, |files| !files.busy());
        own(&files);
        let zebra = extra.path().join("notes/Zebra.md");
        match files.lookup("zebra") {
            Lookup::Rows(found) => assert_eq!(Some(found[0].path.as_str()), zebra.to_str()),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_keystroke_over_a_hundred_thousand_entries_costs_less_than_a_frame() {
        // fx's own gate, over its own inventory and queries
        // (`benchmarks/file_index_bench.zig:16-31`, `:82-135@481104fd`):
        // 100,000 entries, p95 under 16 ms a search. Here every prefix of
        // each query is timed, as every keystroke looks the query up again
        // ([`Files::lookup`]), and the p95 is held to xfx's frame: the lookup
        // runs on the thread that paints, which comes round every `TICK`
        // (8 ms, `event_loop.rs`). It asserts under optimization only, where
        // `scripts/check-at-search-cost.sh` runs it alone; a debug build --
        // the ordinary `cargo test` -- searches each whole query once and
        // asserts nothing. `XFX_AT_BENCH_TREE` names a tree on disk to walk
        // in place of the inventory (the AT-RANK receipt's `gen_tree.py`).
        let entries = match std::env::var_os("XFX_AT_BENCH_TREE") {
            Some(tree) => {
                discover(Path::new(&tree), None, &AtomicBool::new(false)).expect("a walk")
            }
            None => fx_inventory(MOST_INDEXED),
        };
        eprintln!("entries: {}", entries.len());
        let measured = !cfg!(debug_assertions);
        let (warm, passes) = if measured { (3, 23) } else { (0, 1) };
        let queries = [
            ("exact", "src/core/workspace/file_index.zig"),
            ("boundary", "scwfi"),
            ("low-selectivity", "ae"),
            ("long-gap", "ashr"),
            ("no-match", "zzz-no-match-\u{3a9}"),
            ("punctuation", "api-route"),
            ("unicode", "\u{e4}rf"),
            ("unicode-fold", "Klv"),
        ];
        let percentile = |sorted: &[Duration], fraction: f64| {
            sorted[((sorted.len() - 1) as f64 * fraction) as usize]
        };
        let mut every = Vec::new();
        let mut whole = Vec::new();
        // The one slowest search, by prefix and pass, so a lone outlier on a
        // runner can be told from a slow prefix.
        let mut slowest: (Duration, &str, usize) = (Duration::ZERO, "", 0);
        for (name, query) in queries {
            let mut timings = Vec::new();
            let mut hits = 0;
            for (end, _) in query.char_indices().skip(1).chain([(query.len(), ' ')]) {
                if !measured && end < query.len() {
                    continue;
                }
                for pass in 0..passes {
                    let started = Instant::now();
                    hits = std::hint::black_box(search(&entries, &query[..end])).len();
                    let took = started.elapsed();
                    if pass >= warm {
                        if took > slowest.0 {
                            slowest = (took, &query[..end], pass);
                        }
                        timings.push(took);
                        if end == query.len() {
                            whole.push(took);
                        }
                    }
                }
            }
            timings.sort();
            eprintln!(
                "{name:16} {:>2} keys {hits:>2} rows p50 {:?} p95 {:?} max {:?}",
                query.chars().count(),
                percentile(&timings, 0.5),
                percentile(&timings, 0.95),
                timings[timings.len() - 1]
            );
            every.extend(timings);
        }
        every.sort();
        whole.sort();
        let p95 = percentile(&every, 0.95);
        eprintln!(
            "every keystroke: {} searches p50 {:?} p95 {p95:?} p99 {:?} max {:?}",
            every.len(),
            percentile(&every, 0.5),
            percentile(&every, 0.99),
            every[every.len() - 1]
        );
        eprintln!(
            "whole queries (fx's gate): {} searches p50 {:?} p95 {:?}",
            whole.len(),
            percentile(&whole, 0.5),
            percentile(&whole, 0.95)
        );
        eprintln!(
            "slowest: {:?} at {:?}, pass {} of {passes}",
            slowest.0, slowest.1, slowest.2
        );
        assert!(
            !measured || p95 < Duration::from_millis(8),
            "a keystroke's search took {p95:?} at the 95th percentile, against an 8 ms frame"
        );
    }

    #[test]
    fn a_path_shaped_query_browses_rather_than_searches() {
        for query in [
            "~",
            ".",
            "..",
            "~/Dow",
            "/tmp/fi",
            "./src/",
            "../shared/",
            "src/core/",
        ] {
            assert!(is_explicit(query), "{query}");
        }
        for query in ["main", "", "~notes", "a.b"] {
            assert!(!is_explicit(query), "{query}");
        }
        // `path_completion.zig` test "classifies and splits only path-shaped
        // queries".
        let parsed = explicit("../shared/na").expect("a path");
        assert_eq!(
            (parsed.parent, parsed.prefix.as_str(), parsed.name),
            ("../shared", "../shared/", "na")
        );
        let parsed = explicit("/fi").expect("a path");
        assert_eq!(
            (parsed.parent, parsed.prefix.as_str(), parsed.name),
            ("/", "/", "fi")
        );
        let parsed = explicit("~").expect("a path");
        assert_eq!(
            (parsed.parent, parsed.prefix.as_str(), parsed.name),
            ("~", "~/", "")
        );
    }

    #[test]
    fn a_browsed_directory_lists_its_entries_by_name() {
        let dir = seeded();
        let entries =
            list_directory(&dir.path().join("src"), &AtomicBool::new(false)).expect("a listing");
        let browse = |query: &str| -> Vec<String> {
            rank_listing(&entries, &explicit(query).expect("a path"))
                .into_iter()
                .map(|found| match found.kind {
                    Kind::File => found.path,
                    Kind::Directory => format!("{}/", found.path),
                })
                .collect()
        };
        // `fx-composer-40x120-at-dir.txt`: `@src/` lists the directory's own
        // entries in byte order, the directory with its slash -- under the
        // folder itself since fx v0.0.16 (7c8ef4a3; `filepicker`'s `at-dir`).
        assert_eq!(
            browse("src/"),
            ["src/", "src/deep/", "src/lib.rs", "src/main.rs"]
        );
        assert_eq!(browse("src/li"), ["src/lib.rs"]);
        let found = rank_listing(&entries, &explicit("src/li").expect("a path"));
        assert_eq!(
            found[0].spans,
            one(4..6),
            "the span is in the shown path, past the prefix"
        );
        assert!(list_directory(&dir.path().join("missing"), &AtomicBool::new(false)).is_err());
    }

    /// The rows `query` makes of `entries`, a directory with its slash.
    fn listed(entries: &[(String, Kind)], query: &str) -> Vec<(String, Kind)> {
        rank_listing(entries, &explicit(query).expect("a path"))
            .into_iter()
            .map(|found| (found.path, found.kind))
            .collect()
    }

    fn entries(names: &[&str]) -> Vec<(String, Kind)> {
        names
            .iter()
            .map(|name| match name.strip_suffix('/') {
                Some(name) => (name.to_string(), Kind::Directory),
                None => ((*name).to_string(), Kind::File),
            })
            .collect()
    }

    #[test]
    fn a_listed_folder_is_its_own_first_row() {
        // fx's "path completion lists a folder first so it can be selected"
        // and "browses bare current and parent directories"
        // (`path_completion.zig:401-434`, `:307-330@481104fd`; `listedFolder`,
        // `:133-143`).
        let dir = |path: &str| (path.to_string(), Kind::Directory);
        let file = |path: &str| (path.to_string(), Kind::File);
        let src = entries(&["main.zig"]);
        assert_eq!(listed(&src, "src/"), [dir("src"), file("src/main.zig")]);
        let found = rank_listing(&src, &explicit("src/").expect("a path"));
        assert!(found[0].spans.is_empty(), "the folder row bolds nothing");
        // A partial name lists the matching entries only.
        assert_eq!(listed(&src, "src/ma"), [file("src/main.zig")]);
        // `~`, `.` and `..` list themselves first too.
        assert_eq!(
            listed(&entries(&["notes.txt"]), "~"),
            [dir("~"), file("~/notes.txt")]
        );
        assert_eq!(
            listed(&entries(&["local.txt"]), "."),
            [dir("."), file("./local.txt")]
        );
        assert_eq!(
            listed(&entries(&["parent.txt", "workspace/"]), ".."),
            [dir(".."), file("../parent.txt"), dir("../workspace")]
        );
        assert_eq!(
            listed(&entries(&["empty/", "external.txt"]), "../outside/"),
            [
                dir("../outside"),
                dir("../outside/empty"),
                file("../outside/external.txt")
            ]
        );
        // An empty folder is still a row to choose (7c8ef4a3's message).
        assert_eq!(listed(&[], "src/"), [dir("src")]);
        // The root, and a doubled separator, list no row for themselves.
        assert_eq!(listed(&entries(&["etc/"]), "/"), [dir("/etc")]);
        assert_eq!(listed(&entries(&["a.txt"]), "src//"), [file("src//a.txt")]);
        // Nor does a folder that cannot be painted as it stands.
        assert_eq!(
            listed(&entries(&["a.txt"]), "bad\u{1b}dir/"),
            Vec::<(String, Kind)>::new(),
            "an entry under an unpaintable prefix is no row either"
        );
        assert_eq!(listed(&entries(&["a.txt"]), "bad\u{9b}dir/").len(), 0);
        // The folder keeps its row when the entries fill the list: fx gives
        // the entries what is left of `capacity` (`:81-84`).
        let many: Vec<String> = (0..MOST_RESULTS + 5).map(|n| format!("f-{n:02}")).collect();
        let many: Vec<&str> = many.iter().map(String::as_str).collect();
        let rows = listed(&entries(&many), "big/");
        assert_eq!(rows.len(), MOST_RESULTS);
        assert_eq!(rows[0], dir("big"));
        assert_eq!(rows[1], file("big/f-00"));
    }

    #[test]
    fn a_listing_stops_between_entries_once_told_to() {
        // fx's listing checks its cancel flag between entries
        // (`completeCancellable`, `checkCancellation`, `path_completion.zig:
        // 43-146@481104fd`): cooperative, so a listing of a large folder told
        // to stop part of the way through ends there rather than reading
        // every entry. Round 5 (gpt6 r4 SHOULD 1): it asks where fx's does
        // (`:88-96`) -- before the folder is opened, then for each entry
        // before the advance, after it, and before the entry's `stat` -- so
        // a listing of n entries that is never told asks 1 + 3n + 1 times
        // (the last before the advance that finds none), and one told at its
        // k-th question stops there.
        let dir = tempfile::tempdir().expect("a folder");
        for n in 0..2000 {
            std::fs::write(dir.path().join(format!("f-{n:04}")), "").expect("a file");
        }
        let go = AtomicBool::new(false);
        assert_eq!(
            list_directory(dir.path(), &go).map(|entries| entries.len()),
            Ok(2000)
        );
        let stop = AtomicBool::new(true);
        assert_eq!(
            list_directory(dir.path(), &stop).map(|entries| entries.len()),
            Err(()),
            "a stopped listing read the folder"
        );
        let mut asked = 0;
        let listed = list_directory_until(dir.path(), || {
            asked += 1;
            false
        });
        assert_eq!(listed.map(|entries| entries.len()), Ok(2000));
        assert_eq!(asked, 3 * 2000 + 2, "not fx's three questions an entry");
        let mut asked = 0;
        let listed = list_directory_until(dir.path(), || {
            asked += 1;
            asked == 302
        });
        assert_eq!(listed.map(|entries| entries.len()), Err(()));
        assert_eq!(asked, 302, "the listing went on past its stop");
    }

    #[test]
    fn an_advance_that_fails_after_entries_that_read_fails_the_listing() {
        // gpt6 r5 SHOULD 3: fx's listing fails when an advance fails, entries
        // read before it or not (`iterator.next(io) catch return
        // error.PathUnavailable`, `path_completion.zig:90@481104fd`) -- the
        // folder is unavailable, not a shorter one. Two entries that read and
        // then an advance that fails.
        let dir = tempfile::tempdir().expect("a folder");
        for name in ["a.txt", "b.txt"] {
            std::fs::write(dir.path().join(name), "").expect("a file");
        }
        let read = || {
            ["a.txt", "b.txt"]
                .into_iter()
                .map(|name| Ok((std::ffi::OsString::from(name), dir.path().join(name))))
        };
        assert_eq!(
            list_entries(read(), || false).map(|entries| entries.len()),
            Ok(2),
            "the entries that read did not list"
        );
        let failing = read().chain(std::iter::once(Err(std::io::Error::other("an advance"))));
        assert_eq!(
            list_entries(failing, || false),
            Err(()),
            "a failed advance gave a shorter folder"
        );
    }

    #[test]
    fn abandoning_a_listing_tells_it_to_stop() {
        // gpt6 r3 SHOULD 1: the listing is told to stop (`Task.stop`,
        // `directory_completion_job.zig:68-78@481104fd`), not only forgotten.
        let dir = seeded();
        let mut files = Files::new(dir.path().to_path_buf(), None);
        let _answer = driven_listing(&mut files, "src");
        let stop = files.listing_stop_for_test().expect("a listing in flight");
        assert!(files.abandon_listing());
        assert!(
            stop.load(Atomic::Relaxed),
            "the listing was not told to stop"
        );
        assert!(!files.listing_in_flight_for_test());
        assert!(!files.abandon_listing(), "there was a second one");
    }

    /// Polls `files` until `done` says so, for at most two seconds.
    pub(crate) fn settled(files: &mut Files, mut done: impl FnMut(&mut Files) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            files.poll();
            if done(files) {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("the worker never finished");
    }

    #[test]
    fn the_index_is_built_off_the_calling_thread_and_adopted_by_a_poll() {
        let dir = seeded();
        let mut files = Files::new(dir.path().to_path_buf(), None);
        files.git = None;
        assert_eq!(
            files.lookup("sch"),
            Lookup::Loading,
            "nothing is read before a refresh"
        );
        files.refresh_index();
        settled(&mut files, |files| !files.busy());
        match files.lookup("sch") {
            Lookup::Rows(found) => assert_eq!(found[0].path, "src/deep/sched.rs"),
            other => panic!("{other:?}"),
        }
        // A refresh searches the index it has while the next one is built.
        files.refresh_index();
        assert!(matches!(files.lookup("sch"), Lookup::Rows(_)));
    }

    /// A walk this test drives by hand: what the worker would publish is
    /// sent on the returned sender, so a prefix can be held as long as a
    /// test likes.
    pub(crate) fn driven(files: &mut Files) -> mpsc::Sender<Published> {
        let (tx, rx) = mpsc::channel();
        files.walk = Some(Walk {
            rx,
            stop: Arc::new(AtomicBool::new(false)),
            epoch: files.epoch,
            published: Vec::new(),
        });
        tx
    }

    /// The walk in flight -- the one the code started, with the epoch the
    /// code gave it -- driven by the test from here on: what its worker would
    /// publish is sent on the returned sender, and the worker's own sends go
    /// nowhere. A source with no walk in flight fails the case.
    pub(crate) fn held(files: &mut Files) -> mpsc::Sender<Published> {
        let (tx, rx) = mpsc::channel();
        files.walk.as_mut().expect("no walk in flight").rx = rx;
        tx
    }

    /// A listing of the directory a browsed `parent` names (`tree` for
    /// `@tree/`) that this test drives by hand: it is in flight until the
    /// test sends its entries on the returned sender, so a lookup of that
    /// directory says `Loading` for as long as the test likes.
    pub(crate) fn driven_listing(
        files: &mut Files,
        parent: &str,
    ) -> mpsc::Sender<Result<Vec<(String, Kind)>, ()>> {
        let directory = resolve(&files.root, files.home.as_deref(), parent).expect("a directory");
        let (tx, rx) = mpsc::channel();
        files.listing = Some(Listing {
            directory,
            rx,
            stop: Arc::new(AtomicBool::new(false)),
        });
        tx
    }

    fn found_paths(lookup: Lookup) -> Vec<String> {
        match lookup {
            Lookup::Rows(found) => found.into_iter().map(|found| found.path).collect(),
            other => panic!("{other:?}"),
        }
    }

    // Upstream's `search returns partial results while ready_count is below
    // total` (`file_index.zig:2425-2453`).
    #[test]
    fn the_first_index_is_searched_while_it_is_published() {
        let mut files = Files::new(PathBuf::from("/nonexistent-root"), None);
        let tx = driven(&mut files);
        assert_eq!(files.lookup("main"), Lookup::Loading, "nothing published");
        assert!(!files.poll(), "nothing arrived, nothing changed");

        tx.send(Published::More(candidates(&[
            "src/main.zig",
            "src/wasm_term_main.zig",
        ])))
        .expect("the walk listens");
        assert!(files.poll(), "a first index growing is a change");
        assert_eq!(
            found_paths(files.lookup("main")),
            ["src/main.zig", "src/wasm_term_main.zig"]
        );
        assert_eq!(
            files.lookup("README"),
            Lookup::Loading,
            "what the prefix does not match yet is still loading, not missing"
        );

        tx.send(Published::More(candidates(&[
            "README.md",
            "src/core/shared/io.zig",
        ])))
        .expect("the walk listens");
        assert!(files.poll());
        assert_eq!(found_paths(files.lookup("README")), ["README.md"]);
        assert!(files.busy(), "published is not finished");

        tx.send(Published::Ready).expect("the walk listens");
        assert!(files.poll());
        assert!(!files.busy());
        assert_eq!(
            files.lookup("nothing-like-it"),
            Lookup::Rows(Vec::new()),
            "a whole index that matches nothing has no matching files"
        );
    }

    // Upstream's `active generation remains exclusive while replacement loads
    // and is adopted once` (`file_index.zig:3213-3246`).
    #[test]
    fn a_refresh_is_private_until_it_is_whole() {
        let mut files = Files::new(PathBuf::from("/nonexistent-root"), None);
        files.index = Some(Arc::new(candidates(&["old-one.txt", "old-two.txt"])));
        let tx = driven(&mut files);
        tx.send(Published::More(candidates(&["new-one.txt"])))
            .expect("the walk listens");
        assert!(!files.poll(), "a refresh growing changes nothing shown");
        assert_eq!(
            found_paths(files.lookup("")),
            ["old-one.txt", "old-two.txt"]
        );

        tx.send(Published::More(candidates(&["new-two.txt"])))
            .expect("the walk listens");
        tx.send(Published::Ready).expect("the walk listens");
        assert!(files.poll(), "both arrive in one poll, and the last adopts");
        assert_eq!(
            found_paths(files.lookup("")),
            ["new-one.txt", "new-two.txt"]
        );
    }

    #[test]
    fn a_first_index_that_fails_drops_what_it_published() {
        let mut files = Files::new(PathBuf::from("/nonexistent-root"), None);
        let tx = driven(&mut files);
        tx.send(Published::More(candidates(&["src/main.rs"])))
            .expect("the walk listens");
        tx.send(Published::Failed).expect("the walk listens");
        assert!(files.poll());
        assert_eq!(files.lookup("main"), Lookup::Unavailable);

        // A refresh that fails keeps the index there was.
        let mut files = Files::new(PathBuf::from("/nonexistent-root"), None);
        files.index = Some(Arc::new(candidates(&["src/lib.rs"])));
        let tx = driven(&mut files);
        drop(tx);
        assert!(files.poll(), "a worker that vanished is a failed walk");
        assert_eq!(found_paths(files.lookup("lib")), ["src/lib.rs"]);
    }

    #[test]
    fn the_worker_publishes_the_index_in_order_a_run_at_a_time() {
        let dir = tempfile::tempdir().expect("a temp dir");
        for number in 0..(PUBLISHED_AT_ONCE * 2 + 10) {
            write(dir.path(), &format!("f{number:05}.txt"));
        }
        let stop = AtomicBool::new(false);
        let whole = discover(dir.path(), None, &stop).expect("a walk");
        let (tx, rx) = mpsc::channel();
        load(dir.path(), &[], None, &stop, &tx);
        drop(tx);
        let mut runs = Vec::new();
        let mut published = Vec::new();
        let mut ended = None;
        for message in rx {
            match message {
                Published::More(run) => {
                    assert!(ended.is_none(), "nothing is published after the end");
                    runs.push(run.len());
                    published.extend(run);
                }
                other => ended = Some(other),
            }
        }
        assert_eq!(
            runs,
            [PUBLISHED_AT_ONCE, PUBLISHED_AT_ONCE, 10],
            "every run but the last is full"
        );
        assert_eq!(published, whole, "the runs are the index, in its order");
        assert!(matches!(ended, Some(Published::Ready)));

        // A stop that arrives first publishes nothing.
        let stop = AtomicBool::new(true);
        let (tx, rx) = mpsc::channel();
        load(dir.path(), &[], None, &stop, &tx);
        drop(tx);
        assert!(matches!(
            rx.iter().collect::<Vec<_>>().as_slice(),
            [Published::Failed]
        ));
    }

    #[test]
    fn a_browsed_directory_is_read_once_on_a_worker_and_then_ranked_here() {
        let dir = seeded();
        let mut files = Files::new(dir.path().to_path_buf(), None);
        assert_eq!(files.lookup("src/"), Lookup::Loading);
        settled(&mut files, |files| !files.busy());
        // The folder's own row and its three entries (7c8ef4a3).
        assert!(matches!(files.lookup("src/"), Lookup::Rows(found) if found.len() == 4));
        // The same directory, a different name: no second read.
        assert!(matches!(files.lookup("src/m"), Lookup::Rows(found) if found.len() == 1));
        assert!(!files.busy());
        assert_eq!(files.lookup("nope/"), Lookup::Loading);
        settled(&mut files, |files| !files.busy());
        assert_eq!(files.lookup("nope/"), Lookup::Unavailable);
        files.retry();
        assert_eq!(
            files.lookup("nope/"),
            Lookup::Loading,
            "a retry reads it again"
        );
    }

    #[test]
    fn home_is_where_a_tilde_goes() {
        let dir = seeded();
        let mut files = Files::new(
            PathBuf::from("/nonexistent-root"),
            Some(dir.path().to_path_buf()),
        );
        assert_eq!(files.lookup("~/"), Lookup::Loading);
        settled(&mut files, |files| !files.busy());
        match files.lookup("~/") {
            // `~` itself first, then what is in it (`listedFolder`,
            // `path_completion.zig:133-143@481104fd`).
            Lookup::Rows(found) => assert_eq!(
                [found[0].path.as_str(), found[1].path.as_str()],
                ["~", "~/Cargo.toml"]
            ),
            other => panic!("{other:?}"),
        }
        let mut homeless = Files::new(PathBuf::from("/"), None);
        assert_eq!(homeless.lookup("~/"), Lookup::Unavailable);
    }
}
