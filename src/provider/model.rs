//! What a provider says it can run.
//!
//! Two catalog concepts, kept apart the way upstream keeps them: the **static
//! identity** of a provider (`ProviderEntry`, which drives labels) and the
//! **fetched model catalog** below, which is what `/model` renders. Only the
//! second one needs a socket, which is why it is a trait: a provider that
//! advertises no catalog has `None`, not an object that answers nothing.

use std::fmt;
use std::time::Duration;

use serde_json::Value;

use crate::config::RuntimeConfig;
use crate::gateway::{read_bounded, Endpoint, USER_AGENT};
use crate::provider::ProviderId;

/// One model a provider advertises.
///
/// The optional fields are optional because the daemon really may omit them
/// (`2lab-ai/llmux@79f66748656b src/catalog.rs:44-63`: `max_context` is
/// `Option<u64>`, `efforts` is empty when the model takes no reasoning field).
/// Defaulting either would print a promise the provider did not make.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogEntry {
    pub id: String,
    pub aliases: Vec<String>,
    pub name: Option<String>,
    /// Accepted reasoning-effort values, low to high; empty when the model takes
    /// no reasoning field.
    pub efforts: Vec<String>,
    /// Context window in tokens, or `None` when the provider did not publish one.
    pub max_context: Option<u64>,
}

impl CatalogEntry {
    /// Whether `name` selects this entry, as an id or as an alias.
    pub fn matches(&self, name: &str) -> bool {
        self.id == name || self.aliases.iter().any(|alias| alias == name)
    }

    /// How xfx will ask for this model: the first alias when there is one,
    /// because that is the short name the provider publishes and the one an
    /// operator recognizes; the id otherwise. Either resolves at the provider.
    pub fn preferred_name(&self) -> &str {
        self.aliases.first().map(String::as_str).unwrap_or(&self.id)
    }
}

/// Why a catalog could not be read.
#[derive(Debug)]
pub enum CatalogError {
    /// Nothing answered, or what answered was not usable as a catalog source.
    Unavailable { detail: String },
    /// The provider answered a catalog with no models in it.
    Empty,
    /// The body is not a catalog document at all.
    Malformed { detail: String },
}

impl fmt::Display for CatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable { detail } => {
                write!(
                    f,
                    "catalog is unavailable: {}; check the daemon is running at the configured llmux_url, or run `xfx setup llmux` to discover and record it",
                    detail
                )
            }
            Self::Empty => write!(
                f,
                "catalog is empty (no models advertised); check the daemon at the configured llmux_url has models to offer"
            ),
            Self::Malformed { detail } => {
                write!(
                    f,
                    "catalog is malformed: {}; check the daemon is an llmux version compatible with xfx, or rerun `xfx setup llmux`",
                    detail
                )
            }
        }
    }
}

impl std::error::Error for CatalogError {}

/// A source of a provider's model catalog.
///
/// One call is one attempt, like [`crate::gateway::Provider`]: the caller
/// decides whether a failure is worth repeating.
#[async_trait::async_trait(?Send)]
pub trait ModelCatalog {
    async fn fetch(&self) -> Result<Vec<CatalogEntry>, CatalogError>;
}

/// How long the catalog fetch may take to open a connection.
///
/// Short, because the thing being probed is on this machine: a loopback connect
/// that has not completed in three seconds is not slow, it is absent.
const CATALOG_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

/// How long the catalog fetch waits for a local daemon to answer.
const CATALOG_READ_TIMEOUT: Duration = Duration::from_secs(5);

/// The largest catalog response xfx will read.
///
/// The catalog is a list of model descriptors; a body past this is not a catalog
/// and reading it would let whatever is on that port decide how much memory this
/// command uses.
const MAX_CATALOG_BODY_BYTES: usize = 1024 * 1024;

/// Parses a llmux catalog document.
///
/// `None` when the body is not that document at all. An entry without a usable
/// id is skipped rather than invented: a model xfx cannot name is a model it
/// cannot ask for.
pub fn parse_llmux_catalog(body: &str) -> Option<Vec<CatalogEntry>> {
    let Ok(Value::Object(document)) = serde_json::from_str::<Value>(body) else {
        return None;
    };
    let Some(Value::Array(models)) = document.get("models") else {
        return None;
    };
    Some(
        models
            .iter()
            .filter_map(|model| {
                let object = model.as_object()?;
                let id = object.get("id")?.as_str()?;
                if id.is_empty() {
                    return None;
                }
                let aliases = match object.get("aliases") {
                    Some(Value::Array(aliases)) => aliases
                        .iter()
                        .filter_map(Value::as_str)
                        .filter(|alias| !alias.is_empty())
                        .map(str::to_string)
                        .collect(),
                    _ => Vec::new(),
                };
                let name = object
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                let efforts = match object.get("efforts") {
                    Some(Value::Array(efforts)) => efforts
                        .iter()
                        .filter_map(Value::as_str)
                        .filter(|effort| !effort.is_empty())
                        .map(str::to_string)
                        .collect(),
                    _ => Vec::new(),
                };
                let max_context = object.get("max_context").and_then(Value::as_u64);
                Some(CatalogEntry {
                    id: id.to_string(),
                    aliases,
                    name,
                    efforts,
                    max_context,
                })
            })
            .collect(),
    )
}

/// Fetches a bounded response from a loopback URL with short timeouts and no proxy/redirects.
///
/// Used by both catalog fetching and setup probing. The timeouts are the point:
/// what is being probed is on this machine, so a connect that has not completed
/// in three seconds is not slow, it is absent. Bounded while it reads, not
/// after: `read_bounded` streams and stops at the limit, so whatever is on that
/// port cannot decide how much memory is used.
pub(crate) async fn fetch_loopback_bounded(url: &str, max_bytes: usize) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .connect_timeout(CATALOG_CONNECT_TIMEOUT)
        .read_timeout(CATALOG_READ_TIMEOUT)
        .timeout(CATALOG_READ_TIMEOUT)
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(USER_AGENT)
        .build()
        .map_err(|err| format!("could not build HTTP client: {err}"))?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|err| err.to_string())?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("HTTP {}", status.as_u16()));
    }
    Ok(read_bounded(response, max_bytes).await)
}

/// Fetches a catalog from a URL, bounded and with short timeouts.
///
/// The timeouts are the point: what is being probed is on this machine, so a
/// connect that has not completed in three seconds is not slow, it is absent.
/// Bounded while it reads, not after: `read_bounded` streams and stops at the
/// limit, so whatever is on that port cannot decide how much memory this uses.
pub(crate) async fn fetch_catalog(url: &str) -> Result<Vec<CatalogEntry>, CatalogError> {
    let body = fetch_loopback_bounded(url, MAX_CATALOG_BODY_BYTES)
        .await
        .map_err(|err| CatalogError::Unavailable { detail: err })?;
    let entries = parse_llmux_catalog(&body).ok_or_else(|| {
        let clipped = if body.len() > 100 {
            // Safely clip to a UTF-8 character boundary, then show the byte count
            // and first N bytes as evidence (not chars, which is ambiguous with UTF-8).
            let safe_len = body.floor_char_boundary(100);
            format!(
                "({} bytes; first {}: {:?})",
                body.len(),
                safe_len,
                &body[..safe_len]
            )
        } else {
            format!("{:?}", body)
        };
        CatalogError::Malformed {
            detail: format!("response is not a valid catalog document: {}", clipped),
        }
    })?;
    if entries.is_empty() {
        return Err(CatalogError::Empty);
    }
    Ok(entries)
}

/// A catalog fetcher for a llmux daemon.
#[derive(Clone)]
pub struct LlmuxCatalog {
    endpoint: Endpoint,
}

impl LlmuxCatalog {
    /// Creates a new llmux catalog fetcher.
    pub fn new(endpoint: Endpoint) -> Self {
        Self { endpoint }
    }
}

#[async_trait::async_trait(?Send)]
impl ModelCatalog for LlmuxCatalog {
    async fn fetch(&self) -> Result<Vec<CatalogEntry>, CatalogError> {
        let url = format!("{}/models", self.endpoint.url());
        fetch_catalog(&url).await
    }
}

/// The catalog for the configured provider, when it advertises one.
///
/// `None` is a fact about the provider, not a missing feature: the Vercel
/// Gateway publishes no catalog endpoint this port has evidence for, and xfx
/// does not invent a URL to ask. `/model` on that provider therefore reports the
/// model and its source and says there is nothing to browse.
pub fn catalog_for(config: &RuntimeConfig) -> Option<Box<dyn ModelCatalog>> {
    match config.provider {
        ProviderId::Gateway => None,
        ProviderId::Llmux => {
            let url = config.llmux_url.as_deref()?;
            // Re-checked rather than trusted: the rule that decides what may
            // receive a keyless request belongs to the module that made the
            // promise.
            let endpoint = crate::llmux::endpoint(url, crate::llmux::URL_KEY).ok()?;
            Some(Box::new(LlmuxCatalog::new(endpoint)))
        }
    }
}

/// Async variant: fetch catalog from a URL during provider discovery.
/// The discovery phase uses this before the config's provider/url are set.
pub async fn catalog_for_url(base: &str) -> Result<Vec<CatalogEntry>, CatalogError> {
    let catalog_url = format!("{base}/models");
    fetch_catalog(&catalog_url).await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The document a real daemon answers, reduced to the keys xfx reads
    /// (`2lab-ai/llmux@79f66748656b src/catalog.rs:44-63`).
    const REAL_SHAPE: &str = r#"{"models":[
        {"id":"claude-fable-5[1m]","aliases":["fable"],"name":"Claude Fable 5",
         "efforts":["low","medium","high","xhigh","max"],"max_context":1000000,"group":"claude"},
        {"id":"grok-4.5","aliases":["grok"],"name":"Grok 4.5",
         "efforts":["low","medium","high"],"max_context":500000,"group":"grok"}]}"#;

    #[test]
    fn a_catalog_row_carries_everything_the_daemon_publishes_about_a_model() {
        let entries = parse_llmux_catalog(REAL_SHAPE).expect("a catalog document");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, "claude-fable-5[1m]");
        assert_eq!(entries[0].aliases, ["fable"]);
        assert_eq!(entries[0].name.as_deref(), Some("Claude Fable 5"));
        assert_eq!(
            entries[0].efforts,
            ["low", "medium", "high", "xhigh", "max"]
        );
        assert_eq!(entries[0].max_context, Some(1_000_000));
        assert_eq!(
            entries[0].preferred_name(),
            "fable",
            "the short name it publishes"
        );
        assert!(entries[0].matches("fable") && entries[0].matches("claude-fable-5[1m]"));
    }

    #[test]
    fn a_row_without_a_usable_id_is_skipped_rather_than_invented() {
        // A model xfx cannot name is a model it cannot ask for.
        let entries =
            parse_llmux_catalog(r#"{"models":[{"aliases":["x"]},{"id":""},{"id":"real"}]}"#)
                .expect("a catalog document");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "real");
    }

    #[test]
    fn optional_metadata_is_absent_rather_than_defaulted() {
        // A context window nobody published is unknown, not zero: `/model` would
        // otherwise print a window the provider never promised.
        let entries = parse_llmux_catalog(r#"{"models":[{"id":"bare"}]}"#).expect("document");
        assert_eq!(entries[0].max_context, None);
        assert!(entries[0].efforts.is_empty());
        assert_eq!(entries[0].name, None);
        assert_eq!(entries[0].preferred_name(), "bare");
    }

    #[test]
    fn a_body_that_is_not_a_catalog_is_not_an_empty_catalog() {
        for body in ["", "llmux", "[]", r#"{"data":[]}"#] {
            assert!(parse_llmux_catalog(body).is_none(), "for {body:?}");
        }
    }

    #[test]
    fn malformed_response_with_multibyte_utf8_at_boundary_clips_safely() {
        // REGRESSION: A malformed response with a multibyte UTF-8 character
        // spanning byte position 100 should clip safely to the previous char
        // boundary, not panic.
        //
        // Create: 98 ASCII bytes + 1 three-byte UTF-8 character (€) that spans positions 98-100
        // When clipped with floor_char_boundary(100), it should end at byte 98
        // (before the € character).
        let mut body = "x".repeat(98);
        body.push('€'); // E2 82 AC in UTF-8 (positions 98-100)
        body.push('y'); // position 101
                        // Total: 102 bytes

        assert_eq!(body.len(), 102, "body should be 102 bytes");

        // The floor_char_boundary(100) should return 98 (the last safe boundary
        // before position 100, which is in the middle of the € character).
        let safe_len = body.floor_char_boundary(100);
        assert_eq!(
            safe_len, 98,
            "should floor to 98, the char before the multibyte"
        );

        // Clipping to safe_len should succeed and produce valid UTF-8
        let clipped_body = &body[..safe_len];
        assert_eq!(clipped_body, "x".repeat(98));
    }
}
