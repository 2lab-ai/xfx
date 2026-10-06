//! Build metadata: the exact source revision this binary was compiled from and
//! the channel it claims.
//!
//! `status` reports both so that a receipt can be tied back to a commit. Neither
//! value is invented: when the revision cannot be determined the variable is
//! empty and the snapshot omits the field rather than printing a placeholder,
//! and a channel that was asked for and is not a channel fails the build here
//! rather than becoming a false claim in a published binary.
//!
//! The rules themselves live in `src/build_meta.rs` and are included rather
//! than imported, because a build script cannot depend on the crate it builds.
//! That file is where their tests are.

use std::process::Command;

include!("src/build_meta.rs");

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/build_meta.rs");
    println!("cargo:rerun-if-env-changed=XFX_BUILD_REVISION");
    println!("cargo:rerun-if-env-changed=XFX_BUILD_CHANNEL");

    let revision = env_revision().or_else(git_revision).unwrap_or_default();
    println!("cargo:rustc-env=XFX_BUILD_REVISION={revision}");

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

/// A path git prints for `args`, or `None` outside a repository.
fn git_path(args: &[&str]) -> Option<std::path::PathBuf> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    Some(std::path::PathBuf::from(text.trim()))
}

fn env_revision() -> Option<String> {
    normalize_revision(&std::env::var("XFX_BUILD_REVISION").ok()?)
}

fn git_revision() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    normalize_revision(&String::from_utf8(output.stdout).ok()?)
}
