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
        super::signals::spawn_unsignalled(THREAD_NAME, move || {
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

/// Where a clipboard image is copied the moment it is pasted, so its badge
/// links a file that is there (`writeImageBadge`,
/// `image_attachments.zig:1336-1340`).
///
/// Upstream captures a pasted clipboard image into its session's `images/`
/// before the badge goes in, and names and links the image by that copy
/// (`insertImageAtCursor`, `image_commands.zig:138-147`); with no session to
/// hold it, the copy goes to a private temporary directory removed when the
/// session ends (`imageSnapshotStorageDir`, `session_store.zig:102-118`;
/// `createTempSnapshotDir`, `image_attachments.zig:240-262`; removed at
/// `app_session_runtime.zig:1378-1381`). xfx opens its session with the first
/// prompt, on another thread, so a paste always has no session to write into:
/// the copy is that temporary one, and the session keeps its own when the
/// prompt is recorded.
#[derive(Debug, Default)]
pub(crate) struct PasteCopies {
    dir: Option<std::path::PathBuf>,
}

impl PasteCopies {
    /// Writes `bytes` as `name`, owner-only, in the session's private
    /// directory -- made on the first paste -- and hands back where.
    pub(crate) fn write(
        &mut self,
        name: &str,
        bytes: &[u8],
    ) -> std::io::Result<std::path::PathBuf> {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let dir = match &self.dir {
            Some(dir) => dir.clone(),
            None => {
                let dir = crate::clipboard::private_directory("xfx-image-snapshots")?;
                self.dir = Some(dir.clone());
                dir
            }
        };
        let path = dir.join(name);
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .or_else(|err| match err.kind() {
                // The same number and the same bytes: the name carries both.
                std::io::ErrorKind::AlreadyExists => std::fs::OpenOptions::new()
                    .write(true)
                    .truncate(true)
                    .open(&path),
                _ => Err(err),
            })?;
        file.write_all(bytes)?;
        Ok(path)
    }

    /// The directory the copies are in, once one has been written.
    #[cfg(test)]
    pub(crate) fn dir(&self) -> Option<&std::path::Path> {
        self.dir.as_deref()
    }
}

impl Drop for PasteCopies {
    fn drop(&mut self) {
        if let Some(dir) = self.dir.take() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod tests {
    use super::*;
    use crate::clipboard::fake::FakeTool;
    use std::time::{Duration, Instant};

    /// Waits for one read to finish -- a minute, as the other fake-clipboard
    /// cases wait: the fake tool is a fresh script, which macOS assesses on
    /// its first run, past ten seconds under a loaded suite (seen failing at
    /// load 20-60 in fault-injection runs).
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
