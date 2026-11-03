//! The Model Context Protocol client: local and loopback-or-HTTPS servers,
//! their tools, resources and prompts, and the `/mcp` command that manages
//! them.
//!
//! A port of upstream's client for the paths that need no browser
//! (`vercel-labs/fx@c1db9191 src/core/mcp/`): stdio servers and streamable
//! HTTP servers without OAuth, upstream's legacy `initialize` handshake as its
//! default, upstream's profile file and project trust, and upstream's
//! model-facing surface -- a servers block and three meta-tools, with a
//! server's own tools advertised only once a search or an explicit selection
//! picks them ([`catalog`]).
//!
//! | piece | what it is |
//! |---|---|
//! | [`config`] | `~/.xfx/mcp.json`, `.mcp.json`, and the trust choices in `settings.json` |
//! | [`transport`] | one request over stdio or HTTP, synchronously, with a deadline |
//! | [`runtime`] | the live servers, their state machine, reload, and every operation |
//! | [`catalog`] | the model's servers block, `capability_search`, selected schemas, result envelopes |
//! | [`command`] | `/mcp`'s text subcommands |
//! | [`names`] | the `mcp_<server>_<tool>` names |
//! | [`docker`] | a `docker run` server's container, removed with it |
//!
//! # What a server can reach
//!
//! A stdio server is a program the person configured, started as the person
//! with six variables of their environment -- `HOME`, `LOGNAME`, `PATH`,
//! `SHELL`, `TERM`, `USER` -- and the server's own `env`, never xfx's
//! credential or anything else it inherited
//! ([`transport::server_environment`]), in a process group of its own and
//! stopped by that group. A server that needs more of the person's
//! environment -- a proxy, `TMPDIR`, the locale (`LANG`, `LC_*`), on Linux
//! `DISPLAY`, `XDG_RUNTIME_DIR` or `DBUS_SESSION_BUS_ADDRESS`, `NODE_*`, a
//! token -- names it in its `env`. An HTTP server is reached only at an `https` URL or
//! a loopback `http` one with a port ([`config::validate_endpoint`]). Neither is
//! confined: a server does what its program does. A workspace's servers start
//! only once the person approves them, and every call of a server's tool is
//! admitted by the session's permission policy first
//! ([`crate::tools::mcp`]).

pub mod catalog;
pub mod command;
pub mod config;
pub mod docker;
pub mod names;
pub mod runtime;
pub mod transport;

#[cfg(test)]
mod tests;

use std::fmt;

pub use runtime::{McpRuntime, Paths};

/// Why an MCP operation failed, by upstream's error name where it has one:
/// the name is what a person reads (`… failed: McpServerNotFound.`).
#[derive(Debug, Clone, PartialEq)]
pub enum McpError {
    Named(&'static str),
    /// The server answered with a JSON-RPC error.
    Protocol {
        code: i64,
        message: String,
        data: Option<serde_json::Value>,
    },
    /// A stopped server that could not be started again, and why.
    Restart(String),
}

impl fmt::Display for McpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Named(name) => f.write_str(name),
            Self::Protocol { .. } => f.write_str("McpProtocolError"),
            Self::Restart(_) => f.write_str("McpServerRestartFailed"),
        }
    }
}

impl std::error::Error for McpError {}

/// A server-supplied name made safe to put on a terminal row: control
/// characters dropped (`encodeTerminalSafe`).
pub fn terminal_safe(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).collect()
}
