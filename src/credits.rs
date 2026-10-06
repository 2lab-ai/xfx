//! `/credits`: what the Gateway account has left.
//!
//! Upstream's `commandShowCredits` asks the Gateway one authenticated question
//! -- `GET {origin}/coding-agent/v1/credits` with the bearer a turn sends --
//! and prints the three string fields it reads back, `balance`, `used` and
//! `plan`, one per line under a `credits` notice (`vercel-labs/fx@c1db9191
//! src/builtins/gateway.zig:725-822`, `src/core/output/output_contracts.zig:1513-1541`).
//! A provider with no such account answers with a sentence instead
//! (`gateway_provider.zig:89-101`).
//!
//! **llmux has no balance to ask for.** The daemon's routes
//! (`llmux/src/proxy/server.rs:1583-1613` at `4b3d4a4`) carry nothing that is
//! one: `/llmux/reset-credits?account=` is a single account's rate-limit reset
//! entitlements, read from the upstream provider on each call, and
//! `/llmux/keys/usage` is per-proxy-key usage priced by llmux's own estimate
//! table. Neither is the money left on an account, so `/credits` on llmux says
//! upstream's sentence for a provider without credits.
//!
//! The answer is worked out here and nowhere else, so the full-screen shell
//! (which asks it on the runtime thread, never the one holding the terminal)
//! and the line shell say the same thing.

use std::time::Duration;

use serde_json::Value;

use crate::config::RuntimeConfig;
use crate::gateway::{read_bounded, Endpoint, USER_AGENT};
use crate::provider::ProviderId;

/// What every `/credits` notice is filed under.
pub const TOPIC: &str = "credits";

/// Where the Gateway answers it, under the origin a turn is sent to
/// (`credits_path`, `gateway.zig:49`).
pub const CREDITS_PATH: &str = "/coding-agent/v1/credits";

/// What a provider with no credits account says (`fetchCreditsUnavailable`,
/// `gateway_provider.zig:89-101`).
pub const UNAVAILABLE: &str = "Credits are unavailable for the selected provider.";

/// What an answer with none of the three fields says
/// (`renderInteractiveBody`, `output_contracts.zig:1535-1539`).
pub const NO_DATA: &str = "no data returned by gateway";

/// What a body that is not JSON says (`gateway.zig:796-798`).
const INVALID_JSON: &str = "invalid JSON response from gateway";

/// What JSON that is not an object says (`gateway.zig:807-809`).
const NOT_AN_OBJECT: &str = "unexpected response format from gateway";

/// What a request that never got an answer says (`gateway.zig:780-782`).
const FETCH_FAILED: &str = "failed to fetch credits from gateway";

/// The most of an answer that is read. Three short strings; anything past this
/// is not a credits answer.
const MAX_BODY_BYTES: usize = 64 * 1024;

/// The most of a quoted error body that is shown (`max_published_error_bytes`,
/// `gateway_error_format.zig:6`).
const MAX_ERROR_BYTES: usize = 1024;

/// How long the question may take to connect, and to be answered. Bounded,
/// because the thread that waits for it is the one a turn runs on.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const TIMEOUT: Duration = Duration::from_secs(30);

/// What `/credits` found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// The account's fields, each as the Gateway wrote it.
    Credits {
        balance: Option<String>,
        used: Option<String>,
        plan: Option<String>,
    },
    /// Why there is nothing to show.
    Failed(String),
}

impl Answer {
    /// Whether this is said in the error tone.
    pub fn is_error(&self) -> bool {
        matches!(self, Self::Failed(_))
    }

    /// The notice's body: one `key=value` line per field the Gateway sent, in
    /// upstream's order, or the reason there are none
    /// (`renderInteractiveBody`, `output_contracts.zig:1513-1541`).
    pub fn body(&self) -> String {
        match self {
            Self::Failed(reason) => reason.clone(),
            Self::Credits {
                balance,
                used,
                plan,
            } => {
                let lines: Vec<String> = [("balance", balance), ("used", used), ("plan", plan)]
                    .into_iter()
                    .filter_map(|(key, value)| value.as_ref().map(|value| format!("{key}={value}")))
                    .collect();
                if lines.is_empty() {
                    NO_DATA.to_string()
                } else {
                    lines.join("\n")
                }
            }
        }
    }

    /// The line shell's rendering: `[credits] key=value`, or
    /// `[credits] error: …` (`renderText`, `output_contracts.zig:1483-1511`).
    pub fn render_text(&self) -> String {
        match self {
            Self::Failed(reason) => format!("[{TOPIC}] error: {reason}\n"),
            Self::Credits { .. } => self
                .body()
                .lines()
                .map(|line| format!("[{TOPIC}] {line}\n"))
                .collect(),
        }
    }
}

/// The credits URL for a completion endpoint: its scheme and authority, and
/// [`CREDITS_PATH`].
///
/// Derived rather than configured, as the model catalog's is: the question goes
/// to exactly the host a turn already sends the same credential to, so no new
/// place is trusted with the key.
pub fn credits_url(endpoint: &Endpoint) -> String {
    let url = endpoint.url();
    let authority = url.find("://").map_or(0, |at| at + 3);
    let end = url[authority..]
        .find(['/', '?', '#'])
        .map_or(url.len(), |at| authority + at);
    format!("{}{CREDITS_PATH}", &url[..end])
}

/// Asks the configured provider what the account has left.
///
/// The Gateway is asked only **with** a credential, and only at the endpoint a
/// turn would use: xfx does not contact a remote host on its own initiative,
/// and a Gateway with no credential could not run a turn either.
pub async fn fetch(config: &RuntimeConfig) -> Answer {
    match config.provider {
        ProviderId::Llmux => Answer::Failed(UNAVAILABLE.to_string()),
        ProviderId::Gateway => {
            let Some(credential) = config.credential.as_ref() else {
                return Answer::Failed(crate::output::MISSING_AUTH_HELP.to_string());
            };
            let endpoint = match Endpoint::from_process() {
                Ok(endpoint) => endpoint,
                Err(err) => return Answer::Failed(err.to_string()),
            };
            ask(&credits_url(&endpoint), credential.secret()).await
        }
    }
}

/// One authenticated `GET`, bounded while it reads.
///
/// **No redirect is followed**: the URL was judged by the bearer rule and the
/// key goes to it alone, so a `3xx` is an answer that is not a credits answer
/// rather than a second host the key is replayed to.
async fn ask(url: &str, secret: &str) -> Answer {
    let failed = || Answer::Failed(FETCH_FAILED.to_string());
    let Ok(client) = reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(USER_AGENT)
        .build()
    else {
        return failed();
    };
    let Ok(mut authorization) = reqwest::header::HeaderValue::from_str(&format!("Bearer {secret}"))
    else {
        return failed();
    };
    // Keeps the credential out of reqwest's own diagnostics.
    authorization.set_sensitive(true);
    let Ok(response) = client
        .get(url)
        .header(reqwest::header::AUTHORIZATION, authorization)
        .send()
        .await
    else {
        return failed();
    };
    let status = response.status().as_u16();
    let body = read_bounded(response, MAX_BODY_BYTES).await;
    answer(status, &body, secret)
}

/// What a Gateway answer of `status` with `body` says.
///
/// `secret` is the credential the question was asked with: an endpoint that
/// echoes it into an error body does not get it printed.
pub fn answer(status: u16, body: &str, secret: &str) -> Answer {
    if status != 200 {
        return Answer::Failed(http_error(status, &mask(body, secret)));
    }
    let Ok(value) = serde_json::from_str::<Value>(body) else {
        return Answer::Failed(INVALID_JSON.to_string());
    };
    let Some(object) = value.as_object() else {
        return Answer::Failed(NOT_AN_OBJECT.to_string());
    };
    // Only strings: a number in one of these fields is not the shape upstream
    // reads, and is left out rather than reformatted (`gateway.zig:800-812`).
    let field = |key: &str| {
        object
            .get(key)
            .and_then(Value::as_str)
            .map(|text| mask(text, secret))
    };
    Answer::Credits {
        balance: field("balance"),
        used: field("used"),
        plan: field("plan"),
    }
}

/// `text` with every occurrence of the credential replaced.
fn mask(text: &str, secret: &str) -> String {
    if secret.is_empty() {
        text.to_string()
    } else {
        text.replace(secret, "[redacted]")
    }
}

/// A failed status, the way upstream words one (`formatHttpErrorMessage`,
/// `vercel-labs/fx@c1db9191 src/core/shared/gateway_error_format.zig:64-145`).
///
/// A body that is the Gateway's JSON error says what it says, behind a title
/// -- `API access denied` for 401 and 403, `API request failed` otherwise --
/// as `title · HTTP n[ · Provider: p][ · code[: message]]`. Anything else is
/// quoted as it came, `HTTP n: body`, on one line and bounded; an empty body
/// is the status alone.
pub fn http_error(status: u16, body: &str) -> String {
    if body.is_empty() {
        return format!("HTTP {status}");
    }
    let parsed = serde_json::from_str::<Value>(body).ok();
    let Some(error) = parsed
        .as_ref()
        .and_then(Value::as_object)
        .and_then(|root| root.get("error"))
        .and_then(Value::as_object)
    else {
        return safe(&format!("HTTP {status}: {body}"));
    };
    let text = |object: &serde_json::Map<String, Value>, key: &str| {
        object.get(key).and_then(Value::as_str).map(str::to_string)
    };
    let param = error.get("param").and_then(Value::as_object);
    let code = text(error, "code")
        .or_else(|| text(error, "type"))
        .or_else(|| param.and_then(|param| text(param, "name")));
    let message = text(error, "message").or_else(|| param.and_then(|param| text(param, "message")));
    let provider = text(error, "provider").or_else(|| provider_named_in(message.as_deref()));
    let title = if status == 401 || status == 403 {
        "API access denied"
    } else {
        "API request failed"
    };
    let mut out = format!("{title} · HTTP {status}");
    if let Some(provider) = provider {
        out.push_str(&format!(" · Provider: {provider}"));
    }
    match (code, message) {
        (Some(code), Some(message)) if code != message => {
            out.push_str(&format!(" · {code}: {message}"))
        }
        (Some(code), _) => out.push_str(&format!(" · {code}")),
        (None, Some(message)) => out.push_str(&format!(" · {message}")),
        (None, None) => {}
    }
    safe(&out)
}

/// The provider a Gateway message names after `Providers considered: `, up to
/// the first full stop or line break (`providerFromGatewayMessage`).
fn provider_named_in(message: Option<&str>) -> Option<String> {
    let text = message?;
    let start = text.find("Providers considered: ")? + "Providers considered: ".len();
    let rest = text[start..].trim_start();
    let end = rest.find(['.', '\n', '\r', '\t']).unwrap_or(rest.len());
    (end > 0).then(|| rest[..end].to_string())
}

/// Text from the other end of a socket, made one bounded line.
fn safe(text: &str) -> String {
    crate::output::safe_one_line(text, MAX_ERROR_BYTES)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "xfx-credits-test-key-must-not-appear";

    #[test]
    fn three_fields_are_three_lines_in_upstreams_order() {
        let answer = answer(
            200,
            r#"{"plan":"pro","used":"7.83","balance":"42.17"}"#,
            KEY,
        );
        assert!(!answer.is_error());
        assert_eq!(answer.body(), "balance=42.17\nused=7.83\nplan=pro");
        assert_eq!(
            answer.render_text(),
            "[credits] balance=42.17\n[credits] used=7.83\n[credits] plan=pro\n"
        );
    }

    #[test]
    fn a_field_upstream_does_not_read_is_not_printed() {
        // `credits-partial`: `total_used` is not `used`.
        let answer = answer(200, r#"{"balance":"42.17","total_used":"7.83"}"#, KEY);
        assert_eq!(answer.body(), "balance=42.17");
        // A number where a string belongs is left out, not reformatted.
        let answer = super::answer(200, r#"{"balance":42.17,"plan":"pro"}"#, KEY);
        assert_eq!(answer.body(), "plan=pro");
    }

    #[test]
    fn an_answer_with_nothing_in_it_says_so() {
        let answer = answer(200, "{}", KEY);
        assert!(!answer.is_error());
        assert_eq!(answer.body(), "no data returned by gateway");
    }

    #[test]
    fn what_is_not_a_credits_answer_is_an_error_in_upstreams_words() {
        assert_eq!(
            answer(200, "not json", KEY),
            Answer::Failed("invalid JSON response from gateway".to_string())
        );
        assert_eq!(
            answer(200, "[1,2]", KEY),
            Answer::Failed("unexpected response format from gateway".to_string())
        );
        assert_eq!(
            answer(404, "not found", KEY),
            Answer::Failed("HTTP 404: not found".to_string())
        );
        assert_eq!(answer(502, "", KEY), Answer::Failed("HTTP 502".to_string()));
        assert_eq!(
            Answer::Failed("HTTP 404: not found".to_string()).render_text(),
            "[credits] error: HTTP 404: not found\n"
        );
    }

    #[test]
    fn a_gateway_error_object_is_titled_by_its_status() {
        assert_eq!(
            http_error(
                401,
                r#"{"error":{"type":"authentication_error","message":"Invalid API key"}}"#
            ),
            "API access denied · HTTP 401 · authentication_error: Invalid API key"
        );
        assert_eq!(
            http_error(429, r#"{"error":{"message":"slow down"}}"#),
            "API request failed · HTTP 429 · slow down"
        );
        assert_eq!(
            http_error(
                500,
                r#"{"error":{"code":"x","message":"No route. Providers considered: openai. Try later"}}"#
            ),
            "API request failed · HTTP 500 · Provider: openai · x: No route. Providers considered: openai. Try later"
        );
        assert_eq!(
            http_error(
                403,
                r#"{"error":{"code":"forbidden","message":"forbidden"}}"#
            ),
            "API access denied · HTTP 403 · forbidden"
        );
    }

    #[test]
    fn an_error_body_is_one_bounded_line_without_the_credential() {
        let answer = answer(400, &format!("bad\nkey {KEY}\n{}", "x".repeat(4000)), KEY);
        let Answer::Failed(reason) = answer else {
            panic!("an error");
        };
        assert!(!reason.contains(KEY), "{reason}");
        assert!(!reason.contains('\n'), "{reason}");
        assert!(reason.len() <= MAX_ERROR_BYTES + 3, "{}", reason.len());
        assert!(
            reason.starts_with("HTTP 400: bad key [redacted]"),
            "{reason}"
        );
    }

    #[test]
    fn the_question_goes_to_the_origin_a_turn_is_sent_to() {
        for (chat, credits) in [
            (
                "https://ai-gateway.vercel.sh/v3/ai/language-model",
                "https://ai-gateway.vercel.sh/coding-agent/v1/credits",
            ),
            (
                "http://127.0.0.1:4100/v3/ai/language-model",
                "http://127.0.0.1:4100/coding-agent/v1/credits",
            ),
        ] {
            let endpoint = Endpoint::resolve(Some(chat)).expect("an endpoint");
            assert_eq!(credits_url(&endpoint), credits);
        }
    }
}
