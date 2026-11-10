//! Build metadata: the channel this binary claims, and its id -- the UTC
//! minute it was named and the exact source revision it was compiled from,
//! `YYYY-MM-DD-HHMM-<12 hex>`.
//!
//! Every version line says both (`xfx 0.2.0 (preview 2026-11-10-0019-
//! ec40c7ad2e50)`), so a receipt can be tied back to a commit and a build says
//! how old it is. Neither value is invented: when the id cannot be determined
//! the variable is empty and the line says the channel alone rather than a
//! placeholder, and a channel or an id that was given and is not one fails the
//! build here rather than becoming a false claim in a published binary.
//!
//! The rules themselves live in `src/build_meta.rs` and are included rather
//! than imported, because a build script cannot depend on the crate it builds.
//! That file is where their tests are.
//!
//! The library starts every child under one lock (`src/process.rs`,
//! `clippy.toml`); a build script cannot reach it, and needs none: it is a
//! process of its own that starts one `git` at a time from its one thread.
#![allow(
    clippy::disallowed_methods,
    reason = "a build script is its own single-threaded process and cannot reach the library's start lock"
)]

use std::process::Command;

include!("src/build_meta.rs");

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/build_meta.rs");
    println!("cargo:rerun-if-env-changed=XFX_BUILD_ID");
    println!("cargo:rerun-if-env-changed=XFX_BUILD_CHANNEL");

    // A publishing workflow names the build after its own run; any other
    // build is named after the commit it was built from.
    let explicit = std::env::var("XFX_BUILD_ID").ok();
    let id = match resolve_build_id(explicit.as_deref()) {
        Ok(Some(id)) => id,
        Ok(None) => git_build_id().unwrap_or_default(),
        Err(message) => panic!("{message}"),
    };
    println!("cargo:rustc-env=XFX_BUILD_ID={id}");

    // Cargo sets PROFILE to the compile profile actually in use, which is the
    // channel unless the build that runs this said otherwise. A publishing
    // workflow says otherwise: `preview` compiles with the release profile but
    // is not a release.
    let explicit = std::env::var("XFX_BUILD_CHANNEL").ok();
    let profile = std::env::var("PROFILE").ok();
    let channel = match resolve_channel(explicit.as_deref(), profile.as_deref()) {
        Ok(channel) => channel,
        Err(message) => panic!("{message}"),
    };
    println!("cargo:rustc-env=XFX_BUILD_CHANNEL={channel}");

    watch_git_head();
}

/// Watches the files that change when the checked-out commit changes.
///
/// `.git/HEAD` alone is not enough: committing on the current branch leaves HEAD
/// byte-identical and only moves the branch ref, so watching only HEAD would let
/// `status` report a stale revision until an unrelated rebuild. A packaged source
/// tree has no `.git` at all, so both paths are conditional.
fn watch_git_head() {
    // Where HEAD and the refs really live: `.git` itself in a plain checkout,
    // and in a linked worktree a file pointing at the worktree's own git
    // directory (HEAD) and the main one (refs). Asked of git rather than
    // assumed, so a worktree build's revision follows its commits too.
    let Some(git_dir) = git_path(&["rev-parse", "--git-dir"]) else {
        return;
    };
    let common = git_path(&["rev-parse", "--git-common-dir"]).unwrap_or_else(|| git_dir.clone());
    let head = git_dir.join("HEAD");
    if !head.exists() {
        return;
    }
    println!("cargo:rerun-if-changed={}", head.display());

    let Ok(contents) = std::fs::read_to_string(&head) else {
        return;
    };
    if let Some(reference) = contents.trim().strip_prefix("ref: ") {
        // A packed ref has no file of its own, and the next commit on the
        // branch writes one; watching nothing until then left the revision
        // stale. So the loose file if it is there, else the nearest directory
        // above it that is (Cargo rescans a watched directory, so the file
        // appearing is a change), and the packed list either way. A path that
        // does not exist is never named: Cargo would rerun on every build.
        // Never above `refs/`: a watched git directory would be rescanned,
        // objects and all, on every build.
        let refs = common.join("refs");
        let mut watched = common.join(reference);
        while !watched.exists() && watched.pop() && watched.starts_with(&refs) {}
        if watched.exists() && watched.starts_with(&refs) {
            println!("cargo:rerun-if-changed={}", watched.display());
        }
        let packed = common.join("packed-refs");
        if packed.exists() {
            println!("cargo:rerun-if-changed={}", packed.display());
        }
    }
}

/// `git`, with every `GIT_*` this build inherited taken out of its
/// environment.
///
/// Git takes the repository it reads from `GIT_DIR`, `GIT_WORK_TREE`,
/// `GIT_COMMON_DIR` and the rest, and writes a trace wherever `GIT_TRACE`
/// says. An exported one stamped another repository's HEAD into the binary,
/// and wrote the trace file. The revision is this checkout's, so git is asked
/// about this checkout only. scripts/test-gate-env.sh holds it to that.
fn git() -> Command {
    let mut command = Command::new("git");
    for (name, _) in std::env::vars_os() {
        if name.as_encoded_bytes().starts_with(b"GIT_") {
            command.env_remove(name);
        }
    }
    command
}

/// A path git prints for `args`, or `None` outside a repository.
fn git_path(args: &[&str]) -> Option<std::path::PathBuf> {
    let output = git().args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    Some(std::path::PathBuf::from(text.trim()))
}

/// HEAD's id: the UTC minute it was committed, then its revision.
///
/// The committer time rather than the author time, because a rebase or a
/// cherry-pick makes a new commit that keeps the old one's author time: the
/// minute a revision came to be is the minute it was committed.
/// `--no-show-signature` because a `log.showSignature` in the builder's
/// configuration would put a verification report in front of the two values,
/// and anything that is not exactly them names no build.
fn git_build_id() -> Option<String> {
    let output = git()
        .args([
            "show",
            "-s",
            "--no-show-signature",
            "--format=%H %ct",
            "HEAD",
            "--",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    let (revision, committed) = text.trim().split_once(' ')?;
    build_id_of(committed.parse().ok()?, revision)
}
