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
//! and `scripts/check-usage-clock.sh` (a CI step) runs it under each profile.
//!
//! Nothing reads this clock yet: the `/usage` dashboard (P5 US-U3, US-U4)
//! takes every "now" from [`now_ms`], so a capture that pins it pins the
//! dashboard.

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
/// absolute, a file that cannot be read, or one that is not one integer
/// (`readClockFile`, `usage_host.zig:212-219`).
#[cfg(debug_assertions)]
fn pinned(path: OsString) -> Option<i64> {
    use std::io::Read;
    let path = std::path::PathBuf::from(path);
    if !path.is_absolute() {
        return None;
    }
    let mut text = String::new();
    std::fs::File::open(&path)
        .ok()?
        .take(MAX_CLOCK_BYTES)
        .read_to_string(&mut text)
        .ok()?;
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
        (ms - crate::session::event::system_now_ms()).abs() < 60_000
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
}
