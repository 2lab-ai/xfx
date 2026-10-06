//! `/paste` and Ctrl-V on this surface: the clipboard's image, read off the
//! thread that holds the terminal.
//!
//! How the clipboard is read is [`crate::clipboard::Clipboard::read_image`]'s;
//! what this module adds is **where the wait happens**, for the reason
//! [`super::copy`] gives for a copy. Upstream runs `osascript` on the thread
//! that called it (`attachClipboard`, `vercel-labs/fx@c1db9191
//! src/core/images/image_commands.zig:86-104`); here that thread sits in
//! `pselect(2)` owning the terminal, and a tool that does not exit must not be
//! something the screen waits on. So each read runs on a thread of its own and
//! says what it found on a channel the UI reads once a turn of its loop
//! ([`ClipboardReads::finished`]), and the badge goes in wherever the caret is
//! when it does.

use std::sync::mpsc::{self, Receiver, Sender};

use crate::clipboard::{Clipboard, ImageReadError};
use crate::image::MediaType;

/// The name each read's thread is given, for a debugger or a panic message.
const THREAD_NAME: &str = "xfx-clipboard-image";

/// What one read found: the image's format and bytes, or why there is none.
pub(crate) type Read = Result<(MediaType, Vec<u8>), ImageReadError>;

/// The clipboard reads this session has started, and what each found.
#[derive(Debug)]
pub(crate) struct ClipboardReads {
    clipboard: Clipboard,
    /// Cloned into each read's thread, which sends one answer and ends.
    done: Sender<Read>,
    answers: Receiver<Read>,
}

impl ClipboardReads {
    pub(crate) fn new(clipboard: Clipboard) -> Self {
        let (done, answers) = mpsc::channel();
        Self {
            clipboard,
            done,
            answers,
        }
    }

    /// Starts reading the clipboard's image, and returns without waiting.
    ///
    /// `false` when no thread could be started for it: a read that failed and
    /// will never answer on the channel.
    pub(crate) fn start(&self) -> bool {
        let clipboard = self.clipboard.clone();
        let done = self.done.clone();
        std::thread::Builder::new()
            .name(THREAD_NAME.to_string())
            .spawn(move || {
                // Nobody left to tell is a session that has ended.
                let _ = done.send(clipboard.read_image());
            })
            .is_ok()
    }

    /// What one read that has finished found, or `None` when none has
    /// finished since the last call -- in the order they finished.
    pub(crate) fn finished(&self) -> Option<Read> {
        self.answers.try_recv().ok()
    }
}

#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod tests {
    use super::*;
    use crate::clipboard::fake::FakeTool;
    use std::time::{Duration, Instant};

    /// A minute, as `copy.rs`'s cases wait: the fake tool's first run is
    /// assessed by macOS, which on a loaded machine outlasts ten seconds.
    fn answer(reads: &ClipboardReads) -> Read {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            if let Some(read) = reads.finished() {
                return read;
            }
            assert!(Instant::now() < deadline, "the read never finished");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn a_read_is_started_and_answered_later_rather_than_waited_for() {
        let tool = FakeTool::holding(Some(crate::image::fixture::SHOT_PNG));
        let reads = ClipboardReads::new(tool.clipboard());
        assert!(reads.start());
        assert_eq!(
            answer(&reads),
            Ok((MediaType::Png, crate::image::fixture::SHOT_PNG.to_vec()))
        );
        assert_eq!(reads.finished(), None, "one read answered twice");
    }

    #[test]
    fn a_shell_built_by_a_test_reads_no_clipboard_at_all() {
        let reads = ClipboardReads::new(Clipboard::searching(super::super::copy::NOWHERE));
        assert!(reads.start());
        assert_eq!(answer(&reads), Err(ImageReadError::Failed("FileNotFound")));
    }
}
