//! fxr — an unofficial Rust port of the `fx` terminal coding agent.
//!
//! The crate is organized as CLI -> application services -> domain contracts ->
//! adapters. Each module owns one concern:
//!
//! - [`cli`]: the closed command grammar and help metadata
//! - [`config`]: settings discovery, precedence, credentials, and diagnostics
//! - [`output`]: immutable snapshots and the text/JSON/JSONL renderers
//! - [`gateway`]: the provider contract, the Gateway wire shape, and bounded SSE
//! - [`workspace`]: the roots a turn may read, canonical path proofs, and the
//!   bounded project instructions a turn carries
//! - [`permission`]: modes, policies, and one-use execution authorities
//! - [`tools`]: the closed registry, its schemas, and its executors
//! - [`agent`]: the bounded turn state machine and its exactly-once finalizer
//! - [`session`]: the durable event log, its published boundary, and resume
//! - [`app`]: composition and dispatch
//!
//! fxr is not affiliated with Vercel. It is a behavioral port pinned to
//! `vercel-labs/fx@580a0c5da9386317251968c09c1cee69e763487a`; see `UPSTREAM.md`
//! for the attribution and `docs/parity.md` for what is and is not implemented.

pub mod agent;
pub mod app;
pub mod cli;
pub mod config;
pub mod gateway;
pub mod output;
pub mod permission;
pub mod session;
pub mod tools;
pub mod workspace;

/// The product version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Facts about how this binary was built.
///
/// These are recorded at compile time by `build.rs` so a receipt can be tied back
/// to an exact commit. An unknown revision is absent, never a placeholder value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildInfo {
    /// The compile profile, `debug` or `release`. fxr has no updater, so this is
    /// a build fact and not a release-channel promise.
    pub channel: &'static str,
    /// The abbreviated source revision, when it could be determined.
    pub revision: Option<&'static str>,
}

/// Reads the compile-time build metadata.
pub fn build_info() -> BuildInfo {
    let revision = env!("FXR_BUILD_REVISION");
    BuildInfo {
        channel: env!("FXR_BUILD_CHANNEL"),
        revision: (!revision.is_empty()).then_some(revision),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_build_channel_is_a_real_compile_profile() {
        assert!(
            matches!(build_info().channel, "debug" | "release"),
            "got {}",
            build_info().channel
        );
    }

    #[test]
    fn a_reported_revision_is_lowercase_hex_of_fixed_width() {
        if let Some(revision) = build_info().revision {
            assert_eq!(revision.len(), 12, "got {revision}");
            assert!(
                revision
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
                "got {revision}"
            );
        }
    }

    #[test]
    fn the_version_is_the_package_version() {
        assert_eq!(VERSION, env!("CARGO_PKG_VERSION"));
    }
}
