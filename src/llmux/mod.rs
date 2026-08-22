//! The llmux backend: an Anthropic Messages provider aimed at a local daemon.
//!
//! llmux is a proxy that already holds the operator's model credentials. It
//! accepts a **keyless** request from loopback as the tenant `local`, on the
//! data plane only, and refuses a keyless request from anywhere else. That is
//! the whole credential story of this backend, and it is why nothing in this
//! module reads, forwards, stores, or logs an llmux key: there is no key to
//! handle, and code that could handle one would be code that could leak one.
//!
//! What lives here is a sibling of [`crate::gateway::GatewayProvider`], not a
//! variant of it. The two speak different wires -- the Gateway's
//! `prompt`/`tools`/`toolChoice` against Anthropic's
//! `messages`/`tools`/`tool_choice` -- and the decoders disagree about what a
//! finished answer even looks like. What they *do* share is the transport
//! discipline, and that is shared rather than copied: one attempt per call, the
//! same timeouts, the same bounded error body, the same `Retry-After` reading,
//! and the same rule about which URL is allowed to receive a request.

pub mod protocol;

/// Where a llmux daemon listens unless the operator moved it.
pub const DEFAULT_URL: &str = "http://127.0.0.1:3456";

/// The Anthropic API version xfx pins.
///
/// It is required rather than optional: llmux forwards a path it does not
/// recognize to `api.anthropic.com`, which answers 400 without this header, so
/// omitting it would turn a moved endpoint into an unreadable failure.
pub const ANTHROPIC_VERSION: &str = "2023-06-01";
