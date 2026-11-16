//! Every integration test binary runs its cases in a copy of itself whose
//! environment is built from nothing.
//!
//! An integration test links the library as a build does: there
//! `process_env!` is `std::env` itself (`src/lib.rs`), so a command plan
//! would read the developer's `HOME` and ask the developer's git which hooks
//! its global configuration sets, and a provider would take the developer's
//! `XFX_GATEWAY_URL`. So before `main`, [`ISOLATE`] replaces the process the
//! developer started (`exec`, the same process id, arguments and streams)
//! with a copy of the same binary whose environment is `PATH` and `TMPDIR`
//! ([`super::sandbox::clear_env`]), an empty `HOME` made for it, cargo's
//! `CARGO_PKG_NAME`, `RUST_BACKTRACE` and `RUST_TEST_THREADS` where they are
//! set, and [`MARKER`], which names that `HOME`.
//!
//! The copy does not trust the marker. Before any case runs it confirms its
//! environment holds those names and no other, and that `HOME` is, byte for
//! byte, the path its parent made for that marker -- `TMPDIR` (else `/tmp`)
//! joined with [`HOME_PREFIX`] and the marker, no trailing `/` -- and there
//! an empty directory, not a link, of its own and private to it; anything
//! else and it exits at once, 101 as a failed suite does, so no case runs.
//! It removes that `HOME` when it exits. What it prints names what it
//! refused, never a value of the environment.
//!
//! Not handed on: any variable a tool sets for the test process --
//! `LLVM_PROFILE_FILE` under a coverage run, `NEXTEST_*` under
//! cargo-nextest. Under such a tool this binary's cases run without them.
//!
//! The boundary: the copy confirms what its parent made, against a mistake
//! or an inherited marker, not against someone who can already write as this
//! user -- such a process can replace `TMPDIR`'s directories between the
//! check and the cases, or the test binary itself.
//!
//! Every `tests/*.rs` declares `mod support;`, which compiles this in.
//! `scripts/check-env-reads.py` starts every test binary cargo builds with a
//! marker no parent makes, and refuses one that does not refuse itself.

use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{DirBuilderExt, MetadataExt};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::sync::OnceLock;

/// The variable that tells the copy what its `HOME` is called.
pub const MARKER: &str = "XFX_TEST_ISOLATED_COPY";

/// Every name the copy's environment may hold.
const ALLOWED: [&str; 7] = [
    "PATH",
    "TMPDIR",
    "HOME",
    "CARGO_PKG_NAME",
    "RUST_BACKTRACE",
    "RUST_TEST_THREADS",
    MARKER,
];

/// What the copy's `HOME` is called: this, then the marker.
pub const HOME_PREFIX: &str = "xfx-test-home-";

/// The `HOME` the copy confirmed, removed when it exits.
static CONFIRMED_HOME: OnceLock<PathBuf> = OnceLock::new();

/// Run before `main` by the loader, as a C library's constructors are.
#[used]
#[cfg_attr(target_os = "linux", link_section = ".init_array")]
#[cfg_attr(target_vendor = "apple", link_section = "__DATA,__mod_init_func")]
static ISOLATE: extern "C" fn() = isolate;

#[cfg(not(any(target_os = "linux", target_vendor = "apple")))]
compile_error!(
    "tests/support/isolated.rs runs a test binary in a clean copy on Linux and macOS only"
);

extern "C" fn isolate() {
    let refusal = match xfx::env_exception!(var_os("XFX_TEST_ISOLATED_COPY")) {
        Some(marker) => confirm(&marker).err(),
        None => Some(become_the_copy()),
    };
    if let Some(refusal) = refusal {
        // Names and paths the copy made, never a value of the environment.
        eprintln!("tests/support/isolated.rs: no case runs: {refusal}");
        std::process::exit(101);
    }
}

/// The `HOME` a parent makes for `marker`, and the only one a copy accepts:
/// an absolute path, so the copy's exit removes the directory it confirmed
/// whatever directory a case has moved to.
fn home_for(marker: &str) -> Result<PathBuf, String> {
    let temporary = xfx::env_exception!(var_os("TMPDIR"))
        .filter(|dir| !dir.is_empty())
        .map_or_else(|| PathBuf::from("/tmp"), PathBuf::from);
    if !temporary.is_absolute() {
        return Err("TMPDIR is not an absolute path".to_string());
    }
    Ok(temporary.join(format!("{HOME_PREFIX}{marker}")))
}

/// Replaces this process with the copy. Returns only what went wrong, in
/// words that carry no value of the environment.
fn become_the_copy() -> String {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    if std::env::args_os().next().is_none() {
        return "the process's arguments are not readable yet".to_string();
    }
    let nonce = match nonce() {
        Ok(nonce) => nonce,
        Err(err) => return format!("no random name for the copy's home: {}", err.kind()),
    };
    // Everything that can fail before the home exists, first: past here,
    // only the exec can, and it removes the home.
    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(err) => return format!("cannot find this test binary: {}", err.kind()),
    };
    let home = match home_for(&nonce) {
        Ok(home) => home,
        Err(refusal) => return refusal,
    };
    if let Err(err) = std::fs::DirBuilder::new().mode(0o700).create(&home) {
        return format!("cannot make the copy's home under TMPDIR: {}", err.kind());
    }
    let mut copy = super::command(exe);
    super::sandbox::clear_env(&mut copy);
    copy.args(args)
        .env("HOME", &home)
        .env("CARGO_PKG_NAME", env!("CARGO_PKG_NAME"))
        .env(MARKER, &nonce);
    if let Some(value) = xfx::env_exception!(var_os("RUST_BACKTRACE")) {
        copy.env("RUST_BACKTRACE", value);
    }
    if let Some(value) = xfx::env_exception!(var_os("RUST_TEST_THREADS")) {
        copy.env("RUST_TEST_THREADS", value);
    }
    #[allow(
        clippy::disallowed_methods,
        reason = "the one exec: this process becomes its clean copy, before main"
    )]
    let err = copy.exec();
    let _ = std::fs::remove_dir(&home);
    format!("cannot start the copy: {}", err.kind())
}

/// The copy's own invariants, before any case runs.
fn confirm(marker: &OsStr) -> Result<(), String> {
    let unexpected: Vec<OsString> = xfx::env_exception!(names())
        .filter(|name| !ALLOWED.iter().any(|allowed| OsStr::new(allowed) == name))
        .collect();
    if !unexpected.is_empty() {
        return Err(format!(
            "the environment holds {} name(s) the copy is not given: {unexpected:?}",
            unexpected.len()
        ));
    }
    let marker = marker
        .to_str()
        .filter(|marker| marker.len() == 32 && marker.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or("the marker is not a name the copy's parent made")?;
    let home = PathBuf::from(xfx::env_exception!(var_os("HOME")).ok_or("no HOME")?);
    // A trailing `/` makes `lstat` follow a link, and `remove_dir_all` then
    // empty what it points at.
    if home.as_os_str().as_bytes().ends_with(b"/") {
        return Err("HOME ends in /, which would make a link read as a directory".to_string());
    }
    if home.as_os_str() != home_for(marker)?.as_os_str() {
        return Err("HOME is not the path the parent makes for the marker".to_string());
    }
    let meta = std::fs::symlink_metadata(&home)
        .map_err(|err| format!("HOME cannot be read: {}", err.kind()))?;
    // SAFETY: `geteuid` reads the process's effective user and cannot fail.
    let me = unsafe { libc::geteuid() };
    if !meta.is_dir() || meta.uid() != me || meta.mode() & 0o777 != 0o700 {
        return Err("HOME is not a directory of this user's, private to it".to_string());
    }
    let mut entries =
        std::fs::read_dir(&home).map_err(|err| format!("HOME cannot be listed: {}", err.kind()))?;
    if entries.next().is_some() {
        return Err("HOME is not empty, so it is not the one made for the copy".to_string());
    }
    let _ = CONFIRMED_HOME.set(home);
    // SAFETY: `atexit` registers a function with no arguments; it runs once,
    // when the process exits.
    unsafe { libc::atexit(remove_home) };
    Ok(())
}

extern "C" fn remove_home() {
    if let Some(home) = CONFIRMED_HOME.get() {
        let _ = std::fs::remove_dir_all(home);
    }
}

/// Sixteen random bytes, as hex.
fn nonce() -> std::io::Result<String> {
    use std::io::Read as _;
    let mut bytes = [0u8; 16];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
