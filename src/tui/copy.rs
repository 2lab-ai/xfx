//! `/copy` on this surface: the clipboard write, off the thread that holds the
//! terminal.
//!
//! What is copied and how a clipboard tool is run are [`crate::clipboard`]'s;
//! what this module adds is **where the wait happens**. Upstream waits for its
//! clipboard tool on the thread that called it (`copyToClipboard`,
//! `native.zig:15-52`), and so does the line shell. Here that thread is the UI
//! thread, which sits in `pselect(2)` owning the terminal -- and a tool that
//! never exits, or one that reads slowly, would stop the band being painted.
//! So each copy runs on a thread of its own and says how it went on a channel
//! the UI reads once a turn of its loop ([`Copies::finished`]), the way it reads
//! the clock.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

use crate::clipboard::Clipboard;

/// The name each copy's thread is given, for a debugger or a panic message.
const THREAD_NAME: &str = "xfx-clipboard";

/// Where a shell built by a test looks for its clipboard tool: nowhere.
///
/// A test that wants a copy to happen gives the shell a directory with a fake
/// tool in it ([`Copies::new`]); one that does not cannot reach the user's
/// real clipboard by typing `/copy`.
#[cfg(test)]
pub(crate) const NOWHERE: &str = "/nonexistent/xfx-test-has-no-clipboard";

/// The copies this session has started, and how each of them went.
#[derive(Debug)]
pub(crate) struct Copies {
    clipboard: Clipboard,
    /// Cloned into each copy's thread, which sends one outcome and ends.
    done: Sender<bool>,
    outcomes: Receiver<bool>,
    /// The same for a composer selection's copies (Super+C and Super+X),
    /// apart from `/copy`'s because the two are answered differently: a
    /// `/copy` says how it went, and a selection's copy says nothing
    /// (`copy_selection`/`cut_selection`, `app_input_runtime.zig:356-385`,
    /// whose only report is a debug trace) -- but a cut takes its text out of
    /// the draft only once the clipboard has it.
    selection_done: Sender<(Selection, bool)>,
    selections: Receiver<(Selection, bool)>,
}

/// What a selection was copied for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Selection {
    /// Super+C: the clipboard is the whole of it.
    Copy,
    /// Super+X, numbered so the session can tell this cut from a later one.
    Cut(u64),
}

impl Copies {
    pub(crate) fn new(clipboard: Clipboard) -> Self {
        let (done, outcomes) = mpsc::channel();
        let (selection_done, selections) = mpsc::channel();
        Self {
            clipboard,
            done,
            outcomes,
            selection_done,
            selections,
        }
    }

    /// Starts putting a selection on the clipboard, through the same tool
    /// and off the same thread as `/copy` ([`Self::start`]); `false` when no
    /// thread could be started, which is a copy that failed.
    pub(crate) fn start_selection(&self, text: Arc<str>, purpose: Selection) -> bool {
        let clipboard = self.clipboard.clone();
        let done = self.selection_done.clone();
        std::thread::Builder::new()
            .name(THREAD_NAME.to_string())
            .spawn(move || {
                let _ = done.send((purpose, clipboard.copy(&text)));
            })
            .is_ok()
    }

    /// How one selection copy that has finished went, or `None` when none has
    /// finished since the last call.
    pub(crate) fn selection_finished(&self) -> Option<(Selection, bool)> {
        self.selections.try_recv().ok()
    }

    /// Starts putting `reply` on the clipboard, and returns without waiting.
    ///
    /// `false` when no thread could be started for it, which is a copy that
    /// failed and will never say so on the channel.
    pub(crate) fn start(&self, reply: Arc<str>) -> bool {
        let clipboard = self.clipboard.clone();
        let done = self.done.clone();
        std::thread::Builder::new()
            .name(THREAD_NAME.to_string())
            .spawn(move || {
                // Nobody left to tell is a session that has ended, and the
                // clipboard was written for it all the same.
                let _ = done.send(clipboard.copy(&reply));
            })
            .is_ok()
    }

    /// How one copy that has finished went -- `true` when the clipboard has
    /// the reply -- or `None` when none has finished since the last call.
    ///
    /// In the order they finished, which for two copies of one reply is the
    /// only order there is to tell apart.
    pub(crate) fn finished(&self) -> Option<bool> {
        self.outcomes.try_recv().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// Waits for one copy to finish, or fails the case after `limit`.
    fn outcome(copies: &Copies, limit: Duration) -> bool {
        let deadline = Instant::now() + limit;
        loop {
            if let Some(copied) = copies.finished() {
                return copied;
            }
            assert!(Instant::now() < deadline, "the copy never finished");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn a_copy_is_started_and_answered_later_rather_than_waited_for() {
        // A tool that takes its time, so "start returned" and "the copy is
        // over" are two moments apart.
        let tool = crate::clipboard::fake::FakeTool::running("cat >/dev/null\nsleep 1");
        let copies = Copies::new(tool.clipboard());
        let started = Instant::now();
        assert!(copies.start(Arc::from("reply")));
        assert!(
            started.elapsed() < Duration::from_millis(500),
            "starting a copy waited for the tool"
        );
        assert_eq!(copies.finished(), None, "a copy still running was answered");
        assert!(outcome(&copies, Duration::from_secs(10)));
    }

    #[test]
    fn a_selection_copy_is_answered_on_its_own_channel_with_its_purpose() {
        let tool = crate::clipboard::fake::FakeTool::keeping();
        let copies = Copies::new(tool.clipboard());
        assert!(copies.start_selection(Arc::from("picked"), Selection::Cut(7)));
        let deadline = Instant::now() + Duration::from_secs(10);
        let answer = loop {
            if let Some(answer) = copies.selection_finished() {
                break answer;
            }
            assert!(Instant::now() < deadline, "the copy never finished");
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(answer, (Selection::Cut(7), true));
        assert_eq!(tool.copied().as_deref(), Some(&b"picked"[..]));
        assert_eq!(
            copies.finished(),
            None,
            "a selection's copy was answered as a /copy"
        );
    }

    #[test]
    fn a_copy_that_cannot_reach_a_tool_says_so_rather_than_nothing() {
        let copies = Copies::new(Clipboard::searching(NOWHERE));
        assert!(copies.start(Arc::from("reply")));
        assert!(!outcome(&copies, Duration::from_secs(10)));
    }
}
