//! A clipboard tool for the binary under test to find instead of the user's.
//!
//! `/copy` runs `pbcopy` on macOS and `xclip -selection clipboard` on Linux,
//! found on `PATH`. A suite that typed `/copy` with the inherited `PATH` would
//! write the clipboard of whoever is running it, so a case that copies puts
//! this directory first on the child's `PATH`: both tools are the same script,
//! which keeps what it was fed, or reads it and exits 1 once [`FakeClipboard::fail`]
//! has been called.

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

pub struct FakeClipboard {
    dir: tempfile::TempDir,
}

impl FakeClipboard {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().expect("a directory for the fake clipboard");
        let script = concat!(
            "#!/bin/sh\n",
            "if [ -f \"${0%/*}/fail\" ]; then cat >/dev/null; exit 1; fi\n",
            "cat > \"${0%/*}/copied\"\n",
        );
        for tool in ["pbcopy", "xclip"] {
            let path = dir.path().join(tool);
            std::fs::write(&path, script).expect("write a fake clipboard tool");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                .expect("make it executable");
        }
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

    /// What the clipboard was last given, or `None` when nothing was copied.
    pub fn copied(&self) -> Option<Vec<u8>> {
        std::fs::read(self.path("copied")).ok()
    }

    /// Makes every copy from now on exit 1.
    pub fn fail(&self) {
        std::fs::write(self.path("fail"), "").expect("arm the failure");
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }
}
