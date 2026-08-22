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
//! same timeouts, the same bounded error body, and the same `Retry-After`
//! reading.
//!
//! What they do **not** share is which URL may receive a request. The Gateway's
//! rule protects a credential, so https is acceptable anywhere. This one has no
//! credential to protect, and the only reason that is safe is that the request
//! never leaves the machine -- so the endpoint must be a loopback address with
//! an explicit port and no path, whatever the scheme. See [`endpoint`].

pub mod protocol;

pub mod setup;
pub mod sse;

use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

use crate::gateway::protocol::{Completion, CompletionRequest};
use crate::gateway::{
    build_loopback_client, is_retryable_status, parse_retry_after, read_bounded, CancelToken,
    DeltaSink, Endpoint, EndpointError, EndpointPolicy, Provider, ProviderError, CANCEL_POLL,
    MAX_ERROR_BODY_BYTES,
};
use futures_util::StreamExt;
use sse::AnthropicReader;

/// Where a llmux daemon listens unless the operator moved it.
pub const DEFAULT_URL: &str = "http://127.0.0.1:3456";

/// The Anthropic API version xfx pins.
///
/// It is required rather than optional: llmux forwards a path it does not
/// recognize to `api.anthropic.com`, which answers 400 without this header, so
/// omitting it would turn a moved endpoint into an unreadable failure.
pub const ANTHROPIC_VERSION: &str = "2023-06-01";

/// What xfx tells the user to run when the backend has no endpoint to use.
pub const SETUP_HINT: &str = "run `xfx setup llmux` to point xfx at the local llmux daemon";

/// The settings key that names the daemon, and the subject of its refusals.
pub const URL_KEY: &str = "llmux_url";

/// Accepts a URL that may be used as an llmux base address.
///
/// **The single gate.** Configuration, `setup --url`, and provider construction
/// all come through here, so the policy cannot be enforced in one of them and
/// forgotten in another -- which is exactly how a remote URL would reach the
/// wire while `status` went on reporting a keyless loopback arrangement.
///
/// The policy is [`EndpointPolicy::LoopbackService`] rather than the bearer rule
/// the Gateway uses: this request carries the prompt and the project context
/// with **no credential**, and the only reason that is safe is that it does not
/// leave the machine. TLS does not substitute for that, so an https host off
/// this machine is refused too. A remote llmux is a feature with its own
/// credential story and xfx does not have one.
pub fn endpoint(url: &str, subject: &'static str) -> Result<Endpoint, EndpointError> {
    Endpoint::checked_with(url, subject, EndpointPolicy::LoopbackService)
}

/// The message a turn fails with when `backend` is `llmux` and no url resolved.
pub const MISSING_URL_HELP: &str = "xfx is configured to use the llmux backend but no valid \
     `llmux_url` is set; run `xfx setup llmux` to point xfx at the local llmux daemon";

/// The path the Anthropic Messages data plane lives at.
const MESSAGES_PATH: &str = "/v1/messages";

/// Streams completions from a llmux daemon over the Anthropic Messages wire.
pub struct LlmuxProvider {
    client: reqwest::Client,
    /// The daemon's base URL, without a trailing slash.
    base: String,
    cancel: CancelToken,
}

impl LlmuxProvider {
    pub fn new(endpoint: Endpoint, cancel: CancelToken) -> Result<Self, ProviderError> {
        Ok(Self {
            client: build_loopback_client()?,
            base: trim_base(endpoint.url()),
            cancel,
        })
    }

    /// The completion endpoint this provider will post to.
    pub fn messages_url(&self) -> String {
        format!("{}{MESSAGES_PATH}", self.base)
    }

    /// The fixed headers every completion request carries.
    ///
    /// There is deliberately no `authorization` and no `x-api-key`. A keyless
    /// loopback request is the credential story; sending an empty or invented
    /// one would make llmux treat the request as an authenticated tenant's and
    /// refuse it, and sending a real one would mean xfx had read a secret it has
    /// no reason to touch.
    fn headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        headers.insert(
            reqwest::header::ACCEPT,
            HeaderValue::from_static("text/event-stream"),
        );
        headers.insert(
            HeaderName::from_static("anthropic-version"),
            HeaderValue::from_static(ANTHROPIC_VERSION),
        );
        headers
    }
}

/// A base URL with any trailing `/` removed, so joining a path cannot double it.
fn trim_base(url: &str) -> String {
    url.trim_end_matches('/').to_string()
}

#[async_trait::async_trait(?Send)]
impl Provider for LlmuxProvider {
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
        let body = protocol::body(request).map_err(ProviderError::Request)?;

        let response = self
            .client
            .post(self.messages_url())
            .headers(self.headers())
            .body(body)
            .send()
            .await
            .map_err(|err| {
                let detail = err.to_string();
                if err.is_connect() {
                    ProviderError::Connect { detail }
                } else {
                    // Anything past connection setup may already have been
                    // received and acted on.
                    ProviderError::Transport { detail }
                }
            })?;

        let status = response.status();
        if !status.is_success() {
            // Read the header before the body: consuming the response moves it.
            let retry_after = parse_retry_after(response.headers());
            return Err(ProviderError::Status {
                status: status.as_u16(),
                body: read_bounded(response, MAX_ERROR_BODY_BYTES).await,
                retryable: is_retryable_status(status.as_u16()),
                retry_after,
            });
        }

        let mut reader = AnthropicReader::with_cancel(self.cancel.clone());
        let mut stream = response.bytes_stream();
        loop {
            // The same chopped wait the Gateway transport uses: a stream that
            // has started answering and stopped is exactly what a user
            // interrupts, and the flag has to be readable between chunks rather
            // than only when bytes arrive.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gateway::protocol::{Message, ProtocolError, ToolChoice};
    use std::io;

    struct NullDeltas;

    impl DeltaSink for NullDeltas {
        fn text_delta(&mut self, _text: &str) -> io::Result<()> {
            Ok(())
        }
    }

    fn provider(url: &str) -> LlmuxProvider {
        LlmuxProvider::new(
            endpoint(url, URL_KEY).expect("a loopback url"),
            CancelToken::new(),
        )
        .expect("build the provider")
    }

    #[test]
    fn the_messages_path_is_joined_without_doubling_a_slash() {
        assert_eq!(
            provider("http://127.0.0.1:3456").messages_url(),
            "http://127.0.0.1:3456/v1/messages"
        );
        assert_eq!(
            provider("http://127.0.0.1:3456/").messages_url(),
            "http://127.0.0.1:3456/v1/messages"
        );
    }

    #[test]
    fn no_request_header_can_carry_a_credential() {
        let headers = provider(DEFAULT_URL).headers();
        assert!(headers.get(reqwest::header::AUTHORIZATION).is_none());
        assert!(headers.get("x-api-key").is_none());
        assert_eq!(
            headers.get("anthropic-version").unwrap(),
            ANTHROPIC_VERSION,
            "llmux forwards an unmatched path upstream, which 400s without it"
        );
    }

    #[tokio::test]
    async fn an_invalid_prompt_fails_before_the_socket_opens() {
        // A URL that would fail loudly if it were ever contacted.
        let provider = provider("http://127.0.0.1:1");
        let request = CompletionRequest {
            model: "fable".to_string(),
            messages: vec![Message::tool_result("orphan", "read_file", "x")],
            tools: Vec::new(),
            tool_choice: ToolChoice::None,
        };
        let err = provider
            .stream(&request, &mut NullDeltas)
            .await
            .expect_err("an orphan tool result is not sendable");
        assert!(matches!(
            err,
            ProviderError::Request(ProtocolError::UnmatchedToolResult { .. })
        ));
    }

    #[tokio::test]
    async fn a_cancelled_provider_does_not_open_a_socket() {
        let cancel = CancelToken::new();
        cancel.cancel();
        let provider = LlmuxProvider::new(endpoint("http://127.0.0.1:1", URL_KEY).unwrap(), cancel)
            .expect("build the provider");
        let request = CompletionRequest {
            model: "fable".to_string(),
            messages: vec![Message::user("hi")],
            tools: Vec::new(),
            tool_choice: ToolChoice::None,
        };
        assert!(matches!(
            provider.stream(&request, &mut NullDeltas).await,
            Err(ProviderError::Cancelled)
        ));
    }
}
