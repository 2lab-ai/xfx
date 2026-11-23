//! Shared deterministic test infrastructure.
//!
//! Nothing here talks to a real network endpoint or a real credential. The fake
//! Gateway binds a loopback port, records exactly what xfx sent, and replays a
//! scripted response, so a protocol assertion is about bytes rather than about a
//! mock's expectations.

// Each `tests/*.rs` file is its own crate, so a helper used by only one of them
// is dead code in the others. The module is a shared fixture, not a product
// surface; the alternative is per-crate `cfg` noise that hides real warnings.
#![allow(dead_code)]

// The fake clipboard is a shell script, so it is unix-only for the reason the
// pty harness is.
#[cfg(unix)]
pub mod fake_clipboard;
pub mod fake_gateway;
pub mod fake_llmux;
pub mod isolated;
// Profiles fx refuses at launch; the links and modes they lay down are unix.
#[cfg(unix)]
pub mod profiles;
// The pty harness drives a real terminal, so it is unix-only; `tests/cli.rs`
// and its siblings pull this module in on every platform.
#[cfg(unix)]
pub mod pty;
pub mod sandbox;

/// A command for `program`: every child an integration test starts is made
/// here. `clippy.toml` refuses `Command::new` everywhere else; this binary's
/// own environment is already the one [`isolated`] built from nothing, so
/// what the child inherits is that.
pub fn command(program: impl AsRef<std::ffi::OsStr>) -> std::process::Command {
    #[allow(
        clippy::disallowed_methods,
        reason = "the one Command an integration test makes; scripts/check-env-reads.py lists it"
    )]
    let command = std::process::Command::new(program);
    command
}
