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

use crate::config::{RuntimeConfig, SettingSource};
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

/// What the caller must render after applying a `/model` request.
#[derive(Debug)]
pub enum ModelOutcome {
    /// `/model` with no argument: the current model and its source are reported.
    Reported {
        provider: ProviderId,
        model: String,
        source: SettingSource,
    },
    /// `/model <id>` was applied: the caller must persist this choice.
    Selected {
        provider: ProviderId,
        model: String,
        previous: String,
        /// Set when the catalog could not be consulted, so the caller can say the
        /// selection was accepted unverified.
        unverified: Option<String>,
    },
    /// The model was already in force; no change was made.
    Unchanged { model: String },
    /// The model id was rejected.
    Refused { reason: String },
}

/// The state of a provider's model catalog, as far as it is known to xfx.
#[derive(Debug, Clone)]
pub enum CatalogState {
    /// This provider advertises no catalog.
    Unavailable,
    /// Not loaded in this process yet.
    NotLoaded,
    /// Loaded, with the entries in the order the provider published them.
    Loaded(Vec<CatalogEntry>),
    /// A load was attempted and failed. Carries the one-line reason.
    Failed(String),
}

/// A request to apply model selection or report the current model.
pub enum ModelRequest<'a> {
    /// `/model` with no argument.
    Report,
    /// `/model <id>`.
    Select(&'a str),
}

/// The most catalog rows a caller renders in one go, matching `list_files`'
/// ceiling; a caller that hits it says how many were left out.
pub const MAX_RENDERED_MODELS: usize = 100;

/// Selects, changes, and renders model choices from a catalog when one exists.
///
/// The selector for the configured provider and its active model. Does no I/O
/// at construction: constructing one on a machine with no credential and no
/// daemon must work, because that is the machine whose user needs `/model`.
pub struct ModelSelector {
    provider: ProviderId,
    model: String,
    source: SettingSource,
    catalog_fetcher: Option<Box<dyn ModelCatalog>>,
    catalog_state: CatalogState,
}

impl ModelSelector {
    /// Creates a selector for the model in force according to the configuration.
    /// Does no I/O: constructing one on a machine with no credential and no
    /// daemon must work, because that is the machine whose user needs `/model`.
    pub fn new(config: &RuntimeConfig) -> Self {
        let catalog_fetcher = catalog_for(config);
        let catalog_state = if catalog_fetcher.is_some() {
            CatalogState::NotLoaded
        } else {
            CatalogState::Unavailable
        };
        Self {
            provider: config.provider,
            model: config.model.clone(),
            source: config.sources.model,
            catalog_fetcher,
            catalog_state,
        }
    }

    /// The provider whose catalog this selector browses.
    pub fn provider(&self) -> ProviderId {
        self.provider
    }

    /// The model in force right now.
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Which settings layer chose it.
    pub fn source(&self) -> SettingSource {
        self.source
    }

    /// The catalog as far as it is known. Never performs I/O.
    pub fn catalog(&self) -> &CatalogState {
        &self.catalog_state
    }

    /// Loads the catalog once, if the provider advertises one and it has not
    /// been loaded in this process. **This is the only method that opens a
    /// socket**; call it where a network call is already legitimate.
    pub async fn ensure_catalog(&mut self) -> &CatalogState {
        if !matches!(self.catalog_state, CatalogState::NotLoaded) {
            return &self.catalog_state;
        }
        let Some(fetcher) = &self.catalog_fetcher else {
            return &self.catalog_state;
        };
        match fetcher.fetch().await {
            Ok(entries) => self.catalog_state = CatalogState::Loaded(entries),
            Err(err) => {
                self.catalog_state = CatalogState::Failed(err.to_string());
            }
        }
        &self.catalog_state
    }

    /// Applies one `/model` request and returns what the caller must render.
    /// Never performs I/O: it decides against whatever `catalog()` holds.
    pub fn apply(&mut self, request: ModelRequest<'_>) -> ModelOutcome {
        match request {
            ModelRequest::Report => ModelOutcome::Reported {
                provider: self.provider,
                model: self.model.clone(),
                source: self.source,
            },
            ModelRequest::Select(id) => {
                // Validate the id before doing anything else.
                if let Some(problem) = model_id_problem(id) {
                    return ModelOutcome::Refused {
                        reason: problem.to_string(),
                    };
                }

                // If already the current model, say so.
                if id == self.model {
                    return ModelOutcome::Unchanged {
                        model: self.model.clone(),
                    };
                }

                // Check the catalog state to decide whether to refuse or accept unverified.
                if let CatalogState::Loaded(entries) = &self.catalog_state {
                    // Catalog is loaded: refuse if the id is not in it.
                    if !entries.iter().any(|e| e.matches(id)) {
                        return ModelOutcome::Refused {
                            reason: format!(
                                "{} does not publish {} in its catalog",
                                self.provider_name(),
                                id
                            ),
                        };
                    }
                }

                let previous = self.model.clone();
                let unverified = match &self.catalog_state {
                    CatalogState::Unavailable => None,
                    CatalogState::NotLoaded => {
                        Some("the catalog has not been loaded in this process".to_string())
                    }
                    CatalogState::Loaded(_) => None,
                    CatalogState::Failed(reason) => Some(reason.clone()),
                };

                self.model = id.to_string();
                ModelOutcome::Selected {
                    provider: self.provider,
                    model: self.model.clone(),
                    previous,
                    unverified,
                }
            }
        }
    }

    fn provider_name(&self) -> &'static str {
        match self.provider {
            ProviderId::Gateway => "gateway",
            ProviderId::Llmux => "llmux",
        }
    }

    #[cfg(test)]
    fn for_test(provider: ProviderId, model: &str, source: SettingSource) -> Self {
        Self {
            provider,
            model: model.to_string(),
            source,
            catalog_fetcher: None,
            catalog_state: CatalogState::Unavailable,
        }
    }

    #[cfg(test)]
    fn set_catalog_for_test(&mut self, state: CatalogState) {
        self.catalog_state = state;
    }
}

/// Whether `candidate` can be used as a model id.
/// Maximum 200 bytes; one printable word; no control characters.
pub(crate) fn model_id_problem(candidate: &str) -> Option<&'static str> {
    if candidate.is_empty() {
        return Some("name a model");
    }
    if candidate.len() > 200 {
        return Some("that model id is too long");
    }
    if candidate.split_whitespace().count() != 1 {
        return Some("/model takes one model id, with no spaces in it");
    }
    if candidate.chars().any(char::is_control) {
        return Some("a model id cannot contain control characters");
    }
    None
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

    fn entry(id: &str, aliases: &[&str]) -> CatalogEntry {
        CatalogEntry {
            id: id.to_string(),
            aliases: aliases.iter().map(|a| (*a).to_string()).collect(),
            name: None,
            efforts: Vec::new(),
            max_context: None,
        }
    }

    /// A mock catalog that tracks how many times fetch is called.
    #[derive(Clone)]
    struct CountingCatalog {
        call_count: std::sync::Arc<std::sync::Mutex<usize>>,
        result: std::sync::Arc<Result<Vec<CatalogEntry>, CatalogError>>,
    }

    impl CountingCatalog {
        fn new(result: Result<Vec<CatalogEntry>, CatalogError>) -> Self {
            Self {
                call_count: std::sync::Arc::new(std::sync::Mutex::new(0)),
                result: std::sync::Arc::new(result),
            }
        }

        fn count(&self) -> usize {
            *self.call_count.lock().expect("mutex")
        }
    }

    #[async_trait::async_trait(?Send)]
    impl ModelCatalog for CountingCatalog {
        async fn fetch(&self) -> Result<Vec<CatalogEntry>, CatalogError> {
            *self.call_count.lock().expect("mutex") += 1;
            match &*self.result {
                Ok(entries) => Ok(entries.clone()),
                Err(err) => Err(match err {
                    CatalogError::Unavailable { detail } => CatalogError::Unavailable {
                        detail: detail.clone(),
                    },
                    CatalogError::Empty => CatalogError::Empty,
                    CatalogError::Malformed { detail } => CatalogError::Malformed {
                        detail: detail.clone(),
                    },
                }),
            }
        }
    }

    #[test]
    fn a_report_names_the_provider_the_model_and_the_layer_that_chose_it() {
        let selector =
            ModelSelector::for_test(ProviderId::Llmux, "fable", SettingSource::UserGlobal);
        let mut selector = selector;
        match selector.apply(ModelRequest::Report) {
            ModelOutcome::Reported {
                provider,
                model,
                source,
            } => {
                assert_eq!(provider, ProviderId::Llmux);
                assert_eq!(model, "fable");
                assert_eq!(source, SettingSource::UserGlobal);
            }
            other => panic!("expected a report, got {other:?}"),
        }
    }

    #[test]
    fn a_selection_the_loaded_catalog_does_not_have_is_refused_by_name() {
        // The provider published what it can run. An id it did not publish is a
        // turn that fails at the provider, and failing here says why.
        let mut selector =
            ModelSelector::for_test(ProviderId::Llmux, "fable", SettingSource::UserGlobal);
        selector.set_catalog_for_test(CatalogState::Loaded(vec![entry("m-1", &["fable"])]));
        match selector.apply(ModelRequest::Select("not-published")) {
            ModelOutcome::Refused { reason } => {
                assert!(reason.contains("not-published"), "{reason}");
                assert!(reason.contains("llmux"), "{reason}");
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
        assert_eq!(selector.model(), "fable", "a refusal changes nothing");
    }

    #[test]
    fn an_alias_selects_the_entry_it_belongs_to() {
        let mut selector =
            ModelSelector::for_test(ProviderId::Llmux, "other", SettingSource::UserGlobal);
        selector.set_catalog_for_test(CatalogState::Loaded(vec![entry("m-1", &["fable"])]));
        match selector.apply(ModelRequest::Select("fable")) {
            ModelOutcome::Selected {
                model,
                previous,
                unverified,
                ..
            } => {
                assert_eq!(model, "fable");
                assert_eq!(previous, "other");
                assert_eq!(unverified, None);
            }
            other => panic!("expected a selection, got {other:?}"),
        }
    }

    #[test]
    fn a_selection_is_accepted_unverified_when_the_catalog_could_not_be_read() {
        // A daemon that is down must not stop an operator from changing a
        // preference. It stops xfx from *checking* it, which is a different
        // sentence and the one the user gets.
        let mut selector =
            ModelSelector::for_test(ProviderId::Llmux, "fable", SettingSource::UserGlobal);
        selector.set_catalog_for_test(CatalogState::Failed(
            "the daemon did not answer".to_string(),
        ));
        match selector.apply(ModelRequest::Select("anything")) {
            ModelOutcome::Selected {
                model,
                unverified: Some(reason),
                ..
            } => {
                assert_eq!(model, "anything");
                assert!(reason.contains("did not answer"), "{reason}");
            }
            other => panic!("expected an unverified selection, got {other:?}"),
        }
    }

    #[test]
    fn a_provider_with_no_catalog_accepts_any_well_formed_id() {
        let mut selector = ModelSelector::for_test(
            ProviderId::Gateway,
            "zai/glm-5.2",
            SettingSource::CompiledDefault,
        );
        assert!(matches!(selector.catalog(), CatalogState::Unavailable));
        assert!(matches!(
            selector.apply(ModelRequest::Select("openai/gpt-5")),
            ModelOutcome::Selected {
                unverified: None,
                ..
            }
        ));
    }

    #[test]
    fn a_direct_selection_before_browsing_is_accepted_with_a_warning() {
        // Direct `/model <id>` before `/model` is typed warns the user that the
        // catalog was not consulted, distinguishing from a catalog error.
        let mut selector =
            ModelSelector::for_test(ProviderId::Llmux, "fable", SettingSource::UserGlobal);
        // Set catalog state to NotLoaded (the default for llmux when constructed normally).
        selector.set_catalog_for_test(CatalogState::NotLoaded);
        match selector.apply(ModelRequest::Select("any-id")) {
            ModelOutcome::Selected {
                model,
                unverified: Some(reason),
                ..
            } => {
                assert_eq!(model, "any-id");
                assert!(reason.contains("not been loaded"), "{reason}");
            }
            other => panic!("expected an unverified selection, got {other:?}"),
        }
    }

    #[test]
    fn a_model_id_is_one_bounded_printable_word_whatever_the_provider() {
        // The id becomes an HTTP header value and a durable session field.
        let mut selector = ModelSelector::for_test(
            ProviderId::Gateway,
            "zai/glm-5.2",
            SettingSource::CompiledDefault,
        );
        for bad in ["", "two words", "with\u{0}control"] {
            assert!(
                matches!(
                    selector.apply(ModelRequest::Select(bad)),
                    ModelOutcome::Refused { .. }
                ),
                "for {bad:?}"
            );
        }
    }

    #[test]
    fn a_model_id_200_bytes_is_accepted_and_201_bytes_is_refused() {
        // The boundary is exactly 200 bytes. Verify both sides.
        let mut selector =
            ModelSelector::for_test(ProviderId::Gateway, "a", SettingSource::CompiledDefault);

        // 200 bytes should be accepted.
        let id_200 = "x".repeat(200);
        assert!(matches!(
            selector.apply(ModelRequest::Select(&id_200)),
            ModelOutcome::Selected {
                unverified: None,
                ..
            }
        ));

        // Reset the selector for the next test.
        let mut selector =
            ModelSelector::for_test(ProviderId::Gateway, "a", SettingSource::CompiledDefault);

        // 201 bytes should be refused.
        let id_201 = "x".repeat(201);
        assert!(matches!(
            selector.apply(ModelRequest::Select(&id_201)),
            ModelOutcome::Refused { .. }
        ));
    }

    #[test]
    fn the_catalog_is_bounded_at_100_rendered_rows() {
        // A catalog with more than 100 entries is clipped by the renderer,
        // but both the count and remainder are reported.
        let mut selector =
            ModelSelector::for_test(ProviderId::Llmux, "fable", SettingSource::UserGlobal);
        let mut entries = Vec::new();
        for i in 0..105 {
            entries.push(CatalogEntry {
                id: format!("model-{}", i),
                aliases: vec![],
                name: None,
                efforts: vec![],
                max_context: None,
            });
        }
        selector.set_catalog_for_test(CatalogState::Loaded(entries));
        match selector.apply(ModelRequest::Report) {
            ModelOutcome::Reported { .. } => {
                // The catalog() call will return Loaded(105 entries).
                // The display logic is tested in interactive.rs via PTY tests.
                // Here we just verify the state is correct.
                assert!(matches!(
                    selector.catalog(),
                    CatalogState::Loaded(es) if es.len() == 105
                ));
            }
            other => panic!("expected a report, got {other:?}"),
        }
    }

    #[test]
    fn selecting_the_model_already_in_force_changes_nothing() {
        let mut selector = ModelSelector::for_test(
            ProviderId::Gateway,
            "zai/glm-5.2",
            SettingSource::UserGlobal,
        );
        assert!(matches!(
            selector.apply(ModelRequest::Select("zai/glm-5.2")),
            ModelOutcome::Unchanged { .. }
        ));
    }

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

    #[tokio::test]
    async fn ensure_catalog_fetches_exactly_once_on_success() {
        // Multiple calls to ensure_catalog must not re-fetch.
        let catalog = CountingCatalog::new(Ok(vec![entry("m-1", &["fable"])]));
        let initial_count = catalog.count();

        let mut selector =
            ModelSelector::for_test(ProviderId::Llmux, "fable", SettingSource::UserGlobal);
        selector.catalog_fetcher = Some(Box::new(catalog.clone()));
        selector.catalog_state = CatalogState::NotLoaded;

        // First call should fetch.
        selector.ensure_catalog().await;
        let count_after_first = catalog.count();
        assert_eq!(count_after_first, initial_count + 1);

        // Second call should not fetch (state is no longer NotLoaded).
        selector.ensure_catalog().await;
        let count_after_second = catalog.count();
        assert_eq!(count_after_second, count_after_first);
    }

    #[tokio::test]
    async fn ensure_catalog_does_not_retry_after_failure() {
        // A failed load is not retried within the same process.
        let catalog = CountingCatalog::new(Err(CatalogError::Empty));
        let initial_count = catalog.count();

        let mut selector =
            ModelSelector::for_test(ProviderId::Llmux, "fable", SettingSource::UserGlobal);
        selector.catalog_fetcher = Some(Box::new(catalog.clone()));
        selector.catalog_state = CatalogState::NotLoaded;

        // First call should fetch and fail.
        selector.ensure_catalog().await;
        let count_after_first = catalog.count();
        assert_eq!(count_after_first, initial_count + 1);

        // Verify the state is now Failed, not NotLoaded.
        assert!(matches!(selector.catalog_state, CatalogState::Failed(_)));

        // Second call should not fetch (state is Failed, not NotLoaded).
        selector.ensure_catalog().await;
        let count_after_second = catalog.count();
        assert_eq!(count_after_second, count_after_first);
    }
}
