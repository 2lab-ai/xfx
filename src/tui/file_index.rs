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
//! * **How much** (`file_index.zig:23-24`): at most [`MOST_INDEXED`] entries,
//!   none longer than [`MOST_PATH_BYTES`], files and directories merged in byte
//!   order. A query shows at most [`MOST_RESULTS`] of them
//!   (`file_completion_state.zig:5`).
//! * **How a query ranks them** (`scoreAsciiMatch` .. `rankedCandidateBetter`,
//!   `file_index.zig:1309-1499`): a case-blind subsequence match, tried on the
//!   basename first, scored by eight facts in a fixed order -- the query *is*
//!   the path or its basename, the match is in the basename, how many matched
//!   characters start a word, whether it starts at the first character, where
//!   it starts, its longest run, its adjacent pairs, its gaps -- and then the
//!   shorter path, then byte order. An empty query is the index's own order.
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

/// The most entries an index holds (`max_indexed_files`, `file_index.zig:23`).
pub(crate) const MOST_INDEXED: usize = 100_000;

/// The longest path an index holds (`max_path_len`, `file_index.zig:24`).
pub(crate) const MOST_PATH_BYTES: usize = 2048;

/// The most rows one query yields (`file_completion_state.zig:5`, `capacity`).
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
/// with no trailing separator on a directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Candidate {
    pub(crate) path: String,
    pub(crate) kind: Kind,
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
            out.push(Candidate { path, kind });
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

/// What one match is scored on (`MatchScore`, `file_index.zig:1196-1205`), in
/// the order [`better`] compares them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Score {
    exact: bool,
    basename: bool,
    boundaries: usize,
    prefix: bool,
    first: usize,
    longest_run: usize,
    adjacent: usize,
    gaps: usize,
}

/// Whether `left` is the better match (`scoreBetter`, `file_index.zig:1477-1487`).
fn score_order(left: &Score, right: &Score) -> Ordering {
    right
        .exact
        .cmp(&left.exact)
        .then(right.basename.cmp(&left.basename))
        .then(right.boundaries.cmp(&left.boundaries))
        .then(right.prefix.cmp(&left.prefix))
        .then(left.first.cmp(&right.first))
        .then(right.longest_run.cmp(&left.longest_run))
        .then(right.adjacent.cmp(&left.adjacent))
        .then(left.gaps.cmp(&right.gaps))
}

/// The whole order: the score, then the shorter path, then bytes
/// (`rankedCandidateBetter`, `file_index.zig:1489-1496`; `NameQuery.better`,
/// `:1241-1248`, which skips the first two for an empty query).
fn better(left: (&Score, &str), right: (&Score, &str), query_empty: bool) -> Ordering {
    let ranked = if query_empty {
        Ordering::Equal
    } else {
        score_order(left.0, right.0).then(left.1.len().cmp(&right.1.len()))
    };
    ranked.then_with(|| left.1.as_bytes().cmp(right.1.as_bytes()))
}

/// A query, case-folded once.
struct Folded(Vec<char>);

impl Folded {
    fn new(query: &str) -> Self {
        Self(query.chars().map(fold).collect())
    }
}

/// One scalar's case fold: its lower case when that is one scalar, itself
/// otherwise (upstream's `unicode_simple_fold`, which is a one-to-one map).
fn fold(scalar: char) -> char {
    let mut lower = scalar.to_lowercase();
    match (lower.next(), lower.next()) {
        (Some(one), None) => one,
        _ => scalar,
    }
}

/// `path` as scalars, folded, with the byte offset each begins at.
fn folded_scalars(path: &str) -> Vec<(usize, char)> {
    path.char_indices()
        .map(|(at, scalar)| (at, fold(scalar)))
        .collect()
}

/// The basename's first byte: after the last `/`.
fn basename_start(path: &str) -> usize {
    path.rfind('/').map_or(0, |slash| slash + 1)
}

/// How well `query` matches `path` (`scoreAsciiMatch` / `scoreFoldedMatch`,
/// `file_index.zig:1309-1386`): the basename first, the whole path after.
fn score(path: &str, query: &Folded) -> Option<Score> {
    if query.0.is_empty() || path.is_empty() {
        return None;
    }
    let scalars = folded_scalars(path);
    let base = basename_start(path);
    let base_index = scalars
        .iter()
        .position(|(at, _)| *at >= base)
        .unwrap_or(scalars.len());
    let folded: Vec<char> = scalars.iter().map(|(_, scalar)| *scalar).collect();
    let exact = folded == query.0 || folded[base_index..] == query.0[..];
    if let Some(facts) = score_range(path, &scalars, base_index, &query.0) {
        return Some(Score {
            exact,
            basename: true,
            ..facts
        });
    }
    let facts = score_range(path, &scalars, 0, &query.0)?;
    Some(Score {
        exact,
        basename: false,
        ..facts
    })
}

/// The subsequence facts of `query` in `scalars[start..]`
/// (`scoreAsciiRange`, `file_index.zig:1327-1363`), greedy left to right.
fn score_range(
    path: &str,
    scalars: &[(usize, char)],
    start: usize,
    query: &[char],
) -> Option<Score> {
    if query.len() > scalars.len().saturating_sub(start) {
        return None;
    }
    let mut facts = Score::default();
    let mut matched = 0usize;
    let mut previous = 0usize;
    let mut run = 0usize;
    for (position, (byte, scalar)) in scalars.iter().enumerate().skip(start) {
        if matched == query.len() {
            break;
        }
        if *scalar != query[matched] {
            continue;
        }
        if matched == 0 {
            facts.first = position - start;
            facts.prefix = position == start;
            run = 1;
        } else if position == previous + 1 {
            facts.adjacent += 1;
            run += 1;
        } else {
            facts.gaps += position - previous - 1;
            run = 1;
        }
        facts.longest_run = facts.longest_run.max(run);
        facts.boundaries += usize::from(is_boundary(path, *byte));
        previous = position;
        matched += 1;
    }
    (matched == query.len()).then_some(facts)
}

/// Whether a match at `at` starts a word (`isMatchBoundary`,
/// `file_index.zig:1466-1474`).
fn is_boundary(path: &str, at: usize) -> bool {
    if at == 0 {
        return true;
    }
    let bytes = path.as_bytes();
    let previous = bytes[at - 1];
    if matches!(previous, b'/' | b'-' | b'_' | b'.' | b' ') {
        return true;
    }
    bytes
        .get(at)
        .is_some_and(|current| previous.is_ascii_lowercase() && current.is_ascii_uppercase())
}

/// The byte ranges of `path` the query matched, whole grapheme clusters,
/// adjacent ones merged (`reconstructMatchSpans` and `spansFromMatchedOffsets`,
/// `file_index.zig:1501-1574`). The basename first, as the score was.
fn spans(path: &str, query: &Folded) -> Vec<Range<usize>> {
    if query.0.is_empty() {
        return Vec::new();
    }
    let scalars = folded_scalars(path);
    let base = basename_start(path);
    let base_index = scalars
        .iter()
        .position(|(at, _)| *at >= base)
        .unwrap_or(scalars.len());
    let Some(offsets) = matched_offsets(&scalars, base_index, &query.0)
        .or_else(|| matched_offsets(&scalars, 0, &query.0))
    else {
        return Vec::new();
    };
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

/// The byte offsets a greedy match of `query` in `scalars[start..]` lands on.
fn matched_offsets(scalars: &[(usize, char)], start: usize, query: &[char]) -> Option<Vec<usize>> {
    let mut out = Vec::with_capacity(query.len());
    for (byte, scalar) in scalars.iter().skip(start) {
        if out.len() == query.len() {
            break;
        }
        if *scalar == query[out.len()] {
            out.push(*byte);
        }
    }
    (out.len() == query.len()).then_some(out)
}

/// The [`MOST_RESULTS`] best of `entries` for `query`, best first
/// (`searchAtRevision`, `file_index.zig:585-649`).
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
    let mut ranked: Vec<(Score, usize)> = entries
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| {
            score(&candidate.path, &folded).map(|score| (score, index))
        })
        .collect();
    let order = |left: &(Score, usize), right: &(Score, usize)| {
        better(
            (&left.0, &entries[left.1].path),
            (&right.0, &entries[right.1].path),
            false,
        )
    };
    if ranked.len() > MOST_RESULTS {
        ranked.select_nth_unstable_by(MOST_RESULTS - 1, order);
        ranked.truncate(MOST_RESULTS);
    }
    ranked.sort_by(order);
    ranked
        .into_iter()
        .map(|(_, index)| {
            let candidate = &entries[index];
            Found {
                path: candidate.path.clone(),
                kind: candidate.kind,
                spans: spans(&candidate.path, &folded),
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
    let empty = folded.0.is_empty();
    let mut ranked: Vec<(Score, String, Kind)> = entries
        .iter()
        .filter_map(|(name, kind)| {
            let score = if empty {
                Score::default()
            } else {
                score(name, &folded)?
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
            let spans = spans(name, &folded)
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
            entries.push(Candidate {
                path,
                kind: candidate.kind,
            });
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
                Some(directory) => Candidate {
                    path: directory.to_string(),
                    kind: Kind::Directory,
                },
                None => Candidate {
                    path: (*path).to_string(),
                    kind: Kind::File,
                },
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
    fn the_basename_outranks_the_directories_and_an_exact_name_outranks_both() {
        let entries = candidates(&["main/other.rs", "src/main.rs", "main.rs", "x/remain.rs"]);
        // `main.rs` is the query exactly; `src/main.rs`'s basename is too, and
        // is longer; `x/remain.rs` matches inside its basename but not at its
        // start; `main/other.rs` only across the directory.
        assert_eq!(
            rows(&entries, "main.rs"),
            ["main.rs", "src/main.rs", "x/remain.rs", "main/other.rs"]
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
        assert_eq!(found.len(), MOST_RESULTS);
        assert_eq!(found[0].path, "dir/x000");
        assert_eq!(found[31].path, "dir/x031");
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
