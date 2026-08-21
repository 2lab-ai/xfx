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
    let head = std::path::Path::new(".git/HEAD");
    if !head.exists() {
        return;
    }
    println!("cargo:rerun-if-changed=.git/HEAD");

    let Ok(contents) = std::fs::read_to_string(head) else {
        return;
    };
    if let Some(reference) = contents.trim().strip_prefix("ref: ") {
        // A packed ref has no file of its own; watching it is then a no-op.
        let path = format!(".git/{reference}");
        if std::path::Path::new(&path).exists() {
            println!("cargo:rerun-if-changed={path}");
        }
    }
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
