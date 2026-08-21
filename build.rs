//! Build metadata: the exact source revision this binary was compiled from and
//! the compile profile it was built with.
//!
//! `status` reports both so that a receipt can be tied back to a commit. Neither
//! value is invented: when the revision cannot be determined the variable is
//! empty and the snapshot omits the field rather than printing a placeholder.

use std::process::Command;

/// The width upstream uses for an abbreviated revision
/// (`vercel-labs/fx@580a0c5d tests/e2e/cli.test.ts:729`).
const REVISION_WIDTH: usize = 12;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=FXR_BUILD_REVISION");

    let revision = env_revision().or_else(git_revision).unwrap_or_default();
    println!("cargo:rustc-env=FXR_BUILD_REVISION={revision}");

    // Cargo sets PROFILE to the compile profile actually in use. fxr has no
    // updater, so this is a build fact and not a release channel promise.
    let channel = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    println!("cargo:rustc-env=FXR_BUILD_CHANNEL={channel}");

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
    normalize(std::env::var("FXR_BUILD_REVISION").ok()?)
}

fn git_revision() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    normalize(String::from_utf8(output.stdout).ok()?)
}

/// Accepts only a lowercase hex revision and abbreviates it to the fixed width.
/// Anything else is discarded, because a malformed revision in a receipt is
/// worse than an absent one.
fn normalize(raw: String) -> Option<String> {
    let trimmed = raw.trim().to_ascii_lowercase();
    if trimmed.len() < REVISION_WIDTH || !trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some(trimmed[..REVISION_WIDTH].to_string())
}
