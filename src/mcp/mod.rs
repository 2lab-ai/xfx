//! The Model Context Protocol client: local and loopback-or-HTTPS servers,
//! their tools, resources and prompts, and the `/mcp` command that manages
//! them.
//!
//! A port of upstream's client for the paths that need no browser
//! (`vercel-labs/fx@c1db9191 src/core/mcp/`): stdio servers and streamable
//! HTTP servers without OAuth, fx v0.0.16's version detection -- the
//! `server/discover` probe, then the legacy `initialize` for a 2025 server
//! ([`era`]) -- upstream's profile file and project trust, and upstream's
//! model-facing surface -- a servers block and two meta-tools, with a
//! server's own tools advertised only once a search, an explicit selection
//! or a direct call picks them ([`catalog`]).
//!
//! | piece | what it is |
//! |---|---|
//! | [`config`] | `~/.xfx/mcp.json`, `.mcp.json`, and the trust choices in `settings.json` |
//! | [`transport`] | one request over stdio or HTTP, synchronously, with a deadline |
//! | [`era`] | which MCP a server speaks: fx's version detection, pure |
//! | [`wire`] | a JSON-RPC message read from its raw text, as fx reads one |
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
//! ([`transport::server_environment`]) -- defence in depth rather than a
//! boundary, since a process of the same user can read xfx's launch
//! environment (`ps -E` on macOS) -- in a process group of its own and
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
pub mod era;
pub mod names;
pub mod runtime;
pub mod transport;
pub mod wire;

#[cfg(test)]
mod tests;

use std::fmt;

pub use runtime::{McpRuntime, Paths};

/// Why an MCP operation failed, by upstream's error name where it has one:
/// the name is what a person reads (`… failed: McpServerNotFound.`).
#[derive(Debug, Clone, PartialEq)]
pub enum McpError {
    Named(&'static str),
    /// The server answered with a JSON-RPC error: its code and message, and
    /// not its `data`, which fx v0.0.16 does not keep either
    /// (`CallResult.failure`, `vercel-labs/fx@481104fd
    /// src/core/mcp_host/runtime.zig:117`, `:2371`).
    Protocol {
        code: i64,
        message: String,
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

/// The most bytes [`terminal_safe`] returns, upstream's for the trust
/// prompt's name (`encodeTerminalSafe(alloc, name, 256)`,
/// `src/core/app/app_mcp_runtime.zig:1694@c1db9191`).
pub const TERMINAL_SAFE_MAX_BYTES: usize = 256;

/// A name or text a server or a repository supplied, made safe to put on a
/// terminal row: upstream's `encodeTerminalSafe(.., 256)`
/// (`src/core/shared/text_utils.zig:343-380@c1db9191`). Every C0 control and
/// `DEL` -- a line break included -- is `\xNN`, and the C1 controls and the
/// zero-width, line-separator, bidirectional and byte-order-mark code points
/// (`isNonPrintingCodepoint`, `:620-626`) are `\u{NNNN}`, and so is every
/// other default-ignorable code point the approval view names
/// ([`terminal_safe_token`]) -- spelled out rather than dropped, so a name
/// with a character in it is not shown as a different name. At most [`TERMINAL_SAFE_MAX_BYTES`]: past that the text is cut at
/// a token boundary and ends `...`, as upstream's does, so a repository
/// cannot fill a row, or the screen, with one name.
pub fn terminal_safe(text: &str) -> String {
    const MARKER: &str = "...";
    let max = TERMINAL_SAFE_MAX_BYTES;
    let content_limit = max - MARKER.len();
    let mut out = String::new();
    let mut boundary = 0;
    let mut buf = [0u8; 4];
    for character in text.chars() {
        let token = terminal_safe_token(character, &mut buf);
        if out.len() + token.len() > max {
            out.truncate(boundary);
            out.push_str(MARKER);
            return out;
        }
        out.push_str(&token);
        if out.len() <= content_limit {
            boundary = out.len();
        }
    }
    out
}

/// [`terminal_safe`] over text the TUI's channel has already spelled once
/// (`tui::toolgroup::flattened`: fx's `encodeTerminalSafe` set and U+061C,
/// white space one space): each escape that pass wrote -- `\xNN` or
/// `\u{N...}` -- is one token, passed on as it is and never cut into, and
/// every other character is spelled and bounded as [`terminal_safe`] spells
/// and bounds it. An MCP row's reason crosses the channel so spelled, and
/// the characters the channel leaves as they are and this names (U+FE0F,
/// U+00AD and the rest of [`terminal_safe_token`]'s wider set) can carry it
/// past the bound: cut a character at a time, the bound would fall inside
/// an escape and leave a lone `\`. Cut a token at a time, the row is what
/// [`terminal_safe`] makes of the raw text in one pass, since both passes
/// write the same token for every character they both spell. The one
/// exception is text that merely looks like an escape -- a server's own
/// `\x` and two hex digits, or `\u{`, one to six of them and `}` -- which
/// the channel's spelling makes indistinguishable from one of its own: it is
/// kept whole too, so where it straddles the cut (byte 253) the row ends
/// before it, up to nine bytes earlier than one pass over the raw text, which
/// would cut inside it. Printable either way.
pub fn terminal_safe_respelled(text: &str) -> String {
    const MARKER: &str = "...";
    let max = TERMINAL_SAFE_MAX_BYTES;
    let content_limit = max - MARKER.len();
    let mut out = String::new();
    let mut boundary = 0;
    let mut buf = [0u8; 4];
    let mut rest = text;
    while let Some(character) = rest.chars().next() {
        let spelled = spelled_escape(rest);
        let token = match spelled {
            Some(escape) => std::borrow::Cow::Borrowed(escape),
            None => terminal_safe_token(character, &mut buf),
        };
        let taken = spelled.map_or(character.len_utf8(), str::len);
        if out.len() + token.len() > max {
            out.truncate(boundary);
            out.push_str(MARKER);
            return out;
        }
        out.push_str(&token);
        if out.len() <= content_limit {
            boundary = out.len();
        }
        rest = &rest[taken..];
    }
    out
}

/// The escape `text` starts with, as `crate::tools::question::terminal_safe`
/// writes one: `\x` and two lower-case hex digits, or `\u{`, one to six of
/// them and `}`.
fn spelled_escape(text: &str) -> Option<&str> {
    let hex = |b: &u8| b.is_ascii_digit() || (b'a'..=b'f').contains(b);
    let bytes = text.as_bytes();
    if bytes.len() >= 4 && bytes[0] == b'\\' && bytes[1] == b'x' && bytes[2..4].iter().all(hex) {
        return Some(&text[..4]);
    }
    if bytes.len() >= 5 && bytes[0] == b'\\' && bytes[1] == b'u' && bytes[2] == b'{' {
        let digits = bytes[3..].iter().take_while(|b| hex(b)).count();
        if (1..=6).contains(&digits) && bytes.get(3 + digits) == Some(&b'}') {
            return Some(&text[..4 + digits]);
        }
    }
    None
}

/// One character's token: upstream's ([`crate::tools::question::terminal_safe`]),
/// and -- where the TUI's approval view is built -- every default-ignorable
/// code point it names as well (SEC-A4, `approval_view::command_text`:
/// `U+00AD`, `U+3164`, `U+FFA0` and the rest a terminal may draw as nothing
/// and so hide text behind), and everything else it counts as no cells but a
/// combining mark (SEC-A4c). One character at a time has no syllable to
/// complete, so a decomposed Hangul name's vowels and trailing consonants are
/// named here, where the approval view shows them as text. A line break is
/// `\x0a` here, where the approval view keeps it: a name is one row.
#[cfg(unix)]
fn terminal_safe_token<'a>(character: char, buf: &'a mut [u8; 4]) -> std::borrow::Cow<'a, str> {
    if character == '\n' {
        return std::borrow::Cow::Borrowed("\\x0a");
    }
    let one = character.encode_utf8(buf);
    let spelled = crate::tui::approval_view::command_text(one);
    if spelled == *one {
        std::borrow::Cow::Borrowed(one)
    } else {
        std::borrow::Cow::Owned(spelled)
    }
}

#[cfg(not(unix))]
fn terminal_safe_token<'a>(character: char, buf: &'a mut [u8; 4]) -> std::borrow::Cow<'a, str> {
    crate::tools::question::terminal_safe(character.encode_utf8(buf))
}
