//! What `/undo` can take back: the file changes this conversation's own tools
//! committed, newest last, each with what was there before it.
//!
//! Upstream's `ChangeTracker` (`vercel-labs/fx@c1db9191
//! src/core/workspace/change_tracker.zig:30-82`), kept the same way: a stack in
//! memory, pushed by every committed `write_file` and `edit_file`
//! (`publishCommittedTracker`, `app_callbacks.zig:1124-1170`) and by nothing
//! else, at most a hundred deep with the oldest dropped first, and popped by
//! `/undo` whether or not the change could be taken back.
//!
//! # Why it is in memory and nowhere else
//!
//! Upstream writes none of it down. Its stack is cleared by the transition
//! `/new`, `/clear`, `/reset` and `/resume` all go through
//! (`applyIdleLiveSessionTransition`, `app_session_runtime.zig:1966-2001`,
//! reached from `resumeSelectedSession` by `prepareLiveSessionResume`, `:1922`),
//! and its own resume test asks `/undo` after a resume with two committed
//! writes in the log and is answered `Nothing to undo.` with both files as they
//! were (`tests/e2e/tui-resume.test.ts:6029-6037`). So the journal lives in the
//! conversation's [`super::ToolSession`], and goes wherever the conversation
//! goes: a `/new` drops it, and a resumed session is bound to a fresh one.
//! Persisting it would also put up to a hundred preimages -- whole files -- in
//! the session log beside what the model read.
//!
//! # What is kept
//!
//! The target's canonical path, the bytes that were there before (or that
//! nothing was), and the digest of what the change left there. The digest is
//! how `/undo` honours the rule every write in this crate is held to: a file
//! is not replaced with content nobody in this session has seen
//! (`super::mutate`'s read proofs). It is what this session last saw of the
//! file, because this session wrote it.

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::permission::ContentHash;

/// How many changes `/undo` can reach back through
/// (`change_tracker.zig:35`, `max_stack_size`).
pub const MAX_TRACKED_CHANGES: usize = 100;

/// How many bytes of preimage the journal holds at most, oldest dropped first.
///
/// **xfx's own bound; upstream has none.** A preimage is a whole file, and one
/// may be as large as a complete read (`ToolLimits::max_read_bytes`, ten
/// megabytes), so a hundred of them is a gigabyte held for the life of a
/// session. Sixty-four megabytes is every change of any session that edits
/// ordinary source files, and a session that rewrites large files a hundred
/// times reaches back fewer changes rather than holding all of them. The
/// newest change is always kept, whatever its size: the one `/undo` would take
/// back next is the one that must not be the cost of the bound.
pub const MAX_TRACKED_BYTES: usize = 64 * 1024 * 1024;

/// What every `/undo` notice is filed under (`commandUndoLast`,
/// `app_commands.zig:1285-1289`).
pub const TOPIC: &str = "undo";

/// What `/undo` says when there is nothing to take back
/// (`app_commands.zig:1278`).
pub const NOTHING_TO_UNDO: &str = "Nothing to undo.";

/// One committed change, as `/undo` needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackedChange {
    /// The target's canonical absolute path, as the change was located.
    pub path: PathBuf,
    /// What was there before: the bytes, or `None` when nothing was and the
    /// change created the file.
    pub before: Option<Vec<u8>>,
    /// The digest of what the change left there.
    pub after: ContentHash,
}

impl TrackedChange {
    fn cost(&self) -> usize {
        self.before.as_ref().map_or(0, Vec::len)
    }
}

/// The changes a conversation's tools committed, oldest first.
#[derive(Debug, Default)]
pub struct ChangeJournal {
    changes: VecDeque<TrackedChange>,
    /// The bytes of preimage held, kept rather than summed at each push.
    held: usize,
}

impl ChangeJournal {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records one committed change, dropping the oldest past
    /// [`MAX_TRACKED_CHANGES`] or [`MAX_TRACKED_BYTES`].
    pub fn push(&mut self, change: TrackedChange) {
        self.held += change.cost();
        self.changes.push_back(change);
        while self.changes.len() > MAX_TRACKED_CHANGES
            || (self.held > MAX_TRACKED_BYTES && self.changes.len() > 1)
        {
            if let Some(dropped) = self.changes.pop_front() {
                self.held -= dropped.cost();
            }
        }
    }

    /// The newest change, taken off the journal: whatever `/undo` then makes
    /// of it, it is not offered again (`undoLast`, `change_tracker.zig:53-57`).
    pub fn pop(&mut self) -> Option<TrackedChange> {
        let change = self.changes.pop_back()?;
        self.held -= change.cost();
        Some(change)
    }

    pub fn len(&self) -> usize {
        self.changes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    /// The bytes of preimage held now.
    pub fn held_bytes(&self) -> usize {
        self.held
    }

    /// The changes held, oldest first.
    pub fn iter(&self) -> impl Iterator<Item = &TrackedChange> {
        self.changes.iter()
    }
}

/// What one `/undo` did (`UndoResult`, `change_tracker.zig:21-29`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Undone {
    /// The file holds what it held before the change again.
    Restored(PathBuf),
    /// The change created the file, and it is gone again -- or was already.
    Deleted(PathBuf),
    /// The change was taken off the journal and could not be reversed. The
    /// file is as it was before the `/undo`.
    ///
    /// `reason` is for a test and a debugger: upstream's notice names the
    /// file and nothing else (`app_commands.zig:1264-1276`), and so does this
    /// one ([`Self::sentence`]).
    Unavailable { path: PathBuf, reason: String },
    /// The journal was empty.
    Nothing,
}

impl Undone {
    /// The notice's body, in upstream's words (`commandUndoLast`,
    /// `app_commands.zig:1242-1290`), with the path made safe to print the
    /// way upstream's is ([`display_path`]).
    pub fn sentence(&self) -> String {
        match self {
            Self::Restored(path) => format!("Restored {}", display_path(path)),
            Self::Deleted(path) => format!("Deleted {} (was newly created)", display_path(path)),
            Self::Unavailable { path, .. } => format!("Could not undo {}", display_path(path)),
            Self::Nothing => NOTHING_TO_UNDO.to_string(),
        }
    }
}

/// `path` with nothing in it a terminal would act on: every control byte, every
/// byte that is not UTF-8 and every invisible formatting code point spelled
/// out (`encodeTerminalSafe`, `text_utils.zig:343-626`).
///
/// Spelled out rather than blanked, as upstream does for a path it quotes
/// (its hostile-path undo test, `tests/e2e/tui-permissions.test.ts:1949-1980`,
/// reads `name\x1b]2;FX_PWN\x07\x0afile.txt` back as text): a name with a
/// character silently replaced is the name of a different file. The bytes
/// rather than a lossy string, because a byte that is not UTF-8 is spelled
/// `\xNN` there too (`terminalSafeToken`, `:560-584`) rather than drawn as a
/// replacement character.
pub fn display_path(path: &Path) -> String {
    let mut out = String::new();
    for chunk in path_bytes(path).utf8_chunks() {
        out.push_str(&super::question::terminal_safe(chunk.valid()));
        for byte in chunk.invalid() {
            let _ = write!(out, "\\x{byte:02x}");
        }
    }
    out
}

#[cfg(unix)]
fn path_bytes(path: &Path) -> std::borrow::Cow<'_, [u8]> {
    use std::os::unix::ffi::OsStrExt;
    std::borrow::Cow::Borrowed(path.as_os_str().as_bytes())
}

/// No platform but Unix changes a file through these tools
/// (`super::mutate`'s `namespace`), so nothing reaches here with a path that
/// is not text; the lossy form is the one that compiles everywhere.
#[cfg(not(unix))]
fn path_bytes(path: &Path) -> std::borrow::Cow<'_, [u8]> {
    match path.to_string_lossy() {
        std::borrow::Cow::Borrowed(text) => std::borrow::Cow::Borrowed(text.as_bytes()),
        std::borrow::Cow::Owned(text) => std::borrow::Cow::Owned(text.into_bytes()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(name: &str, before: Option<&[u8]>) -> TrackedChange {
        TrackedChange {
            path: PathBuf::from(format!("/workspace/{name}")),
            before: before.map(<[u8]>::to_vec),
            after: ContentHash::of(name.as_bytes()),
        }
    }

    #[test]
    fn an_empty_journal_has_nothing_to_give_back() {
        let mut journal = ChangeJournal::new();
        assert!(journal.is_empty());
        assert_eq!(journal.pop(), None);
    }

    #[test]
    fn the_newest_change_comes_back_first_and_only_once() {
        let mut journal = ChangeJournal::new();
        journal.push(change("a", None));
        journal.push(change("b", Some(b"before b")));
        assert_eq!(journal.pop(), Some(change("b", Some(b"before b"))));
        assert_eq!(journal.pop(), Some(change("a", None)));
        assert_eq!(journal.pop(), None);
        assert_eq!(journal.held_bytes(), 0);
    }

    #[test]
    fn past_a_hundred_changes_the_oldest_goes_and_the_order_holds() {
        // `pushOperation evicts the oldest operation with stable ordering`
        // (`change_tracker.zig:241-260`): file-0 is dropped, file-1 is the
        // oldest kept, file-100 the newest.
        let mut journal = ChangeJournal::new();
        for i in 0..=MAX_TRACKED_CHANGES {
            journal.push(change(&format!("file-{i}"), None));
        }
        assert_eq!(journal.len(), MAX_TRACKED_CHANGES);
        let kept: Vec<_> = journal.iter().map(|change| change.path.clone()).collect();
        assert_eq!(kept[0], PathBuf::from("/workspace/file-1"));
        assert_eq!(
            kept[MAX_TRACKED_CHANGES - 1],
            PathBuf::from(format!("/workspace/file-{MAX_TRACKED_CHANGES}"))
        );
    }

    #[test]
    fn past_the_byte_bound_the_oldest_preimages_go_and_the_newest_stays() {
        let quarter = vec![b'x'; MAX_TRACKED_BYTES / 4];
        let mut journal = ChangeJournal::new();
        for i in 0..5 {
            journal.push(change(&format!("big-{i}"), Some(&quarter)));
        }
        // Four quarters fit exactly; the fifth pushes the first out.
        assert_eq!(journal.len(), 4);
        assert_eq!(journal.held_bytes(), MAX_TRACKED_BYTES);
        assert_eq!(
            journal.iter().next().map(|change| change.path.clone()),
            Some(PathBuf::from("/workspace/big-1"))
        );
        // One change larger than the whole bound is still kept: it is the one
        // `/undo` would reach next.
        let whole = vec![b'y'; MAX_TRACKED_BYTES + 1];
        journal.push(change("huge", Some(&whole)));
        assert_eq!(journal.len(), 1);
        assert_eq!(
            journal.pop().map(|change| change.path),
            Some(PathBuf::from("/workspace/huge"))
        );
        assert_eq!(journal.held_bytes(), 0);
    }

    #[test]
    fn each_outcome_says_upstreams_sentence() {
        let path = PathBuf::from("/private/tmp/work/a.txt");
        assert_eq!(
            Undone::Restored(path.clone()).sentence(),
            "Restored /private/tmp/work/a.txt"
        );
        assert_eq!(
            Undone::Deleted(path.clone()).sentence(),
            "Deleted /private/tmp/work/a.txt (was newly created)"
        );
        assert_eq!(
            Undone::Unavailable {
                path,
                reason: "it changed".to_string()
            }
            .sentence(),
            "Could not undo /private/tmp/work/a.txt"
        );
        assert_eq!(Undone::Nothing.sentence(), "Nothing to undo.");
    }

    #[cfg(unix)]
    #[test]
    fn a_hostile_name_is_spelled_out_as_upstream_spells_it() {
        // `tests/e2e/tui-permissions.test.ts:1949-1980`: the window title
        // sequence and the line feed in the name come back as text.
        let path = PathBuf::from("/w/name\u{1b}]2;FX_PWN\u{7}\nfile.txt");
        assert_eq!(
            display_path(&path),
            "/w/name\\x1b]2;FX_PWN\\x07\\x0afile.txt"
        );
        assert_eq!(
            Undone::Deleted(path).sentence(),
            "Deleted /w/name\\x1b]2;FX_PWN\\x07\\x0afile.txt (was newly created)"
        );
        // A byte that is not UTF-8 is named, not replaced; an invisible
        // formatting code point is named too.
        use std::os::unix::ffi::OsStrExt;
        let raw = std::ffi::OsStr::from_bytes(b"/w/bad\xffname\xe2\x80\x8bz");
        assert_eq!(display_path(Path::new(raw)), "/w/bad\\xffname\\u{200b}z");
    }
}
