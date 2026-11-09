//! A case that runs xfx's library in its own process -- a command plan, the
//! terminal tool, a provider -- runs again in a copy of its test binary whose
//! environment is built from nothing.
//!
//! An integration test links the library as a build does: there
//! `process_env!` is `std::env` itself (`src/lib.rs`), so a plan reads the
//! developer's `HOME`, asks the developer's git which hooks its global
//! configuration sets, and hands that `HOME` to the command it runs; a
//! provider takes the developer's `XFX_GATEWAY_URL`. Such a case calls
//! [`ran_in_a_clean_copy`] first. In the test binary the developer started,
//! that runs the case again in a copy started with `PATH`, `TMPDIR`
//! ([`super::sandbox::clear_env`]), an empty temporary `HOME` and cargo's
//! `CARGO_PKG_NAME` -- nothing else -- and passes or fails as the copy does;
//! in the copy, the case runs.
//!
//! `tests/support/` cases that only start the `xfx` binary need none of this:
//! that binary's environment is `clear_env`'s already.

use std::process::Command;

/// Set only in the copy [`ran_in_a_clean_copy`] starts.
const COPY: &str = "XFX_TEST_ISOLATED_COPY";

/// Whether this process is that copy.
fn in_the_copy() -> bool {
    xfx::env_exception!(var_os("XFX_TEST_ISOLATED_COPY")).is_some()
}

/// Runs `case` -- the calling test's name, as `--exact` takes it -- in a
/// clean copy of this test binary, unless this process is that copy.
///
/// `true` in the binary the developer started: the case ran in the copy and
/// passed there (it fails here otherwise), so the caller returns. `false` in
/// the copy: the caller goes on with its body. A name that matches no case
/// fails, because the copy then runs none.
pub fn ran_in_a_clean_copy(case: &str) -> bool {
    if in_the_copy() {
        return false;
    }
    let home = tempfile::TempDir::new().expect("an empty home");
    let mut copy = Command::new(std::env::current_exe().expect("this test binary"));
    super::sandbox::clear_env(&mut copy);
    copy.env("HOME", home.path())
        // Cargo sets it in every test process; a case says a command does not
        // inherit it.
        .env("CARGO_PKG_NAME", env!("CARGO_PKG_NAME"))
        .env(COPY, "1")
        .args(["--exact", "--test-threads=1", case]);
    let output = xfx::process::output(&mut copy).expect("a copy of this test binary");
    let report = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    // Counted, not only exited-zero: a name that matched nothing passes.
    assert!(
        output.status.success() && report.contains("test result: ok. 1 passed"),
        "`{case}` failed in its clean copy:\n{report}"
    );
    true
}
