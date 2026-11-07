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
///
/// The same directory holds an unsaved session's copy of a file the user
/// attached (`super::shell`'s `captured`). Its root is upstream's: the real
/// path of `/tmp`, not `$TMPDIR` as given, so a relative or in-project
/// `TMPDIR` never puts a copy in the working tree
/// ([`crate::clipboard::snapshot_root`]); in it, a fresh `0700` directory of
/// a random name, and each copy `0600`. Removed when the shell ends normally
/// or unwinds from a panic -- best effort: a signal that ends the process
/// (`SIGINT`, `SIGTERM`, `SIGHUP`, re-raised without destructors,
/// `super::signals`) leaves the copies where they are, still owner-only.
#[derive(Debug, Default)]
pub(crate) struct PasteCopies {
    dir: Option<std::path::PathBuf>,
}

impl PasteCopies {
    /// Writes `bytes` as `name`, owner-only, in the session's private
    /// directory -- made on the first copy -- and hands back where.
    ///
    /// A name already there is removed and made again, never written
    /// through: a link planted at it, symbolic or hard, is unlinked rather
    /// than followed, and its target left as it was. The same number and the
    /// same bytes make the same name, so making it again is the honest
    /// retry. A write that fails removes what it had written.
    pub(crate) fn write(
        &mut self,
        name: &str,
        bytes: &[u8],
    ) -> std::io::Result<std::path::PathBuf> {
        use std::io::Write;
        let dir = match &self.dir {
            Some(dir) => dir.clone(),
            None => {
                let dir = crate::clipboard::private_directory_in(
                    &crate::clipboard::snapshot_root()?,
                    "xfx-image-snapshots",
                )?;
                self.dir = Some(dir.clone());
                dir
            }
        };
        let path = dir.join(name);
        let mut file = match create_private(&path) {
            Ok(file) => file,
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                std::fs::remove_file(&path)?;
                create_private(&path)?
            }
            Err(err) => return Err(err),
        };
        if let Err(err) = file.write_all(bytes).and_then(|()| file.flush()) {
            drop(file);
            let _ = std::fs::remove_file(&path);
            return Err(err);
        }
        Ok(path)
    }

    /// Removes one copy this made, when an attachment it was made for is
    /// refused after all. Only a file in this directory.
    pub(crate) fn discard(&self, path: &std::path::Path) {
        if let Some(dir) = &self.dir {
            if path.parent() == Some(dir.as_path()) {
                let _ = std::fs::remove_file(path);
            }
        }
    }

    /// The directory the copies are in, once one has been written.
    #[cfg(test)]
    pub(crate) fn dir(&self) -> Option<&std::path::Path> {
        self.dir.as_deref()
    }

    /// Copies kept in `dir`, as though it had been made: a case that needs a
    /// directory a write fails in, or one it can plant files in first.
    #[cfg(test)]
    pub(crate) fn at(dir: std::path::PathBuf) -> Self {
        Self { dir: Some(dir) }
    }
}

/// A new file at `path`, owner-only from the moment it exists, refused when
/// anything -- a link included, which is not followed -- is already there.
fn create_private(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
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
    fn a_link_planted_at_a_copys_name_is_unlinked_and_its_target_left_alone() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let outside = tempfile::tempdir().expect("somewhere else");
        let target = outside.path().join("target");
        std::fs::write(&target, "TARGET").expect("a target");
        let mut copies = PasteCopies::default();
        let first = copies.write("image-1.bin", b"one").expect("a first copy");
        let dir = copies.dir().expect("its directory").to_path_buf();
        assert_eq!(first.parent(), Some(dir.as_path()));
        // A symbolic link and a hard link, each at the name the next copy takes.
        std::os::unix::fs::symlink(&target, dir.join("image-2.bin")).expect("a symlink");
        std::fs::hard_link(&target, dir.join("image-3.bin")).expect("a hard link");
        for name in ["image-2.bin", "image-3.bin"] {
            let copy = copies.write(name, b"the picture").expect("a copy");
            let metadata = std::fs::symlink_metadata(&copy).expect("the copy");
            assert!(metadata.file_type().is_file(), "{name} is still a link");
            assert_eq!(metadata.nlink(), 1, "{name} is still the target's");
            assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
            assert_eq!(std::fs::read(&copy).expect("bytes"), b"the picture");
        }
        assert_eq!(
            std::fs::read_to_string(&target).expect("the target"),
            "TARGET"
        );
        assert_eq!(std::fs::metadata(&target).expect("the target").nlink(), 1);
        // The same name again is the same copy made again.
        let again = copies.write("image-1.bin", b"one").expect("the same copy");
        assert_eq!(again, first);
        assert_eq!(std::fs::read(&again).expect("bytes"), b"one");
    }

    #[test]
    fn the_copies_live_under_the_real_tmp_whatever_tmpdir_says_and_never_under_a_relative_base() {
        // Upstream's root (`createTempSnapshotDir`, `realpath("/tmp")`,
        // `image_attachments.zig:246-273@481104fd`), read without `$TMPDIR`.
        let root = crate::clipboard::snapshot_root().expect("a root");
        assert!(root.is_absolute());
        assert_eq!(root, std::fs::canonicalize("/tmp").expect("/tmp"));
        let mut copies = PasteCopies::default();
        let copy = copies.write("image-1.bin", b"one").expect("a copy");
        let dir = copy.parent().expect("its directory");
        assert_eq!(dir.parent(), Some(root.as_path()));
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir).expect("mode").permissions().mode() & 0o777;
            assert_eq!(mode, 0o700);
        }
        // A relative base is refused, and nothing is made relative to here.
        let relative = std::path::Path::new("xfx-relative-tmp-for-a-test");
        let refused = crate::clipboard::private_directory_in(relative, "xfx-image-snapshots")
            .expect_err("a relative base was used");
        assert_eq!(refused.kind(), std::io::ErrorKind::InvalidInput);
        assert!(
            !relative.exists(),
            "a directory was made relative to the current folder"
        );
        let gone = dir.to_path_buf();
        drop(copies);
        assert!(!gone.exists(), "{gone:?} outlived its copies");
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
