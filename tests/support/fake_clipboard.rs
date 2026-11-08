//! A clipboard tool for the binary under test to find instead of the user's.
//!
//! `/copy` runs `pbcopy` on macOS and `xclip -selection clipboard` on Linux,
//! found on `PATH`. A suite that typed `/copy` with the inherited `PATH` would
//! write the clipboard of whoever is running it, so a case that copies puts
//! this directory first on the child's `PATH`: both tools are the same script,
//! which keeps what it was fed, or reads it and exits 1 once [`FakeClipboard::fail`]
//! has been called.
//!
//! A case that needs a copy still running when the session ends calls
//! [`FakeClipboard::hold_copies_on`]: from then on the copy tool takes what it
//! is fed and then holds until it is killed, writing a heartbeat ([`BEAT`])
//! into the given terminal while it is alive -- the same stream the binary
//! writes to, so a case reads off the terminal what came while it lived.
//!
//! `/trace` copies a **file** on macOS, through `osascript`, and the directory
//! has one of those too: it keeps the path it was given as its last argument
//! ([`FakeClipboard::copied_file`]), or exits 1 once [`FakeClipboard::fail`]
//! has been called. A case that types `/trace` also gives the child a
//! `TMPDIR` of its own ([`FakeClipboard::reports_into`]), so no report lands
//! in the suite runner's temporary directory.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

/// What a holding copy tool writes into its terminal, one `write(2)` each,
/// about every 10 ms while it is alive ([`FakeClipboard::hold_copies_on`]).
pub const BEAT: &str = "<<BEAT>>";

pub struct FakeClipboard {
    dir: tempfile::TempDir,
}

impl FakeClipboard {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().expect("a directory for the fake clipboard");
        let script = concat!(
            "#!/bin/sh\n",
            "d=${0%/*}\n",
            "if [ -f \"$d/fail\" ]; then cat >/dev/null; exit 1; fi\n",
            // Holding: the input taken, the pid told, then a heartbeat until
            // the tool is killed -- or until its directory goes, so a tool a
            // failed case leaves behind ends by itself.
            "if [ -f \"$d/hold\" ]; then\n",
            "  cat > \"$d/copied\"\n",
            "  t=$(cat \"$d/hold\")\n",
            "  echo $$ > \"$d/held\"\n",
            "  while [ -d \"$d\" ]; do printf '<<BEAT>>' > \"$t\"; sleep 0.01; done\n",
            "  exit 1\n",
            "fi\n",
            "cat > \"$d/copied\"\n",
        );
        for tool in ["pbcopy", "xclip"] {
            let path = dir.path().join(tool);
            std::fs::write(&path, script).expect("write a fake clipboard tool");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                .expect("make it executable");
        }
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
        // Run once now: macOS assesses an executable the first time it runs,
        // which can take seconds -- past the five the binary gives a file copy.
        xfx::process::status(Command::new(&osascript).arg("warm-up"))
            .expect("run the fake osascript once");
        std::fs::remove_file(dir.path().join("copied-file")).expect("forget the warm-up");
        std::fs::create_dir(dir.path().join("reports")).expect("a report directory");
        Self { dir }
    }

    /// Puts the fake tools ahead of everything else on `command`'s `PATH`.
    pub fn first_on_path(&self, command: &mut Command) {
        let inherited = std::env::var_os("PATH").unwrap_or_default();
        let mut path = std::ffi::OsString::from(self.dir.path());
        path.push(":");
        path.push(inherited);
        command.env("PATH", path);
    }

    /// Gives `command` a `TMPDIR` of this fake's own, which is where `/trace`
    /// writes its report ([`Self::reports`]).
    pub fn reports_into(&self, command: &mut Command) {
        command.env("TMPDIR", self.path("reports"));
    }

    /// Every report written into [`Self::reports_into`]'s directory.
    pub fn reports(&self) -> Vec<PathBuf> {
        let mut reports: Vec<PathBuf> = std::fs::read_dir(self.path("reports"))
            .expect("the report directory")
            .map(|entry| entry.expect("an entry").path())
            .collect();
        reports.sort();
        reports
    }

    /// The path the file copy's tool was last given, or `None`.
    pub fn copied_file(&self) -> Option<String> {
        std::fs::read_to_string(self.path("copied-file")).ok()
    }

    /// What the clipboard was last given, or `None` when nothing was copied.
    pub fn copied(&self) -> Option<Vec<u8>> {
        std::fs::read(self.path("copied")).ok()
    }

    /// Makes every file copy from now on wait far past its deadline.
    pub fn hang(&self) {
        std::fs::write(self.path("hang"), "").expect("arm the wait");
    }

    /// The process id of the last file copy that hung, once it has started.
    pub fn hung(&self) -> Option<i32> {
        std::fs::read_to_string(self.path("hung"))
            .ok()
            .and_then(|pid| pid.trim().parse().ok())
    }

    /// Makes every copy from now on take its input and then hold until it is
    /// killed or this fake is dropped, writing [`BEAT`] into `tty` (a pty's
    /// slave, which the binary's own output goes to) while it is alive. Its
    /// process id is [`Self::holding`] once it holds.
    pub fn hold_copies_on(&self, tty: &Path) {
        std::fs::write(self.path("hold"), tty.as_os_str().as_encoded_bytes())
            .expect("arm the hold");
    }

    /// The process id of the copy tool that holds, once it does.
    pub fn holding(&self) -> Option<i32> {
        std::fs::read_to_string(self.path("held"))
            .ok()
            .and_then(|pid| pid.trim().parse().ok())
    }

    /// Makes every copy from now on exit 1.
    pub fn fail(&self) {
        std::fs::write(self.path("fail"), "").expect("arm the failure");
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }
}
