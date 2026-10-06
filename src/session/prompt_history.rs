//! The prompt history: what was submitted at the composer, kept across
//! sessions so the next one's recall starts where this one's ended.
//!
//! Upstream's store (`vercel-labs/fx@c1db9191
//! src/core/session/prompt_history_store.zig`), in this product's profile:
//! `~/.xfx/history.jsonl`, one JSON record per line --
//!
//! ```text
//! {"schema_version":1,"timestamp_ms":1791318260296,"workspace_root":"/abs/work","text":"first prompt"}
//! ```
//!
//! -- appended under an exclusive `history.lock` beside it, created owner-only
//! in an owner-only directory. A record names the workspace it was typed in,
//! and a session recalls only its own workspace's lines.
//!
//! The rules are upstream's, each for the reason upstream has it:
//!
//! * **A record past 256 KiB is not written** (`max_record_bytes`, `:12`): a
//!   pasted megabyte is a prompt, not a line anyone walks back to.
//! * **A line the same as the workspace's latest is not written again**
//!   (`.duplicate`, `:122-131`). A line typed twice with another between them
//!   is two records.
//! * **Past 1 MiB the file is compacted** to its newest 1000 records and at
//!   most 1 MiB (`compaction_threshold_bytes`, `:13-15`, `compactRecords`,
//!   `:778-829`), through a staged file and a rename.
//! * **A torn last line is cut before the next append** (`repairIncompleteTail`,
//!   `:684-711`) and never read as a record.
//! * **A launch loads the newest 100 of its workspace's lines**
//!   (`loadAndInstallPromptHistory`, `input_history_runtime.zig:20-28`).
//!
//! What is recorded, and whether anything is, is the caller's
//! (`crate::tui`'s shell): this module is the file.
//!
//! **Not ported**: upstream writes a text that is not valid UTF-8 as a base64
//! object (`writeDurableBytes`, `session_codec.zig:467-475`). Every text xfx
//! records is a Rust `String`, so this store never writes one, and a record of
//! that shape is skipped when read rather than decoded.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;

/// The file's name inside the profile directory (`prompt_history_file_name`,
/// `profile_paths.zig:16`).
pub const FILE_NAME: &str = "history.jsonl";

/// The lock beside it (`history_lock_file`, `prompt_history_store.zig:9`).
const LOCK_FILE_NAME: &str = "history.lock";

/// How many of a workspace's lines a launch loads (`input_history_runtime.zig:23`).
pub const LOAD_LIMIT: usize = 100;

/// The longest record written, newline included (`max_record_bytes`, `:12`).
const MAX_RECORD_BYTES: usize = 256 * 1024;

/// The size past which an append compacts the file (`:13`).
const COMPACTION_THRESHOLD_BYTES: u64 = 1024 * 1024;

/// What a compaction keeps at most: records (`:14`) and bytes (`:15`).
const COMPACTION_RECORD_LIMIT: usize = 1000;
const COMPACTION_BYTE_LIMIT: usize = 1024 * 1024;

/// How long an append waits for another process's lock (`lock_deadline_ms`,
/// `:10`), and how often it looks again.
const LOCK_DEADLINE: Duration = Duration::from_millis(2000);
#[cfg(unix)]
const LOCK_RETRY: Duration = Duration::from_millis(10);

/// The most of the file a load reads, from its end.
///
/// Upstream scans backwards until it has its hundred lines, however far that
/// is. A file this store compacts is never much past 1 MiB, so the bound only
/// matters for one that something else grew, and there a launch that reads
/// four megabytes is better than one that reads everything.
const MOST_BYTES_LOADED: u64 = 4 * 1024 * 1024;

/// The first window a load reads from the end of the file.
const SCAN_BLOCK_BYTES: u64 = 64 * 1024;

/// What an append did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Appended {
    /// The record is on disk.
    Appended,
    /// The workspace's latest record is already this text.
    Duplicate,
    /// The record would be past [`MAX_RECORD_BYTES`].
    TooLarge,
}

/// The history file of one profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptHistory {
    /// The profile directory: `~/.xfx`.
    dir: PathBuf,
    /// `~/.xfx/history.jsonl`.
    path: PathBuf,
}

impl PromptHistory {
    /// The history kept in `profile_dir`. Nothing is read or created yet.
    pub fn in_profile(profile_dir: &Path) -> Self {
        Self {
            dir: profile_dir.to_path_buf(),
            path: profile_dir.join(FILE_NAME),
        }
    }

    /// Where the records are.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The newest `limit` texts recorded in `workspace_root`, oldest first --
    /// the order a recall walks them in reverse. No file is no lines.
    ///
    /// A line that is not a record of this schema is skipped, as upstream
    /// skips one (`processCompleteLine`, `:657-681`), and so is a last line the
    /// file does not end: it is a write that never finished.
    pub fn load_recent(&self, workspace_root: &str, limit: usize) -> io::Result<Vec<String>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let mut file = match File::open(&self.path) {
            Ok(file) => file,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => return Err(err),
        };
        let length = file.metadata()?.len();
        // **From the end, in a window that doubles** until it holds `limit`
        // of this workspace's lines, covers the file, or reaches
        // [`MOST_BYTES_LOADED`] -- upstream's backward scan
        // (`loadRecentFromFile`, `:374-474`) in a shape that cannot misjoin a
        // line across two blocks: each window is read whole. The append's
        // duplicate check asks for one line, so it reads the last block and
        // not the file.
        let mut window = SCAN_BLOCK_BYTES.min(length);
        loop {
            let start = length - window;
            file.seek(SeekFrom::Start(start))?;
            let mut bytes = Vec::with_capacity(usize::try_from(window).unwrap_or(0));
            (&mut file).take(window).read_to_end(&mut bytes)?;
            // A window that began inside a line drops that line's head.
            let first = if start > 0 {
                bytes
                    .iter()
                    .position(|&b| b == b'\n')
                    .map_or(bytes.len(), |at| at + 1)
            } else {
                0
            };
            let end = bytes
                .iter()
                .rposition(|&b| b == b'\n')
                .map_or(0, |at| at + 1);
            let complete = bytes.get(first..end).unwrap_or_default();
            let mut texts: Vec<String> = complete
                .split(|&b| b == b'\n')
                .rev()
                .filter_map(parse)
                .filter(|record| record.workspace_root == workspace_root)
                .map(|record| record.text)
                .take(limit)
                .collect();
            if texts.len() >= limit || window >= length || window >= MOST_BYTES_LOADED {
                texts.reverse();
                return Ok(texts);
            }
            window = (window * 2).min(length).min(MOST_BYTES_LOADED);
        }
    }

    /// Appends one record of `text`, typed in `workspace_root` at
    /// `timestamp_ms`, under the lock, and compacts the file when it has
    /// grown past [`COMPACTION_THRESHOLD_BYTES`].
    ///
    /// A compaction that fails leaves the appended file as it was: the record
    /// is on disk either way, which is what upstream reports for one too
    /// (`.compaction_stale => .appended`, `prompt_history_runtime.zig:146-153`).
    pub fn append(
        &self,
        timestamp_ms: u64,
        workspace_root: &str,
        text: &str,
    ) -> io::Result<Appended> {
        if !Path::new(workspace_root).is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "the workspace path is not absolute",
            ));
        }
        let line = record(timestamp_ms, workspace_root, text);
        if line.len() > MAX_RECORD_BYTES {
            return Ok(Appended::TooLarge);
        }
        create_private_dir(&self.dir)?;
        let _lock = HistoryLock::acquire(&self.dir, LOCK_DEADLINE)?;
        let mut file = open_private(&self.path)?;
        repair_incomplete_tail(&mut file)?;
        if self
            .load_recent(workspace_root, 1)?
            .first()
            .map(String::as_str)
            == Some(text)
        {
            return Ok(Appended::Duplicate);
        }
        let end = file.seek(SeekFrom::End(0))?;
        file.write_all(line.as_bytes())?;
        file.sync_data()?;
        if end + line.len() as u64 > COMPACTION_THRESHOLD_BYTES {
            let _ = self.compact();
        }
        Ok(Appended::Appended)
    }

    /// Rewrites the file as its newest records, at most
    /// [`COMPACTION_RECORD_LIMIT`] of them and [`COMPACTION_BYTE_LIMIT`] bytes,
    /// each re-serialized, the malformed ones dropped (`compactRecords`).
    /// Called with the lock held.
    fn compact(&self) -> io::Result<()> {
        let bytes = fs::read(&self.path)?;
        let mut kept: Vec<String> = Vec::new();
        let mut kept_bytes = 0;
        for line in bytes.split(|&b| b == b'\n') {
            let Some(parsed) = parse(line) else {
                continue;
            };
            let canonical = record(parsed.timestamp_ms, &parsed.workspace_root, &parsed.text);
            if canonical.len() > MAX_RECORD_BYTES {
                continue;
            }
            kept_bytes += canonical.len();
            kept.push(canonical);
        }
        let mut drop = 0;
        while kept.len() - drop > COMPACTION_RECORD_LIMIT || kept_bytes > COMPACTION_BYTE_LIMIT {
            kept_bytes -= kept[drop].len();
            drop += 1;
        }
        replace_private(&self.dir, &self.path, kept[drop..].concat().as_bytes())
    }
}

/// One record as it was read.
struct Parsed {
    timestamp_ms: u64,
    workspace_root: String,
    text: String,
}

/// One record's line, newline included (`serializeRecord`, `:581-597`).
fn record(timestamp_ms: u64, workspace_root: &str, text: &str) -> String {
    let quoted = |text: &str| serde_json::to_string(text).expect("a string always serializes");
    format!(
        "{{\"schema_version\":1,\"timestamp_ms\":{timestamp_ms},\"workspace_root\":{},\"text\":{}}}\n",
        quoted(workspace_root),
        quoted(text)
    )
}

/// A line read back as a record, or `None` when it is not one: exactly the four
/// keys, schema 1, an integer time, an absolute workspace and a text string
/// (`parseRecord`, `:599-635`).
fn parse(line: &[u8]) -> Option<Parsed> {
    if line.is_empty() || line.len() + 1 > MAX_RECORD_BYTES {
        return None;
    }
    let Value::Object(object) = serde_json::from_slice::<Value>(line).ok()? else {
        return None;
    };
    if object.len() != 4 || object.get("schema_version")?.as_u64()? != 1 {
        return None;
    }
    let timestamp_ms = object.get("timestamp_ms")?.as_u64()?;
    let workspace_root = object.get("workspace_root")?.as_str()?;
    if !Path::new(workspace_root).is_absolute() {
        return None;
    }
    let text = object.get("text")?.as_str()?;
    Some(Parsed {
        timestamp_ms,
        workspace_root: workspace_root.to_string(),
        text: text.to_string(),
    })
}

/// Cuts a last line the file does not end back to the newline before it, or
/// the whole file when it has none (`repairIncompleteTail`, `:684-711`).
fn repair_incomplete_tail(file: &mut File) -> io::Result<()> {
    let length = file.metadata()?.len();
    if length == 0 {
        return Ok(());
    }
    let mut last = [0u8; 1];
    file.seek(SeekFrom::Start(length - 1))?;
    file.read_exact(&mut last)?;
    if last[0] == b'\n' {
        return Ok(());
    }
    // Backwards in blocks to the newline before the torn line, as upstream
    // reads it (8 KiB at a time), rather than the whole file.
    let mut cursor = length;
    let mut block = vec![0u8; 8192];
    let mut keep = 0;
    while cursor > 0 {
        let start = cursor.saturating_sub(block.len() as u64);
        let size = usize::try_from(cursor - start).unwrap_or(block.len());
        file.seek(SeekFrom::Start(start))?;
        file.read_exact(&mut block[..size])?;
        if let Some(at) = block[..size].iter().rposition(|&b| b == b'\n') {
            keep = start + at as u64 + 1;
            break;
        }
        cursor = start;
    }
    file.set_len(keep)?;
    file.sync_data()
}

/// Opens the history for reading and appending, creating it owner-only, and
/// never through a link planted where it goes.
fn open_private(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    options.open(path)
}

/// Creates the profile directory owner-only, if it is not already there --
/// created `0700` rather than tightened afterwards, as the settings writer does
/// (`crate::provider::profile`).
fn create_private_dir(dir: &Path) -> io::Result<()> {
    if dir.is_dir() {
        return Ok(());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
    }
    #[cfg(not(unix))]
    {
        fs::create_dir_all(dir)
    }
}

/// Replaces `path` with `bytes` through a stage of its own beside it, opened
/// `create_new` at `0600`, synced, renamed over the file, and the directory
/// synced -- so a reader sees the old file or the new one, never half of
/// either (upstream's `durableReplaceVerified`).
fn replace_private(dir: &Path, path: &Path, bytes: &[u8]) -> io::Result<()> {
    let nonce: String = crate::session::new_identifier().chars().take(16).collect();
    let stage = dir.join(format!(
        "{FILE_NAME}.{}.{nonce}{}",
        std::process::id(),
        crate::session::STAGE_SUFFIX
    ));
    let written = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&stage)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&stage, path)
    })();
    if written.is_err() {
        let _ = fs::remove_file(&stage);
    }
    written?;
    File::open(dir)?.sync_all()
}

/// `history.lock`, held for one append (and its compaction) against other
/// processes: a second xfx on the same profile.
///
/// Upstream's discipline (`acquireTimedAdvisoryLock`, `io.zig:780-896`): a
/// private file never followed through a link, locked exclusively without
/// blocking, retried every ten milliseconds until a deadline, and a failure
/// after it rather than a wait for ever. Released by the kernel when the file
/// closes, so a process that dies cannot leave it held.
struct HistoryLock {
    #[cfg(unix)]
    _file: File,
}

impl HistoryLock {
    #[cfg(unix)]
    fn acquire(dir: &Path, deadline: Duration) -> io::Result<Self> {
        use rustix::fs::{flock, FlockOperation};
        let file = open_private(&dir.join(LOCK_FILE_NAME))?;
        let started = std::time::Instant::now();
        loop {
            match flock(&file, FlockOperation::NonBlockingLockExclusive) {
                Ok(()) => return Ok(Self { _file: file }),
                Err(rustix::io::Errno::WOULDBLOCK) if started.elapsed() < deadline => {
                    std::thread::sleep(LOCK_RETRY.min(deadline));
                }
                Err(rustix::io::Errno::WOULDBLOCK) => {
                    return Err(io::Error::new(
                        io::ErrorKind::WouldBlock,
                        "another process is writing the prompt history (history.lock is held)",
                    ));
                }
                Err(err) => return Err(io::Error::from(err)),
            }
        }
    }

    /// No `flock` to take; nothing else serializes this file here.
    #[cfg(not(unix))]
    fn acquire(dir: &Path, deadline: Duration) -> io::Result<Self> {
        let _ = (dir, deadline);
        Ok(Self {})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORK: &str = "/abs/work";

    fn store() -> (tempfile::TempDir, PromptHistory) {
        let home = tempfile::tempdir().expect("temp home");
        let history = PromptHistory::in_profile(&home.path().join(".xfx"));
        (home, history)
    }

    /// Writes `records` as the whole file, as appends would have left it --
    /// the cases about what a long file does, without an `fsync` per line.
    fn seeded(history: &PromptHistory, records: impl Iterator<Item = (u64, String, String)>) {
        create_private_dir(&history.dir).expect("dir");
        let bytes: String = records
            .map(|(at, workspace, text)| record(at, &workspace, &text))
            .collect();
        fs::write(history.path(), bytes).expect("seed");
    }

    fn lines(history: &PromptHistory) -> Vec<String> {
        fs::read_to_string(history.path())
            .expect("read the history")
            .lines()
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn a_record_is_upstreams_four_keys_on_one_line() {
        let (_home, history) = store();
        assert_eq!(
            history
                .append(1791318260296, WORK, "first \"prompt\"\nline two")
                .expect("append"),
            Appended::Appended
        );
        assert_eq!(
            lines(&history),
            [concat!(
                r#"{"schema_version":1,"timestamp_ms":1791318260296,"#,
                r#""workspace_root":"/abs/work","text":"first \"prompt\"\nline two"}"#
            )]
        );
        assert_eq!(
            history.load_recent(WORK, LOAD_LIMIT).expect("load"),
            ["first \"prompt\"\nline two"]
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_file_and_its_directory_are_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let (_home, history) = store();
        history.append(1, WORK, "one").expect("append");
        let mode = |path: &Path| fs::metadata(path).expect("stat").permissions().mode() & 0o777;
        assert_eq!(mode(history.path()), 0o600);
        assert_eq!(mode(history.path().parent().expect("dir")), 0o700);
        assert_eq!(mode(&history.dir.join(LOCK_FILE_NAME)), 0o600);
    }

    #[test]
    fn the_workspaces_latest_line_is_not_written_twice_but_a_repeat_with_one_between_is() {
        let (_home, history) = store();
        assert_eq!(
            history.append(1, WORK, "alpha").expect("a"),
            Appended::Appended
        );
        assert_eq!(
            history.append(2, WORK, "alpha").expect("a"),
            Appended::Duplicate
        );
        // Another workspace's line between them is not "one between": the
        // latest line *of this workspace* is still alpha.
        assert_eq!(
            history.append(3, "/other", "beta").expect("b"),
            Appended::Appended
        );
        assert_eq!(
            history.append(4, WORK, "alpha").expect("a"),
            Appended::Duplicate
        );
        assert_eq!(
            history.append(5, WORK, "/version").expect("v"),
            Appended::Appended
        );
        assert_eq!(
            history.append(6, WORK, "alpha").expect("a"),
            Appended::Appended
        );
        assert_eq!(
            history.load_recent(WORK, LOAD_LIMIT).expect("load"),
            ["alpha", "/version", "alpha"]
        );
        assert_eq!(
            history.load_recent("/other", LOAD_LIMIT).expect("load"),
            ["beta"]
        );
    }

    #[test]
    fn a_load_is_the_newest_lines_of_its_own_workspace_oldest_first() {
        let (_home, history) = store();
        seeded(
            &history,
            (0..150u64).flat_map(|at| {
                [
                    (at, WORK.to_string(), format!("line {at}")),
                    (at, "/elsewhere".to_string(), format!("other {at}")),
                ]
            }),
        );
        let loaded = history.load_recent(WORK, LOAD_LIMIT).expect("load");
        assert_eq!(loaded.len(), 100);
        assert_eq!(loaded.first().map(String::as_str), Some("line 50"));
        assert_eq!(loaded.last().map(String::as_str), Some("line 149"));
        assert!(history
            .load_recent("/nowhere", LOAD_LIMIT)
            .expect("load")
            .is_empty());
    }

    #[test]
    fn a_load_reads_back_past_its_first_window_for_a_quiet_workspace() {
        // The workspace's one line is behind 200 KiB of another's: the first
        // 64 KiB window finds nothing and the window grows until it does.
        let (_home, history) = store();
        let body = "z".repeat(1000);
        seeded(
            &history,
            std::iter::once((1, WORK.to_string(), "far back".to_string())).chain(
                (0..200u64).map(|at| (at, "/elsewhere".to_string(), format!("{at} {body}"))),
            ),
        );
        assert_eq!(
            history.load_recent(WORK, LOAD_LIMIT).expect("load"),
            ["far back"]
        );
        // And the duplicate check sees it there too.
        assert_eq!(
            history.append(2, WORK, "far back").expect("append"),
            Appended::Duplicate
        );
    }

    #[test]
    fn no_file_is_no_lines() {
        let (_home, history) = store();
        assert!(history
            .load_recent(WORK, LOAD_LIMIT)
            .expect("load")
            .is_empty());
        assert!(!history.path().exists(), "a load created the file");
    }

    #[test]
    fn a_record_past_the_cap_is_not_written() {
        let (_home, history) = store();
        let huge = "x".repeat(MAX_RECORD_BYTES);
        assert_eq!(
            history.append(1, WORK, &huge).expect("append"),
            Appended::TooLarge
        );
        assert!(!history.path().exists());
        let fits = "x".repeat(MAX_RECORD_BYTES - 200);
        assert_eq!(
            history.append(2, WORK, &fits).expect("append"),
            Appended::Appended
        );
    }

    #[test]
    fn a_torn_last_line_is_never_read_and_is_cut_before_the_next_append() {
        let (_home, history) = store();
        history.append(1, WORK, "whole").expect("append");
        let mut torn = fs::read(history.path()).expect("read");
        torn.extend_from_slice(br#"{"schema_version":1,"timestamp_ms":2,"workspace"#);
        fs::write(history.path(), &torn).expect("tear");
        assert_eq!(
            history.load_recent(WORK, LOAD_LIMIT).expect("load"),
            ["whole"]
        );
        history.append(3, WORK, "after").expect("append");
        assert_eq!(lines(&history).len(), 2);
        assert_eq!(
            history.load_recent(WORK, LOAD_LIMIT).expect("load"),
            ["whole", "after"]
        );
    }

    #[test]
    fn a_line_that_is_not_a_record_is_skipped() {
        let (_home, history) = store();
        history.append(1, WORK, "kept").expect("append");
        let mut bytes = fs::read(history.path()).expect("read");
        bytes.extend_from_slice(b"not json\n");
        bytes.extend_from_slice(
            br#"{"schema_version":2,"timestamp_ms":1,"workspace_root":"/abs/work","text":"v2"}"#,
        );
        bytes.push(b'\n');
        bytes.extend_from_slice(
            br#"{"schema_version":1,"timestamp_ms":1,"workspace_root":"relative","text":"rel"}"#,
        );
        bytes.push(b'\n');
        bytes.extend_from_slice(br#"{"schema_version":1,"timestamp_ms":1,"workspace_root":"/abs/work","text":{"encoding":"base64","data":"AA=="}}"#);
        bytes.push(b'\n');
        fs::write(history.path(), &bytes).expect("write");
        assert_eq!(
            history.load_recent(WORK, LOAD_LIMIT).expect("load"),
            ["kept"]
        );
    }

    #[test]
    fn an_append_past_a_mebibyte_compacts_the_file() {
        let (_home, history) = store();
        // ~1 KiB records, a little under the threshold; the next appends cross
        // it, and the append that does compacts.
        let body = "y".repeat(900);
        seeded(
            &history,
            (0..1020u64).map(|at| (at, WORK.to_string(), format!("{at} {body}"))),
        );
        assert!(fs::metadata(history.path()).expect("stat").len() <= COMPACTION_THRESHOLD_BYTES);
        for at in 1020..1100u64 {
            history
                .append(at, WORK, &format!("{at} {body}"))
                .expect("append");
        }
        let kept = lines(&history);
        assert!(kept.len() < 1100, "nothing was compacted: {}", kept.len());
        assert!(
            !kept[0].contains("\"text\":\"0 "),
            "the oldest record survived: {}",
            &kept[0][..80]
        );
        let size = fs::metadata(history.path()).expect("stat").len();
        assert!(
            size <= COMPACTION_THRESHOLD_BYTES + MAX_RECORD_BYTES as u64,
            "{size}"
        );
        let loaded = history.load_recent(WORK, LOAD_LIMIT).expect("load");
        assert!(loaded.last().expect("newest").starts_with("1099 "));
        // No stage left beside it.
        let names: Vec<String> = fs::read_dir(&history.dir)
            .expect("list")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert!(
            names
                .iter()
                .all(|name| name == FILE_NAME || name == LOCK_FILE_NAME),
            "{names:?}"
        );
    }

    #[test]
    fn a_compaction_keeps_the_newest_thousand_records_and_drops_what_is_not_one() {
        let (_home, history) = store();
        create_private_dir(&history.dir).expect("dir");
        let mut bytes = String::new();
        for at in 0..1500u64 {
            bytes.push_str(&record(at, WORK, &format!("line {at}")));
            if at == 10 {
                bytes.push_str("not a record\n");
            }
        }
        fs::write(history.path(), bytes).expect("seed");
        history.compact().expect("compact");
        let kept = lines(&history);
        assert_eq!(kept.len(), COMPACTION_RECORD_LIMIT);
        assert_eq!(kept[0], record(500, WORK, "line 500").trim_end());
        assert_eq!(kept[999], record(1499, WORK, "line 1499").trim_end());
    }

    #[test]
    fn a_relative_workspace_is_refused() {
        let (_home, history) = store();
        assert!(history.append(1, "work", "x").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_lock_held_past_the_deadline_is_a_failure_rather_than_a_wait_for_ever() {
        use rustix::fs::{flock, FlockOperation};
        let (_home, history) = store();
        create_private_dir(&history.dir).expect("dir");
        let held = open_private(&history.dir.join(LOCK_FILE_NAME)).expect("open");
        flock(&held, FlockOperation::NonBlockingLockExclusive).expect("hold");
        let err = HistoryLock::acquire(&history.dir, Duration::from_millis(50))
            .err()
            .expect("a held lock is not waited out");
        assert_eq!(err.kind(), io::ErrorKind::WouldBlock, "{err}");
    }
}
