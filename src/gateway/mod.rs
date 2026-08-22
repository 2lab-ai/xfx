//! The model provider boundary and its Vercel AI Gateway implementation.
//!
//! Three things live here:
//!
//! - [`Provider`], the trait the agent talks to, so a turn can be driven by a
//!   scripted stream in a test and by a real socket in the binary without
//!   either knowing about the other;
//! - [`Endpoint`], which decides what URL is allowed to receive a bearer
//!   credential; and
//! - [`GatewayProvider`], one HTTP attempt over rustls, streamed through the
//!   bounded decoder in [`sse`].
//!
//! **One attempt per call.** `stream` performs exactly one transport attempt and
//! reports whether the failed attempt provably delivered nothing. Retry policy
//! belongs to the turn, which is the only layer that knows whether an answer has
//! already reached the user. Upstream draws the same line: when the agent owns
//! attempts, the transport's retry count is forced to one
//! (`vercel-labs/fx@580a0c5d src/gateway/client.zig:1169-1172`).

pub mod protocol;
pub mod sse;

use std::fmt;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

use crate::config::Credential;
use protocol::{Completion, CompletionRequest, ProtocolError};
use sse::{SseError, SseReader};

/// The Vercel AI Gateway completion endpoint
/// (`vercel-labs/fx@580a0c5d src/builtins/gateway.zig:41`).
pub const DEFAULT_CHAT_URL: &str = "https://ai-gateway.vercel.sh/v3/ai/language-model";

/// The environment variable that overrides the endpoint.
///
/// It is read here rather than in `config` because the safety rule that governs
/// it is a transport rule: the URL receives a bearer token, so only this module
/// can say which ones are acceptable.
pub const GATEWAY_URL_ENV: &str = "XFX_GATEWAY_URL";

/// How many transport attempts one model step may spend.
///
/// An xfx bound, not an upstream constant. It exists so that a single flaky
/// connection does not fail a turn, and it is small because every attempt past
/// the first is a chance to duplicate model intent.
pub const DEFAULT_MAX_ATTEMPTS: u32 = 3;

/// How long a connection may take to establish.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// How long the stream may stall between chunks before xfx gives up.
const READ_TIMEOUT: Duration = Duration::from_secs(120);

/// How often a stalled stream re-reads the cancellation flag.
///
/// Short enough that Ctrl-C feels immediate, long enough that a healthy stream
/// never notices. It bounds latency, not the wait itself: [`READ_TIMEOUT`] is
/// still what decides that a silent server is a failed attempt.
pub(crate) const CANCEL_POLL: Duration = Duration::from_millis(50);

/// How much of a failed response body is quoted back to the user.
pub(crate) const MAX_ERROR_BODY_BYTES: usize = 4 * 1024;

/// How a Gateway failure names the endpoint it was talking to.
///
/// A failure has to say *which* endpoint could not be reached. Hard-coding one
/// name into the messages made every llmux failure read as a Gateway failure --
/// a stopped local daemon printed "cannot reach the Gateway: Connection
/// refused" and sent the operator to look at Vercel.
pub const GATEWAY_SUBJECT: &str = "the Gateway";

/// The product's own user agent. xfx does not claim to be `fx`.
pub const USER_AGENT: &str = concat!("xfx/", env!("CARGO_PKG_VERSION"));

/// The HTTP client a remote provider streams over.
///
/// One builder rather than one per backend: the timeouts and the user agent are
/// facts about xfx as a client, not about which wire it happens to be speaking,
/// and two copies would be two places for them to drift apart.
///
/// It honours the system proxy environment, which is correct for an endpoint on
/// the internet -- reaching it may require one.
pub(crate) fn build_client() -> Result<reqwest::Client, ProviderError> {
    finish_client(base_client_builder())
}

/// The same client for a service on this machine, with proxies refused.
///
/// A loopback backend must never route through a proxy, for two reasons that
/// point the same way. It would be wrong: the request carries the prompt and the
/// project context with no credential, and the only thing making that safe is
/// that it does not leave the machine -- an `ALL_PROXY` would hand all of it to
/// a third party while `status` went on reporting a keyless loopback
/// arrangement. And it would be broken: on a machine with a corporate
/// `HTTP_PROXY` set, every connection to `127.0.0.1` would be attempted through
/// the proxy, so a daemon that is running would look like a daemon that is not.
pub(crate) fn build_loopback_client() -> Result<reqwest::Client, ProviderError> {
    finish_client(base_client_builder().no_proxy())
}

fn base_client_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(READ_TIMEOUT)
        .user_agent(USER_AGENT)
}

fn finish_client(builder: reqwest::ClientBuilder) -> Result<reqwest::Client, ProviderError> {
    builder.build().map_err(|err| ProviderError::Transport {
        subject: GATEWAY_SUBJECT,
        detail: err.to_string(),
    })
}

/// Where a turn's assistant text goes as it is decoded.
///
/// Separate from [`crate::output::EventSink`] on purpose: the transport must be
/// able to stream text without knowing what a turn event is.
pub trait DeltaSink {
    /// Accepts one assistant text fragment, in arrival order.
    fn text_delta(&mut self, text: &str) -> io::Result<()>;
}

/// A cancellation flag shared by a turn, its transport, and its decoder.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    /// Requests cancellation. Idempotent, and safe from any thread.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    /// Clears the request, so the next turn starts uncancelled.
    ///
    /// A one-shot `ask` never needs this: its token dies with the process. A
    /// shell does, because it runs many turns through one provider and one tool
    /// context, and an interrupt has to mean "stop *that* turn" rather than
    /// "stop everything from now on". The caller is responsible for calling it
    /// while no turn is running -- in the shell that is the same lock that
    /// decides whether a signal is a cancellation or an exit -- so this cannot
    /// race a cancellation it was supposed to honor.
    pub fn reset(&self) {
        self.0.store(false, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// A model endpoint that is allowed to receive a bearer credential.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    url: String,
}

impl Endpoint {
    /// Resolves the endpoint, applying an override only when it is safe.
    ///
    /// The chat URL carries the bearer token and the whole request payload, so
    /// an HTTP override is accepted only for a loopback test server
    /// (`vercel-labs/fx@580a0c5d src/builtins/gateway.zig:759-765`,
    /// `src/gateway/client.zig:1787-1803`).
    ///
    /// Where upstream silently falls back to the default, xfx fails. A silent
    /// fallback sends a request the operator did not ask for, to an endpoint
    /// they did not name, which is the more surprising of the two outcomes.
    pub fn resolve(override_url: Option<&str>) -> Result<Self, EndpointError> {
        let Some(candidate) = override_url else {
            return Ok(Self {
                url: DEFAULT_CHAT_URL.to_string(),
            });
        };
        Self::checked(candidate, GATEWAY_URL_ENV)
    }

    /// Applies the bearer-transport rule to a URL that came from somewhere else.
    ///
    /// The rule belongs to this module because it is a transport rule. Only the
    /// *name of the knob* differs, so `subject` is carried into the refusal
    /// rather than the environment variable this module happens to own -- a
    /// message that told an operator to fix `XFX_GATEWAY_URL` when they had
    /// mistyped `llmux_url` would send them to edit a variable they never set.
    pub fn checked(url: &str, subject: &'static str) -> Result<Self, EndpointError> {
        Self::checked_with(url, subject, EndpointPolicy::BearerTransport)
    }

    /// Accepts a URL under the policy that fits what will be sent to it.
    ///
    /// Both policies refuse a malformed URL, a scheme outside http/https, and
    /// embedded userinfo. They part company on *why* an endpoint is allowed to
    /// receive a request at all, and that is a question about the promise xfx has
    /// made to the operator rather than about URLs -- see [`EndpointPolicy`].
    pub fn checked_with(
        url: &str,
        subject: &'static str,
        policy: EndpointPolicy,
    ) -> Result<Self, EndpointError> {
        let candidate = url.trim();
        let Some(parsed) = ParsedUrl::parse(candidate) else {
            return Err(EndpointError::Malformed {
                subject,
                url: candidate.to_string(),
            });
        };
        if parsed.scheme != "http" && parsed.scheme != "https" {
            return Err(EndpointError::UnsupportedScheme {
                subject,
                url: candidate.to_string(),
                scheme: parsed.scheme,
            });
        }
        if parsed.has_userinfo {
            return Err(EndpointError::EmbeddedCredentials {
                subject,
                url: candidate.to_string(),
            });
        }
        match policy {
            EndpointPolicy::BearerTransport => {
                if parsed.scheme == "http" && !parsed.is_loopback_with_port() {
                    return Err(EndpointError::NonLoopbackHttp {
                        subject,
                        url: candidate.to_string(),
                    });
                }
            }
            EndpointPolicy::LoopbackService => {
                if !parsed.is_loopback_with_port() {
                    return Err(EndpointError::NotLoopback {
                        subject,
                        url: candidate.to_string(),
                    });
                }
                if !parsed.path.is_empty() && parsed.path != "/" {
                    return Err(EndpointError::UnexpectedPath {
                        subject,
                        url: candidate.to_string(),
                        path: parsed.path,
                    });
                }
            }
        }
        Ok(Self {
            url: candidate.to_string(),
        })
    }

    /// Resolves the endpoint from the process environment.
    ///
    /// A blank value is ignored rather than treated as an override, matching how
    /// every other xfx environment knob behaves.
    pub fn from_process() -> Result<Self, EndpointError> {
        let raw = std::env::var(GATEWAY_URL_ENV).ok();
        let candidate = raw.as_deref().map(str::trim).filter(|s| !s.is_empty());
        Self::resolve(candidate)
    }

    pub fn url(&self) -> &str {
        &self.url
    }
}

/// What an endpoint is allowed to be, given what will be sent to it.
///
/// Two policies, because xfx makes two different promises.
///
/// [`Self::BearerTransport`] guards a URL that receives a **credential**. TLS is
/// what protects it, so https is acceptable anywhere and cleartext http only to
/// a loopback address with an explicit port.
///
/// [`Self::LoopbackService`] guards a URL that receives a **prompt with no
/// credential at all**. The reason that is safe is that the request never leaves
/// the machine -- so "it never leaves the machine" has to be enforced rather than
/// assumed, and TLS does not substitute for it: an https collector on the
/// internet would receive the prompt and the project context, keyless, while
/// `status` went on reporting `llmux-keyless-loopback`. The label is true by
/// construction only if a remote host cannot be named in the first place. The
/// base URL must also carry no path, because a provider appends its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointPolicy {
    /// https anywhere, http only on loopback with an explicit port.
    BearerTransport,
    /// http or https, loopback host with an explicit port, and no path.
    LoopbackService,
}

/// Why an endpoint was refused.
///
/// Every variant carries the `subject`: the name of the knob that supplied the
/// URL, so the message points at what the operator wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EndpointError {
    Malformed {
        subject: &'static str,
        url: String,
    },
    UnsupportedScheme {
        subject: &'static str,
        url: String,
        scheme: String,
    },
    EmbeddedCredentials {
        subject: &'static str,
        url: String,
    },
    NonLoopbackHttp {
        subject: &'static str,
        url: String,
    },
    /// A local-service endpoint named a host that is not on this machine.
    NotLoopback {
        subject: &'static str,
        url: String,
    },
    /// A local-service base URL carried a path, which a provider would append to.
    UnexpectedPath {
        subject: &'static str,
        url: String,
        path: String,
    },
}

impl fmt::Display for EndpointError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed { subject, url } => {
                write!(f, "{subject} is not a URL: `{url}`")
            }
            Self::UnsupportedScheme {
                subject,
                url,
                scheme,
            } => write!(
                f,
                "{subject} must use https, or http on loopback; `{url}` uses `{scheme}`"
            ),
            Self::EmbeddedCredentials { subject, url } => write!(
                f,
                "{subject} must not embed credentials in the URL: `{url}`"
            ),
            Self::NonLoopbackHttp { subject, url } => write!(
                f,
                "{subject} may use http only for a loopback address with an \
                 explicit port, because the request travels in cleartext; \
                 `{url}` is not one"
            ),
            Self::NotLoopback { subject, url } => write!(
                f,
                "{subject} must name a service on this machine -- a loopback address \
                 with an explicit port, such as `http://127.0.0.1:3456` -- because the \
                 request carries the prompt with no credential and is safe only while \
                 it does not leave the machine; `{url}` is remote, and a remote llmux \
                 is not supported"
            ),
            Self::UnexpectedPath { subject, url, path } => write!(
                f,
                "{subject} must be a base address with no path, because xfx appends \
                 the API path itself; `{url}` carries `{path}`"
            ),
        }
    }
}

impl std::error::Error for EndpointError {}

/// Just enough URL structure to answer the safety question.
///
/// A dependency would parse more of RFC 3986 than this decision needs, and the
/// decision is small enough to read in one screen, which matters more for a rule
/// that guards a credential.
struct ParsedUrl {
    scheme: String,
    has_userinfo: bool,
    host: String,
    has_port: bool,
    /// Everything after the authority, including a query or fragment.
    ///
    /// Read only by [`EndpointPolicy::LoopbackService`], which needs a base
    /// address: a provider appends its own path, so anything here would be
    /// doubled onto the request.
    path: String,
}

impl ParsedUrl {
    fn parse(url: &str) -> Option<Self> {
        let (scheme, rest) = url.split_once("://")?;
        if scheme.is_empty()
            || !scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        {
            return None;
        }
        let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        let authority = &rest[..authority_end];
        let path = rest[authority_end..].to_string();
        if authority.is_empty() {
            return None;
        }
        // Split on the last `@`: userinfo may itself contain one.
        let (has_userinfo, host_port) = match authority.rsplit_once('@') {
            Some((_, host_port)) => (true, host_port),
            None => (false, authority),
        };

        let (host, has_port) = if let Some(rest) = host_port.strip_prefix('[') {
            // An IPv6 literal keeps its brackets, matching how upstream compares
            // against `[::1]` (`src/gateway/client.zig:1802`).
            let end = rest.find(']')?;
            let remainder = &rest[end + 1..];
            (
                format!("[{}]", &rest[..end]),
                remainder.starts_with(':') && remainder.len() > 1,
            )
        } else {
            match host_port.split_once(':') {
                Some((host, port)) => (host.to_string(), !port.is_empty()),
                None => (host_port.to_string(), false),
            }
        };
        if host.is_empty() || host == "[]" {
            return None;
        }

        Some(Self {
            scheme: scheme.to_ascii_lowercase(),
            has_userinfo,
            host,
            has_port,
            path,
        })
    }

    /// The upstream loopback test, plus upstream's requirement that the port be
    /// explicit (`src/gateway/client.zig:1789-1803`).
    fn is_loopback_with_port(&self) -> bool {
        self.has_port
            && (self.host == "127.0.0.1"
                || self.host == "[::1]"
                || self.host.eq_ignore_ascii_case("localhost"))
    }
}

/// A failed transport attempt.
#[derive(Debug)]
pub enum ProviderError {
    /// The endpoint itself was refused, before any credential was sent.
    Endpoint(EndpointError),
    /// The request could not be built from the prompt it was given.
    Request {
        subject: &'static str,
        source: ProtocolError,
    },
    /// A header value could not be transmitted.
    InvalidHeader { name: &'static str },
    /// The connection could not be established, so the request was never sent.
    Connect {
        subject: &'static str,
        detail: String,
    },
    /// The exchange failed at a point where the request may already have been
    /// processed.
    Transport {
        subject: &'static str,
        detail: String,
    },
    /// The endpoint answered with a non-success status.
    Status {
        subject: &'static str,
        status: u16,
        body: String,
        retryable: bool,
        /// The server's own `Retry-After` delay, when it sent a readable one.
        /// The turn decides whether and how long to wait; the transport only
        /// reports what the server asked for.
        retry_after: Option<Duration>,
    },
    /// The response body was not a usable stream.
    Protocol(SseError),
    /// The assistant output could not be written.
    Sink(io::Error),
    /// The turn was cancelled.
    Cancelled,
}

impl ProviderError {
    /// Whether this failure provably delivered nothing and may be replayed.
    ///
    /// Everything ambiguous answers `false`. A retry after an ambiguous delivery
    /// can duplicate model intent and its cost, which is worse than failing the
    /// turn (design, "Risks and controls").
    pub fn is_replayable(&self) -> bool {
        match self {
            // The connection never opened, so the payload never left.
            Self::Connect { .. } => true,
            // The Gateway rejected the request outright and streamed nothing.
            Self::Status { retryable, .. } => *retryable,
            _ => false,
        }
    }

    /// How long the server asked the client to wait before trying again.
    ///
    /// `None` means the server said nothing, not that it said zero. Only the
    /// turn may act on this; ignoring a server's own backoff request is how a
    /// client turns a rate limit into an outage.
    pub fn retry_after(&self) -> Option<Duration> {
        match self {
            Self::Status { retry_after, .. } => *retry_after,
            _ => None,
        }
    }
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Endpoint(err) => write!(f, "{err}"),
            Self::Request { subject, source } => {
                write!(f, "cannot build the request for {subject}: {source}")
            }
            Self::InvalidHeader { name } => {
                write!(f, "the `{name}` header value cannot be transmitted")
            }
            Self::Connect { subject, detail } => write!(f, "cannot reach {subject}: {detail}"),
            Self::Transport { subject, detail } => {
                write!(f, "the connection to {subject} failed: {detail}")
            }
            Self::Status {
                subject,
                status,
                body,
                retryable: _,
                retry_after,
            } => {
                write!(f, "{subject} returned HTTP {status}")?;
                if let Some(delay) = retry_after {
                    write!(f, " (retry after {}s)", delay.as_secs())?;
                }
                if !body.is_empty() {
                    write!(f, ": {body}")?;
                }
                Ok(())
            }
            Self::Protocol(err) => write!(f, "{err}"),
            Self::Sink(err) => write!(f, "cannot write assistant output: {err}"),
            Self::Cancelled => write!(f, "the turn was cancelled"),
        }
    }
}

impl std::error::Error for ProviderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Endpoint(err) => Some(err),
            Self::Request { source, .. } => Some(source),
            Self::Protocol(err) => Some(err),
            Self::Sink(err) => Some(err),
            _ => None,
        }
    }
}

impl From<SseError> for ProviderError {
    fn from(err: SseError) -> Self {
        match err {
            // A write failure is the consumer's problem, not the protocol's.
            SseError::Sink(err) => Self::Sink(err),
            SseError::Cancelled => Self::Cancelled,
            other => Self::Protocol(other),
        }
    }
}

/// A source of model completions.
///
/// One call is one transport attempt. An implementation must not retry
/// internally, because only the turn knows whether an answer already reached
/// the user.
#[async_trait::async_trait(?Send)]
pub trait Provider {
    async fn stream(
        &self,
        request: &CompletionRequest,
        deltas: &mut dyn DeltaSink,
    ) -> Result<Completion, ProviderError>;
}

/// Streams completions from the Vercel AI Gateway over HTTPS.
pub struct GatewayProvider {
    client: reqwest::Client,
    endpoint: Endpoint,
    credential: Credential,
    cancel: CancelToken,
}

impl GatewayProvider {
    pub fn new(
        endpoint: Endpoint,
        credential: Credential,
        cancel: CancelToken,
    ) -> Result<Self, ProviderError> {
        Ok(Self {
            client: build_client()?,
            endpoint,
            credential,
            cancel,
        })
    }

    /// The fixed headers every completion request carries.
    ///
    /// The shape follows upstream (`src/gateway/client.zig:1459-1494`), but the
    /// identity is xfx's own: claiming to be `fx` would misattribute xfx's
    /// traffic to the product it is a port of.
    fn headers(&self, model: &str) -> Result<HeaderMap, ProviderError> {
        let mut headers = HeaderMap::new();
        let mut authorization =
            HeaderValue::from_str(&format!("Bearer {}", self.credential.secret())).map_err(
                |_| ProviderError::InvalidHeader {
                    name: "authorization",
                },
            )?;
        // Keeps the credential out of reqwest's own diagnostics.
        authorization.set_sensitive(true);
        headers.insert(reqwest::header::AUTHORIZATION, authorization);
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        headers.insert(
            reqwest::header::ACCEPT,
            HeaderValue::from_static("text/event-stream"),
        );

        let model = HeaderValue::from_str(model).map_err(|_| ProviderError::InvalidHeader {
            name: "ai-language-model-id",
        })?;
        for (name, value) in [
            (
                "http-referer",
                HeaderValue::from_static("https://github.com/2lab-ai/xfx"),
            ),
            ("x-title", HeaderValue::from_static("xfx")),
            (
                "ai-gateway-protocol-version",
                HeaderValue::from_static("0.0.1"),
            ),
            (
                "ai-language-model-specification-version",
                HeaderValue::from_static("4"),
            ),
            ("ai-language-model-id", model),
            (
                "ai-language-model-streaming",
                HeaderValue::from_static("true"),
            ),
        ] {
            headers.insert(HeaderName::from_static(name), value);
        }
        Ok(headers)
    }
}

#[async_trait::async_trait(?Send)]
impl Provider for GatewayProvider {
    async fn stream(
        &self,
        request: &CompletionRequest,
        deltas: &mut dyn DeltaSink,
    ) -> Result<Completion, ProviderError> {
        if self.cancel.is_cancelled() {
            return Err(ProviderError::Cancelled);
        }
        // Built and validated before the socket opens, so an invalid prompt
        // never costs a round trip.
        let body = request.body().map_err(|source| ProviderError::Request {
            subject: GATEWAY_SUBJECT,
            source,
        })?;
        let headers = self.headers(&request.model)?;

        let response = self
            .client
            .post(self.endpoint.url())
            .headers(headers)
            .body(body)
            .send()
            .await
            .map_err(|err| transport_failure(GATEWAY_SUBJECT, &err))?;

        let status = response.status();
        if !status.is_success() {
            // Read the header before the body: consuming the response moves it.
            let retry_after = parse_retry_after(response.headers());
            return Err(ProviderError::Status {
                subject: GATEWAY_SUBJECT,
                status: status.as_u16(),
                body: read_bounded(response, MAX_ERROR_BODY_BYTES).await,
                retryable: is_retryable_status(status.as_u16()),
                retry_after,
            });
        }

        let mut reader = SseReader::with_cancel(self.cancel.clone());
        let mut stream = response.bytes_stream();
        loop {
            // Waiting for the *next* chunk is where a cancelled turn actually
            // spends its time: a stream that has started answering and stopped
            // is exactly what a user interrupts. Checking the flag only when
            // bytes arrive would mean Ctrl-C said "stopping the turn" and then
            // the turn kept the terminal for as long as the server felt like
            // holding the connection open. So the wait is chopped into short
            // polls; the flag is read between them.
            let chunk = match tokio::time::timeout(CANCEL_POLL, stream.next()).await {
                Ok(Some(chunk)) => chunk,
                // End of body. Whether that is a completion or a truncation is
                // the decoder's judgement, not the transport's.
                Ok(None) => break,
                Err(_elapsed) => {
                    if self.cancel.is_cancelled() {
                        return Err(ProviderError::Cancelled);
                    }
                    continue;
                }
            };
            if self.cancel.is_cancelled() {
                return Err(ProviderError::Cancelled);
            }
            let chunk = chunk.map_err(|err| ProviderError::Transport {
                subject: GATEWAY_SUBJECT,
                detail: err.to_string(),
            })?;
            reader.push(&chunk, deltas)?;
            if reader.is_complete() {
                // The model finished; the rest of the body is trailer.
                break;
            }
        }
        Ok(reader.finish()?)
    }
}

/// The server's requested backoff, in `Retry-After` delta-seconds.
///
/// Only the delta-seconds form is read, matching upstream, which parses the
/// header as an integer and treats anything else as absent
/// (`vercel-labs/fx@580a0c5d src/gateway/client.zig:1838-1846`). An HTTP-date
/// value therefore reads as "the server said nothing" and the turn falls back to
/// its own backoff, which is slower than obeying the date but never faster --
/// the failure mode of not parsing dates is politeness, not a thundering herd.
///
/// A value too large for `u64` is reported as [`Duration::MAX`]; the turn caps
/// every delay anyway, so an absurd number cannot become an absurd wait.
pub(crate) fn parse_retry_after(headers: &HeaderMap) -> Option<Duration> {
    let raw = headers.get(reqwest::header::RETRY_AFTER)?.to_str().ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() || !trimmed.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some(
        trimmed
            .parse::<u64>()
            .map(Duration::from_secs)
            .unwrap_or(Duration::MAX),
    )
}

/// Classifies one `reqwest` send failure for `subject`.
///
/// The line it draws is the one the turn depends on: a connection that never
/// opened provably delivered nothing, and anything past connection setup may
/// already have been received and acted on.
pub(crate) fn transport_failure(subject: &'static str, err: &reqwest::Error) -> ProviderError {
    let detail = err.to_string();
    if err.is_connect() {
        ProviderError::Connect { subject, detail }
    } else {
        ProviderError::Transport { subject, detail }
    }
}

/// Statuses the Gateway edge returns without having produced a completion
/// (`vercel-labs/fx@580a0c5d src/gateway/client.zig:1810-1820`).
pub(crate) fn is_retryable_status(status: u16) -> bool {
    matches!(status, 429 | 500 | 502 | 503 | 504)
}

/// Reads at most `limit` bytes of a failed response body.
///
/// A failing endpoint is exactly the one whose body length cannot be trusted,
/// so the quote shown to the user is bounded.
pub(crate) async fn read_bounded(response: reqwest::Response, limit: usize) -> String {
    let mut collected: Vec<u8> = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(Ok(chunk)) = stream.next().await {
        let remaining = limit.saturating_sub(collected.len());
        if remaining == 0 {
            break;
        }
        let take = remaining.min(chunk.len());
        collected.extend_from_slice(&chunk[..take]);
    }
    String::from_utf8_lossy(&collected).trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::{Message, ToolChoice};

    #[test]
    fn the_default_endpoint_is_used_when_nothing_overrides_it() {
        assert_eq!(Endpoint::resolve(None).unwrap().url(), DEFAULT_CHAT_URL);
    }

    #[test]
    fn a_url_parses_into_the_parts_the_safety_rule_needs() {
        let parsed = ParsedUrl::parse("http://127.0.0.1:8080/v3/ai").expect("parse");
        assert_eq!(parsed.scheme, "http");
        assert_eq!(parsed.host, "127.0.0.1");
        assert!(parsed.has_port && !parsed.has_userinfo);

        let parsed = ParsedUrl::parse("HTTPS://Example.com/v3").expect("parse");
        assert_eq!(parsed.scheme, "https", "the scheme is compared lowercased");
        assert!(!parsed.has_port);

        let bracketed = ParsedUrl::parse("http://[::1]:9/v3").expect("parse");
        assert_eq!(bracketed.host, "[::1]");
        assert!(bracketed.has_port);

        assert!(!ParsedUrl::parse("http://[::1]/v3").expect("parse").has_port);
        assert!(ParsedUrl::parse("http:///v3").is_none(), "no host");
        assert!(ParsedUrl::parse("//127.0.0.1:8080").is_none(), "no scheme");
    }

    #[test]
    fn a_refusal_names_the_setting_that_supplied_the_url() {
        // The rule is one rule, but it now guards more than one knob: the
        // environment override and the `llmux_url` setting. A refusal has to
        // name the one the user actually wrote, or it sends them to edit a
        // variable they never set.
        let message = Endpoint::checked("http://example.com/v1", "llmux_url")
            .expect_err("a non-loopback http url is refused")
            .to_string();
        assert!(message.contains("llmux_url"), "{message}");
        assert!(!message.contains(GATEWAY_URL_ENV), "{message}");

        let message = Endpoint::resolve(Some("http://example.com/v3"))
            .expect_err("the same rule refuses the override")
            .to_string();
        assert!(message.contains(GATEWAY_URL_ENV), "{message}");
    }

    #[test]
    fn the_loopback_service_policy_refuses_a_remote_host_whatever_the_scheme() {
        // The bearer rule lets https go anywhere, because TLS is what it is
        // protecting. A local daemon is a different promise: xfx tells the
        // operator the request is keyless *because* it never leaves the machine,
        // so a remote host has to be refused rather than trusted to TLS.
        for url in [
            "https://collector.example.com:443",
            "https://collector.example.com",
            "http://198.51.100.7:3456",
            "http://127.0.0.1.example.com:3456",
        ] {
            let err = Endpoint::checked_with(url, "llmux_url", EndpointPolicy::LoopbackService)
                .expect_err("`{url}` is not on this machine");
            assert!(
                matches!(err, EndpointError::NotLoopback { .. }),
                "`{url}` got {err:?}"
            );
            let message = err.to_string();
            assert!(message.contains("llmux_url"), "{message}");
            assert!(message.contains("remote"), "{message}");
        }
        // The same URLs are still fine for the bearer rule, which is a different
        // question about a different endpoint.
        assert!(Endpoint::checked("https://collector.example.com", GATEWAY_URL_ENV).is_ok());
    }

    #[test]
    fn the_loopback_service_policy_accepts_https_on_loopback() {
        for url in [
            "http://127.0.0.1:3456",
            "https://127.0.0.1:3456",
            "http://localhost:3456",
            "http://[::1]:3456",
            "http://127.0.0.1:3456/",
        ] {
            assert!(
                Endpoint::checked_with(url, "llmux_url", EndpointPolicy::LoopbackService).is_ok(),
                "`{url}` is a local service"
            );
        }
        // A port is still required: an implicit one names a service by accident.
        assert!(matches!(
            Endpoint::checked_with(
                "http://127.0.0.1",
                "llmux_url",
                EndpointPolicy::LoopbackService
            ),
            Err(EndpointError::NotLoopback { .. })
        ));
        // And the scheme set is still closed.
        assert!(matches!(
            Endpoint::checked_with(
                "ftp://127.0.0.1:3456",
                "llmux_url",
                EndpointPolicy::LoopbackService
            ),
            Err(EndpointError::UnsupportedScheme { .. })
        ));
    }

    #[test]
    fn a_local_service_base_url_may_not_carry_a_path() {
        // `http://127.0.0.1:3456/v1` plus the provider's own `/v1/messages`
        // makes `/v1/v1/messages`, which llmux does not match and therefore
        // forwards upstream -- keyless -- surfacing as a confusing 401.
        for url in [
            "http://127.0.0.1:3456/v1",
            "http://127.0.0.1:3456/v1/messages",
            "http://127.0.0.1:3456/?x=1",
            "http://127.0.0.1:3456/#frag",
        ] {
            let err = Endpoint::checked_with(url, "llmux_url", EndpointPolicy::LoopbackService)
                .expect_err("a base url carries no path");
            assert!(
                matches!(err, EndpointError::UnexpectedPath { .. }),
                "`{url}` got {err:?}"
            );
            assert!(err.to_string().contains("llmux_url"), "{err}");
        }
    }

    #[test]
    fn a_loopback_http_url_needs_an_explicit_port() {
        // Upstream requires the port (`src/gateway/client.zig:1792`), which
        // keeps a local test endpoint from being named by accident.
        assert!(matches!(
            Endpoint::resolve(Some("http://localhost/v3")),
            Err(EndpointError::NonLoopbackHttp { .. })
        ));
        assert!(Endpoint::resolve(Some("http://localhost:1/v3")).is_ok());
    }

    #[test]
    fn userinfo_after_an_at_sign_in_the_path_is_not_mistaken_for_credentials() {
        let endpoint =
            Endpoint::resolve(Some("https://gateway.example.com/v3/a@b")).expect("accepted");
        assert_eq!(endpoint.url(), "https://gateway.example.com/v3/a@b");
    }

    #[test]
    fn surrounding_whitespace_in_an_override_is_ignored() {
        let endpoint = Endpoint::resolve(Some("  https://gateway.example.com/v3  ")).unwrap();
        assert_eq!(endpoint.url(), "https://gateway.example.com/v3");
    }

    fn retry_after_header(value: &str) -> Option<Duration> {
        let mut headers = HeaderMap::new();
        headers.insert(
            reqwest::header::RETRY_AFTER,
            HeaderValue::from_str(value).expect("a testable header value"),
        );
        parse_retry_after(&headers)
    }

    #[test]
    fn a_retry_after_delta_seconds_value_is_read() {
        assert_eq!(retry_after_header("1"), Some(Duration::from_secs(1)));
        assert_eq!(retry_after_header("  30  "), Some(Duration::from_secs(30)));
        assert_eq!(retry_after_header("0"), Some(Duration::ZERO));
    }

    #[test]
    fn a_retry_after_value_that_is_not_delta_seconds_reads_as_absent() {
        // Upstream parses the header as an integer and ignores anything else
        // (`src/gateway/client.zig:1838-1846`), so an HTTP-date is not obeyed.
        for value in [
            "",
            "   ",
            "-5",
            "1.5",
            "soon",
            "Wed, 21 Oct 2026 07:28:00 GMT",
        ] {
            assert_eq!(retry_after_header(value), None, "for `{value}`");
        }
        assert_eq!(parse_retry_after(&HeaderMap::new()), None);
    }

    #[test]
    fn an_unrepresentable_retry_after_is_reported_as_the_longest_possible_wait() {
        // The caller caps every delay, so an absurd number becomes the cap
        // rather than an absurd wait or a silently ignored header.
        assert_eq!(
            retry_after_header("99999999999999999999999999"),
            Some(Duration::MAX)
        );
    }

    #[test]
    fn only_a_status_error_carries_a_server_delay() {
        assert_eq!(
            ProviderError::Status {
                subject: GATEWAY_SUBJECT,
                status: 429,
                body: String::new(),
                retryable: true,
                retry_after: Some(Duration::from_secs(2)),
            }
            .retry_after(),
            Some(Duration::from_secs(2))
        );
        assert_eq!(
            ProviderError::Connect {
                subject: GATEWAY_SUBJECT,
                detail: "refused".to_string()
            }
            .retry_after(),
            None
        );
    }

    #[test]
    fn a_status_message_names_the_server_delay_it_will_wait_for() {
        let message = ProviderError::Status {
            subject: GATEWAY_SUBJECT,
            status: 429,
            body: "slow down".to_string(),
            retryable: true,
            retry_after: Some(Duration::from_secs(2)),
        }
        .to_string();
        assert!(message.contains("429"), "{message}");
        assert!(message.contains("retry after 2s"), "{message}");
        assert!(message.contains("slow down"), "{message}");
    }

    #[test]
    fn only_edge_statuses_are_replayable() {
        for status in [429, 500, 502, 503, 504] {
            assert!(is_retryable_status(status), "{status}");
        }
        for status in [400, 401, 403, 404, 409, 422, 501] {
            assert!(!is_retryable_status(status), "{status}");
        }
    }

    #[test]
    fn only_a_connection_setup_failure_and_an_edge_status_are_replayable() {
        assert!(ProviderError::Connect {
            subject: GATEWAY_SUBJECT,
            detail: "refused".to_string()
        }
        .is_replayable());
        assert!(ProviderError::Status {
            subject: GATEWAY_SUBJECT,
            status: 503,
            body: String::new(),
            retryable: true,
            retry_after: None,
        }
        .is_replayable());
        for err in [
            ProviderError::Transport {
                subject: GATEWAY_SUBJECT,
                detail: "reset".to_string(),
            },
            ProviderError::Status {
                subject: GATEWAY_SUBJECT,
                status: 401,
                body: String::new(),
                retryable: false,
                retry_after: None,
            },
            ProviderError::Protocol(SseError::MissingFinish),
            ProviderError::Cancelled,
            ProviderError::Sink(io::Error::other("closed")),
        ] {
            assert!(!err.is_replayable(), "{err} must not be replayed");
        }
    }

    #[test]
    fn a_sink_failure_from_the_decoder_is_not_reported_as_a_protocol_failure() {
        let converted = ProviderError::from(SseError::Sink(io::Error::other("closed")));
        assert!(matches!(converted, ProviderError::Sink(_)));
        assert!(matches!(
            ProviderError::from(SseError::Cancelled),
            ProviderError::Cancelled
        ));
        assert!(matches!(
            ProviderError::from(SseError::MissingFinish),
            ProviderError::Protocol(SseError::MissingFinish)
        ));
    }

    /// A sink for tests that do not inspect the assistant text.
    struct NullDeltas;

    impl DeltaSink for NullDeltas {
        fn text_delta(&mut self, _text: &str) -> io::Result<()> {
            Ok(())
        }
    }

    /// A credential built through the real configuration path, so the test
    /// cannot construct one the product could not.
    fn test_credential() -> Credential {
        let workspace = tempfile::tempdir().expect("temp workspace");
        let env = crate::config::Environment::new(
            None,
            std::collections::BTreeMap::from([(
                "AI_GATEWAY_API_KEY".to_string(),
                "test-key".to_string(),
            )]),
        );
        crate::config::RuntimeConfig::load_with(&env, workspace.path())
            .expect("load config")
            .credential
            .expect("credential resolved")
    }

    #[tokio::test]
    async fn an_invalid_prompt_fails_before_the_socket_opens() {
        let provider = GatewayProvider::new(
            // A URL that would fail loudly if it were ever contacted.
            Endpoint::resolve(Some("http://127.0.0.1:1/v3")).unwrap(),
            test_credential(),
            CancelToken::new(),
        )
        .expect("build the provider");

        let request = CompletionRequest {
            model: "vendor/model".to_string(),
            messages: vec![Message::tool_result("orphan", "read_file", "x")],
            tools: Vec::new(),
            tool_choice: ToolChoice::None,
        };
        let mut deltas = NullDeltas;
        let err = provider
            .stream(&request, &mut deltas)
            .await
            .expect_err("an orphan tool result is not sendable");
        assert!(matches!(
            err,
            ProviderError::Request {
                source: ProtocolError::UnmatchedToolResult { .. },
                ..
            }
        ));
    }

    #[test]
    fn a_reset_token_is_shared_with_every_clone_of_itself() {
        let cancel = CancelToken::new();
        let clone = cancel.clone();
        cancel.cancel();
        assert!(clone.is_cancelled());
        // The shell resets between turns, and the tool context that holds a
        // clone has to see it: a copy that stayed cancelled would refuse every
        // later tool call in the same session.
        clone.reset();
        assert!(!cancel.is_cancelled());
        assert!(!clone.is_cancelled());
    }

    #[tokio::test]
    async fn a_cancelled_provider_does_not_open_a_socket() {
        let cancel = CancelToken::new();
        cancel.cancel();
        let provider = GatewayProvider::new(
            Endpoint::resolve(Some("http://127.0.0.1:1/v3")).unwrap(),
            test_credential(),
            cancel,
        )
        .expect("build the provider");
        let request = CompletionRequest {
            model: "vendor/model".to_string(),
            messages: vec![Message::user("hi")],
            tools: Vec::new(),
            tool_choice: ToolChoice::None,
        };
        let mut deltas = NullDeltas;
        assert!(matches!(
            provider.stream(&request, &mut deltas).await,
            Err(ProviderError::Cancelled)
        ));
    }

    #[tokio::test]
    async fn a_model_name_that_cannot_be_a_header_is_rejected() {
        let provider = GatewayProvider::new(
            Endpoint::resolve(Some("http://127.0.0.1:1/v3")).unwrap(),
            test_credential(),
            CancelToken::new(),
        )
        .expect("build the provider");
        let request = CompletionRequest {
            model: "vendor/\nmodel".to_string(),
            messages: vec![Message::user("hi")],
            tools: Vec::new(),
            tool_choice: ToolChoice::None,
        };
        let mut deltas = NullDeltas;
        assert!(matches!(
            provider.stream(&request, &mut deltas).await,
            Err(ProviderError::InvalidHeader {
                name: "ai-language-model-id"
            })
        ));
    }
}
