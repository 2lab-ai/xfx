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

/// `env_exception!`, the literal-named reads of the real process environment
/// that stay outside [`process_env!`].
#[macro_use]
mod env_exception;

/// A read of the real process environment by `$who`, the function that makes
/// it: `process_env!("config::Environment::from_process", var(key))`.
///
/// In a build, `::std::env::var` or `::std::env::var_os` itself -- and
/// `lookup`, a closure over `::std::env::var` -- so what a release binary does
/// is unchanged. In this crate's unit tests, a read that fails closed
/// ([`test_env`]), never the home, profile, variables or git configuration of
/// the person running the suite:
///
/// - `var`, `var_os` and `lookup` are values xfx itself acts on: the case's
///   own fake process, or a panic that names `$who`.
/// - `child_var` and `child_var_os` are values only handed on to a child's
///   environment: the fake process's, or none (`PATH` and `TMPDIR` excepted).
///
/// Every other call of a reader `clippy.toml`'s `disallowed-methods` names,
/// in `src` and `tests`, is refused by clippy on the resolved path unless it
/// is an `env_exception!`, whose names are literals; `env!` and
/// `option_env!` are held to the names cargo and `build.rs` set.
/// `scripts/check-env-reads.py` runs clippy with those lints forced on, so
/// no allow of any spelling hides a read, and refuses every read but these
/// macros' own arms -- this one's, whose names are computed, and
/// `env_exception!`'s, each held to its file and its names. A C function
/// declared by hand, the `environ` static and `/proc/self/environ` are
/// outside what clippy can name.
#[cfg(not(test))]
macro_rules! process_env {
    ($who:literal, var($name:expr)) => {
        match () {
            #[allow(
                clippy::disallowed_methods,
                reason = "env: process_env! -- a build reads the process"
            )]
            () => ::std::env::var($name),
        }
    };
    ($who:literal, var_os($name:expr)) => {
        match () {
            #[allow(
                clippy::disallowed_methods,
                reason = "env: process_env! -- a build reads the process"
            )]
            () => ::std::env::var_os($name),
        }
    };
    ($who:literal, child_var($name:expr)) => {
        match () {
            #[allow(
                clippy::disallowed_methods,
                reason = "env: process_env! -- a build reads the process"
            )]
            () => ::std::env::var($name),
        }
    };
    ($who:literal, child_var_os($name:expr)) => {
        match () {
            #[allow(
                clippy::disallowed_methods,
                reason = "env: process_env! -- a build reads the process"
            )]
            () => ::std::env::var_os($name),
        }
    };
    ($who:literal, lookup) => {
        |name: &str| match () {
            #[allow(
                clippy::disallowed_methods,
                reason = "env: process_env! -- a build reads the process"
            )]
            () => ::std::env::var(name).ok(),
        }
    };
}

#[cfg(test)]
macro_rules! process_env {
    ($who:literal, var($name:expr)) => {
        $crate::test_env::var($who, $name)
    };
    ($who:literal, var_os($name:expr)) => {
        $crate::test_env::var_os($who, $name)
    };
    ($who:literal, child_var($name:expr)) => {
        $crate::test_env::child_var($name)
    };
    ($who:literal, child_var_os($name:expr)) => {
        $crate::test_env::child_var_os($name)
    };
    ($who:literal, lookup) => {
        $crate::test_env::lookup($who)
    };
}

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
/// to an exact commit. An unknown revision is absent, never a placeholder value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildInfo {
    /// The channel this build claims: the compile profile, `debug` or
    /// `release`, unless the build declared `preview`. xfx has no updater, so a
    /// channel records where a binary came from and never promises that
    /// anything will be delivered to it.
    pub channel: &'static str,
    /// The abbreviated source revision, when it could be determined.
    pub revision: Option<&'static str>,
}

/// Reads the compile-time build metadata.
pub fn build_info() -> BuildInfo {
    let revision = env!("XFX_BUILD_REVISION");
    BuildInfo {
        channel: env!("XFX_BUILD_CHANNEL"),
        revision: (!revision.is_empty()).then_some(revision),
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

/// What a unit test has in place of the process environment: its own fake
/// one, or a panic.
#[cfg(test)]
mod test_env;

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
