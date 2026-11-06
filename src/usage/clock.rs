//! The wall clock `/usage` reads: which UTC day is "today", how long ago a
//! cached part was fetched, when the floor and the timer allow a fetch.
//!
//! fx reads its usage clock from a file named by `FX_E2E_USAGE_CLOCK_FILE`
//! when that variable is set, so its e2e driver can pin and move the clock
//! (`vercel-labs/fx@481104fd src/core/app/usage_host.zig:20-22, :205-219`); a
//! usage cache seeded for one UTC day then reads the same on any day. xfx's
//! equivalent is `XFX_E2E_USAGE_CLOCK_FILE`, read the same way -- an absolute
//! path, at most 32 bytes, an integer in milliseconds since the epoch with
//! blanks around it, and the system clock for anything else.
//!
//! **Only a build with debug assertions reads it.** fx honours its variable
//! in every build; xfx compiles the reader, and the variable's name, only
//! under `cfg(debug_assertions)`: `cargo build` and `cargo test`, the builds
//! the capture harness (`scripts/fxcap/capture.sh`'s `clock` verb) and the CI
//! sweep run. `cargo build --release`, which every published binary is
//! (`[profile.release]` leaves debug assertions off), has neither, so no
//! environment can move a shipped binary's clock. The test
//! `the_pinned_clock_is_read_only_with_debug_assertions` holds both halves,
//! and `scripts/check-usage-clock.sh` (a CI step) runs it under each profile;
//! `scripts/smoke.sh`, which CI, preview and release run on the binary they
//! ship, refuses a binary whose bytes carry the variable's name.
//!
//! **For US-U3 and US-U4.** Nothing reads this clock yet. Every "now" in the
//! dashboard runtime (US-U3: the open floor, the 15-minute timer, the minute
//! redraw, a fetch's stamp) *and* the screen (US-U4: today's UTC day,
//! `updated Nm ago`, the empty-period texts) must come from [`now_ms`], never
//! from `SystemTime` or `system_now_ms` directly, or a capture that pins the
//! clock pins half a dashboard. And the arithmetic on it must hold for any
//! `i64` it returns: the pinned clock returns whatever integer the file holds,
//! `i64::MIN` and `i64::MAX` included, and `system_now_ms` returns a negative
//! time for a clock set before 1970 and `i64::MAX` past its range. So a
//! difference saturates or is checked, never a bare `-` that overflows; a UTC
//! day is `div_euclid`, which floors a negative time; and no difference is
//! cast `as u64`. fx's own rule is the model: a clock that reads earlier than
//! a stamp was corrected backward, and counts as time passed
//! (`since`, `src/usage/policy.zig:31-35@481104fd`).

#[cfg(debug_assertions)]
use std::ffi::OsString;

/// The variable naming the file that pins the usage clock, in a build with
/// debug assertions only. Test-only, like fx's `FX_E2E_USAGE_CLOCK_FILE`.
#[cfg(debug_assertions)]
pub const CLOCK_FILE_ENV: &str = "XFX_E2E_USAGE_CLOCK_FILE";

/// The most of the clock file that is read (`readClockFile`'s buffer,
/// `usage_host.zig:212`).
#[cfg(debug_assertions)]
const MAX_CLOCK_BYTES: u64 = 32;

/// Milliseconds since the Unix epoch, as `/usage` should see them: the
/// pinned clock in a build with debug assertions whose environment names
/// one, the system clock otherwise.
pub fn now_ms() -> i64 {
    now_ms_with(|name| std::env::var_os(name))
}

/// [`now_ms`] with the environment as a lookup, so a test can name a file
/// without changing the process's environment under other threads.
#[cfg_attr(not(debug_assertions), allow(unused_variables))]
fn now_ms_with(var: impl Fn(&str) -> Option<std::ffi::OsString>) -> i64 {
    #[cfg(debug_assertions)]
    if let Some(ms) = var(CLOCK_FILE_ENV).and_then(pinned) {
        return ms;
    }
    crate::session::event::system_now_ms()
}

/// The time the clock file holds, or `None` for a path that is not
/// absolute, anything that is not a regular file, a file that cannot be read,
/// or one that is not one integer (`readClockFile`, `usage_host.zig:212-219`).
///
/// Opened `O_NONBLOCK`: a plain `open(2)` of a FIFO with no writer blocks
/// until one comes -- and so would the thread asking the time. And
/// `O_NOCTTY`: the open happens before the type check, and opening a terminal
/// (or a link to one) without it makes that terminal the controlling one of a
/// session leader that has none, on Linux; std does not add it. The opened
/// descriptor's own type then turns away a FIFO, a terminal or any other
/// device, so the check and the read are of the same file.
#[cfg(debug_assertions)]
fn pinned(path: OsString) -> Option<i64> {
    use std::io::Read;
    let path = std::path::PathBuf::from(path);
    if !path.is_absolute() {
        return None;
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK | libc::O_NOCTTY);
    }
    let file = options.open(&path).ok()?;
    if !file.metadata().ok()?.is_file() {
        return None;
    }
    let mut text = String::new();
    file.take(MAX_CLOCK_BYTES).read_to_string(&mut text).ok()?;
    text.trim_matches([' ', '\t', '\r', '\n']).parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::path::Path;

    /// fx's own e2e time, 2026-10-12 15:30 UTC (`tests/e2e/usage-fakes.ts:10`).
    const PINNED: i64 = 1_791_819_000_000;
    /// The variable's name, spelled here so the release half can name it too.
    const NAME: &str = "XFX_E2E_USAGE_CLOCK_FILE";

    fn naming(path: &Path) -> impl Fn(&str) -> Option<OsString> + '_ {
        move |name| (name == NAME).then(|| path.as_os_str().to_owned())
    }

    fn near_system(ms: i64) -> bool {
        ms.saturating_sub(crate::session::event::system_now_ms())
            .saturating_abs()
            < 60_000
    }

    #[test]
    fn the_pinned_clock_is_read_only_with_debug_assertions() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("clock");
        std::fs::write(&file, format!("{PINNED}\n")).unwrap();
        let read = now_ms_with(naming(&file));
        if cfg!(debug_assertions) {
            #[cfg(debug_assertions)]
            assert_eq!(CLOCK_FILE_ENV, NAME);
            assert_eq!(read, PINNED, "a debug build reads the pinned clock");
            // Moved: each read reads the file again, blanks around it.
            std::fs::write(&file, " 1791905400000\t\r\n").unwrap();
            assert_eq!(now_ms_with(naming(&file)), 1_791_905_400_000);
            // Only the first 32 bytes are read, as fx reads them.
            std::fs::write(&file, format!("{PINNED:>32}junk past the read\n")).unwrap();
            assert_eq!(now_ms_with(naming(&file)), PINNED);
            // The same file named by a relative path is not read.
            let cwd = std::env::current_dir().unwrap();
            let up = "../".repeat(cwd.components().count() - 1);
            let relative = Path::new(&up).join(file.strip_prefix("/").unwrap());
            assert!(relative.is_relative() && relative.is_file(), "{relative:?}");
            assert!(near_system(now_ms_with(naming(&relative))), "relative");
            println!("usage clock: honoured (debug assertions)");
        } else {
            assert!(
                near_system(read),
                "a release build ignores {NAME}: read {read}"
            );
            println!("usage clock: ignored (release)");
        }
    }

    #[test]
    fn anything_but_an_absolute_file_of_one_integer_is_the_system_clock() {
        let dir = tempfile::tempdir().unwrap();
        let cases: [(&str, &[u8]); 6] = [
            ("empty", b""),
            ("words", b"noon\n"),
            ("two", b"1791819000000 1\n"),
            ("float", b"1791819000000.5\n"),
            ("cut", b"1791819000000 and more text past the 32 bytes\n"),
            ("not-utf8", b"\xff1791819000000\n"),
        ];
        for (name, bytes) in cases {
            let file = dir.path().join(name);
            std::fs::write(&file, bytes).unwrap();
            assert!(near_system(now_ms_with(naming(&file))), "{name}");
        }
        let missing = dir.path().join("missing");
        assert!(near_system(now_ms_with(naming(&missing))), "missing");
        assert!(near_system(now_ms_with(naming(dir.path()))), "a directory");
        assert!(near_system(now_ms_with(|_| None)), "unset");
        assert!(near_system(now_ms()), "the process's own environment");
    }

    /// The pinned clock hands back any `i64` as it is, the extremes and a
    /// time before 1970 included: what US-U3 and US-U4 must be able to take.
    #[cfg(debug_assertions)]
    #[test]
    fn the_pinned_clock_returns_any_i64_as_it_is() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("clock");
        for ms in [i64::MIN, -1, 0, i64::MAX] {
            std::fs::write(&file, format!("{ms}\n")).unwrap();
            assert_eq!(now_ms_with(naming(&file)), ms);
        }
    }

    /// A FIFO with no writer, a pipe holding a time, a device: none is a
    /// regular file, so each reads as the system clock, and none blocks. The
    /// FIFO is read on a thread with a deadline, so a blocking open fails the
    /// test instead of hanging it.
    #[cfg(unix)]
    #[test]
    fn a_fifo_a_pipe_or_a_device_is_the_system_clock_and_never_blocks() {
        use std::io::Write;
        use std::os::fd::AsRawFd;
        use std::os::unix::ffi::OsStrExt;
        use std::os::unix::fs::OpenOptionsExt;
        use std::time::Duration;

        let dir = tempfile::tempdir().unwrap();
        let fifo = dir.path().join("fifo");
        let name = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
        // SAFETY: `name` is a NUL-terminated path that outlives the call.
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        let (sent, read) = std::sync::mpsc::channel();
        let path = fifo.clone();
        std::thread::spawn(move || {
            let _ = sent.send(now_ms_with(naming(&path)));
        });
        match read.recv_timeout(Duration::from_secs(10)) {
            Ok(ms) => assert!(near_system(ms), "a FIFO with no writer: {ms}"),
            Err(why) => {
                // Free a reader stuck in open(2): opening the write end
                // completes its open, so the thread ends rather than outliving
                // the test. Non-blocking, so this cannot hang either: with no
                // reader left (the thread died without sending) it fails at
                // once with ENXIO, and there is nothing to free.
                let _ = std::fs::OpenOptions::new()
                    .write(true)
                    .custom_flags(libc::O_NONBLOCK)
                    .open(&fifo);
                panic!(
                    "reading the clock from a FIFO with no writer did not answer within 10 s ({why:?})"
                );
            }
        }

        // A pipe whose writer wrote a time and closed: readable to its end,
        // so only the descriptor's type turns it away.
        let (pipe, mut writer) = std::io::pipe().unwrap();
        writer.write_all(format!("{PINNED}\n").as_bytes()).unwrap();
        drop(writer);
        let by_fd = std::path::PathBuf::from(format!("/dev/fd/{}", pipe.as_raw_fd()));
        assert!(
            near_system(now_ms_with(naming(&by_fd))),
            "a pipe holding a time"
        );
        drop(pipe);

        assert!(
            near_system(now_ms_with(naming(Path::new("/dev/null")))),
            "a device"
        );
    }
}
