//! xfx — an unofficial Rust port of the `fx` terminal coding agent.
//!
//! The crate is organized as CLI -> application services -> domain contracts ->
//! adapters. Each module owns one concern:
//!
//! - [`cli`]: the closed command grammar and help metadata
//! - [`config`]: settings discovery, precedence, credentials, and diagnostics
//! - [`credits`]: what the Gateway account has left, for `/credits`
//! - [`output`]: immutable snapshots and the text/JSON/JSONL renderers
//! - [`gateway`]: the provider contract, the Gateway wire shape, and bounded SSE
//! - [`image`]: an attached image -- its format, its bytes, and the path rule
//! - [`workspace`]: the roots a turn may read, canonical path proofs, and the
//!   bounded project instructions a turn carries
//! - [`permission`]: modes, policies, and one-use execution authorities
//! - [`provider`]: provider identity, the selected bundle, and model catalogs
//! - [`profile_home`]: how `~/.xfx` is made private, and the rule an existing
//!   one is held to
//! - [`tools`]: the closed registry, its schemas, and its executors
//! - [`agent`]: the bounded turn state machine and its exactly-once finalizer
//! - [`session`]: the durable event log, its published boundary, and resume
//! - [`usage`]: what the provider says the account has spent, for `/usage`
//! - [`interactive`]: the line-oriented shell a bare `xfx` runs
//! - [`trace`]: `/trace`'s private diagnostic report, for both shells
//! - `local_time`: a moment's reading in the user's time zone, for a screen
//! - [`tui`]: the opt-in raw-mode shell `XFX_TUI=1` runs in its place
//! - [`app`]: composition and dispatch
//!
//! xfx is not affiliated with Vercel. It is a behavioral port pinned to
//! `vercel-labs/fx@580a0c5da9386317251968c09c1cee69e763487a`; see `UPSTREAM.md`
//! for the attribution and `docs/parity.md` for what is and is not implemented.

pub mod agent;
pub mod app;
pub mod cli;
pub mod clipboard;
pub mod config;
pub mod credits;
pub mod gateway;
pub mod image;
pub mod interactive;
pub mod llmux;
pub(crate) mod local_time;
pub mod markdown;
pub mod mcp;
pub mod output;
pub mod permission;
pub mod process;
pub mod profile_home;
pub mod provider;
pub mod session;
pub mod skills;
pub mod sound;
pub mod tools;
pub mod trace;
#[cfg(unix)]
pub mod tui;
pub mod usage;
pub mod workspace;

/// The product version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Facts about how this binary was built.
///
/// These are recorded at compile time by `build.rs` so a receipt can be tied back
/// to an exact commit. An unknown id is absent, never a placeholder value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildInfo {
    /// The channel this build claims: the compile profile, `debug` or
    /// `release`, unless the build declared `preview`. xfx has no updater, so a
    /// channel records where a binary came from and never promises that
    /// anything will be delivered to it.
    pub channel: &'static str,
    /// The build's id, `YYYY-MM-DD-HHMM-<12 hex>`, when it could be determined:
    /// the UTC minute a preview's run named it, or a local build's commit was
    /// made, then the source revision it was compiled from.
    pub id: Option<&'static str>,
}

impl BuildInfo {
    /// The abbreviated source revision: the id's last part.
    pub fn revision(&self) -> Option<&'static str> {
        let id = self.id?;
        id.rsplit_once('-').map(|(_, revision)| revision)
    }

    /// `<version> (<channel> <id>)`, or `<version> (<channel>)` when the id is
    /// not known: what every surface that says which build this is says --
    /// `--version`, `/version`, the welcome line, `status` and `/trace`.
    ///
    /// llmux's line (`version_with_build`), at the user's word of 2026-11-10:
    /// a local build has to say when it is from.
    pub fn version(&self) -> String {
        match self.id {
            Some(id) => format!("{VERSION} ({} {id})", self.channel),
            None => format!("{VERSION} ({})", self.channel),
        }
    }
}

/// Reads the compile-time build metadata.
pub fn build_info() -> BuildInfo {
    let id = env!("XFX_BUILD_ID");
    BuildInfo {
        channel: env!("XFX_BUILD_CHANNEL"),
        id: (!id.is_empty()).then_some(id),
    }
}

/// The rules `build.rs` compiles into the two values above.
///
/// The build script `include!`s the same file. Nothing runs a build script's
/// own tests, so the library carries them.
#[cfg(test)]
mod build_meta;

/// The loopback server, proxy and copied test binary that prove a client
/// takes no proxy from the environment.
#[cfg(test)]
mod test_net;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_build_channel_is_one_of_the_declared_channels() {
        assert!(
            build_meta::CHANNELS.contains(&build_info().channel),
            "got {}",
            build_info().channel
        );
    }

    #[test]
    fn a_reported_revision_is_lowercase_hex_of_fixed_width() {
        if let Some(revision) = build_info().revision() {
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
    fn a_reported_id_is_a_real_minute_and_the_revision_is_its_last_part() {
        let build = build_info();
        if let Some(id) = build.id {
            assert!(build_meta::is_build_id(id), "got {id}");
            assert_eq!(build.revision(), Some(&id[id.len() - 12..]));
        } else {
            assert_eq!(build.revision(), None);
        }
    }

    #[test]
    fn the_version_line_is_the_version_then_the_channel_and_the_id() {
        let preview = BuildInfo {
            channel: "preview",
            id: Some("2026-11-10-0019-ec40c7ad2e50"),
        };
        assert_eq!(
            preview.version(),
            format!("{VERSION} (preview 2026-11-10-0019-ec40c7ad2e50)")
        );
        assert_eq!(preview.revision(), Some("ec40c7ad2e50"));
        let unnamed = BuildInfo {
            channel: "debug",
            id: None,
        };
        assert_eq!(unnamed.version(), format!("{VERSION} (debug)"));
        assert_eq!(unnamed.revision(), None);
    }

    #[test]
    fn the_version_is_the_package_version() {
        assert_eq!(VERSION, env!("CARGO_PKG_VERSION"));
    }
}
