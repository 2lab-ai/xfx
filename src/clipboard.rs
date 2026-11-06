//! `/copy`: the last thing the model said, onto the system clipboard.
//!
//! Upstream's command (`commandCopyLast`, `vercel-labs/fx@c1db9191
//! src/core/app/app_commands.zig:1974-2001`) is three answers, and each is
//! given here once for both shells:
//!
//! - **What** is copied: [`last_reply`], the last assistant step that said
//!   anything, exactly as the model wrote it (`lastAssistantReply`,
//!   `src/core/session/session.zig:2067-2079`). The markdown, not the document
//!   it was rendered into: no gutter, no colour, no line break added.
//! - **How**: [`Clipboard::copy`], `pbcopy` on macOS and `xclip -selection
//!   clipboard` on Linux, found on `PATH`, fed the bytes on its standard input
//!   and judged by its exit status alone (`src/core/hosts/native.zig:15-52`,
//!   `:229-242`).
//! - **What is said** about it: [`NO_REPLY`], [`COPIED`] and [`COPY_FAILED`],
//!   under the topic [`TOPIC`].
//!
//! **Where** it runs is each shell's own business. The line shell copies on
//! its read loop, which waits for a line anyway; the TUI hands the copy to a
//! thread of its own, because its UI thread holds the terminal and a clipboard
//! tool that does not exit must not be something the screen waits for.
//!
//! `/trace` uses the same clipboard for a **file** rather than for text
//! ([`Clipboard::copy_file`]): upstream's second clipboard operation, which on
//! macOS is a script for `osascript` rather than bytes for `pbcopy`.

use std::ffi::OsString;
use std::io::Write;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use crate::session::{DurableState, TurnStep};

/// The topic every `/copy` notice is filed under (`app_commands.zig:1978`).
pub const TOPIC: &str = "clipboard";

/// What `/copy` says when the conversation has no reply in it
/// (`app_commands.zig:1979`).
pub const NO_REPLY: &str = "No assistant reply to copy.";

/// What `/copy` says when the clipboard tool took the bytes and exited 0
/// (`app_commands.zig:1996`).
pub const COPIED: &str = "Copied to clipboard.";

/// What `/copy` says about every other outcome: no tool for this system, one
/// that could not be started or fed, and one that exited with anything but 0
/// (`app_commands.zig:1988`). Upstream says the same sentence for all of them,
/// and so does this: which of them it was is a fact about the machine, not
/// something the user can act on from inside a session.
pub const COPY_FAILED: &str = "Failed to copy to clipboard.";

/// The last thing the model said in `state`, as it said it.
///
/// The newest assistant step whose text is not empty, searched from the end
/// of the newest turn backwards -- so a turn that ended on a tool call, or one
/// whose last step was only calls, still yields the text the model wrote
/// before them, as upstream's history walk does. Not trimmed: the bytes are
/// the model's, and a reply that ended without a line break is copied without
/// one.
pub fn last_reply(state: &DurableState) -> Option<&str> {
    state
        .turns
        .iter()
        .rev()
        .flat_map(|turn| turn.steps.iter().rev())
        .find_map(|step| match step {
            TurnStep::Assistant { text, .. } if !text.is_empty() => Some(text.as_str()),
            _ => None,
        })
}

/// The tool this system copies with, and its arguments
/// (`clipboardCommand`, `native.zig:229-235`).
///
/// `None` where upstream has none either, which makes every copy a failure
/// there rather than a guess at what a third system calls its clipboard.
const TOOL: Option<(&str, &[&str])> = if cfg!(target_os = "macos") {
    Some(("pbcopy", &[]))
} else if cfg!(target_os = "linux") {
    Some(("xclip", &["-selection", "clipboard"]))
} else {
    None
};

/// What puts a file on the macOS pasteboard, byte for byte upstream's
/// (`copy_file_to_clipboard`, `native.zig:177-199`).
///
/// The file's URL as `public.file-url` and its path as `NSFilenamesPboardType`,
/// each read back and compared, so a pasteboard that took neither -- or took
/// something else -- is a failure rather than a success nobody can paste.
/// Eager representations, so the pasteboard server owns them once this
/// short-lived process has gone. The path is the script's one argument.
const FILE_SCRIPT: &str = r#"function run(argv) {
  ObjC.import("AppKit");
  var url = $.NSURL.fileURLWithPath(argv[0]).standardizedURL;
  var expected = ObjC.unwrap(url.absoluteString);
  var pb = $.NSPasteboard.generalPasteboard;
  pb.clearContents;
  if (!pb.setStringForType(expected, "public.file-url")) {
    throw new Error("public.file-url materialization failed");
  }
  var files = $.NSArray.arrayWithObject(argv[0]);
  if (!pb.setPropertyListForType(files, "NSFilenamesPboardType")) {
    throw new Error("NSFilenamesPboardType materialization failed");
  }
  var copiedValue = pb.stringForType("public.file-url");
  if (!copiedValue) throw new Error("public.file-url readback missing");
  var copied = ObjC.unwrap(copiedValue);
  if (copied !== expected) throw new Error("public.file-url readback mismatch");
  var copiedFiles = ObjC.deepUnwrap(pb.propertyListForType("NSFilenamesPboardType"));
  if (!copiedFiles || copiedFiles.length !== 1 || copiedFiles[0] !== argv[0]) {
    throw new Error("NSFilenamesPboardType readback mismatch");
  }
}"#;

/// How long a file copy may take before its tool is killed and the copy is a
/// failure (`copy_file_to_clipboard`'s five seconds, `native.zig:201-205`).
const FILE_DEADLINE: Duration = Duration::from_secs(5);

/// The system clipboard, reached through the tool on `PATH`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Clipboard {
    /// Where the tool is looked for, when it is not the process's own `PATH`.
    ///
    /// Set on the child alone ([`Command::env`]), which is also the path the
    /// standard library resolves the program against -- so a test can put a
    /// fake `pbcopy` first without touching the environment every other test
    /// in the process is reading.
    search: Option<OsString>,
    /// How long [`Self::copy_file`] waits, when it is not [`FILE_DEADLINE`]:
    /// a test of the deadline should not cost five seconds.
    deadline: Option<Duration>,
}

impl Clipboard {
    /// The clipboard the user has: the tool found on the process's `PATH`.
    pub fn system() -> Self {
        Self::default()
    }

    /// The tool found on `search` instead.
    pub fn searching(search: impl Into<OsString>) -> Self {
        Self {
            search: Some(search.into()),
            deadline: None,
        }
    }

    /// The same clipboard, giving a file copy `deadline` rather than five
    /// seconds.
    #[cfg(test)]
    pub(crate) fn waiting(mut self, deadline: Duration) -> Self {
        self.deadline = Some(deadline);
        self
    }

    /// Puts the **file** at `path` on the clipboard -- what a paste into a
    /// mail, a chat or an issue attaches -- and says whether it worked
    /// (`copy_file_to_clipboard`, `native.zig:172-225`).
    ///
    /// macOS only, as upstream's is: `osascript -l JavaScript` found on `PATH`
    /// runs [`FILE_SCRIPT`] with the path as its last argument. Everywhere else
    /// there is no file clipboard to put it on, nothing is run, and the answer
    /// is `false` (`:176`).
    ///
    /// **Blocks** until the tool exits or [`FILE_DEADLINE`] passes, when it is
    /// killed and the copy is a failure; the TUI calls it from a thread of its
    /// own ([`Self::copy_file_until`]). Its output goes nowhere, for the reason
    /// [`Self::copy`]'s does, and only an exit status of 0 is a success.
    pub fn copy_file(&self, path: &Path) -> bool {
        self.copy_file_until(path, &Stop::default())
    }

    /// [`Self::copy_file`], given up -- the tool killed, the copy a failure --
    /// once `stop` is.
    pub fn copy_file_until(&self, path: &Path, stop: &Stop) -> bool {
        if !cfg!(target_os = "macos") {
            return false;
        }
        let mut command = crate::process::command("osascript");
        command
            .args(["-l", "JavaScript", "-e", FILE_SCRIPT])
            .arg(path)
            .stdin(Stdio::null());
        self.run(
            command,
            None,
            Some(self.deadline.unwrap_or(FILE_DEADLINE)),
            stop,
        )
    }

    /// Puts `text` on the clipboard, byte for byte, and says whether it worked.
    ///
    /// **Blocks until the tool exits.** That is upstream's contract too
    /// (`copyToClipboard` waits for the child), and it is why the TUI never
    /// calls this on its UI thread ([`Self::copy_until`]).
    ///
    /// Everything that is not "the tool took every byte and exited 0" is
    /// `false`: a system with no tool, a tool that is not on the path, a pipe
    /// it closed before the bytes were through, and any end but an exit status
    /// of 0 -- a signal included (`copySucceeded`, `native.zig:237-242`). Standard output and
    /// error go nowhere, because the only thing the tool is asked is its exit
    /// status -- and because the TUI owns the terminal they would land on.
    pub fn copy(&self, text: &str) -> bool {
        self.copy_until(text, &Stop::default())
    }

    /// [`Self::copy`], given up -- the tool killed, the copy a failure -- once
    /// `stop` is.
    pub fn copy_until(&self, text: &str, stop: &Stop) -> bool {
        let Some((program, args)) = TOOL else {
            return false;
        };
        let mut command = crate::process::command(program);
        command.args(args).stdin(Stdio::piped());
        self.run(command, Some(text.as_bytes()), None, stop)
    }

    /// Runs one clipboard tool to its end: fed `input` when there is some, and
    /// waited for until it exits, `deadline` passes or `stop` is given up.
    ///
    /// Work already given up is refused before the tool is started, not
    /// started and then killed: a kill that lands a moment after the start
    /// still leaves the tool time to act.
    ///
    /// The child is held by `stop` while it runs ([`Stop::stop`]), and is only
    /// ever reaped under that hold -- so a kill from another thread always
    /// reaches this tool and never a process that has since taken its id. A
    /// write the tool is not reading blocks here until the kill closes its end.
    fn run(
        &self,
        mut command: Command,
        input: Option<&[u8]>,
        deadline: Option<Duration>,
        stop: &Stop,
    ) -> bool {
        command.stdout(Stdio::null()).stderr(Stdio::null());
        if let Some(search) = &self.search {
            command.env("PATH", search);
        }
        // Outside the lock, so a stop never waits on a start -- which macOS
        // can take seconds over for a fresh executable. A stop that lands
        // after this look is caught by the hold or by the stop itself,
        // whichever takes the lock second.
        if stop.stopped() {
            return false;
        }
        let Ok(mut child) = crate::process::spawn(&mut command) else {
            return false;
        };
        let stdin = child.stdin.take();
        if !stop.hold(child) {
            return false;
        }
        // Dropped at the end of the statement: the tool sees its end of input
        // there, and only then can it exit. A write that failed still drops it,
        // so the wait below cannot be a wait for input that is never coming.
        let written = match (stdin, input) {
            (Some(mut stdin), Some(bytes)) => stdin.write_all(bytes).is_ok(),
            (_, None) => true,
            (None, Some(_)) => false,
        };
        let deadline = deadline.map(|limit| Instant::now() + limit);
        loop {
            let exited = stop.settle(|| deadline.is_some_and(|at| Instant::now() >= at));
            if let Some(exited) = exited {
                return written && exited;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

/// A way to give up clipboard work running on another thread, from the thread
/// that started it: the tool it is waiting for is killed and the work says it
/// failed.
///
/// What a session that is ending does with a copy still in flight
/// (`crate::tui`'s `copy` and `trace`): the terminal is about to be given back,
/// and a tool that never exits must not keep a thread of xfx's alive past
/// that. One `Stop` per piece of work.
#[derive(Debug, Clone, Default)]
pub struct Stop(Arc<StopState>);

#[derive(Debug, Default)]
struct StopState {
    stopped: AtomicBool,
    /// The tool the work is waiting for, while it runs.
    child: Mutex<Option<Child>>,
}

impl Stop {
    /// Gives the work up: a tool it is running is killed, one it has not
    /// reached yet is never started, and one it is starting at that moment is
    /// killed as soon as it has been. Idempotent, and it never waits on a
    /// process start: only on the lock, while the work hands a tool over or
    /// reaps one that was killed.
    pub fn stop(&self) {
        self.0.stopped.store(true, Ordering::Release);
        if let Some(child) = self.lock().as_mut() {
            // Not reaped yet -- only `settle` reaps, under this lock -- so the
            // id is still this tool's.
            let _ = child.kill();
        }
    }

    /// Whether [`Self::stop`] has been called.
    pub fn stopped(&self) -> bool {
        self.0.stopped.load(Ordering::Acquire)
    }

    /// Takes the tool that was just started, or kills and reaps it when the
    /// work was given up while it was starting. `false` when it was.
    fn hold(&self, mut child: Child) -> bool {
        let mut slot = self.lock();
        if self.stopped() {
            let _ = child.kill();
            let _ = child.wait();
            return false;
        }
        *slot = Some(child);
        true
    }

    /// How the held tool ended -- `Some(true)` for an exit status of 0 --
    /// once it has; or, given up or `overdue`, kills and reaps it and says
    /// `Some(false)`; `None` while it is still running.
    fn settle(&self, overdue: impl FnOnce() -> bool) -> Option<bool> {
        let mut slot = self.lock();
        let child = slot.as_mut()?;
        let ended = match child.try_wait() {
            Ok(Some(status)) => Some(status.success()),
            Ok(None) if !self.stopped() && !overdue() => None,
            // Given up, past its deadline, or a wait that failed: either way
            // the tool is not answering, and it is not left running.
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                Some(false)
            }
        };
        if ended.is_some() {
            *slot = None;
        }
        ended
    }

    fn lock(&self) -> MutexGuard<'_, Option<Child>> {
        self.0.child.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Why the clipboard gave no image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageReadError {
    /// It holds none: the tool said so by its exit status.
    NoImage,
    /// This system has no way to read one that xfx knows of. Upstream says
    /// nothing at all here (`image_commands.zig:91`).
    Unsupported,
    /// Anything else, by upstream's name for it: the tool could not be
    /// started (`FileNotFound`), it said it wrote a file that is not there,
    /// what it gave is no image this build reads, or it is too large.
    Failed(&'static str),
}

/// The AppleScript upstream reads the clipboard with
/// (`loadClipboardImageAttachment`, `image_attachments.zig:1779-1823`): the
/// clipboard as PNG, written to `path`. The clipboard converts whatever image
/// it holds on the way out, so what comes back is always a PNG.
fn clipboard_script(path: &std::path::Path) -> String {
    format!(
        "set outFile to POSIX file \"{}\"\n\
         set pngData to the clipboard as \u{ab}class PNGf\u{bb}\n\
         set fileRef to open for access outFile with write permission\n\
         set eof fileRef to 0\n\
         write pngData to fileRef\n\
         close access fileRef\n",
        path.display()
    )
}

impl Clipboard {
    /// The image on the clipboard, as its format and bytes.
    ///
    /// **Blocks until the tool exits**, as [`Self::copy`] does, so the TUI
    /// never calls it on its UI thread (`crate::tui`'s `clipboard_image`).
    ///
    /// On macOS, upstream's way: `osascript` found on the path writes the
    /// clipboard as PNG into a directory of this read's own, owner-only, and
    /// removed again before this returns; an exit status other than 0 is a
    /// clipboard with no image. On Linux, the copy tool's other half: `xclip
    /// -selection clipboard -t image/png -o`, its standard output read up to the
    /// ceiling -- where upstream has nothing (`error.Unsupported`), and a
    /// command that answered nothing at all would be one that did not work.
    /// Elsewhere, [`ImageReadError::Unsupported`].
    pub fn read_image(&self) -> Result<(crate::image::MediaType, Vec<u8>), ImageReadError> {
        if cfg!(target_os = "macos") {
            self.read_image_with_osascript()
        } else if cfg!(target_os = "linux") {
            self.read_image_with_xclip()
        } else {
            Err(ImageReadError::Unsupported)
        }
    }

    fn command(&self, program: &str) -> Command {
        let mut command = crate::process::command(program);
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Some(search) = &self.search {
            command.env("PATH", search);
        }
        command
    }

    fn read_image_with_osascript(
        &self,
    ) -> Result<(crate::image::MediaType, Vec<u8>), ImageReadError> {
        let dir = private_directory("xfx-clipboard")
            .map_err(|err| ImageReadError::Failed(crate::image::io_name(err.kind())))?;
        let path = dir.join("clipboard.png");
        let mut command = self.command("osascript");
        command.arg("-e").arg(clipboard_script(&path));
        let outcome = crate::process::spawn(&mut command).and_then(|mut child| child.wait());
        let read = match outcome {
            Err(err) => Err(ImageReadError::Failed(crate::image::io_name(err.kind()))),
            Ok(status) if !status.success() => Err(ImageReadError::NoImage),
            Ok(_) => crate::image::read(&path).map_err(|err| ImageReadError::Failed(err.name())),
        };
        // Gone whichever way it went: what the clipboard held is the session's
        // to keep, in its own copy, and not this directory's.
        let _ = std::fs::remove_dir_all(&dir);
        read
    }

    fn read_image_with_xclip(&self) -> Result<(crate::image::MediaType, Vec<u8>), ImageReadError> {
        use std::io::Read;
        let mut command = self.command("xclip");
        command
            .args(["-selection", "clipboard", "-t", "image/png", "-o"])
            .stdout(Stdio::piped());
        let mut child = crate::process::spawn(&mut command)
            .map_err(|err| ImageReadError::Failed(crate::image::io_name(err.kind())))?;
        let mut bytes = Vec::new();
        let read = child.stdout.take().map(|stdout| {
            stdout
                .take(crate::image::MAX_IMAGE_BYTES + 1)
                .read_to_end(&mut bytes)
        });
        // Over the ceiling, the tool may still be writing: it is not waited
        // on to finish what will not be read.
        if bytes.len() as u64 > crate::image::MAX_IMAGE_BYTES {
            let _ = child.kill();
            let _ = child.wait();
            return Err(ImageReadError::Failed(
                crate::image::AttachError::TooLarge.name(),
            ));
        }
        let status = child.wait();
        if !status.is_ok_and(|status| status.success()) || !matches!(read, Some(Ok(_))) {
            return Err(ImageReadError::NoImage);
        }
        let media_type = crate::image::MediaType::sniff(&bytes).ok_or(ImageReadError::Failed(
            crate::image::AttachError::Unsupported.name(),
        ))?;
        Ok((media_type, bytes))
    }
}

/// A new directory only this user can enter, under the system's temporary
/// directory, named `<prefix>-<random>`: one clipboard read's, or a session's
/// pasted images' (`crate::tui`'s paste copies).
pub(crate) fn private_directory(prefix: &str) -> std::io::Result<std::path::PathBuf> {
    use std::os::unix::fs::DirBuilderExt;
    let base = std::env::temp_dir();
    let mut last = None;
    for _ in 0..16 {
        let dir = base.join(format!("{prefix}-{}", crate::session::new_identifier()));
        match std::fs::DirBuilder::new().mode(0o700).create(&dir) {
            Ok(()) => return Ok(dir),
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => last = Some(err),
            Err(err) => return Err(err),
        }
    }
    Err(last.unwrap_or_else(|| std::io::Error::from(std::io::ErrorKind::AlreadyExists)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PermissionMode;
    use crate::session::{
        NewSession, RecordedToolCall, SessionEvent, SessionId, SessionRecorder, SessionStore,
    };

    /// A session written through the real log, so the state a reply is read
    /// out of is the one replay builds rather than one a test put together.
    struct Recorded {
        _home: tempfile::TempDir,
        recorder: SessionRecorder,
    }

    impl Recorded {
        fn new() -> Self {
            let home = tempfile::tempdir().expect("a home");
            // A directory the store creates, so it is created private.
            let store = SessionStore::open(&home.path().join(".xfx")).expect("a store");
            let session = store
                .create(
                    SessionId::generate(),
                    NewSession {
                        origin_workspace_root: home.path().to_path_buf(),
                        workspace_root: home.path().to_path_buf(),
                        model: "fake/model-a".to_string(),
                        permission_mode: PermissionMode::Ask,
                        provider: None,
                    },
                )
                .expect("a session");
            Self {
                recorder: SessionRecorder::new(store, session),
                _home: home,
            }
        }

        fn commit(mut self, event: SessionEvent) -> Self {
            self.recorder.commit(event);
            assert_eq!(self.recorder.failure(), None, "the log refused it");
            self
        }

        fn asked(self) -> Self {
            self.commit(SessionEvent::UserMessage {
                text: "asked".to_string(),
            })
        }

        /// One assistant step: `text`, and one tool call when `calls`.
        fn said(self, text: &str, calls: bool) -> Self {
            let tool_calls = if calls {
                vec![RecordedToolCall {
                    id: "call-1".to_string(),
                    name: "read_file".to_string(),
                    input: serde_json::json!({"path": "a"}),
                }]
            } else {
                Vec::new()
            };
            self.commit(SessionEvent::AssistantMessage {
                text: text.to_string(),
                tool_calls,
                raw_content: Vec::new(),
                responses_state: Vec::new(),
                wire: None,
                model: None,
            })
        }

        fn ran(self) -> Self {
            self.commit(SessionEvent::ToolResult {
                ending: None,
                lines: None,
                change: None,
                call_id: "call-1".to_string(),
                tool: "read_file".to_string(),
                ok: true,
                output: "the file".to_string(),
            })
        }

        fn reply(&self) -> Option<&str> {
            last_reply(self.recorder.state())
        }
    }

    #[test]
    fn a_conversation_with_nothing_said_in_it_has_no_reply() {
        assert_eq!(Recorded::new().reply(), None);
        // A turn whose model only called a tool, or said an empty step, has
        // not replied either.
        let quiet = Recorded::new().asked().said("", true).ran().said("", false);
        assert_eq!(quiet.reply(), None);
    }

    #[test]
    fn the_reply_is_the_newest_text_the_model_wrote_byte_for_byte() {
        // `fx-sessions-40x120-sh.log`: the backticks stay and no line break is
        // added. The tab and the escape are what the TUI's own copy of the
        // stream no longer has (`tui::bridge::inert`), which is why the reply
        // is read from here.
        let answer = "ALPHA answer line one.\n\nSecond paragraph with `code`.\t\u{1b}[1m";
        let recorded = Recorded::new()
            .asked()
            .said("an older answer", false)
            .asked()
            .said(answer, false);
        assert_eq!(recorded.reply(), Some(answer));
    }

    #[test]
    fn a_turn_that_ended_on_a_tool_still_has_the_text_written_before_it() {
        // `session.zig:2067-2079`: the walk passes over every entry that is not
        // an assistant entry with text, wherever it is.
        let said_then_called = Recorded::new()
            .asked()
            .said("the previous answer", false)
            .asked()
            .said("let me look", true)
            .ran()
            .said("", true);
        assert_eq!(said_then_called.reply(), Some("let me look"));
        let only_called = Recorded::new()
            .asked()
            .said("the previous answer", false)
            .asked()
            .said("", true)
            .ran();
        assert_eq!(only_called.reply(), Some("the previous answer"));
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    mod tool {
        use super::super::fake::FakeTool;

        #[test]
        fn the_tool_on_the_path_is_fed_the_exact_bytes() {
            let tool = FakeTool::keeping();
            let text = "ALPHA answer line one.\n\nSecond paragraph with `code`.\t\u{1b}[1m";
            assert!(tool.clipboard().copy(text));
            assert_eq!(
                tool.copied().expect("the fake was fed"),
                text.as_bytes(),
                "the clipboard did not get the reply byte for byte"
            );
        }

        #[test]
        fn a_tool_that_exits_with_anything_but_zero_is_a_failure() {
            let tool = FakeTool::keeping();
            tool.fail();
            assert!(!tool.clipboard().copy("reply"));
            assert_eq!(tool.copied(), None, "the failing tool kept the bytes");
        }

        #[test]
        fn an_image_on_the_clipboard_is_read_whole_and_typed_by_its_bytes() {
            let tool = FakeTool::holding(Some(crate::image::fixture::SHOT_PNG));
            assert_eq!(
                tool.clipboard().read_image(),
                Ok((
                    crate::image::MediaType::Png,
                    crate::image::fixture::SHOT_PNG.to_vec()
                ))
            );
        }

        #[test]
        fn a_clipboard_with_no_image_says_so_by_the_tools_exit_status() {
            let tool = FakeTool::holding(None);
            assert_eq!(
                tool.clipboard().read_image(),
                Err(super::super::ImageReadError::NoImage)
            );
        }

        #[test]
        fn what_the_clipboard_gave_that_is_no_image_is_refused_by_upstreams_name() {
            let tool = FakeTool::holding(Some(b"just some text"));
            assert_eq!(
                tool.clipboard().read_image(),
                Err(super::super::ImageReadError::Failed("UnsupportedImageType"))
            );
        }

        #[test]
        fn a_reader_that_is_not_on_the_path_is_upstreams_file_not_found() {
            let empty = tempfile::tempdir().expect("an empty directory");
            assert_eq!(
                super::super::Clipboard::searching(empty.path().as_os_str()).read_image(),
                Err(super::super::ImageReadError::Failed("FileNotFound"))
            );
        }

        /// The capture harness's shim: `osascript` that exits 0 and writes
        /// nothing. Upstream then fails to load the file it was promised, and
        /// says so by that name (`fx-paste-40x120-paste.ansi`).
        #[cfg(target_os = "macos")]
        #[test]
        fn an_osascript_that_writes_nothing_is_a_file_not_found() {
            use std::os::unix::fs::PermissionsExt;
            let dir = tempfile::tempdir().expect("a directory");
            let shim = dir.path().join("osascript");
            std::fs::write(&shim, "#!/bin/sh\nexit 0\n").expect("a shim");
            std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755))
                .expect("executable");
            let clipboard =
                super::super::Clipboard::searching(format!("{}:/bin", dir.path().display()));
            assert_eq!(
                clipboard.read_image(),
                Err(super::super::ImageReadError::Failed("FileNotFound"))
            );
        }

        #[test]
        fn a_tool_that_is_not_on_the_path_is_a_failure() {
            let empty = tempfile::tempdir().expect("an empty directory");
            assert!(!super::super::Clipboard::searching(empty.path().as_os_str()).copy("reply"));
        }

        #[test]
        fn a_tool_that_leaves_before_reading_is_a_failure_and_not_a_hang() {
            let tool = FakeTool::running("exit 0");
            // Far more than a pipe holds, so the write cannot be through before
            // the tool has gone: the broken pipe is an error here, not a signal
            // (the runtime ignores `SIGPIPE`), and an exit status of 0 does not
            // make a copy that never arrived a success.
            let text = "x".repeat(4 << 20);
            assert!(!tool.clipboard().copy(&text));
        }

        /// A report's path, as `/trace` would hand one over.
        const REPORT: &str = "/private/tmp/xfx-trace-2026-10-07-031245-0123456789ab.md";

        #[test]
        #[cfg(target_os = "macos")]
        fn a_file_copy_hands_the_tool_the_path_as_its_last_argument() {
            // Upstream's e2e reads the fixture's last argument and expects the
            // report's path (`tui-command-permissions.test.ts:1880-1881`).
            let tool = FakeTool::keeping();
            assert!(tool.clipboard().copy_file(std::path::Path::new(REPORT)));
            assert_eq!(tool.copied_file().as_deref(), Some(REPORT));
            assert_eq!(tool.copied(), None, "a file copy fed the text tool");
        }

        #[test]
        #[cfg(target_os = "macos")]
        fn a_file_copy_whose_tool_fails_or_is_missing_is_a_failure() {
            let tool = FakeTool::keeping();
            tool.fail();
            assert!(!tool.clipboard().copy_file(std::path::Path::new(REPORT)));
            assert_eq!(tool.copied_file(), None);
            let empty = tempfile::tempdir().expect("an empty directory");
            assert!(
                !super::super::Clipboard::searching(empty.path().as_os_str())
                    .copy_file(std::path::Path::new(REPORT))
            );
        }

        #[test]
        #[cfg(target_os = "macos")]
        fn a_file_copy_that_outlives_its_deadline_is_killed_and_a_failure() {
            let tool = FakeTool::keeping();
            tool.hang();
            let started = std::time::Instant::now();
            assert!(!tool
                .clipboard()
                .waiting(std::time::Duration::from_millis(200))
                .copy_file(std::path::Path::new(REPORT)));
            assert!(
                started.elapsed() < std::time::Duration::from_secs(10),
                "the copy waited for a tool that never answered"
            );
        }

        /// Waits, up to a minute, until `ready` says so: a fresh fake tool is
        /// assessed by macOS the first time it runs, slowly under load.
        fn until(ready: impl Fn() -> bool) {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
            while !ready() {
                assert!(std::time::Instant::now() < deadline, "never happened");
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }

        #[test]
        fn a_copy_given_up_from_another_thread_kills_its_tool_and_fails() {
            // What a session that is ending does with a copy still waiting on a
            // tool that will not exit (`crate::tui`'s `Errands`).
            let tool =
                FakeTool::running("cat >/dev/null\necho $$ > \"${0%/*}/hung\"\nexec sleep 30");
            let stop = super::super::Stop::default();
            let copying = {
                let (clipboard, stop) = (tool.clipboard(), stop.clone());
                std::thread::spawn(move || clipboard.copy_until("reply", &stop))
            };
            until(|| tool.hung().is_some());
            let pid = tool.hung().expect("the tool started");
            let started = std::time::Instant::now();
            stop.stop();
            assert!(
                !copying.join().expect("the copy's thread"),
                "a killed copy succeeded"
            );
            assert!(started.elapsed() < std::time::Duration::from_secs(5));
            assert!(!FakeTool::alive(pid), "the tool outlived the copy");
        }

        /// Runs `work` on a thread of its own while this case holds `stop`'s
        /// lock -- the one a started tool is handed over under -- and says
        /// what it returned.
        ///
        /// Work that starts its tool waits in the hand-over for as long as the
        /// case keeps the lock, so the tool always has the time to say it ran
        /// (`started`); work that refuses first never needs the lock and is
        /// done at once.
        fn held(
            stop: &super::super::Stop,
            started: impl Fn() -> bool,
            work: impl FnOnce(super::super::Stop) -> bool + Send + 'static,
        ) -> bool {
            let lock = stop.lock();
            let working = {
                let stop = stop.clone();
                std::thread::spawn(move || work(stop))
            };
            until(|| working.is_finished() || started());
            drop(lock);
            working.join().expect("the work's thread")
        }

        #[test]
        fn work_given_up_before_its_tool_starts_never_starts_it() {
            // Not started and then killed: a kill a moment after the start
            // still leaves the tool time to act, and on a Linux runner its
            // shell had made its file before the kill landed (PR #32). Held,
            // a tool that is started always gets that time.
            let tool = FakeTool::running("echo $$ > \"${0%/*}/hung\"\nexec cat >/dev/null");
            let stop = super::super::Stop::default();
            stop.stop();
            assert!(stop.stopped());
            let clipboard = tool.clipboard();
            assert!(!held(
                &stop,
                || tool.hung().is_some(),
                move |stop| clipboard.copy_until("reply", &stop)
            ));
            assert_eq!(tool.hung(), None, "a copy given up started its tool");
            // A tool of its own, so what the first copy did cannot answer for it.
            let tool = FakeTool::keeping();
            tool.hang();
            let clipboard = tool.clipboard();
            assert!(!held(
                &stop,
                || tool.hung().is_some(),
                move |stop| clipboard.copy_file_until(std::path::Path::new(REPORT), &stop)
            ));
            assert_eq!(tool.hung(), None, "a file copy given up started its tool");
        }

        #[test]
        fn a_stop_that_lands_after_the_tool_starts_still_kills_it() {
            // Between the start and the hand-over: the look before the start
            // has passed, so it is the hand-over's own look that catches it.
            let tool = FakeTool::running("echo $$ > \"${0%/*}/hung\"\nexec cat >/dev/null");
            let stop = super::super::Stop::default();
            let lock = stop.lock();
            let copying = {
                let (clipboard, stop) = (tool.clipboard(), stop.clone());
                std::thread::spawn(move || clipboard.copy_until("reply", &stop))
            };
            until(|| tool.hung().is_some());
            let pid = tool.hung().expect("the tool started");
            // It gives the copy up, then waits for the lock this case holds.
            let stopping = {
                let stop = stop.clone();
                std::thread::spawn(move || stop.stop())
            };
            until(|| stop.stopped());
            drop(lock);
            stopping.join().expect("the stop's thread");
            assert!(
                !copying.join().expect("the copy's thread"),
                "a copy given up succeeded"
            );
            assert!(!FakeTool::alive(pid), "the tool outlived the copy");
        }

        #[test]
        #[cfg(target_os = "macos")]
        fn a_file_copy_given_up_kills_its_tool_long_before_its_deadline() {
            let tool = FakeTool::keeping();
            tool.hang();
            let stop = super::super::Stop::default();
            let copying = {
                let (clipboard, stop) = (tool.clipboard(), stop.clone());
                std::thread::spawn(move || {
                    clipboard.copy_file_until(std::path::Path::new(REPORT), &stop)
                })
            };
            until(|| tool.hung().is_some());
            let pid = tool.hung().expect("the tool started");
            stop.stop();
            assert!(!copying.join().expect("the copy's thread"));
            assert!(!FakeTool::alive(pid), "the tool outlived the copy");
        }

        #[test]
        #[cfg(not(target_os = "macos"))]
        fn a_file_copy_off_macos_runs_nothing_and_is_not_a_success() {
            // `copy_file_to_clipboard` returns false before it spawns anything
            // on any other system (`native.zig:176`).
            let tool = FakeTool::keeping();
            assert!(!tool.clipboard().copy_file(std::path::Path::new(REPORT)));
            assert_eq!(tool.copied_file(), None, "a tool was run");
        }

        #[test]
        #[cfg(target_os = "macos")]
        fn a_file_copy_given_up_while_its_tool_starts_fails_though_the_tool_finished() {
            // The hand-over's own look at the stop, and nothing else. The tool
            // has exited 0 before it is handed over, so `settle` would call the
            // copy a success, and the stop finds no tool to kill. The stop is
            // given up before the lock is let go, so whichever of the stop and
            // the hand-over takes the lock first, the hand-over sees it. macOS
            // only: elsewhere a file copy starts nothing, and a text copy's
            // write to a tool that has gone fails it anyway.
            use std::os::unix::fs::PermissionsExt;
            let dir = tempfile::tempdir().expect("a directory");
            let shim = dir.path().join("osascript");
            std::fs::write(&shim, "#!/bin/sh\necho $$ > \"${0%/*}/ran\"\n").expect("a shim");
            std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755))
                .expect("executable");
            let ran = || {
                std::fs::read_to_string(dir.path().join("ran"))
                    .ok()
                    .and_then(|pid| pid.trim().parse::<i32>().ok())
            };
            let clipboard =
                super::super::Clipboard::searching(format!("{}:/bin", dir.path().display()))
                    .waiting(std::time::Duration::from_secs(60));
            let stop = super::super::Stop::default();
            let lock = stop.lock();
            let copying = {
                let stop = stop.clone();
                std::thread::spawn(move || {
                    clipboard.copy_file_until(std::path::Path::new(REPORT), &stop)
                })
            };
            until(|| ran().is_some());
            let pid = rustix::process::Pid::from_raw(ran().expect("the tool started"))
                .expect("a process id");
            // Exited and not reaped: only the copy reaps it, after the lock.
            until(|| {
                rustix::process::waitid(
                    rustix::process::WaitId::Pid(pid),
                    rustix::process::WaitIdOptions::EXITED
                        | rustix::process::WaitIdOptions::NOWAIT
                        | rustix::process::WaitIdOptions::NOHANG,
                )
                .is_ok_and(|status| status.is_some())
            });
            let stopping = {
                let stop = stop.clone();
                std::thread::spawn(move || stop.stop())
            };
            until(|| stop.stopped());
            drop(lock);
            stopping.join().expect("the stop's thread");
            assert!(
                !copying.join().expect("the copy's thread"),
                "a file copy given up while its tool started was a success"
            );
        }
    }
}

/// A clipboard tool for a test to run instead of the user's.
#[cfg(all(test, unix))]
pub(crate) mod fake {
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    use super::Clipboard;

    /// One directory holding a `pbcopy` and an `xclip` that are the same
    /// script, so a case reads the same whichever system runs it -- and an
    /// `osascript` for a file copy, which keeps the path it was given
    /// ([`Self::copied_file`]).
    pub(crate) struct FakeTool {
        dir: tempfile::TempDir,
    }

    impl FakeTool {
        /// A tool that keeps what it was fed ([`Self::copied`]), or reads it
        /// and exits 1 once [`Self::fail`] has been called.
        pub(crate) fn keeping() -> Self {
            Self::running(concat!(
                "if [ -f \"${0%/*}/fail\" ]; then cat >/dev/null; exit 1; fi\n",
                "cat > \"${0%/*}/copied\"",
            ))
        }

        /// A tool that runs `body` under `/bin/sh`.
        pub(crate) fn running(body: &str) -> Self {
            let dir = tempfile::tempdir().expect("a directory for the fake tools");
            for tool in ["pbcopy", "xclip"] {
                let path = dir.path().join(tool);
                std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("write a fake tool");
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                    .expect("make it executable");
            }
            // The file copy's tool takes the path as its last argument, as
            // upstream's own fixture reads it
            // (`tests/e2e/tui-command-permissions.test.ts:1842-1845`); `exec`
            // so a deadline's kill reaches the `sleep` and leaves nothing.
            let osascript = dir.path().join("osascript");
            std::fs::write(
                &osascript,
                concat!(
                    "#!/bin/sh\n",
                    "if [ -f \"${0%/*}/fail\" ]; then exit 1; fi\n",
                    "if [ -f \"${0%/*}/hang\" ]; then echo $$ > \"${0%/*}/hung\"; exec sleep 30; fi\n",
                    "for arg in \"$@\"; do last=$arg; done\n",
                    "printf '%s' \"$last\" > \"${0%/*}/copied-file\"\n",
                ),
            )
            .expect("write a fake osascript");
            std::fs::set_permissions(&osascript, std::fs::Permissions::from_mode(0o755))
                .expect("make it executable");
            Self { dir }
        }

        /// The clipboard that finds this tool: its directory, then `/bin` for
        /// the `sh` and `cat` it runs -- and not `/usr/bin`, where macOS keeps
        /// the real `pbcopy`.
        ///
        /// A file copy gets a minute rather than five seconds: a deadline is
        /// what a case about the deadline sets ([`Clipboard::waiting`]), and
        /// every other case is about what the copy did, not how fast a loaded
        /// machine ran it. macOS assesses an executable the first time it
        /// runs -- 0.5 to 2.4 s for a fresh script on an idle machine, past
        /// five under the suite's load -- and each fake here is fresh.
        pub(crate) fn clipboard(&self) -> Clipboard {
            Clipboard::searching(format!("{}:/bin", self.dir.path().display()))
                .waiting(std::time::Duration::from_secs(60))
        }

        /// A clipboard holding `image`: an `osascript` that writes it to the
        /// file its script names (`POSIX file "<path>"`) and an `xclip` that
        /// prints it, so a case reads the same on either system. With no
        /// image, both exit 1 -- a clipboard that holds none.
        pub(crate) fn holding(image: Option<&[u8]>) -> Self {
            let dir = tempfile::tempdir().expect("a directory for the fake tools");
            let held = dir.path().join("held");
            if let Some(image) = image {
                std::fs::write(&held, image).expect("the clipboard's image");
            }
            // The path is cut out of the script with the shell's own
            // expansions, so the fake needs nothing but `cat` from `/bin`.
            let osascript = format!(
                "[ -f '{held}' ] || exit 1\n\
                 out=${{2#*POSIX file \\\"}}\n\
                 out=${{out%%\\\"*}}\n\
                 cat '{held}' > \"$out\"\n",
                held = held.display()
            );
            let xclip = format!(
                "[ -f '{held}' ] || exit 1\ncat '{held}'\n",
                held = held.display()
            );
            for (tool, body) in [("osascript", osascript), ("xclip", xclip)] {
                let path = dir.path().join(tool);
                std::fs::write(&path, format!("#!/bin/sh\n{body}")).expect("write a fake tool");
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                    .expect("make it executable");
            }
            Self { dir }
        }

        /// What the tool was last fed, or `None` when it has kept nothing.
        pub(crate) fn copied(&self) -> Option<Vec<u8>> {
            std::fs::read(self.path("copied")).ok()
        }

        /// The path the file copy's tool was last given, or `None` when it
        /// has been given none.
        pub(crate) fn copied_file(&self) -> Option<String> {
            std::fs::read_to_string(self.path("copied-file")).ok()
        }

        /// Makes every copy from now on exit 1.
        pub(crate) fn fail(&self) {
            std::fs::write(self.path("fail"), "").expect("arm the failure");
        }

        /// Makes every file copy from now on wait far past any deadline.
        pub(crate) fn hang(&self) {
            std::fs::write(self.path("hang"), "").expect("arm the wait");
        }

        /// The process id of the last tool that hung -- a file copy armed
        /// with [`Self::hang`], or a tool whose body writes `$$` to `hung`
        /// before it waits -- once it has started.
        pub(crate) fn hung(&self) -> Option<i32> {
            std::fs::read_to_string(self.path("hung"))
                .ok()
                .and_then(|pid| pid.trim().parse().ok())
        }

        /// Whether process `pid` still exists.
        pub(crate) fn alive(pid: i32) -> bool {
            rustix::process::Pid::from_raw(pid)
                .is_some_and(|pid| rustix::process::test_kill_process(pid).is_ok())
        }

        fn path(&self, name: &str) -> PathBuf {
            self.dir.path().join(name)
        }
    }
}
