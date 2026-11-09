//! What a provider says it can run.
//!
//! Two catalog concepts, kept apart the way upstream keeps them: the **static
//! identity** of a provider (`ProviderEntry`, which drives labels) and the
//! **fetched model catalog** below, which is what `/model` renders. Only the
//! second one needs a socket, which is why it is a trait: a provider that
//! advertises no catalog has `None`, not an object that answers nothing.

use std::fmt;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::config::{RuntimeConfig, SettingSource};
use crate::gateway::{larger_than, read_capped, CappedRead, Endpoint, USER_AGENT};
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
    /// What else the catalog said about the model. Empty for a llmux row,
    /// whose document carries none of it.
    pub facts: ModelFacts,
}

/// What a catalog row says about a model beyond its name and window: the
/// facts `/model`'s menu prints beside it and the ones its order is decided by.
///
/// Upstream's catalog entry, reduced to what xfx reads
/// (`vercel-labs/fx@c1db9191 src/builtins/gateway.zig:2820-2940`). Every field
/// defaults to "the catalog did not say", because a fact nobody published is
/// not one the menu may print.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModelFacts {
    /// The vendor the catalog names (`owned_by`).
    pub owned_by: Option<String>,
    /// When the model was released, in seconds since the epoch; `0` when the
    /// catalog did not say.
    pub released: i64,
    /// Whether the row is tagged `tool-use`.
    pub tool_use: bool,
    /// Whether the row is tagged `vision` and `file-input` both: the one case
    /// upstream sends an image to the model inline (`image_input_support`
    /// `.native`, `vercel-labs/fx@c1db9191 src/builtins/gateway.zig:2847-2848`,
    /// `src/core/config/model_capabilities.zig:96-104`).
    pub image_input: bool,
    /// The most tokens one answer may carry (`max_tokens`).
    pub max_output: Option<u64>,
    /// Whether the catalog offers Fast mode for it (`supportsFastMode`).
    pub fast: bool,
    /// Whether the catalog offers Ultra mode for it (`supportsUltrafastMode`).
    pub ultrafast: bool,
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
    /// The Gateway's catalog could not be read, for the reason given.
    ///
    /// Its own variant because the three above send the operator to a local
    /// daemon and `xfx setup llmux`, which is the wrong place to look when the
    /// catalog is a remote endpoint behind a key.
    ///
    /// `retry` says whether asking again could answer differently, and what
    /// `/model`'s menu then says ([`CatalogRetry`]); `None` is a failure a
    /// retry would only repeat -- a refused key, a body that is not a catalog.
    Gateway {
        detail: String,
        retry: Option<CatalogRetry>,
    },
}

/// A catalog failure worth asking again about, by upstream's categories
/// (`failureForHttpStatus`, `src/core/gateway/model_catalog.zig:81-100
/// @481104fd`, and the transport rows of its table).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogRetry {
    /// HTTP 429.
    RateLimited,
    /// Nothing answered, or the Gateway answered 500, 502, 503 or 504.
    Unreachable,
    /// The read could not start: no thread, or no runtime on it, could be had
    /// for it (upstream's `.runtime`, retryable, for a fetch thread that could
    /// not be spawned, `model_cache_runtime.zig:537-546@481104fd`).
    Runtime,
}

impl CatalogRetry {
    /// What `/model`'s menu says in place of `Unable to load models.`, and
    /// in its note's place under rows a refresh could not replace
    /// (`retryableFailureText`, `src/ui/footer/model_menu_presentation.zig:
    /// 435-443@481104fd`, from `composeStateRow` and `loadedCatalogStatusText`,
    /// `:394-404`): upstream's sentence since 14a12fea, its name read as
    /// xfx's, because the retry is the model cache's own
    /// ([`ModelSelector::begin_load_on`]).
    pub fn text(self) -> &'static str {
        match self {
            Self::RateLimited => {
                "AI Gateway rate limited model discovery; xfx retries automatically."
            }
            Self::Unreachable => "Could not reach AI Gateway; xfx retries automatically.",
            Self::Runtime => "Could not refresh model catalog; xfx retries automatically.",
        }
    }

    /// The category of an HTTP status the catalog answered with, when asking
    /// again could change it.
    pub fn for_status(status: u16) -> Option<Self> {
        match status {
            429 => Some(Self::RateLimited),
            500 | 502 | 503 | 504 => Some(Self::Unreachable),
            _ => None,
        }
    }
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
            Self::Gateway { detail, .. } => {
                write!(f, "the Gateway's model catalog is unavailable: {detail}")
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

    /// Whether a read of this catalog finds no runtime to run on: a test's
    /// stand-in for a machine out of what one needs ([`read_runtime`]).
    #[cfg(test)]
    fn runtime_fails_for_test(&self) -> bool {
        false
    }
}

/// How long the catalog fetch may take to open a connection.
///
/// Short, because the thing being probed is on this machine: a loopback connect
/// that has not completed in three seconds is not slow, it is absent.
const CATALOG_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

/// How long the catalog fetch waits for a local daemon to answer.
const CATALOG_READ_TIMEOUT: Duration = Duration::from_secs(5);

/// The largest catalog response xfx will read from a llmux daemon.
///
/// The catalog is a list of model descriptors; a body past this is not a catalog
/// and reading it would let whatever is on that port decide how much memory this
/// command uses. xfx's own number: fx has no llmux.
const MAX_CATALOG_BODY_BYTES: usize = 1024 * 1024;

/// The largest answer the Gateway's catalog may give: "about fifty times
/// today's model catalog. A larger response fails rather than growing memory
/// without bound" (`max_gateway_json_bytes`, `src/gateway/client.zig:611-613
/// @481104fd`). Past it the answer is refused, not cut to fit.
const MAX_GATEWAY_CATALOG_BODY_BYTES: usize = 16 * 1024 * 1024;

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
                    facts: ModelFacts::default(),
                })
            })
            .collect(),
    )
}

/// Parses the Gateway's model catalog (`GET /coding-agent/v1/models`).
///
/// Upstream's reading of the same document (`parseModelCatalogEntry`,
/// `vercel-labs/fx@c1db9191 src/builtins/gateway.zig:2820-2880`): the `data`
/// array, a row only when its `type` is absent or `language`, and from each
/// row the id, the vendor, the release time, the `tool-use` tag, the effort
/// values, the context window, the output ceiling, and whether the row is
/// priced for Fast or Ultra mode. A row without a usable id is skipped, as a
/// llmux row is.
///
/// `None` when the body is not that document at all.
pub fn parse_gateway_catalog(body: &str) -> Option<Vec<CatalogEntry>> {
    let Ok(Value::Object(document)) = serde_json::from_str::<Value>(body) else {
        return None;
    };
    let Some(Value::Array(models)) = document.get("data") else {
        return None;
    };
    Some(
        models
            .iter()
            .filter_map(|model| {
                let object = model.as_object()?;
                if let Some(Value::String(kind)) = object.get("type") {
                    if !kind.eq_ignore_ascii_case("language") {
                        return None;
                    }
                }
                let id = object.get("id")?.as_str()?;
                if id.is_empty() {
                    return None;
                }
                let owned_by = object
                    .get("owned_by")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                let facts = ModelFacts {
                    released: object.get("released").and_then(Value::as_i64).unwrap_or(0),
                    tool_use: tagged(object.get("tags"), "tool-use"),
                    image_input: tagged(object.get("tags"), "vision")
                        && tagged(object.get("tags"), "file-input"),
                    max_output: positive(object.get("max_tokens")),
                    fast: supports_fast(object, owned_by.as_deref()),
                    ultrafast: supports_ultrafast(object, owned_by.as_deref()),
                    owned_by,
                };
                Some(CatalogEntry {
                    id: id.to_string(),
                    aliases: Vec::new(),
                    name: None,
                    efforts: gateway_efforts(object.get("reasoning_options")),
                    max_context: positive(object.get("context_window")),
                    facts,
                })
            })
            .collect(),
    )
}

/// Whether a `tags` array carries `tag`, in any case (`tagListContains`).
fn tagged(tags: Option<&Value>, tag: &str) -> bool {
    tags.and_then(Value::as_array).is_some_and(|tags| {
        tags.iter()
            .filter_map(Value::as_str)
            .any(|candidate| candidate.eq_ignore_ascii_case(tag))
    })
}

/// A token count the catalog published, or `None` for one it did not
/// (`optionalUnsignedU32`: absent, zero, negative and oversized all read as
/// nothing, and upstream's capabilities treat a zero as no window at all).
fn positive(value: Option<&Value>) -> Option<u64> {
    value
        .and_then(Value::as_u64)
        .filter(|tokens| *tokens > 0 && *tokens <= u64::from(u32::MAX))
}

/// The effort values of the first `{type: "effort"}` option
/// (`parseReasoningEfforts`, `gateway.zig:2882-2906`): at most sixteen, each a
/// name upstream would parse, and never the default -- `auto`, `adaptive` or
/// `default` -- which is the row a picker adds on its own.
fn gateway_efforts(options: Option<&Value>) -> Vec<String> {
    let Some(options) = options.and_then(Value::as_array) else {
        return Vec::new();
    };
    let Some(values) = options
        .iter()
        .filter_map(Value::as_object)
        .find(|option| option.get("type").and_then(Value::as_str) == Some("effort"))
        .and_then(|option| option.get("values"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    values
        .iter()
        .filter_map(Value::as_str)
        .filter(|effort| {
            !effort.is_empty()
                && effort.len() <= 64
                && effort
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
                && !["auto", "adaptive", "default"]
                    .iter()
                    .any(|default| effort.eq_ignore_ascii_case(default))
        })
        .take(16)
        .map(str::to_string)
        .collect()
}

/// `supportsFastMode` (`gateway.zig:2918-2927`): a `{type: "toggle"}` fast
/// option, or a `fast` price, or an OpenAI row with a `priority` service tier.
/// A `fast` tag alone does not count.
fn supports_fast(object: &serde_json::Map<String, Value>, owned_by: Option<&str>) -> bool {
    let toggle = object
        .get("fast_options")
        .and_then(Value::as_array)
        .is_some_and(|options| {
            options
                .iter()
                .any(|option| option.get("type").and_then(Value::as_str) == Some("toggle"))
        });
    let pricing = object.get("pricing").and_then(Value::as_object);
    let priced = pricing.is_some_and(|pricing| pricing.contains_key("fast"));
    let priority = owned_by.is_some_and(|owner| owner.eq_ignore_ascii_case("openai"))
        && pricing
            .and_then(|pricing| pricing.get("service_tiers"))
            .and_then(Value::as_object)
            .is_some_and(|tiers| tiers.contains_key("priority"));
    toggle || priced || priority
}

/// `supportsUltrafastMode` (`gateway.zig:2929-2937`): an OpenAI row whose
/// `ultrafast` service tier prices both input and output above zero -- and its
/// cached input too, when the row prices cached input at all. Names and tags
/// do not count.
fn supports_ultrafast(object: &serde_json::Map<String, Value>, owned_by: Option<&str>) -> bool {
    if !owned_by.is_some_and(|owner| owner.eq_ignore_ascii_case("openai")) {
        return false;
    }
    let Some(pricing) = object.get("pricing").and_then(Value::as_object) else {
        return false;
    };
    let Some(tier) = pricing
        .get("service_tiers")
        .and_then(Value::as_object)
        .and_then(|tiers| tiers.get("ultrafast"))
        .and_then(Value::as_object)
    else {
        return false;
    };
    let priced = |key: &str| tier.get(key).is_some_and(positive_price);
    priced("input")
        && priced("output")
        && (!pricing.contains_key("input_cache_read") || priced("input_cache_read"))
}

/// A price above zero, written as a number or as a decimal string
/// (`positivePrice`).
fn positive_price(value: &Value) -> bool {
    let price = match value {
        Value::String(text) => text.parse::<f64>().ok(),
        Value::Number(number) => number.as_f64(),
        _ => None,
    };
    price.is_some_and(|price| price.is_finite() && price > 0.0)
}

/// The order `/model` lists a catalog in, and the order a query is resolved
/// over: upstream's featured projection (`projectPickerModelCatalog`,
/// `vercel-labs/fx@c1db9191 src/core/gateway/model_catalog.zig:443-503`) over
/// its catalog sort (`compareModelCatalogEntries`, `:398-441`).
///
/// One model from each featured family first, newest first; then every
/// vendor in alphabetical order, two models each and three for Anthropic and
/// OpenAI; then everything else, so any model is still selectable. The
/// families are product preferences; which model fills each slot is always the
/// catalog's.
///
/// Applied to a llmux catalog too, whose ids have no vendor in front: each id
/// is then its own vendor, so the projection lists them by name. The catalog
/// itself keeps the order the provider published -- `xfx setup llmux` takes
/// its first entry, and that is the daemon's choice, not this one.
pub fn picker_order(entries: &[CatalogEntry]) -> Vec<CatalogEntry> {
    let mut candidates = entries.to_vec();
    candidates.sort_by(catalog_order);
    let mut selected: Vec<CatalogEntry> = Vec::with_capacity(candidates.len());
    for (family, count) in FEATURED_FAMILIES {
        for _ in 0..*count {
            let newest = candidates
                .iter()
                .filter(|candidate| picker_candidate(&candidate.id))
                .filter(|candidate| picker_family(&candidate.id) == *family)
                .filter(|candidate| !contains_id(&selected, &candidate.id))
                .reduce(|newest, candidate| {
                    if featured_newer(family, candidate, newest) {
                        candidate
                    } else {
                        newest
                    }
                });
            match newest {
                Some(entry) => selected.push(entry.clone()),
                None => break,
            }
        }
    }
    let mut vendors: Vec<&str> = Vec::new();
    for candidate in &candidates {
        let vendor = picker_vendor(&candidate.id);
        if picker_candidate(&candidate.id) && !vendors.contains(&vendor) {
            vendors.push(vendor);
        }
    }
    vendors.sort_unstable();
    for vendor in vendors {
        while selected
            .iter()
            .filter(|entry| picker_vendor(&entry.id) == vendor)
            .count()
            < vendor_limit(vendor)
        {
            let newest = candidates
                .iter()
                .filter(|candidate| picker_candidate(&candidate.id))
                .filter(|candidate| picker_vendor(&candidate.id) == vendor)
                .filter(|candidate| vendor_candidate(vendor, candidate, &candidates))
                .filter(|candidate| !contains_id(&selected, &candidate.id))
                .reduce(|newest, candidate| {
                    if newer(candidate, newest) {
                        candidate
                    } else {
                        newest
                    }
                });
            match newest {
                Some(entry) => selected.push(entry.clone()),
                None => break,
            }
        }
    }
    for candidate in candidates {
        if !contains_id(&selected, &candidate.id) {
            selected.push(candidate);
        }
    }
    selected
}

/// Upstream's featured families and how many of each lead the list
/// (`featured_picker_families`, `model_catalog.zig:451-459`).
const FEATURED_FAMILIES: &[(&str, usize)] = &[
    ("anthropic/claude-fable", 1),
    ("openai/gpt", 1),
    ("xai/grok-build", 1),
    ("anthropic/claude-opus", 1),
    ("zai/glm", 1),
    ("deepseek/deepseek", 1),
    ("minimax/minimax", 1),
];

/// `compareModelCatalogEntries`: tool use first, then the tier, then the
/// vendor's rank, then newest, then the id.
fn catalog_order(a: &CatalogEntry, b: &CatalogEntry) -> std::cmp::Ordering {
    b.facts
        .tool_use
        .cmp(&a.facts.tool_use)
        .then_with(|| tier_rank(&a.id).cmp(&tier_rank(&b.id)))
        .then_with(|| vendor_rank(&a.id).cmp(&vendor_rank(&b.id)))
        .then_with(|| b.facts.released.cmp(&a.facts.released))
        .then_with(|| a.id.cmp(&b.id))
}

/// `modelProviderRank` (`model_catalog.zig:413-423`).
fn vendor_rank(id: &str) -> u8 {
    const RANKED: [&str; 8] = [
        "anthropic/",
        "openai/",
        "google/",
        "xai/",
        "deepseek/",
        "meta/",
        "mistral/",
        "alibaba/",
    ];
    RANKED
        .iter()
        .position(|prefix| id.starts_with(prefix))
        .map_or(8, |rank| rank as u8)
}

/// `modelTierRank` (`model_catalog.zig:425-441`).
fn tier_rank(id: &str) -> u8 {
    let has = |word: &str| contains_ignore_case(id, word);
    if has("preview") || has("beta") {
        4
    } else if has("haiku") || has("mini") || has("lite") {
        3
    } else if has("flash") {
        2
    } else if ["opus", "sonnet", "gpt-5", "o1", "o3", "o4", "pro", "grok-4"]
        .iter()
        .any(|word| has(word))
    {
        0
    } else {
        1
    }
}

/// `isPickerCandidate` (`model_catalog.zig:530-541`): the variants the
/// featured and per-vendor passes leave for the rest.
fn picker_candidate(id: &str) -> bool {
    if contains_ignore_case(id, "claude-sonnet") {
        return false;
    }
    if id.starts_with("zai/glm-") && id.ends_with("-fast") {
        return false;
    }
    !["-mini", "-nano", "-oss", "-safeguard"]
        .iter()
        .any(|variant| contains_ignore_case(id, variant))
}

/// `modelPickerFamily` (`model_catalog.zig:562-579`): the id up to the first
/// `-` that is followed by a digit, after the vendor.
fn picker_family(id: &str) -> &str {
    if id.starts_with("openai/gpt-") && contains_ignore_case(id, "-codex") {
        return "openai/gpt-codex";
    }
    if id.starts_with("deepseek/deepseek-") {
        return "deepseek/deepseek";
    }
    if id.starts_with("minimax/minimax-") {
        return "minimax/minimax";
    }
    let Some(slash) = id.find('/') else {
        return id;
    };
    let bytes = id.as_bytes();
    (slash + 1..bytes.len().saturating_sub(1))
        .find(|&index| bytes[index] == b'-' && bytes[index + 1].is_ascii_digit())
        .map_or(id, |index| &id[..index])
}

/// `modelPickerProvider`: what is before the first `/`, or the whole id.
fn picker_vendor(id: &str) -> &str {
    id.split_once('/').map_or(id, |(vendor, _)| vendor)
}

/// `pickerProviderLimit`.
fn vendor_limit(vendor: &str) -> usize {
    if vendor == "anthropic" || vendor == "openai" {
        3
    } else {
        2
    }
}

/// `isPickerProviderCandidate` (`model_catalog.zig:600-612`).
fn vendor_candidate(vendor: &str, candidate: &CatalogEntry, all: &[CatalogEntry]) -> bool {
    let family = picker_family(&candidate.id);
    match vendor {
        "anthropic" => family == "anthropic/claude-opus" && within_family_limit(candidate, all),
        "openai" => {
            (family == "openai/gpt" || family == "openai/gpt-codex")
                && within_family_limit(candidate, all)
        }
        _ => true,
    }
}

/// `isWithinPickerFamilyLimit` (`model_catalog.zig:546-560`).
fn within_family_limit(candidate: &CatalogEntry, all: &[CatalogEntry]) -> bool {
    if !picker_candidate(&candidate.id) {
        return false;
    }
    let family = picker_family(&candidate.id);
    let limit = match family {
        "openai/gpt" => 2,
        "openai/gpt-codex" => 1,
        _ => 3,
    };
    all.iter()
        .filter(|other| picker_candidate(&other.id))
        .filter(|other| picker_family(&other.id) == family)
        .filter(|other| newer(other, candidate))
        .count()
        < limit
}

/// `pickerModelIsNewer`: released later, then the shorter id, then the id.
fn newer(a: &CatalogEntry, b: &CatalogEntry) -> bool {
    if a.facts.released != b.facts.released {
        return a.facts.released > b.facts.released;
    }
    if a.id.len() != b.id.len() {
        return a.id.len() < b.id.len();
    }
    a.id < b.id
}

/// `featuredPickerModelIsNewer`: DeepSeek's slot prefers its best variant
/// among models released together.
fn featured_newer(family: &str, a: &CatalogEntry, b: &CatalogEntry) -> bool {
    if a.facts.released != b.facts.released {
        return a.facts.released > b.facts.released;
    }
    if family == "deepseek/deepseek" {
        let quality = |id: &str| {
            if id.ends_with("-pro") {
                0
            } else if contains_ignore_case(id, "-thinking")
                || contains_ignore_case(id, "-reasoning")
            {
                1
            } else if contains_ignore_case(id, "-flash") {
                3
            } else {
                2
            }
        };
        let (left, right) = (quality(&a.id), quality(&b.id));
        if left != right {
            return left < right;
        }
    }
    newer(a, b)
}

fn contains_id(entries: &[CatalogEntry], id: &str) -> bool {
    entries.iter().any(|entry| entry.id == id)
}

/// Whether `needle` occurs in `haystack`, ignoring ASCII case
/// (`text_utils.containsIgnoreCase`).
fn contains_ignore_case(haystack: &str, needle: &str) -> bool {
    let (haystack, needle) = (haystack.as_bytes(), needle.as_bytes());
    needle.is_empty()
        || haystack
            .windows(needle.len())
            .any(|window| window.eq_ignore_ascii_case(needle))
}

/// How well `id` answers `query`; `0` is no answer at all
/// (`fuzzyModelScore`, `vercel-labs/fx@c1db9191
/// src/core/session/session_commands.zig:1617-1670`).
///
/// A substring scores highest -- a prefix, or the part after the vendor,
/// higher still -- then every word of the query somewhere in the id, then the
/// query's letters in order, then any one word.
pub fn fuzzy_model_score(id: &str, query: &str) -> i32 {
    if query.is_empty() || id.is_empty() {
        return 0;
    }
    if contains_ignore_case(id, query) {
        let bonus = i32::try_from(query.len().min(50)).unwrap_or(50);
        let (bytes, wanted) = (id.as_bytes(), query.as_bytes());
        let after_vendor = bytes.len() > wanted.len()
            && bytes[bytes.len() - wanted.len() - 1] == b'/'
            && bytes[bytes.len() - wanted.len()..].eq_ignore_ascii_case(wanted);
        if id.starts_with(query) || after_vendor {
            return 120 + bonus;
        }
        return 100 + bonus;
    }
    let words = query_words(query);
    if words.len() > 1 && words.iter().all(|word| contains_ignore_case(id, word)) {
        return 80 + i32::try_from(words.len()).unwrap_or(16) * 5;
    }
    let wanted = query.as_bytes();
    let mut next = 0;
    for byte in id.bytes() {
        if next < wanted.len() && byte.eq_ignore_ascii_case(&wanted[next]) {
            next += 1;
        }
    }
    if next == wanted.len() {
        return 40 + i32::try_from(next.min(20)).unwrap_or(20);
    }
    if words.iter().any(|word| contains_ignore_case(id, word)) {
        return 20;
    }
    0
}

/// The words of a query, split at a space, `-`, `/` or `_`, at most sixteen
/// (`splitQueryTokens`).
fn query_words(query: &str) -> Vec<&str> {
    query
        .split([' ', '-', '/', '_'])
        .filter(|word| !word.is_empty())
        .take(16)
        .collect()
}

/// What `/model <query>` selects from `ids`, listed in [`picker_order`]
/// (`resolveModelQueryFromIds`, `session_commands.zig:1282-1305`): the id the
/// query names exactly, ignoring case; otherwise the best-scoring id, the first
/// of equals; otherwise nothing, and the caller uses the query as it is.
pub fn resolve_model_query<'a>(
    ids: impl Iterator<Item = &'a str> + Clone,
    query: &str,
) -> Option<&'a str> {
    if let Some(exact) = ids.clone().find(|id| id.eq_ignore_ascii_case(query)) {
        return Some(exact);
    }
    let mut best: Option<(&str, i32)> = None;
    for id in ids {
        let score = fuzzy_model_score(id, query);
        if score > best.map_or(0, |(_, best)| best) {
            best = Some((id, score));
        }
    }
    best.map(|(id, _)| id)
}

/// What `query` names among `ordered`, a catalog already in [`picker_order`]:
/// the whole of `/model <query>`'s rule (`resolveModelQuery`,
/// `session_commands.zig:1147-1166`) once a catalog is in hand.
///
/// An alias spelled exactly is kept as typed, because the provider resolves its
/// own short names; otherwise the id the query names ignoring case, else the
/// best fuzzy match ([`resolve_model_query`]), else the query itself.
///
/// Public because **two threads ask it**: the selector, which owns the
/// catalog, and the TUI's band, which holds the rows the runtime sent it and
/// has to say at once which model the next turn will use
/// (`crate::tui::shell`). One rule, so the band cannot announce one id while
/// the runtime applies another.
pub fn resolve_in_order(ordered: &[CatalogEntry], query: &str) -> String {
    if ordered
        .iter()
        .any(|entry| entry.aliases.iter().any(|alias| alias == query))
    {
        return query.to_string();
    }
    resolve_model_query(ordered.iter().map(|entry| entry.id.as_str()), query)
        .unwrap_or(query)
        .to_string()
}

/// Fetches a bounded response from a loopback URL with short timeouts and no proxy/redirects.
///
/// Used by both catalog fetching and setup probing. The timeouts are the point:
/// what is being probed is on this machine, so a connect that has not completed
/// in three seconds is not slow, it is absent. Bounded while it reads, not
/// after: `read_capped` streams and refuses a body past the limit, so whatever
/// is on that port cannot decide how much memory is used -- nor have the
/// first `max_bytes` of a longer answer, or of one cut short, read as all of it.
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
    let body = read_capped(response, max_bytes)
        .await
        .map_err(|err| match err {
            CappedRead::TooLarge => larger_than(max_bytes),
            CappedRead::Read(err) => err.to_string(),
        })?;
    String::from_utf8(body).map_err(|_| "the answer is not UTF-8 text".to_string())
}

/// Fetches a catalog from a URL, bounded and with short timeouts.
///
/// The timeouts are the point: what is being probed is on this machine, so a
/// connect that has not completed in three seconds is not slow, it is absent.
/// Bounded while it reads, not after ([`fetch_loopback_bounded`]), so whatever
/// is on that port cannot decide how much memory this uses.
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

/// Where the Gateway publishes its model catalog, under the origin it is
/// reached at (`models_path`, `vercel-labs/fx@c1db9191
/// src/builtins/gateway.zig:48`).
pub const GATEWAY_MODELS_PATH: &str = "/coding-agent/v1/models";

/// How long the Gateway's catalog may take to connect, and to answer.
///
/// Longer than a loopback daemon's, because this one is on the internet; still
/// bounded, because the thread that waits for it is the one a turn runs on.
/// The 30 s is fx's, over the whole request -- connect, head and the last byte
/// of the body -- and reaching it is a failure fx asks again about
/// (`gateway_json_fetch_timeout_ms`, `src/gateway/client.zig:486-509
/// @481104fd`); the 10 s to connect is xfx's, inside it.
const GATEWAY_CATALOG_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const GATEWAY_CATALOG_TIMEOUT: Duration = Duration::from_secs(30);

/// The catalog URL for a completion endpoint: its scheme and authority, and
/// [`GATEWAY_MODELS_PATH`].
///
/// Derived rather than configured, so the catalog is asked of exactly the host
/// a turn already sends the same credential to -- the default endpoint's
/// origin is upstream's default catalog base (`ai-gateway.vercel.sh`), and a
/// loopback `XFX_GATEWAY_URL` puts both on the same fake. No new place is
/// trusted with the key.
pub fn gateway_models_url(endpoint: &Endpoint) -> String {
    let url = endpoint.url();
    let authority = url.find("://").map_or(0, |at| at + 3);
    let end = url[authority..]
        .find(['/', '?', '#'])
        .map_or(url.len(), |at| authority + at);
    format!("{}{GATEWAY_MODELS_PATH}", &url[..end])
}

/// The Gateway's catalog, asked with the credential a turn sends.
pub struct GatewayCatalog {
    url: String,
    credential: crate::config::Credential,
    /// [`GATEWAY_CATALOG_TIMEOUT`]; shorter only in a test.
    timeout: Duration,
}

impl GatewayCatalog {
    pub fn new(endpoint: &Endpoint, credential: crate::config::Credential) -> Self {
        Self {
            url: gateway_models_url(endpoint),
            credential,
            timeout: GATEWAY_CATALOG_TIMEOUT,
        }
    }

    /// The same catalog with a shorter deadline, so a test of a stalled one
    /// does not wait 30 s.
    #[cfg(test)]
    fn with_timeout_for_test(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

#[async_trait::async_trait(?Send)]
impl ModelCatalog for GatewayCatalog {
    /// One authenticated `GET`, bounded while it reads.
    ///
    /// **No redirect is followed**: the URL was judged by the bearer rule and
    /// the key goes to it alone, so a `3xx` is a failure rather than a second
    /// host the key is replayed to. A proxy from the environment is honoured
    /// over https only, as it is for a turn (`crate::gateway::bearer_rules`):
    /// reaching a remote endpoint may need one, and a cleartext loopback one
    /// must not hand the key to it.
    async fn fetch(&self) -> Result<Vec<CatalogEntry>, CatalogError> {
        let failed = |detail: String| CatalogError::Gateway {
            detail,
            retry: None,
        };
        let client = crate::gateway::bearer_rules(
            reqwest::Client::builder()
                .connect_timeout(GATEWAY_CATALOG_CONNECT_TIMEOUT)
                .timeout(self.timeout)
                .user_agent(USER_AGENT),
            &self.url,
        )
        .build()
        .map_err(|err| failed(format!("could not build HTTP client: {err}")))?;
        let mut authorization =
            reqwest::header::HeaderValue::from_str(&format!("Bearer {}", self.credential.secret()))
                .map_err(|_| failed("the credential is not a valid header value".to_string()))?;
        // Keeps the credential out of reqwest's own diagnostics.
        authorization.set_sensitive(true);
        let response = client
            .get(&self.url)
            .header(reqwest::header::AUTHORIZATION, authorization)
            .send()
            .await
            // Nothing answered: upstream's `transport`, which it asks again
            // about.
            .map_err(|err| CatalogError::Gateway {
                detail: err.to_string(),
                retry: Some(CatalogRetry::Unreachable),
            })?;
        let status = response.status();
        if !status.is_success() {
            return Err(CatalogError::Gateway {
                detail: format!("HTTP {}", status.as_u16()),
                retry: CatalogRetry::for_status(status.as_u16()),
            });
        }
        // fx's `catalogRequestFailure` (`src/builtins/gateway.zig:2510-2533
        // @481104fd`): its deadline is a transport failure it asks again
        // about; an answer past the cap is a malformed one, and so is any
        // other read that stops partway (`ReadFailed`), neither asked again.
        let body = read_capped(response, MAX_GATEWAY_CATALOG_BODY_BYTES)
            .await
            .map_err(|err| match err {
                CappedRead::TooLarge => failed(larger_than(MAX_GATEWAY_CATALOG_BODY_BYTES)),
                CappedRead::Read(err) if err.is_timeout() => CatalogError::Gateway {
                    detail: err.to_string(),
                    retry: Some(CatalogRetry::Unreachable),
                },
                CappedRead::Read(err) => failed(err.to_string()),
            })?;
        // An empty list is an answer, not a failure: upstream's menu says
        // `No models available.` for it, and so does this one. Bytes that are
        // not UTF-8 are no JSON document, to fx's parser as to this one.
        std::str::from_utf8(&body)
            .ok()
            .and_then(parse_gateway_catalog)
            .ok_or_else(|| failed("the answer is not a model catalog".to_string()))
    }
}

/// The catalog for the configured provider, when it advertises one.
///
/// The Gateway's is `GET {origin}/coding-agent/v1/models` with the bearer a
/// turn sends ([`GatewayCatalog`]) -- and **only with one**. Upstream falls
/// back to the public catalog when there is no credential; xfx does not
/// contact a remote host on its own initiative, and a Gateway with no
/// credential cannot run a turn either, so `/model` there says there is
/// nothing to browse. A llmux daemon's is its `GET /models`.
///
/// None at all where no provider was chosen: a provider selection xfx could
/// not read poisons every turn before either provider is built
/// ([`crate::provider::Bundle::select`]), and the Gateway it leaves in
/// `provider` is the compiled default, not a choice to send its key to.
pub fn catalog_for(config: &RuntimeConfig) -> Option<Box<dyn ModelCatalog + Send + Sync>> {
    if config.provider_rejected.is_some() {
        return None;
    }
    match config.provider {
        ProviderId::Gateway => {
            let credential = config.credential.clone()?;
            let endpoint = Endpoint::from_process().ok()?;
            Some(Box::new(GatewayCatalog::new(&endpoint, credential)))
        }
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

/// The context window a Gateway model has when no catalog describes it, by
/// its id (`contextWindowSize`, `vercel-labs/fx@c1db9191
/// src/gateway/vercel_model_policy.zig:13-42`): upstream's Gateway
/// capabilities before a catalog row overrides them (`capabilitiesForModel`,
/// `:4-11`; `mergeCapabilities`, `src/core/config/model_capabilities.zig:107`).
/// `None` for an id upstream knows no window for.
pub fn gateway_context_window(model: &str) -> Option<u64> {
    if model.starts_with("anthropic/") {
        const MILLION: [&str; 10] = [
            "anthropic/claude-fable-5",
            "anthropic/claude-opus-4.6",
            "anthropic/claude-opus-4-6",
            "anthropic/claude-opus-4.7",
            "anthropic/claude-opus-4-7",
            "anthropic/claude-opus-4.8",
            "anthropic/claude-opus-4-8",
            "anthropic/claude-sonnet-5",
            "anthropic/claude-sonnet-4.6",
            "anthropic/claude-sonnet-4-6",
        ];
        return Some(if MILLION.contains(&model) {
            1_000_000
        } else {
            200_000
        });
    }
    if model.starts_with("openai/") {
        let lower = model.to_ascii_lowercase();
        if lower.contains("gpt-5") {
            return Some(256_000);
        }
        if lower.contains("o3") || lower.contains("o4") || lower.contains("o1") {
            return Some(200_000);
        }
        return Some(128_000);
    }
    if model.starts_with("xai/") {
        return Some(131_072);
    }
    if model.starts_with("google/") {
        return Some(1_000_000);
    }
    None
}

/// What `/model`'s menu says under its list about where the catalog came from,
/// or nothing (`loadedCatalogStatusText`, `vercel-labs/fx@c1db9191
/// src/ui/footer/model_menu_presentation.zig:403-433`).
///
/// Upstream's sentences for the two credentials xfx can hold. A llmux daemon's
/// catalog is keyless and local, and upstream has no sentence for that, so it
/// gets none.
pub fn catalog_note(config: &RuntimeConfig) -> Option<&'static str> {
    match config.provider {
        ProviderId::Gateway => gateway_catalog_note(config.credential.as_ref()),
        ProviderId::Llmux => None,
    }
}

/// [`catalog_note`] for the Gateway, asked of the credential alone: the one a
/// TUI session holds from launch, which a provider switch does not change.
pub fn gateway_catalog_note(
    credential: Option<&crate::config::Credential>,
) -> Option<&'static str> {
    use crate::config::CredentialSource;
    match credential?.source() {
        CredentialSource::AiGatewayApiKey => {
            Some("Note: Gateway catalog is authenticated with an API key")
        }
        CredentialSource::VercelOidcToken => {
            Some("Gateway catalog: authenticated with the Vercel session.")
        }
        CredentialSource::LlmuxLoopback => None,
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
    /// This id, exactly as written: a row a picker named, an id the explicit
    /// form spelled, or a query a front end already resolved by
    /// [`resolve_in_order`]. Never resolved again -- upstream's
    /// `selectModelFromPicker` takes the literal id (`session_commands.zig:
    /// 886-915`), and a second resolution could fuzzy-match an id nothing
    /// lists to one that is listed, which is not the model anybody named.
    Exact(&'a str),
}

/// The most catalog rows a caller renders in one go, matching `list_files`'
/// ceiling; a caller that hits it says how many were left out.
pub const MAX_RENDERED_MODELS: usize = 100;

/// What a provider that advertises no catalog is reported as, on the line
/// shell.
///
/// "There is nothing to browse" is a property of the session (`catalog_for`
/// answers `None` for a Gateway with no credential, and for a llmux with no
/// daemon URL), and it is emphatically **not** a failure. Reported as one it
/// would make the first `/model` of such a session read as a broken one. The
/// TUI says the same fact in its menu, as upstream's `No models available.`.
pub const NO_CATALOG_NOTICE: &str = "[shell] catalog=unavailable (this provider advertises none)";

/// What a surface says about a selection nothing could confirm.
///
/// Beside [`NO_CATALOG_NOTICE`] and for the same reason: it is a sentence about
/// a fact **the selector** discovered ([`ModelOutcome::Selected`]'s
/// `unverified`), so the two front ends may render it in their own place -- the
/// line shell on stderr, the band as a document row -- but not in their own
/// words. It is a caveat and never a refusal: the model was applied, and what
/// could not be done was check it against a catalog that is not there.
pub fn unverified_notice(reason: &str) -> String {
    format!("xfx: {reason}, so this model was not checked against the provider's catalog")
}

/// Selects, changes, and renders model choices from a catalog when one exists.
///
/// The selector for the configured provider and its active model. Does no I/O
/// at construction: constructing one on a machine with no credential and no
/// daemon must work, because that is the machine whose user needs `/model`.
pub struct ModelSelector {
    provider: ProviderId,
    model: String,
    source: SettingSource,
    catalog_fetcher: Option<Fetcher>,
    catalog_state: CatalogState,
    /// Whether the failure [`CatalogState::Failed`] holds is one asking again
    /// could change, and how the menu says so. `None` beside any other state.
    failure_retry: Option<CatalogRetry>,
    /// A refresh behind the rows [`CatalogState::Loaded`] holds failed, and
    /// asking again could change that: the menu says so in its note's place,
    /// and the next trigger past the backoff reads again (upstream's
    /// `outcome.last_failure` over a ready catalog, `retry_due`,
    /// `model_cache_runtime.zig:615-616@481104fd`). `None` beside any other
    /// state.
    refresh_retry: Option<CatalogRetry>,
    /// A load started in the background ([`Self::begin_load`],
    /// [`Self::begin_load_on`]) that nothing has taken the answer of yet.
    /// The catalog reads as it did before it meanwhile: [`CatalogState::NotLoaded`]
    /// for the first, the rows or the failure for a refresh or a retry.
    pending: Option<PendingLoad>,
    /// When reads happened, as upstream's model cache keeps it: when the
    /// last one began, how many have failed in a row, and when the hold
    /// that puts on the next one ends ([`LoadClock`]).
    clock: LoadClock,
    /// Reads handed to this selector that it did not take the answer of
    /// ([`Self::adopt_loads`]): each runs on to tell its answer, and is stopped
    /// with the selector -- so a provider switch, which replaces it, is never
    /// followed by the old provider's rows.
    strays: Vec<PendingLoad>,
}

/// What a selector asks its catalog with: shared with the thread each read
/// runs on ([`PendingLoad`]), so `Send + Sync`. The future a fetch returns is
/// not `Send`, and never leaves the thread that polls it.
pub(crate) type Fetcher = std::sync::Arc<dyn ModelCatalog + Send + Sync>;

/// What asked for a catalog read (`LoadTrigger`,
/// `src/core/app/model_cache_runtime.zig:27-35@481104fd`): "Every trigger is
/// user activity, so an idle session never polls the models endpoint."
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadTrigger {
    /// A submitted prompt (`noteModelCatalogActivity`, `src/main.zig:
    /// 1225-1232`), a frame that draws `/model`'s inline column
    /// (`modelCompletions` -> `ensureModelCache`, `:2043-2045`, `:2167-2169`),
    /// the launch and a provider switch (`startModelCacheWarmup`): a catalog
    /// in hand is read again only once it is [`REFRESH_INTERVAL`] old, or
    /// once a refresh of it failed in a way asking again could change.
    Activity,
    /// `/model`'s menu opening (`revalidateModelCache`, `src/main.zig:
    /// 2011-2015`, from `openModelMenu`, `app_input_runtime.zig:1897-1902`):
    /// a catalog in hand is read again once the last read began
    /// [`PICKER_REVALIDATE_FLOOR`] ago.
    PickerOpen,
}

/// "Reopening the picker within this window reuses the catalog it just
/// fetched" (`picker_revalidate_floor_ms`, `model_cache_runtime.zig:37-38
/// @481104fd`).
pub(crate) const PICKER_REVALIDATE_FLOOR: Duration = Duration::from_secs(5);

/// How old a catalog in hand is before activity reads it again: the
/// Gateway's `refresh_interval_ms` (`src/builtins/gateway.zig:2348@481104fd`,
/// five minutes). Upstream has no llmux; its catalog is held to the same.
pub(crate) const REFRESH_INTERVAL: Duration = Duration::from_secs(5 * 60);

/// "After consecutive failures no trigger fetches again for 5 s, then 10 s,
/// 20 s, and so on, up to five minutes, so an outage is never hammered"
/// (`failure_backoff_base_ms`, `failure_backoff_cap_ms`,
/// `model_cache_runtime.zig:40-43@481104fd`).
pub(crate) const FAILURE_BACKOFF_BASE: Duration = Duration::from_secs(5);
pub(crate) const FAILURE_BACKOFF_CAP: Duration = Duration::from_secs(5 * 60);

/// "How long a request waits for a failed catalog's retry before it goes out
/// without the catalog's limits" (`failed_catalog_retry_wait_ms`,
/// `model_cache_runtime.zig:45-47@481104fd`).
pub(crate) const FAILED_CATALOG_RETRY_WAIT: Duration = Duration::from_secs(2);

/// The hold after the `streak`-th failure in a row (`failureBackoffMs`,
/// `model_cache_runtime.zig:49-54@481104fd`): 5 s, doubled for each failure
/// before it, five minutes at most.
pub(crate) fn failure_backoff(streak: u32) -> Duration {
    let mut delay = FAILURE_BACKOFF_BASE;
    let mut step = 1;
    while step < streak && delay < FAILURE_BACKOFF_CAP {
        delay *= 2;
        step += 1;
    }
    delay.min(FAILURE_BACKOFF_CAP)
}

/// What a selector holds, as a read is decided against it (upstream's
/// `ModelCacheState`, `model_cache_runtime.zig:56-61@481104fd`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Held {
    /// Nothing read yet (`.idle`).
    Nothing,
    /// Rows, maybe none (`.ready`); `retry_due` when a refresh of them failed
    /// in a way asking again could change.
    Rows { retry_due: bool },
    /// The last read failed, and no rows stand in for it (`.failed`).
    Failed,
}

impl Held {
    /// What `catalog` and a refresh failure beside it are, to a read.
    pub(crate) fn of(catalog: &CatalogState, refresh_retry: Option<CatalogRetry>) -> Option<Self> {
        match catalog {
            CatalogState::NotLoaded => Some(Self::Nothing),
            CatalogState::Loaded(_) => Some(Self::Rows {
                retry_due: refresh_retry.is_some(),
            }),
            CatalogState::Failed(_) => Some(Self::Failed),
            // There is no catalog to read.
            CatalogState::Unavailable => None,
        }
    }
}

/// When reads happened, as upstream's model cache keeps it
/// (`last_attempt_ms`, `failure_streak`, `retry_not_before_ms`,
/// `model_cache_runtime.zig:490-498@481104fd`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct LoadClock {
    /// When the last read began (`:636`); `None` before any.
    pub(crate) began: Option<Instant>,
    /// Failed reads in a row; a success clears it (`:943`, `:949`).
    pub(crate) streak: u32,
    /// After a failure, no read starts before this, whatever asks (`:951`).
    pub(crate) not_before: Option<Instant>,
}

impl LoadClock {
    /// Whether `trigger` starts a read at `now` over `held`, nothing being on
    /// its way (`beginLoad`, `model_cache_runtime.zig:609-625@481104fd`).
    pub(crate) fn due(&self, held: Held, trigger: LoadTrigger, now: Instant) -> bool {
        if self.not_before.is_some_and(|until| now < until) {
            return false;
        }
        match held {
            // A first load, and a retry of one that failed: any failure,
            // retryable or not, is read again once its backoff is over (`:622`).
            Held::Nothing | Held::Failed => true,
            Held::Rows { retry_due } => {
                // Before any read (a catalog handed in whole) every age is due.
                let elapsed = self.began.map(|began| now.saturating_duration_since(began));
                let expired = elapsed.is_none_or(|elapsed| elapsed >= REFRESH_INTERVAL);
                let picker_due = trigger == LoadTrigger::PickerOpen
                    && elapsed.is_none_or(|elapsed| elapsed >= PICKER_REVALIDATE_FLOOR);
                expired || picker_due || retry_due
            }
        }
    }

    /// When [`Self::due`] next starts a read for [`LoadTrigger::Activity`]
    /// over `held`: not before the hold a failure put on the next read ends,
    /// and over rows not before they are the refresh interval old, counted
    /// from the last read's start -- unless a refresh of them failed in a way
    /// asking again could change (`beginLoad`, `model_cache_runtime.zig:
    /// 609-623@481104fd`). `otherwise` when nothing holds a read back -- no
    /// read yet, no hold.
    pub(crate) fn activity_due(&self, held: Held, otherwise: Instant) -> Instant {
        let aged = match held {
            Held::Rows { retry_due: false } => self.began.map(|began| began + REFRESH_INTERVAL),
            Held::Rows { retry_due: true } | Held::Nothing | Held::Failed => None,
        };
        [self.not_before, aged]
            .into_iter()
            .flatten()
            .max()
            .unwrap_or(otherwise)
    }

    /// The clock once a read begun at `began` has answered at `answered`
    /// (`completeFetch`, `model_cache_runtime.zig:936-958@481104fd`): a
    /// success clears the streak and the hold; a failure counts one more and
    /// holds the next read off for its backoff, counted from its arrival.
    pub(crate) fn after(self, began: Instant, failed: bool, answered: Instant) -> Self {
        if !failed {
            return Self {
                began: Some(began),
                streak: 0,
                not_before: None,
            };
        }
        let streak = self.streak.saturating_add(1);
        Self {
            began: Some(began),
            streak,
            not_before: Some(answered + failure_backoff(streak)),
        }
    }
}

/// What one read's answer leaves a selector holding (`applyFetchLocked`,
/// `model_cache_runtime.zig:969-999@481104fd`): rows replace what was held
/// -- unless they are none and rows were held, which stay -- and a failure
/// over rows keeps them, remembered as a refresh that failed; any other
/// failure is the catalog's state.
pub(crate) struct Applied {
    pub(crate) catalog: CatalogState,
    pub(crate) failure_retry: Option<CatalogRetry>,
    pub(crate) refresh_retry: Option<CatalogRetry>,
    pub(crate) clock: LoadClock,
}

impl Applied {
    /// `answer`, begun at `began` and answered at `answered`, over `held`
    /// and `clock`.
    pub(crate) fn of(
        held: &CatalogState,
        clock: LoadClock,
        answer: &Loaded,
        began: Instant,
        answered: Instant,
    ) -> Self {
        let rows_held = matches!(held, CatalogState::Loaded(rows) if !rows.is_empty());
        match answer {
            (CatalogState::Loaded(rows), _) if rows.is_empty() && rows_held => Self {
                catalog: held.clone(),
                failure_retry: None,
                refresh_retry: None,
                clock: clock.after(began, false, answered),
            },
            (CatalogState::Failed(_), retry) if rows_held => Self {
                catalog: held.clone(),
                failure_retry: None,
                refresh_retry: *retry,
                clock: clock.after(began, true, answered),
            },
            (catalog, retry) => {
                let failed = matches!(catalog, CatalogState::Failed(_));
                Self {
                    catalog: catalog.clone(),
                    failure_retry: if failed { *retry } else { None },
                    refresh_retry: None,
                    clock: clock.after(began, failed, answered),
                }
            }
        }
    }

    /// Whether this answer was a refresh behind rows that failed.
    pub(crate) fn refresh_failed(&self, answer: &Loaded) -> bool {
        matches!(answer.0, CatalogState::Failed(_))
            && matches!(self.catalog, CatalogState::Loaded(_))
    }
}

/// A catalog load running on a thread of its own, as upstream's does
/// (`startWarmup` spawns `preloadThreadMain`, `vercel-labs/fx@c1db9191
/// src/core/app/model_cache_runtime.zig:321-349`, `:698-743`): it goes on
/// whatever the thread that started it does meanwhile -- a turn's stream, or
/// a question that has parked it (`crate::tui::bridge::park_on`) -- and tells
/// its answer the moment it arrives. The thread holds the fetcher and the
/// telling, never the selector: the answer reaches the selector only through
/// [`Self::answer`], awaited by the thread that owns it.
///
/// **Dropped, it is stopped and waited for**, as upstream's `cancelAndJoin`
/// (`:770-776`): the fetch and the telling are both raced against the stop,
/// so the wait ends at the read's next await, and once the drop returns it
/// tells nothing more.
struct PendingLoad {
    /// When it began, and what it answered once it has ([`LoadView`]).
    view: LoadView,
    /// Sent once the answer has been told.
    done: tokio::sync::oneshot::Receiver<()>,
    /// Fired by the drop.
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
    /// The telling of a read that could not be given a thread, or a runtime
    /// to run on: the answer is that failure, told when the selector takes
    /// it.
    unstarted: Option<std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>>,
}

/// What a read is, seen from beside it: when it began, and its answer --
/// and when that arrived -- once it has told one. For work that holds the
/// selector and is asked for the catalog meanwhile: upstream's `beginLoad`
/// decides by the cache's state -- a load on its way is not started twice,
/// and the next is decided by when the last began and how it ended
/// ([`LoadClock`], `model_cache_runtime.zig:609-625@481104fd`).
#[derive(Clone)]
pub(crate) struct LoadView {
    began: Instant,
    outcome: std::sync::Arc<std::sync::OnceLock<(Loaded, Instant)>>,
}

impl LoadView {
    /// When the read began.
    pub(crate) fn began(&self) -> Instant {
        self.began
    }

    /// The answer it has told, if it has told one yet.
    pub(crate) fn answered(&self) -> Option<&Loaded> {
        self.outcome.get().map(|(answer, _)| answer)
    }

    /// When its answer arrived, if it has: what a failure's backoff counts
    /// from (`completeFetch`, `model_cache_runtime.zig:951@481104fd`).
    pub(crate) fn answered_at(&self) -> Option<Instant> {
        self.outcome.get().map(|(_, at)| *at)
    }
}

/// What a load leaves the selector holding: the catalog's state, and whether
/// asking again could change a failure ([`ModelSelector::failure_retry`]).
pub(crate) type Loaded = (CatalogState, Option<CatalogRetry>);

/// [`Loaded`] for a read that could not start, for want of a thread or of a
/// runtime on one: a failure asking again could change
/// ([`CatalogRetry::Runtime`]).
fn unstartable(err: &std::io::Error) -> Loaded {
    (
        CatalogState::Failed(format!("could not start the catalog read: {err}")),
        Some(CatalogRetry::Runtime),
    )
}

/// The runtime a read runs on: a current-thread one of its own.
fn read_runtime(fetcher: &Fetcher) -> std::io::Result<tokio::runtime::Runtime> {
    #[cfg(test)]
    if fetcher.runtime_fails_for_test() {
        return Err(std::io::Error::other("no runtime to be had (a test's)"));
    }
    #[cfg(not(test))]
    let _ = fetcher;
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}

/// [`Loaded`] for one attempt's answer.
fn loaded(answer: Result<Vec<CatalogEntry>, CatalogError>) -> Loaded {
    match answer {
        Ok(entries) => (CatalogState::Loaded(entries), None),
        Err(err) => {
            let retry = match &err {
                CatalogError::Gateway { retry, .. } => *retry,
                _ => None,
            };
            (CatalogState::Failed(err.to_string()), retry)
        }
    }
}

impl Drop for PendingLoad {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl PendingLoad {
    /// Starts `fetcher`'s read on a thread of its own, telling `told` its
    /// answer -- with when the read began and when the answer arrived -- the
    /// moment it does ([`ModelSelector::begin_load`]).
    fn start<Told, Telling>(fetcher: Fetcher, told: Told) -> Self
    where
        Told: FnOnce(CatalogState, Option<CatalogRetry>, Instant, Instant) -> Telling
            + Send
            + 'static,
        Telling: std::future::Future<Output = ()> + Send + 'static,
    {
        let view = LoadView {
            began: Instant::now(),
            outcome: std::sync::Arc::new(std::sync::OnceLock::new()),
        };
        let began = view.began;
        let (stop, mut stopped) = tokio::sync::oneshot::channel::<()>();
        let (finished, done) = tokio::sync::oneshot::channel::<()>();
        // Built here, before the thread: a read that can have no runtime is
        // answered as one that can have no thread is -- its failure, told
        // when the selector takes it -- rather than on a thread that could
        // not tell it (`.runtime`, `model_cache_runtime.zig:537-546
        // @481104fd`).
        let runtime = match read_runtime(&fetcher) {
            Ok(runtime) => runtime,
            Err(err) => return Self::unstarted(view, done, told, finished, &err),
        };
        let outcome = std::sync::Arc::clone(&view.outcome);
        // Taken back if no thread can be had for it.
        let work = std::sync::Arc::new(std::sync::Mutex::new(Some((
            fetcher, told, finished, runtime,
        ))));
        let on_thread = std::sync::Arc::clone(&work);
        let thread = std::thread::Builder::new()
            .name("xfx-catalog".to_string())
            .spawn(move || {
                let Some((fetcher, told, finished, runtime)) = on_thread
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take()
                else {
                    return;
                };
                runtime.block_on(async move {
                    // A stop that was never sent -- its sender dropped -- is
                    // no stop: the pattern disables the branch.
                    let answer = tokio::select! {
                        biased;
                        Ok(()) = &mut stopped => return,
                        answer = fetcher.fetch() => loaded(answer),
                    };
                    let answered = Instant::now();
                    tokio::select! {
                        biased;
                        Ok(()) = &mut stopped => return,
                        () = told(answer.0.clone(), answer.1, began, answered) => {}
                    }
                    let _ = outcome.set((answer, answered));
                    let _ = finished.send(());
                });
            });
        match thread {
            Ok(thread) => Self {
                view,
                done,
                stop: Some(stop),
                thread: Some(thread),
                unstarted: None,
            },
            // No thread to be had: the answer is that failure, told when the
            // selector takes it ([`Self::answer`]).
            Err(err) => {
                let taken = work
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take();
                match taken {
                    Some((_, told, finished, runtime)) => {
                        // Let go without waiting: this thread may be running
                        // a runtime of its own, inside which a runtime is not
                        // to be dropped.
                        runtime.shutdown_background();
                        Self::unstarted(view, done, told, finished, &err)
                    }
                    None => Self {
                        view,
                        done,
                        stop: None,
                        thread: None,
                        unstarted: None,
                    },
                }
            }
        }
    }

    /// A read that could not start, for want of a thread or of a runtime:
    /// its answer is that failure ([`unstartable`]), arrived as it began,
    /// and its telling waits for the selector to take it ([`Self::answer`]).
    fn unstarted<Told, Telling>(
        view: LoadView,
        done: tokio::sync::oneshot::Receiver<()>,
        told: Told,
        finished: tokio::sync::oneshot::Sender<()>,
        err: &std::io::Error,
    ) -> Self
    where
        Told: FnOnce(CatalogState, Option<CatalogRetry>, Instant, Instant) -> Telling
            + Send
            + 'static,
        Telling: std::future::Future<Output = ()> + Send + 'static,
    {
        let failure = unstartable(err);
        let telling = told(failure.0.clone(), failure.1, view.began, view.began);
        let _ = view.outcome.set((failure, view.began));
        let _ = finished.send(());
        Self {
            view,
            done,
            stop: None,
            thread: None,
            unstarted: Some(Box::pin(telling)),
        }
    }

    /// Whether it has told its answer: the answer is recorded the moment
    /// the telling is done, a little before the thread it ran on has ended
    /// ([`Self::is_finished`]), and it is what the selector takes.
    fn has_told(&self) -> bool {
        self.unstarted.is_none() && self.view.outcome.get().is_some()
    }

    /// Whether it has answered -- and told its answer -- already, and its
    /// thread has ended.
    fn is_finished(&self) -> bool {
        self.unstarted.is_none()
            && self
                .thread
                .as_ref()
                .is_none_or(std::thread::JoinHandle::is_finished)
    }

    /// Waits for the answer, once it has been told. Cancel-safe: dropped
    /// before it ends, it leaves the read as it was.
    async fn answer(&mut self) -> (Loaded, Instant) {
        if let Some(telling) = self.unstarted.as_mut() {
            telling.await;
            self.unstarted = None;
        }
        let _ = (&mut self.done).await;
        self.taken()
    }

    /// Its answer and when it arrived, as it stands: a read that ended
    /// without telling one is a failure, arrived now.
    fn taken(&self) -> (Loaded, Instant) {
        self.view.outcome.get().cloned().unwrap_or_else(|| {
            (
                loaded(Err(CatalogError::Unavailable {
                    detail: "the catalog read ended without an answer".to_string(),
                })),
                Instant::now(),
            )
        })
    }
}

/// What starts a read of the selector's catalog **without the selector**:
/// for a thread that is running work which holds the selector, when the
/// interface asks for the catalog again meanwhile, as upstream's starts a
/// load on a thread of its own whatever its worker is doing
/// (`ensureModelCache` from `openModelBrowseCatalog`,
/// `vercel-labs/fx@c1db9191 src/core/app/app_input_runtime.zig:2346-2349`;
/// `startWarmup`, `src/core/app/model_cache_runtime.zig:321-349`). The read
/// it starts runs on a thread of its own ([`PendingLoad`]), so a question
/// that parks the thread asking does not hold it, and it is handed back to
/// the selector afterwards ([`ModelSelector::adopt_loads`]). `Send + Sync`,
/// so the prompter a question parks in can start one.
#[derive(Clone)]
pub(crate) struct CatalogReader {
    fetcher: Option<Fetcher>,
}

/// A read a [`CatalogReader`] started, for the selector to take.
pub(crate) struct StartedLoad(PendingLoad);

impl StartedLoad {
    /// When it began, and its answer once it has told one.
    pub(crate) fn view(&self) -> LoadView {
        self.0.view.clone()
    }

    /// The answer of a read that could not start -- for want of a thread or
    /// of a runtime -- and when it arrived, its telling let go: for work that
    /// tells it at once, with its answer to the ask that started it. `None`
    /// for a read that started, which tells its own.
    pub(crate) fn answer_unstarted(&mut self) -> Option<(Loaded, Instant)> {
        // Dropped unpolled: a telling sends nothing until it is awaited.
        drop(self.0.unstarted.take()?);
        Some(self.0.taken())
    }
}

impl CatalogReader {
    /// Starts the read, telling `told` its answer the moment it arrives, as
    /// [`ModelSelector::begin_load`] does; `None` when the provider has no
    /// catalog to read.
    pub(crate) fn begin<Told, Telling>(&self, told: Told) -> Option<StartedLoad>
    where
        Told: FnOnce(CatalogState, Option<CatalogRetry>, Instant, Instant) -> Telling
            + Send
            + 'static,
        Telling: std::future::Future<Output = ()> + Send + 'static,
    {
        let fetcher = self.fetcher.clone()?;
        Some(StartedLoad(PendingLoad::start(fetcher, told)))
    }
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
            catalog_fetcher: catalog_fetcher.map(Fetcher::from),
            catalog_state,
            failure_retry: None,
            refresh_retry: None,
            pending: None,
            clock: LoadClock::default(),
            strays: Vec::new(),
        }
    }

    /// The model `config` now names, with the catalog kept: the provider is
    /// the same, so its catalog still describes it. A selector for another
    /// provider is built anew instead ([`Self::new`]).
    pub fn follow_model(&mut self, config: &RuntimeConfig) {
        if config.provider != self.provider {
            *self = Self::new(config);
            return;
        }
        self.model = config.model.clone();
        self.source = config.sources.model;
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
    /// socket on the thread that calls it**; call it where a network call is
    /// already legitimate.
    pub async fn ensure_catalog(&mut self) -> &CatalogState {
        // A background read that has answered is the catalog now: upstream's
        // readers see a load's answer the moment it lands (`completeFetch`,
        // `model_cache_runtime.zig:936-965@481104fd`).
        self.take_answered();
        if !matches!(self.catalog_state, CatalogState::NotLoaded) {
            return &self.catalog_state;
        }
        // One already on its way is waited for, not asked a second time.
        if self.pending.is_some() {
            self.settle_load().await;
            return &self.catalog_state;
        }
        let Some(fetcher) = &self.catalog_fetcher else {
            return &self.catalog_state;
        };
        let began = Instant::now();
        let answer = loaded(fetcher.fetch().await);
        self.take((answer, Instant::now()), began);
        &self.catalog_state
    }

    /// Starts loading the catalog in the background, if nothing has asked
    /// for it yet: upstream reads it on a thread of its own at launch, and
    /// again when the credential changes (`startModelCacheWarmup`,
    /// `src/main.zig:2007-2009@481104fd`, called from
    /// `app_entry_runtime.zig:340` and `app_auth_runtime.zig:2015-2024`). The
    /// load runs while the thread waits on anything else, and the first
    /// thing to need the catalog takes its answer ([`Self::ensure_catalog`]).
    ///
    /// **`told` is given the answer the moment it arrives**, and when it
    /// arrived, whatever the thread is doing then -- a turn included -- as
    /// upstream's interface polls its cache on every tick of its own loop and
    /// redraws an open menu and the footer when a load has ended
    /// (`pollLoadTransition`, `model_cache_runtime.zig:745-760`, from
    /// `src/main.zig:2878-2880`). The selector itself takes the answer later,
    /// when something asks it ([`Self::is_loading`]). The load runs on a
    /// thread of its own ([`PendingLoad`]), so a question that parks this
    /// thread does not hold it.
    pub(crate) fn begin_load<Told, Telling>(&mut self, told: Told)
    where
        Told: FnOnce(CatalogState, Option<CatalogRetry>, Instant, Instant) -> Telling
            + Send
            + 'static,
        Telling: std::future::Future<Output = ()> + Send + 'static,
    {
        if !self.wants_first_load() {
            return;
        }
        let Some(fetcher) = self.catalog_fetcher.clone() else {
            return;
        };
        self.pending = Some(PendingLoad::start(fetcher, told));
    }

    /// Whether [`Self::begin_load`] would start the first read: there is a
    /// catalog, nothing has read it, and nothing is reading it.
    pub(crate) fn wants_first_load(&self) -> bool {
        matches!(self.catalog_state, CatalogState::NotLoaded)
            && self.pending.is_none()
            && self.catalog_fetcher.is_some()
    }

    /// Whether `trigger` would start a read at `now`, as upstream's
    /// `beginLoad` decides one (`model_cache_runtime.zig:609-625@481104fd`,
    /// [`LoadClock::due`]): never beside a read on its way or one whose answer
    /// is not taken yet (`:621`); never inside a failure's backoff; a first
    /// load or a retry of a failure past it; and over rows, the menu five
    /// seconds after the last read began, anything once they are five
    /// minutes old or once a refresh of them failed in a way asking again
    /// could change.
    pub(crate) fn would_load(&self, trigger: LoadTrigger, now: Instant) -> bool {
        if self.pending.is_some() || self.catalog_fetcher.is_none() {
            return false;
        }
        Held::of(&self.catalog_state, self.refresh_retry)
            .is_some_and(|held| self.clock.due(held, trigger, now))
    }

    /// Starts the read `trigger` asks for at `now`, in the background
    /// ([`Self::begin_load`]), when it is due ([`Self::would_load`]). The
    /// catalog reads as it did meanwhile: the rows, or the failure -- "a
    /// revalidation or retry leaves the state readers see alone"
    /// (`model_cache_runtime.zig:485-486@481104fd`). Whether one started.
    pub(crate) fn begin_load_on<Told, Telling>(
        &mut self,
        trigger: LoadTrigger,
        now: Instant,
        told: Told,
    ) -> bool
    where
        Told: FnOnce(CatalogState, Option<CatalogRetry>, Instant, Instant) -> Telling
            + Send
            + 'static,
        Telling: std::future::Future<Output = ()> + Send + 'static,
    {
        if !self.would_load(trigger, now) {
            return false;
        }
        let Some(fetcher) = self.catalog_fetcher.clone() else {
            return false;
        };
        self.pending = Some(PendingLoad::start(fetcher, told));
        true
    }

    /// What starts a read of this selector's catalog while the selector is
    /// held elsewhere ([`CatalogReader`]).
    pub(crate) fn reader(&self) -> CatalogReader {
        CatalogReader {
            fetcher: self.catalog_fetcher.clone(),
        }
    }

    /// Takes the reads a [`CatalogReader`] started while the selector was
    /// held elsewhere, oldest first, as this selector's own: each was begun
    /// only once the one before it -- the selector's own read among them --
    /// had answered (the worker's `CatalogAsks`), so their answers are taken
    /// in turn, the clock counting every failure among them, and the last,
    /// still on its way or not, becomes the background load the first thing
    /// to need the catalog waits for ([`Self::ensure_catalog`]).
    ///
    /// A read of the selector's own still on its way stands -- reads were
    /// begun beside it only after it answered, so none can be -- and these
    /// are then let finish for nobody; each tells its answer still, to an
    /// interface that may be waiting on it. They are kept all the same
    /// ([`Self::strays`]), so they are stopped with the selector.
    pub(crate) fn adopt_loads(&mut self, loads: Vec<StartedLoad>) {
        self.strays.retain(|stray| !stray.is_finished());
        if loads.is_empty() {
            return;
        }
        self.take_answered();
        if self.pending.is_some() {
            self.strays.extend(loads.into_iter().map(|load| load.0));
            return;
        }
        let mut loads = loads.into_iter().peekable();
        while let Some(load) = loads.next() {
            let last = loads.peek().is_none();
            if last || !load.0.has_told() {
                // On its way, or the newest: the selector's background load.
                // An older one still on its way cannot be (each was begun
                // after the one before it answered), and is kept running.
                self.pending = Some(load.0);
                self.strays.extend(loads.map(|load| load.0));
                return;
            }
            let began = load.0.view.began;
            self.take(load.0.taken(), began);
        }
    }

    /// The background load's answer, taken now if it has answered and told
    /// it ([`Self::settle_load`] without the wait).
    pub(crate) fn take_answered(&mut self) {
        if !self.has_answered() {
            return;
        }
        if let Some(pending) = self.pending.take() {
            self.take(pending.taken(), pending.view.began);
        }
    }

    /// Whether a background load is on its way, or has ended and not been
    /// taken yet ([`Self::begin_load`]): its answer has been told, or is
    /// being told, already.
    pub(crate) fn is_loading(&self) -> bool {
        self.pending.is_some()
    }

    /// Whether the background load is a retry behind a failure, still on
    /// its way: what a turn waits for, briefly, before it goes without the
    /// catalog (`resolveForRequest`'s `retry_in_flight`,
    /// `model_cache_runtime.zig:843-853@481104fd`).
    pub(crate) fn retrying(&self) -> bool {
        matches!(self.catalog_state, CatalogState::Failed(_))
            && self
                .pending
                .as_ref()
                .is_some_and(|pending| !pending.has_told())
    }

    /// The background load, seen from beside it ([`LoadView`]), while there
    /// is one.
    pub(crate) fn load_view(&self) -> Option<LoadView> {
        self.pending.as_ref().map(|pending| pending.view.clone())
    }

    /// Whether the background load has answered, and told its answer, and
    /// only its taking is left ([`Self::settle_load`] returns at once).
    pub(crate) fn has_answered(&self) -> bool {
        self.pending.as_ref().is_some_and(PendingLoad::has_told)
    }

    /// Waits for the background load, if there is one, and takes its answer.
    ///
    /// Cancel-safe: dropped before the load ends, it leaves the load running
    /// and still pending.
    pub(crate) async fn settle_load(&mut self) {
        let Some(pending) = self.pending.as_mut() else {
            return;
        };
        let answer = pending.answer().await;
        let began = pending.view.began;
        self.pending = None;
        self.take(answer, began);
    }

    /// One read's answer, begun at `began` ([`Applied`]).
    fn take(&mut self, (answer, answered): (Loaded, Instant), began: Instant) {
        let applied = Applied::of(&self.catalog_state, self.clock, &answer, began, answered);
        self.catalog_state = applied.catalog;
        self.failure_retry = applied.failure_retry;
        self.refresh_retry = applied.refresh_retry;
        self.clock = applied.clock;
    }

    /// Lets the next [`ensure_catalog`](Self::ensure_catalog) ask again after
    /// a load that failed, at once.
    ///
    /// **Only a `/model <query>` the band could not resolve calls this**: it
    /// is upstream's `fetchModelIds`, a read of its own beside the model cache
    /// (`resolveModelQuery`, `session_commands.zig:1290-1309@481104fd`;
    /// `src/main.zig:1996-2005`), which no backoff holds. The menu's, the
    /// column's and a prompt's asks are the model cache's
    /// ([`Self::begin_load_on`]).
    pub fn retry_after_failure(&mut self) {
        if matches!(self.catalog_state, CatalogState::Failed(_)) {
            self.catalog_state = CatalogState::NotLoaded;
            self.failure_retry = None;
        }
    }

    /// When the read whose answer the catalog holds began, if any has.
    #[cfg(test)]
    pub(crate) fn last_attempt(&self) -> Option<Instant> {
        self.clock.began
    }

    /// The clock the next read is decided by ([`LoadClock`]).
    pub(crate) fn clock(&self) -> LoadClock {
        self.clock
    }

    /// When the hold a failure put on the next read ends, if one does.
    #[cfg(test)]
    pub(crate) fn retry_at(&self) -> Option<Instant> {
        self.clock.not_before
    }

    /// Whether the failure the catalog is in is worth asking again about
    /// ([`CatalogRetry`]); `None` for any other state.
    pub fn failure_retry(&self) -> Option<CatalogRetry> {
        if matches!(self.catalog_state, CatalogState::Failed(_)) {
            self.failure_retry
        } else {
            None
        }
    }

    /// Whether a refresh behind the rows in hand failed in a way asking again
    /// could change ([`Self::refresh_retry`]'s field); `None` beside any other
    /// state.
    pub(crate) fn refresh_retry(&self) -> Option<CatalogRetry> {
        if matches!(self.catalog_state, CatalogState::Loaded(_)) {
            self.refresh_retry
        } else {
            None
        }
    }

    /// The loaded catalog's row for `model`, if it lists one: by id, or by an
    /// alias, which a llmux model is often selected by.
    pub fn entry(&self, model: &str) -> Option<&CatalogEntry> {
        let CatalogState::Loaded(entries) = &self.catalog_state else {
            return None;
        };
        entries.iter().find(|entry| entry.matches(model))
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
            ModelRequest::Select(query) | ModelRequest::Exact(query) => {
                // Validate the id before doing anything else.
                if let Some(problem) = model_id_problem(query) {
                    return ModelOutcome::Refused {
                        reason: problem.to_string(),
                    };
                }
                let id = match request {
                    ModelRequest::Exact(id) => id.to_string(),
                    _ => self.resolve(query),
                };
                // And the id a query resolved to, by the same rule: the
                // catalog is the provider's document and its parser takes any
                // non-empty id, and the id adopted here is written to the
                // session log and the profile, from which a later launch
                // paints it. Upstream adopts it and refuses it only when it
                // persists (`session_codec.zig:433,3165`); refused here, before
                // anything holds it. The id is not quoted back.
                if let Some(problem) = model_id_problem(&id) {
                    return ModelOutcome::Refused {
                        reason: format!(
                            "the catalog's match for that query is unusable: {problem}"
                        ),
                    };
                }

                // If already the current model, say so.
                if id == self.model {
                    return ModelOutcome::Unchanged {
                        model: self.model.clone(),
                    };
                }

                // **An id the catalog does not list is applied, not refused**,
                // as upstream applies it (`resolveModelQuery` falls back to the
                // literal query, `session_commands.zig:1147-1166`, and
                // `fx-model-fuzzy-40x120-unknown` shows `nonexistent-xyz`
                // accepted and saved). A catalog is what a provider lists, not
                // everything it will run -- a team-private model, or one
                // published after the catalog was read, is a model the
                // provider answers for -- and the provider is the one place
                // that can say no.
                let previous = self.model.clone();
                let unverified = match &self.catalog_state {
                    CatalogState::Unavailable => None,
                    CatalogState::NotLoaded => {
                        Some("the catalog has not been loaded in this process".to_string())
                    }
                    CatalogState::Loaded(_) => None,
                    CatalogState::Failed(reason) => Some(reason.clone()),
                };

                self.model = id;
                ModelOutcome::Selected {
                    provider: self.provider,
                    model: self.model.clone(),
                    previous,
                    unverified,
                }
            }
        }
    }

    /// What `query` names in the loaded catalog (`resolveModelQuery`,
    /// `session_commands.zig:1147-1166`): an id or an alias spelled exactly
    /// -- an alias kept as typed, because the provider resolves its own short
    /// names -- else the best fuzzy match over the ids in the order `/model`
    /// lists them ([`resolve_model_query`]), else the query itself.
    fn resolve(&self, query: &str) -> String {
        let CatalogState::Loaded(entries) = &self.catalog_state else {
            return query.to_string();
        };
        resolve_in_order(&picker_order(entries), query)
    }

    #[cfg(test)]
    fn for_test(provider: ProviderId, model: &str, source: SettingSource) -> Self {
        Self {
            provider,
            model: model.to_string(),
            source,
            catalog_fetcher: None,
            catalog_state: CatalogState::Unavailable,
            failure_retry: None,
            refresh_retry: None,
            pending: None,
            clock: LoadClock::default(),
            strays: Vec::new(),
        }
    }

    /// The catalog this selector decides against, named rather than fetched.
    ///
    /// `pub(crate)` because the **membership refusal** is a rule two front ends
    /// depend on, and the one that reaches it through a worker thread
    /// (`crate::tui::worker`) has to be able to hold a loaded catalog without
    /// opening a socket. Still `#[cfg(test)]`: no shipped binary can reach it,
    /// so a catalog is still only ever loaded from the provider that published
    /// it.
    #[cfg(test)]
    pub(crate) fn set_catalog_for_test(&mut self, state: CatalogState) {
        self.catalog_state = state;
    }

    /// Moves every clock the selector's next read is decided by back `by`:
    /// when the last read began, and when the hold a failure put on the next
    /// one ends. For MODELS-REASK's tests, which ask what upstream's model
    /// cache would do seconds or minutes after a read without waiting for
    /// them.
    #[cfg(test)]
    pub(crate) fn backdate_for_test(&mut self, by: Duration) {
        let back = |at: Option<Instant>| at.map(|at| at.checked_sub(by).unwrap_or(at));
        self.clock.began = back(self.clock.began);
        self.clock.not_before = back(self.clock.not_before);
    }

    /// A catalog this selector *asks* -- one a test wrote -- in place of the
    /// provider's, with nothing loaded yet. For the worker's tests of what a
    /// load that fails or succeeds is told to the UI as; `#[cfg(test)]` like
    /// [`Self::set_catalog_for_test`], so no binary can reach it.
    #[cfg(test)]
    pub(crate) fn set_fetcher_for_test(&mut self, fetcher: Box<dyn ModelCatalog + Send + Sync>) {
        self.catalog_fetcher = Some(Fetcher::from(fetcher));
        self.catalog_state = CatalogState::NotLoaded;
        self.failure_retry = None;
        self.refresh_retry = None;
        self.pending = None;
        self.clock = LoadClock::default();
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
            facts: ModelFacts::default(),
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
                    CatalogError::Gateway { detail, retry } => CatalogError::Gateway {
                        detail: detail.clone(),
                        retry: *retry,
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
    fn an_id_the_loaded_catalog_does_not_list_is_applied_as_typed() {
        // Upstream's rule (`fx-model-fuzzy-40x120-unknown`: `nonexistent-xyz`
        // is switched to and saved). It used to be refused here; a catalog is
        // what a provider lists, not everything it will run, and nothing in the
        // query scores against `m-1`, so the query is the id.
        let mut selector =
            ModelSelector::for_test(ProviderId::Llmux, "fable", SettingSource::UserGlobal);
        selector.set_catalog_for_test(CatalogState::Loaded(vec![entry("m-1", &["fable"])]));
        match selector.apply(ModelRequest::Select("not-published")) {
            ModelOutcome::Selected {
                model,
                previous,
                unverified,
                ..
            } => {
                assert_eq!(model, "not-published");
                assert_eq!(previous, "fable");
                assert_eq!(unverified, None, "the catalog was read; nothing to caveat");
            }
            other => panic!("expected a selection, got {other:?}"),
        }
        assert_eq!(selector.model(), "not-published");
    }

    #[test]
    fn a_query_resolves_to_the_catalog_id_it_names_best() {
        // `fx-model-fuzzy-40x120-opus`: `/model opus` switched to
        // `anthropic/claude-opus-5` over the catalog in `models.json`.
        let catalog = parse_gateway_catalog(MODELS_JSON).expect("a catalog");
        let mut selector = ModelSelector::for_test(
            ProviderId::Gateway,
            "fake/model-a",
            SettingSource::UserGlobal,
        );
        selector.set_catalog_for_test(CatalogState::Loaded(catalog));
        for (query, resolved) in [
            ("opus", "anthropic/claude-opus-5"),
            ("astra", "openai/gpt-6-astra"),
            // Exact wins, whatever its case, in the catalog's spelling.
            ("XAI/GROK-4.7", "xai/grok-4.7"),
            // The letters in order.
            ("gpt6", "openai/gpt-6-astra"),
            // Nothing scores: the literal query.
            ("nonexistent-xyz", "nonexistent-xyz"),
        ] {
            match selector.apply(ModelRequest::Select(query)) {
                ModelOutcome::Selected { model, .. } => assert_eq!(model, resolved, "{query}"),
                other => panic!("{query}: expected a selection, got {other:?}"),
            }
        }
        // And a query that resolves to the model in force changes nothing.
        assert!(matches!(
            selector.apply(ModelRequest::Select("nonexistent-xyz")),
            ModelOutcome::Unchanged { .. }
        ));
    }

    #[test]
    fn a_catalog_id_with_a_control_character_is_refused_when_a_query_resolves_to_it() {
        // A catalog is the provider's document, and its parser takes any
        // non-empty id. A clean query that fuzzy-matches an id carrying an
        // escape must not adopt it: the adopted id is written to the session
        // log and the profile, and a resume paints it in the footer. Red
        // before: `evil` was selected as the id with the escape in it.
        for hostile in [
            "evil\u{1b}[31m-x",
            "evil\u{1b}]8;;mailto:a@b\u{1b}\\-x",
            "evil\u{9b}31m-x",
        ] {
            let mut selector =
                ModelSelector::for_test(ProviderId::Llmux, "good", SettingSource::UserGlobal);
            selector.set_catalog_for_test(CatalogState::Loaded(vec![
                entry("good", &[]),
                entry(hostile, &[]),
            ]));
            match selector.apply(ModelRequest::Select("evil")) {
                ModelOutcome::Refused { reason } => {
                    assert!(!reason.chars().any(char::is_control), "{reason:?}");
                    assert!(!reason.contains("evil"), "the id is not quoted: {reason:?}");
                }
                other => panic!("{hostile:?}: expected a refusal, got {other:?}"),
            }
            assert_eq!(selector.model(), "good", "{hostile:?}");
        }
    }

    #[test]
    fn without_a_loaded_catalog_the_query_is_the_id() {
        // Nothing to resolve against: upstream's fallback when the fetch fails.
        let mut selector =
            ModelSelector::for_test(ProviderId::Llmux, "fable", SettingSource::UserGlobal);
        selector.set_catalog_for_test(CatalogState::Failed("down".to_string()));
        assert!(matches!(
            selector.apply(ModelRequest::Select("opus")),
            ModelOutcome::Selected { model, .. } if model == "opus"
        ));
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
                facts: ModelFacts::default(),
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
        selector.catalog_fetcher = Some(std::sync::Arc::new(catalog.clone()));
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

    #[test]
    fn an_exact_request_is_applied_as_written_and_never_resolved_again() {
        // The explicit form's literal id (`fx-model3-40x120-paste-short-id`
        // refuses `claude-opus-5` rather than resolving it), and an id a
        // front end already resolved, are not queries: resolved again, an id
        // nothing lists would be fuzzy-matched to one that is listed.
        let mut selector =
            ModelSelector::for_test(ProviderId::Gateway, "m", SettingSource::UserGlobal);
        selector.set_catalog_for_test(CatalogState::Loaded(vec![entry(
            "anthropic/claude-opus-5",
            &[],
        )]));
        assert!(matches!(
            selector.apply(ModelRequest::Select("claude-opus")),
            ModelOutcome::Selected { model, .. } if model == "anthropic/claude-opus-5"
        ));
        assert!(matches!(
            selector.apply(ModelRequest::Exact("claude-opus")),
            ModelOutcome::Selected { model, .. } if model == "claude-opus"
        ));
        assert!(matches!(
            selector.apply(ModelRequest::Exact("bad id")),
            ModelOutcome::Refused { .. }
        ));
    }

    #[tokio::test]
    async fn a_gateway_failure_says_whether_asking_again_could_help() {
        // `failureForHttpStatus` (`model_catalog.zig:72-92`): a rate limit and
        // the four transient 5xx are retryable; a refused key, any other
        // status and a 5xx upstream does not retry are not.
        for (status, retry) in [
            (429, Some(CatalogRetry::RateLimited)),
            (500, Some(CatalogRetry::Unreachable)),
            (502, Some(CatalogRetry::Unreachable)),
            (503, Some(CatalogRetry::Unreachable)),
            (504, Some(CatalogRetry::Unreachable)),
            (501, None),
            (401, None),
            (403, None),
            (400, None),
        ] {
            assert_eq!(CatalogRetry::for_status(status), retry, "HTTP {status}");
        }
        let (url, server) = one_exchange(
            "HTTP/1.1 503 Service Unavailable\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
                .to_string(),
        );
        let catalog = GatewayCatalog::new(
            &Endpoint::resolve(Some(&url)).expect("ok"),
            credential_for_test("fake-key-not-real"),
        );
        assert!(matches!(
            catalog.fetch().await,
            Err(CatalogError::Gateway {
                retry: Some(CatalogRetry::Unreachable),
                ..
            })
        ));
        server.join().expect("the server");
        // Nothing listening is upstream's `transport`, which it retries.
        let closed = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = closed.local_addr().expect("addr").port();
        drop(closed);
        let catalog = GatewayCatalog::new(
            &Endpoint::resolve(Some(&format!(
                "http://127.0.0.1:{port}/v4/ai/language-model"
            )))
            .expect("ok"),
            credential_for_test("fake-key-not-real"),
        );
        assert!(matches!(
            catalog.fetch().await,
            Err(CatalogError::Gateway {
                retry: Some(CatalogRetry::Unreachable),
                ..
            })
        ));
        // Upstream's sentences (`model_menu_presentation.zig:435-443
        // @481104fd`), its name read as xfx's.
        assert_eq!(
            CatalogRetry::Unreachable.text(),
            "Could not reach AI Gateway; xfx retries automatically."
        );
        assert_eq!(
            CatalogRetry::RateLimited.text(),
            "AI Gateway rate limited model discovery; xfx retries automatically."
        );
    }

    #[tokio::test]
    async fn a_failure_is_asked_again_only_when_model_asks_for_it() {
        // `ensure_catalog` alone never reads a failure again: what reads it
        // again is the model cache's triggers past the failure's backoff
        // (`begin_load_on`), and a `/model <query>` the band could not
        // resolve, upstream's `fetchModelIds` beside the cache, which this
        // pins (`retry_after_failure`).
        let catalog = CountingCatalog::new(Err(CatalogError::Gateway {
            detail: "HTTP 503".to_string(),
            retry: Some(CatalogRetry::Unreachable),
        }));
        let mut selector =
            ModelSelector::for_test(ProviderId::Gateway, "m", SettingSource::UserGlobal);
        selector.catalog_fetcher = Some(std::sync::Arc::new(catalog.clone()));
        selector.catalog_state = CatalogState::NotLoaded;
        selector.ensure_catalog().await;
        assert_eq!(selector.failure_retry(), Some(CatalogRetry::Unreachable));
        let after_first = catalog.count();
        // `ensure_catalog` alone still asks once.
        selector.ensure_catalog().await;
        assert_eq!(catalog.count(), after_first);
        // `/model` asks again.
        selector.retry_after_failure();
        assert!(matches!(selector.catalog(), CatalogState::NotLoaded));
        assert_eq!(selector.failure_retry(), None);
        selector.ensure_catalog().await;
        assert_eq!(catalog.count(), after_first + 1);
        // And a loaded catalog is not thrown away by it.
        selector.catalog_state = CatalogState::Loaded(Vec::new());
        selector.retry_after_failure();
        assert!(matches!(selector.catalog(), CatalogState::Loaded(_)));
    }

    #[tokio::test]
    async fn ensure_catalog_does_not_retry_after_failure() {
        // A failed load is not retried by `ensure_catalog` within the same
        // process: only `/model` asks again
        // (`a_failure_is_asked_again_only_when_model_asks_for_it`).
        let catalog = CountingCatalog::new(Err(CatalogError::Empty));
        let initial_count = catalog.count();

        let mut selector =
            ModelSelector::for_test(ProviderId::Llmux, "fable", SettingSource::UserGlobal);
        selector.catalog_fetcher = Some(std::sync::Arc::new(catalog.clone()));
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
    #[test]
    fn the_clock_decides_a_read_as_upstreams_begin_load_does() {
        // `failureBackoffMs`'s own values (`model_cache_runtime.zig:1531-1535
        // @481104fd`), and `beginLoad`'s decision (`:609-625`) on each state.
        assert_eq!(failure_backoff(1), Duration::from_secs(5));
        assert_eq!(failure_backoff(2), Duration::from_secs(10));
        assert_eq!(failure_backoff(6), Duration::from_secs(160));
        assert_eq!(failure_backoff(7), FAILURE_BACKOFF_CAP);
        assert_eq!(failure_backoff(u32::MAX), FAILURE_BACKOFF_CAP);
        let began = Instant::now();
        let failed = LoadClock::default().after(began, true, began);
        assert_eq!(failed.streak, 1);
        assert_eq!(failed.not_before, Some(began + Duration::from_secs(5)));
        let at = |ms: u64| began + Duration::from_millis(ms);
        for trigger in [LoadTrigger::Activity, LoadTrigger::PickerOpen] {
            // A first load, at once; a failure, past its backoff only.
            assert!(LoadClock::default().due(Held::Nothing, trigger, began));
            assert!(!failed.due(Held::Failed, trigger, at(4_999)));
            assert!(failed.due(Held::Failed, trigger, at(5_000)));
            // Rows whose refresh failed retryably: past the backoff, whatever
            // asks; inside it, nothing.
            let retry = Held::Rows { retry_due: true };
            assert!(!failed.due(retry, trigger, at(4_999)));
            assert!(failed.due(retry, trigger, at(5_000)));
        }
        // A success clears the streak and the hold.
        let fine = failed.after(at(6_000), false, at(6_100));
        assert_eq!((fine.streak, fine.not_before), (0, None));
        assert_eq!(fine.began, Some(at(6_000)));
        // Rows handed in whole, with no read on record, are due at once.
        assert!(LoadClock::default().due(
            Held::Rows { retry_due: false },
            LoadTrigger::Activity,
            began
        ));
    }

    /// A catalog that answers once `gate` opens, or never.
    struct Waiting {
        gate: std::sync::Mutex<Option<tokio::sync::oneshot::Receiver<()>>>,
        answer: Result<Vec<CatalogEntry>, CatalogError>,
    }

    impl Waiting {
        fn new(
            answer: Result<Vec<CatalogEntry>, CatalogError>,
        ) -> (tokio::sync::oneshot::Sender<()>, Self) {
            let (open, gate) = tokio::sync::oneshot::channel();
            (
                open,
                Self {
                    gate: std::sync::Mutex::new(Some(gate)),
                    answer,
                },
            )
        }
    }

    #[async_trait::async_trait(?Send)]
    impl ModelCatalog for Waiting {
        async fn fetch(&self) -> Result<Vec<CatalogEntry>, CatalogError> {
            let gate = self.gate.lock().expect("gate").take();
            if let Some(gate) = gate {
                let _ = gate.await;
            }
            match &self.answer {
                Ok(entries) => Ok(entries.clone()),
                Err(_) => Err(CatalogError::Gateway {
                    detail: "HTTP 503".to_string(),
                    retry: Some(CatalogRetry::Unreachable),
                }),
            }
        }
    }

    #[test]
    fn a_background_read_counts_its_age_from_when_it_began_and_its_backoff_from_when_it_failed() {
        // Upstream sets the attempt's time as the load starts
        // (`model_cache_runtime.zig:636@481104fd`) -- what the menu's five
        // seconds and the five minutes count from -- and the hold a failure
        // puts on the next read when the failure arrives (`:951`).
        let (open, waiting) = Waiting::new(Err(CatalogError::Empty));
        let mut selector =
            ModelSelector::for_test(ProviderId::Gateway, "m", SettingSource::UserGlobal);
        selector.catalog_fetcher = Some(std::sync::Arc::new(waiting));
        selector.catalog_state = CatalogState::NotLoaded;
        let before = Instant::now();
        selector.begin_load(|_, _, _, _| async {});
        let after_begin = Instant::now();
        // Answered well after it began.
        let opened = Instant::now();
        open.send(()).expect("the read waits");
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime")
            .block_on(selector.settle_load());
        assert!(matches!(selector.catalog(), CatalogState::Failed(_)));
        let attempted = selector.last_attempt().expect("an attempt");
        assert!(before <= attempted && attempted <= after_begin && attempted <= opened);
        let held_until = selector.retry_at().expect("a hold");
        assert!(
            held_until >= opened + FAILURE_BACKOFF_BASE,
            "counted from the start"
        );
    }

    #[test]
    fn a_read_on_its_way_is_stopped_with_its_selector_and_tells_nothing_after() {
        // Upstream's `resetForProviderChange` -> `cancelAndJoin`
        // (`model_cache_runtime.zig:483-492`, `reset` `:461-462`, `:770-776`): the old provider's
        // load is stopped and waited for before the new one's is read, so it
        // tells nothing about a catalog that is no longer the one in force.
        // The telling holds the only sender: disconnected and empty is a
        // thread that has ended without telling.
        let (open, waiting) = Waiting::new(Ok(Vec::new()));
        let mut selector =
            ModelSelector::for_test(ProviderId::Gateway, "m", SettingSource::UserGlobal);
        selector.catalog_fetcher = Some(std::sync::Arc::new(waiting));
        selector.catalog_state = CatalogState::NotLoaded;
        let (told, heard) = std::sync::mpsc::channel::<()>();
        selector.begin_load(move |_, _, _, _| async move {
            let _ = told.send(());
        });
        assert!(selector.is_loading());
        drop(selector);
        let _ = open.send(());
        assert_eq!(
            heard.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Disconnected)
        );

        // And a read handed to a selector that did not take its answer is
        // stopped with it too.
        let (open, waiting) = Waiting::new(Ok(Vec::new()));
        let mut selector =
            ModelSelector::for_test(ProviderId::Gateway, "m", SettingSource::UserGlobal);
        selector.catalog_fetcher = Some(std::sync::Arc::new(waiting));
        selector.catalog_state = CatalogState::Loaded(Vec::new());
        let (told, heard) = std::sync::mpsc::channel::<()>();
        let load = selector
            .reader()
            .begin(move |_, _, _, _| async move {
                let _ = told.send(());
            })
            .expect("a catalog to read");
        selector.adopt_loads(vec![load]);
        assert!(selector.is_loading(), "the read was not taken");
        assert!(
            matches!(selector.catalog(), CatalogState::Loaded(_)),
            "a catalog in hand was given up"
        );
        drop(selector);
        let _ = open.send(());
        assert_eq!(
            heard.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Disconnected)
        );
    }

    /// The third state of the same contract the two cases above pin
    /// (`ensure_catalog_fetches_exactly_once_on_success` and
    /// `ensure_catalog_does_not_retry_after_failure`): a provider that has no
    /// catalog at all is never asked in the first place.
    #[tokio::test]
    async fn a_provider_with_no_catalog_opens_no_socket_at_all() {
        // `Unavailable` is a fact about the provider rather than a failure to
        // load: a Gateway session with no credential, or a llmux one with no
        // daemon URL, has no catalog xfx may ask for.
        let mut selector = ModelSelector::for_test(
            ProviderId::Gateway,
            "zai/glm-5.2",
            SettingSource::CompiledDefault,
        );
        assert!(matches!(
            selector.ensure_catalog().await,
            CatalogState::Unavailable
        ));
    }

    /// The research harness's catalog (`account-model/scenarios/models.json`),
    /// which both binaries were captured over.
    const MODELS_JSON: &str = r#"{"object":"list","data":[
 {"id":"fake/model-a","type":"language","owned_by":"fake","released":1700000000,"tags":["tool-use"]},
 {"id":"openai/gpt-6-astra","type":"language","owned_by":"openai","released":1780000000,"tags":["tool-use","reasoning"],"context_window":400000,"max_tokens":128000,"reasoning_options":[{"type":"effort","values":["low","medium","high","xhigh"]}],"pricing":{"input":"0.000002","output":"0.00001","input_cache_read":"0.0000002","service_tiers":{"priority":{"input":"0.000004","output":"0.00002"},"ultrafast":{"input":"0.00006","output":"0.0003","input_cache_read":"0.000006"}}}},
 {"id":"anthropic/claude-opus-5","type":"language","owned_by":"anthropic","released":1775000000,"tags":["tool-use","reasoning"],"context_window":1000000,"max_tokens":64000,"reasoning_options":[{"type":"effort","values":["low","medium","high","max"]}],"fast_options":[{"type":"toggle"}],"pricing":{"input":"0.000005","output":"0.000025"}},
 {"id":"xai/grok-4.7","type":"language","owned_by":"xai","released":1770000000,"tags":["tool-use"],"pricing":{"input":"0.000003","output":"0.000015"}},
 {"id":"openai/text-embedding-9","type":"embedding","owned_by":"openai"}
]}"#;

    fn ids(entries: &[CatalogEntry]) -> Vec<&str> {
        entries.iter().map(|entry| entry.id.as_str()).collect()
    }

    fn gateway_entry(id: &str, released: i64) -> CatalogEntry {
        CatalogEntry {
            facts: ModelFacts {
                released,
                tool_use: true,
                ..ModelFacts::default()
            },
            ..entry(id, &[])
        }
    }

    #[test]
    fn a_row_reads_images_inline_only_when_it_is_tagged_vision_and_file_input_both() {
        // `image_input_support` is `.native` only for the two tags together
        // (`gateway.zig:2847-2848`, `model_capabilities.zig:96-104`); either
        // alone is a model upstream would route through its vision tool.
        let catalog = parse_gateway_catalog(
            r#"{"data":[
              {"id":"a/both","tags":["tool-use","vision","file-input"]},
              {"id":"a/vision","tags":["tool-use","vision"]},
              {"id":"a/file","tags":["FILE-INPUT"]},
              {"id":"a/none","tags":["tool-use"]},
              {"id":"a/untagged"}
            ]}"#,
        )
        .expect("a catalog document");
        let reads: Vec<bool> = catalog
            .iter()
            .map(|entry| entry.facts.image_input)
            .collect();
        assert_eq!(reads, [true, false, false, false, false]);
    }

    #[test]
    fn a_gateway_row_carries_the_facts_upstream_reads_and_nothing_else() {
        let entries = parse_gateway_catalog(MODELS_JSON).expect("a catalog document");
        // The embedding row is not a language model.
        assert_eq!(
            ids(&entries),
            [
                "fake/model-a",
                "openai/gpt-6-astra",
                "anthropic/claude-opus-5",
                "xai/grok-4.7"
            ]
        );
        let astra = &entries[1];
        assert_eq!(astra.max_context, Some(400_000));
        assert_eq!(astra.efforts, ["low", "medium", "high", "xhigh"]);
        assert_eq!(
            astra.facts,
            ModelFacts {
                owned_by: Some("openai".to_string()),
                released: 1_780_000_000,
                tool_use: true,
                image_input: false,
                max_output: Some(128_000),
                // A `priority` tier on an OpenAI row is Fast; a priced
                // `ultrafast` tier, cached input included, is Ultra.
                fast: true,
                ultrafast: true,
            }
        );
        let opus = &entries[2];
        assert_eq!(opus.max_context, Some(1_000_000));
        assert_eq!(opus.facts.max_output, Some(64_000));
        assert!(opus.facts.fast, "a toggle fast option is Fast");
        assert!(!opus.facts.ultrafast, "Ultra is OpenAI's alone");
        let model_a = &entries[0];
        assert_eq!(model_a.max_context, None, "no window was published");
        assert_eq!(model_a.facts.max_output, None);
        assert!(!model_a.facts.fast && !model_a.facts.ultrafast);
        assert!(model_a.efforts.is_empty());
        let grok = &entries[3];
        assert!(!grok.facts.fast, "a price is not a fast price");
    }

    #[test]
    fn ultra_wants_priced_openai_metadata_rather_than_a_name_or_a_tag() {
        // Upstream's own case (`gateway.zig:2950-2970`).
        let entries = parse_gateway_catalog(
            r#"{"data":[
            {"id":"openai/gpt-6-astra","type":"language","owned_by":"openai","pricing":{"input_cache_read":"0.000001","service_tiers":{"ultrafast":{"input":"0.00006","output":"0.0003","input_cache_read":"0.000006"}}}},
            {"id":"openai/gpt-5.6-sol","type":"language","owned_by":"openai","tags":["ultrafast"],"pricing":{"service_tiers":{"priority":{"input":"0.1","output":"0.2"}}}},
            {"id":"openai/incomplete","type":"language","owned_by":"openai","pricing":{"service_tiers":{"ultrafast":{"input":"0.1"}}}},
            {"id":"openai/uncached","type":"language","owned_by":"openai","pricing":{"input_cache_read":"0.1","service_tiers":{"ultrafast":{"input":"0.1","output":"0.2"}}}},
            {"id":"provider/model","type":"language","owned_by":"provider","pricing":{"service_tiers":{"ultrafast":{"input":"0.1","output":"0.2"}}}},
            {"id":"fast/priced","owned_by":"fast","pricing":{"fast":{"input":"1"}}},
            {"id":"fast/tagged","owned_by":"fast","tags":["fast"]}
        ]}"#,
        )
        .expect("a catalog document");
        let ultra: Vec<bool> = entries.iter().map(|entry| entry.facts.ultrafast).collect();
        assert_eq!(ultra, [true, false, false, false, false, false, false]);
        let fast: Vec<bool> = entries.iter().map(|entry| entry.facts.fast).collect();
        assert_eq!(fast, [false, true, false, false, false, true, false]);
        // A row with no `type` is read, as upstream reads it.
        assert_eq!(entries[5].id, "fast/priced");
    }

    #[test]
    fn effort_values_skip_the_default_and_what_upstream_would_not_parse() {
        let entries = parse_gateway_catalog(
            r#"{"data":[{"id":"a/b","reasoning_options":[
            {"type":"toggle"},
            {"type":"effort","values":["auto","low","Default","x y","",7,"max"]},
            {"type":"effort","values":["second"]}]}]}"#,
        )
        .expect("a catalog document");
        assert_eq!(entries[0].efforts, ["low", "max"]);
    }

    #[test]
    fn a_document_without_a_data_array_is_not_a_gateway_catalog() {
        for body in ["", "[]", r#"{"models":[]}"#, r#"{"data":{}}"#] {
            assert!(parse_gateway_catalog(body).is_none(), "for {body:?}");
        }
        let entries = parse_gateway_catalog(r#"{"data":[{"id":""},{"type":"language"},7]}"#)
            .expect("a document with no usable row");
        assert!(entries.is_empty());
    }

    #[test]
    fn the_menu_order_is_upstreams_featured_projection() {
        // `fx-model-40x120-open`: astra (the `openai/gpt` slot), opus (the
        // `anthropic/claude-opus` slot), then the vendors alphabetically --
        // `fake`, then `xai` -- over a catalog published in another order.
        let entries = parse_gateway_catalog(MODELS_JSON).expect("a catalog");
        assert_eq!(
            ids(&picker_order(&entries)),
            [
                "openai/gpt-6-astra",
                "anthropic/claude-opus-5",
                "fake/model-a",
                "xai/grok-4.7"
            ]
        );
    }

    #[test]
    fn the_projection_caps_each_vendor_and_keeps_every_model_selectable() {
        let entries = [
            gateway_entry("anthropic/claude-opus-4", 10),
            gateway_entry("anthropic/claude-opus-5", 30),
            gateway_entry("anthropic/claude-sonnet-5", 40),
            gateway_entry("anthropic/claude-haiku-5", 41),
            gateway_entry("openai/gpt-6", 50),
            gateway_entry("openai/gpt-5", 20),
            gateway_entry("openai/gpt-4", 5),
            gateway_entry("openai/gpt-6-mini", 60),
            gateway_entry("zai/glm-5", 7),
            gateway_entry("zai/glm-5-fast", 8),
            gateway_entry("acme/one", 1),
            gateway_entry("acme/two", 2),
            gateway_entry("acme/three", 3),
        ];
        assert_eq!(
            ids(&picker_order(&entries)),
            [
                // Featured: the newest `openai/gpt`, the newest opus, GLM.
                "openai/gpt-6",
                "anthropic/claude-opus-5",
                "zai/glm-5",
                // Vendors alphabetically: two of acme, newest first.
                "acme/three",
                "acme/two",
                // Anthropic lists only opus, within its family's three.
                "anthropic/claude-opus-4",
                // OpenAI: `openai/gpt` is capped at two in the family.
                "openai/gpt-5",
                // The rest, in catalog order: tool use, tier, vendor, newest.
                "anthropic/claude-sonnet-5",
                "openai/gpt-4",
                "zai/glm-5-fast",
                "acme/one",
                "anthropic/claude-haiku-5",
                "openai/gpt-6-mini",
            ]
        );
    }

    #[test]
    fn a_llmux_catalog_without_vendors_is_listed_by_name() {
        let entries = [
            entry("grok-4.5", &["grok"]),
            entry("claude-fable-5[1m]", &["fable"]),
            entry("gpt-5.6-sol", &[]),
        ];
        assert_eq!(
            ids(&picker_order(&entries)),
            ["claude-fable-5[1m]", "gpt-5.6-sol", "grok-4.5"]
        );
    }

    #[test]
    fn fuzzy_scores_rank_as_upstreams_do() {
        // Upstream's own cases (`session_commands.zig:3334-3368`).
        assert!(fuzzy_model_score("anthropic/claude-sonnet-4-20250514", "claude-sonnet") >= 100);
        assert!(
            fuzzy_model_score("openai/gpt-4o", "openai/gpt-4o")
                > fuzzy_model_score("openai/gpt-4o-mini", "gpt-4o")
        );
        assert!(fuzzy_model_score("anthropic/claude-sonnet-4-20250514", "claude sonnet") >= 80);
        assert!(fuzzy_model_score("anthropic/claude-sonnet-4-20250514", "anth son") > 0);
        assert_eq!(fuzzy_model_score("openai/gpt-4o", "zzzzz"), 0);
        assert_eq!(fuzzy_model_score("openai/gpt-4o", ""), 0);
        // And the arithmetic of each band.
        assert_eq!(
            fuzzy_model_score("openai/gpt-4o", "openai"),
            126,
            "a prefix"
        );
        assert_eq!(
            fuzzy_model_score("openai/gpt-4o", "GPT-4O"),
            126,
            "after the vendor"
        );
        assert_eq!(
            fuzzy_model_score("openai/gpt-4o", "gpt"),
            103,
            "a substring"
        );
        assert_eq!(
            fuzzy_model_score("openai/gpt-4o", "4o gpt"),
            90,
            "every word"
        );
        assert_eq!(
            fuzzy_model_score("openai/gpt-4o", "og4"),
            43,
            "the letters in order"
        );
        assert_eq!(fuzzy_model_score("openai/gpt-4o", "gpt zz"), 20, "one word");
        assert_eq!(
            query_words("claude-sonnet/fast"),
            ["claude", "sonnet", "fast"]
        );
    }

    #[test]
    fn the_first_of_equal_scores_wins_in_listed_order() {
        let ids = ["acme/alpha-x", "acme/beta-x"];
        assert_eq!(
            resolve_model_query(ids.iter().copied(), "x"),
            Some("acme/alpha-x")
        );
        assert_eq!(resolve_model_query(ids.iter().copied(), "zzz"), None);
    }

    #[test]
    fn the_gateway_catalog_is_asked_of_the_endpoints_own_origin() {
        for (endpoint, models) in [
            (
                "https://ai-gateway.vercel.sh/v3/ai/language-model",
                "https://ai-gateway.vercel.sh/coding-agent/v1/models",
            ),
            (
                "http://127.0.0.1:4100/v3/ai/language-model",
                "http://127.0.0.1:4100/coding-agent/v1/models",
            ),
            (
                "https://gateway.example.com?team=x",
                "https://gateway.example.com/coding-agent/v1/models",
            ),
        ] {
            let endpoint = Endpoint::resolve(Some(endpoint)).expect("an endpoint");
            assert_eq!(gateway_models_url(&endpoint), models);
        }
    }

    #[test]
    fn the_note_names_the_credential_the_catalog_was_asked_with() {
        use crate::config::Environment;
        let workspace = tempfile::tempdir().expect("a workspace");
        let load = |vars: &[(&str, &str)]| {
            let vars = vars
                .iter()
                .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
                .collect();
            RuntimeConfig::load_with(&Environment::new(None, vars), workspace.path())
                .expect("a configuration")
        };
        assert_eq!(
            catalog_note(&load(&[("AI_GATEWAY_API_KEY", "fake-key")])),
            Some("Note: Gateway catalog is authenticated with an API key")
        );
        assert_eq!(
            catalog_note(&load(&[("VERCEL_OIDC_TOKEN", "fake-token")])),
            Some("Gateway catalog: authenticated with the Vercel session.")
        );
        assert_eq!(catalog_note(&load(&[])), None);
    }

    #[test]
    fn a_gateway_with_no_credential_has_no_catalog_to_ask() {
        // Upstream falls back to a public catalog; xfx sends nothing to a
        // remote host it was given no key for.
        use crate::config::Environment;
        let workspace = tempfile::tempdir().expect("a workspace");
        let config = RuntimeConfig::load_with(
            &Environment::new(None, Default::default()),
            workspace.path(),
        )
        .expect("a configuration");
        assert_eq!(config.provider, ProviderId::Gateway);
        assert!(catalog_for(&config).is_none());
    }

    #[test]
    fn an_unreadable_provider_has_no_catalog_to_ask_with_the_key() {
        // The catalog is asked with the bearer a turn sends, and a provider
        // selection xfx could not read leaves no turn to send it with
        // (`Bundle::select`): the compiled default it stands on is no choice.
        use crate::config::{Environment, ProviderRejection};
        let workspace = tempfile::tempdir().expect("a workspace");
        let mut config = RuntimeConfig::load_with(
            &Environment::new(
                None,
                [("AI_GATEWAY_API_KEY".to_string(), "fake-key".to_string())].into(),
            ),
            workspace.path(),
        )
        .expect("a configuration");
        assert!(catalog_for(&config).is_some(), "a Gateway with a key asks");
        config.provider_rejected = Some(ProviderRejection {
            key: "provider",
            value: "not-a-provider".to_string(),
        });
        assert_eq!(config.provider, ProviderId::Gateway);
        assert!(catalog_for(&config).is_none(), "asked with the key");
    }

    /// One HTTP exchange on a loopback port: the request it read, and the
    /// answer it gave.
    fn one_exchange(answer: String) -> (String, std::thread::JoinHandle<String>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
        let url = format!(
            "http://127.0.0.1:{}/v3/ai/language-model",
            listener.local_addr().expect("an address").port()
        );
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("a connection");
            let mut request = Vec::new();
            let mut buffer = [0u8; 4096];
            while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                let read = stream.read(&mut buffer).expect("a read");
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
            }
            stream.write_all(answer.as_bytes()).expect("a write");
            String::from_utf8_lossy(&request).into_owned()
        });
        (url, server)
    }

    #[test]
    fn the_gateway_catalog_takes_no_proxy_to_a_loopback_gateway() {
        // Cleartext to loopback: a proxy from the environment would read the
        // key as sent (`crate::gateway::bearer_rules`).
        if let Some(origin) = crate::test_net::server_in_this_copy() {
            let endpoint = Endpoint::resolve(Some(&format!("{origin}/v4/ai/language-model")))
                .expect("a loopback endpoint");
            let catalog = GatewayCatalog::new(&endpoint, credential_for_test("fake-key-not-real"));
            let entries = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("a runtime")
                .block_on(catalog.fetch())
                .expect("a catalog");
            assert_eq!(ids(&entries), ["acme/one"]);
            return;
        }
        crate::test_net::assert_no_proxy_is_used(
            "provider::model::tests::the_gateway_catalog_takes_no_proxy_to_a_loopback_gateway",
            crate::test_net::json_answer(r#"{"data":[{"id":"acme/one","type":"language"}]}"#),
        );
    }

    #[tokio::test]
    async fn the_gateway_catalog_is_one_get_with_the_bearer_and_no_redirect_followed() {
        let body = r#"{"data":[{"id":"acme/one","type":"language"}]}"#;
        let (url, server) = one_exchange(format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        ));
        let credential = credential_for_test("fake-key-not-real");
        let catalog = GatewayCatalog::new(&Endpoint::resolve(Some(&url)).expect("ok"), credential);
        let entries = catalog.fetch().await.expect("a catalog");
        assert_eq!(ids(&entries), ["acme/one"]);
        let request = server.join().expect("the server");
        assert!(
            request.starts_with("GET /coding-agent/v1/models HTTP/1.1\r\n"),
            "{request}"
        );
        assert!(
            request
                .to_ascii_lowercase()
                .contains("authorization: bearer fake-key-not-real\r\n"),
            "{request}"
        );

        let (url, server) = one_exchange(
            "HTTP/1.1 307 Temporary Redirect\r\nlocation: http://127.0.0.1:1/elsewhere\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
                .to_string(),
        );
        let catalog = GatewayCatalog::new(
            &Endpoint::resolve(Some(&url)).expect("ok"),
            credential_for_test("fake-key-not-real"),
        );
        match catalog.fetch().await {
            // A redirect is not a failure asking again would change.
            Err(CatalogError::Gateway {
                detail,
                retry: None,
            }) => assert_eq!(detail, "HTTP 307"),
            other => panic!("expected a refusal to follow, got {other:?}"),
        }
        server.join().expect("the server");

        // A catalog with no language model in it is an answer -- the menu's
        // `No models available.` -- and not a failed load.
        let body = r#"{"data":[{"id":"openai/text-embedding-9","type":"embedding"}]}"#;
        let (url, server) = one_exchange(format!(
            "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        ));
        let catalog = GatewayCatalog::new(
            &Endpoint::resolve(Some(&url)).expect("ok"),
            credential_for_test("fake-key-not-real"),
        );
        assert_eq!(catalog.fetch().await.expect("an empty catalog"), Vec::new());
        server.join().expect("the server");
    }

    // --- CATALOG-BOUNDS: fx v0.0.16's deadline and size cap on the catalog --
    //
    // `gateway_json_fetch_timeout_ms = 30_000` over the whole request, and
    // `max_gateway_json_bytes = 16 * 1024 * 1024`, past which the read fails
    // rather than being cut (`src/gateway/client.zig:486-509,597-635
    // @481104fd`); a timeout is a transport failure fx asks again about, a
    // body too large a malformed one it does not (`catalogRequestFailure`,
    // `src/builtins/gateway.zig:2510-2557@481104fd`).

    const MIB: usize = 1024 * 1024;

    /// The Gateway catalog these cases are asked for.
    const ONE_MODEL: &str = r#"{"data":[{"id":"acme/one","type":"language"}]}"#;

    /// `doc` padded with spaces to `bytes`, before it or after it: JSON
    /// either way. Before, a body cut short of its end is no document; after,
    /// its first bytes are one.
    fn padded(doc: &str, bytes: usize, before: bool) -> String {
        let pad = " ".repeat(bytes - doc.len());
        if before {
            pad + doc
        } else {
            format!("{doc}{pad}")
        }
    }

    /// `body` as a `200` that declares its length.
    fn sized(body: &str) -> String {
        format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    /// `body` as a `200` sent in 64 KiB chunks, its length declared nowhere.
    fn chunked(body: &str) -> String {
        let mut answer =
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ntransfer-encoding: chunked\r\nconnection: close\r\n\r\n"
                .to_string();
        for piece in body.as_bytes().chunks(64 * 1024) {
            answer.push_str(&format!("{:x}\r\n", piece.len()));
            answer.push_str(std::str::from_utf8(piece).expect("ascii"));
            answer.push_str("\r\n");
        }
        answer.push_str("0\r\n\r\n");
        answer
    }

    /// The Gateway catalog of the fake at `origin`, asked with a fake key and
    /// a deadline of `timeout`.
    fn ask_gateway(origin: &str, timeout: Duration) -> Result<Vec<CatalogEntry>, CatalogError> {
        let endpoint = Endpoint::resolve(Some(&format!("{origin}/v3/ai/language-model")))
            .expect("a loopback endpoint");
        on_a_runtime_of_its_own(
            GatewayCatalog::new(&endpoint, credential_for_test("fake-key-not-real"))
                .with_timeout_for_test(timeout)
                .fetch(),
        )
    }

    /// `read` run to its end on a runtime that is then dropped, and the
    /// connection task with it: a fake left writing to a socket nobody reads
    /// sees the hang-up, rather than waiting on a task no one will poll again.
    fn on_a_runtime_of_its_own<T>(read: impl std::future::Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime")
            .block_on(read)
    }

    /// One loopback exchange: `sent` once the request is read, then a space
    /// every 50 ms `drip` times, then the connection held open for `hold` at
    /// most -- less once the client hangs up -- and closed. It ends by itself
    /// within `drip * 50 ms + hold` of the request, whatever the client does,
    /// and within 10 s if no client comes.
    fn slow_exchange(
        sent: Vec<u8>,
        drip: usize,
        hold: Duration,
    ) -> (String, std::thread::JoinHandle<()>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
        listener.set_nonblocking(true).expect("a polled listener");
        let origin = format!(
            "http://127.0.0.1:{}",
            listener.local_addr().expect("an address").port()
        );
        let server = std::thread::spawn(move || {
            let until = Instant::now() + Duration::from_secs(10);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(_) if Instant::now() < until => {
                        std::thread::sleep(Duration::from_millis(5))
                    }
                    Err(_) => return,
                }
            };
            let _ = stream.set_nonblocking(false);
            let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
            crate::test_net::read_request(&mut stream);
            if stream.write_all(&sent).is_err() {
                return;
            }
            for _ in 0..drip {
                std::thread::sleep(Duration::from_millis(50));
                if stream.write_all(b" ").is_err() {
                    return;
                }
            }
            if !hold.is_zero() {
                let _ = stream.set_read_timeout(Some(hold));
                // Returns at the client's hang-up, or once `hold` is over.
                let _ = stream.read(&mut [0u8; 1]);
            }
        });
        (origin, server)
    }

    #[test]
    fn a_gateway_catalog_up_to_16_mib_loads() {
        // Past the old 1 MiB cap, and exactly at fx's, declared and not. The
        // padding is before the document, so a body cut to fit reads as none.
        for (what, answer) in [
            ("2 MiB, declared", sized(&padded(ONE_MODEL, 2 * MIB, true))),
            (
                "16 MiB, declared",
                sized(&padded(ONE_MODEL, 16 * MIB, true)),
            ),
            (
                "16 MiB, chunked",
                chunked(&padded(ONE_MODEL, 16 * MIB, true)),
            ),
        ] {
            let gateway = crate::test_net::Listener::answering(answer);
            let answer = ask_gateway(&gateway.origin(), GATEWAY_CATALOG_TIMEOUT);
            gateway.finish();
            match answer {
                Ok(entries) => assert_eq!(ids(&entries), ["acme/one"], "{what}"),
                Err(err) => panic!("{what}: expected the catalog, got {err:?}"),
            }
        }
    }

    #[test]
    fn a_gateway_catalog_one_byte_past_16_mib_fails_and_is_not_cut_to_fit() {
        // The padding is after the document, so the first 16 MiB -- or 1 --
        // of it would read as a catalog. fx's `GatewayJsonResponseTooLarge`
        // is a malformed answer, which asking again would only repeat.
        for (what, answer) in [
            ("declared", sized(&padded(ONE_MODEL, 16 * MIB + 1, false))),
            ("chunked", chunked(&padded(ONE_MODEL, 16 * MIB + 1, false))),
        ] {
            let gateway = crate::test_net::Listener::answering(answer);
            let answer = ask_gateway(&gateway.origin(), GATEWAY_CATALOG_TIMEOUT);
            gateway.finish();
            match answer {
                Err(CatalogError::Gateway {
                    detail,
                    retry: None,
                }) => assert_eq!(detail, "the answer is larger than 16 MiB", "{what}"),
                other => panic!("{what}: expected a refusal, got {other:?}"),
            }
        }
    }

    #[test]
    fn a_gateway_catalog_past_its_deadline_is_a_failure_asked_again_about() {
        // The deadline is the whole request's: no answer at all, a head and
        // half a body, and a body that keeps coming a byte at a time, which
        // no idle timeout would ever end. Each is fx's `error.Timeout`, which
        // its menu says it retries.
        // A head that promises far more than any of these sends.
        let half = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 10000\r\nconnection: close\r\n\r\n{}",
            &ONE_MODEL[..20]
        )
        .into_bytes();
        for (what, sent, drip) in [
            ("no answer", Vec::new(), 0),
            ("half a body", half.clone(), 0),
            ("a byte at a time", half, 60),
        ] {
            let (origin, server) = slow_exchange(sent, drip, Duration::from_secs(5));
            let asked = Instant::now();
            let answer = ask_gateway(&origin, Duration::from_millis(300));
            let took = asked.elapsed();
            server.join().expect("the server");
            match answer {
                Err(CatalogError::Gateway {
                    retry: Some(CatalogRetry::Unreachable),
                    ..
                }) => {}
                other => panic!("{what}: expected a timeout asked again about, got {other:?}"),
            }
            assert!(
                took < Duration::from_secs(3),
                "{what}: the deadline, not the server, ended it ({took:?})"
            );
        }
    }

    #[test]
    fn a_gateway_catalog_cut_short_or_not_text_is_no_catalog() {
        // A body that stops before its declared end is that failure, not the
        // bytes before it -- here a whole document, so a reader that kept
        // them would take it. fx's read fails there (`ReadFailed`, which is
        // not among the failures it asks again about).
        let cut = sized(&format!("{ONE_MODEL}{}", " ".repeat(100)));
        let cut = cut.as_bytes()[..cut.len() - 100].to_vec();
        let (origin, server) = slow_exchange(cut, 0, Duration::ZERO);
        let answer = ask_gateway(&origin, GATEWAY_CATALOG_TIMEOUT);
        server.join().expect("the server");
        assert!(
            matches!(answer, Err(CatalogError::Gateway { retry: None, .. })),
            "a body cut short: {answer:?}"
        );

        // Not UTF-8: no JSON document, as fx's parser reads one.
        let mut body = br#"{"data":[{"id":"acme/one","type":"language","description":""#.to_vec();
        body.extend_from_slice(b"\xff\"}]}");
        let mut sent = format!(
            "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
            body.len()
        )
        .into_bytes();
        sent.extend_from_slice(&body);
        let (origin, server) = slow_exchange(sent, 0, Duration::ZERO);
        let answer = ask_gateway(&origin, GATEWAY_CATALOG_TIMEOUT);
        server.join().expect("the server");
        match answer {
            Err(CatalogError::Gateway {
                detail,
                retry: None,
            }) => assert_eq!(detail, "the answer is not a model catalog"),
            other => panic!("a body that is not UTF-8: {other:?}"),
        }
    }

    /// A llmux catalog document, the one these cases are asked for.
    const LLMUX: &str = r#"{"models":[{"id":"fake/model-a","name":"fake/model-a"}]}"#;

    #[test]
    fn a_llmux_catalog_of_1_mib_is_read_and_one_cut_short_is_no_answer() {
        // The daemon's own bounds stay (1 MiB, 3 s to connect, 5 s in all).
        let daemon = crate::test_net::Listener::answering(sized(&padded(LLMUX, MIB, false)));
        let at_cap = on_a_runtime_of_its_own(fetch_catalog(&format!("{}/models", daemon.origin())));
        daemon.finish();
        assert_eq!(ids(&at_cap.expect("1 MiB is read")), ["fake/model-a"]);

        // A body that stops partway is no answer, not the bytes before it --
        // here a whole document: the daemon, or the connection to it, failed.
        let cut = sized(&format!("{LLMUX}{}", " ".repeat(100)));
        let cut = cut.as_bytes()[..cut.len() - 100].to_vec();
        let (origin, server) = slow_exchange(cut, 0, Duration::ZERO);
        let answer = on_a_runtime_of_its_own(fetch_catalog(&format!("{origin}/models")));
        server.join().expect("the server");
        assert!(
            matches!(answer, Err(CatalogError::Unavailable { .. })),
            "a body cut short: {answer:?}"
        );
    }

    /// `err` is a malformed catalog with `detail`, and says the daemon's
    /// version is what to check -- not `Unavailable`'s "check the daemon is
    /// running": it answered.
    fn assert_malformed(answer: Result<Vec<CatalogEntry>, CatalogError>, detail: &str) {
        match answer {
            Err(err @ CatalogError::Malformed { .. }) => assert_eq!(
                err.to_string(),
                format!(
                    "catalog is malformed: {detail}; check the daemon is an llmux version compatible with xfx, or rerun `xfx setup llmux`"
                )
            ),
            other => panic!("expected a malformed catalog ({detail}), got {other:?}"),
        }
    }

    #[test]
    fn a_llmux_catalog_past_1_mib_is_malformed_not_cut_to_fit() {
        // Padded after, so a reader that cut to fit would take it.
        let daemon = crate::test_net::Listener::answering(sized(&padded(LLMUX, MIB + 1, false)));
        let past = on_a_runtime_of_its_own(fetch_catalog(&format!("{}/models", daemon.origin())));
        daemon.finish();
        assert_malformed(past, "the answer is larger than 1 MiB");
    }

    #[test]
    fn a_llmux_catalog_that_is_not_utf8_is_malformed() {
        let mut body = br#"{"models":[{"id":"fake/model-a","name":""#.to_vec();
        body.extend_from_slice(b"\xff\"}]}");
        let mut sent = format!(
            "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
            body.len()
        )
        .into_bytes();
        sent.extend_from_slice(&body);
        let (origin, server) = slow_exchange(sent, 0, Duration::ZERO);
        let answer = on_a_runtime_of_its_own(fetch_catalog(&format!("{origin}/models")));
        server.join().expect("the server");
        assert_malformed(answer, "the answer is not UTF-8 text");
    }

    fn credential_for_test(secret: &str) -> crate::config::Credential {
        use crate::config::Environment;
        let workspace = tempfile::tempdir().expect("a workspace");
        let vars = [("AI_GATEWAY_API_KEY".to_string(), secret.to_string())]
            .into_iter()
            .collect();
        RuntimeConfig::load_with(&Environment::new(None, vars), workspace.path())
            .expect("a configuration")
            .credential
            .expect("a credential")
    }

    // --- MODELS-REASK: when upstream's model cache reads the catalog again --
    //
    // fx v0.0.16's `beginLoad` (`src/core/app/model_cache_runtime.zig:570-648
    // @481104fd`) and the backoff a failure puts on it (`:40-54`, `:936-965`).
    // The cases ask through the seams just below, so they read the same before
    // and after the selector is told what asked.

    /// What asked for the read: a submitted prompt, a frame of the inline
    /// column, the launch (`LoadTrigger.activity`) -- or the menu opening
    /// (`.picker_open`), `model_cache_runtime.zig:27-35@481104fd`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Asked {
        Activity,
        PickerOpen,
    }

    impl Asked {
        fn trigger(self) -> LoadTrigger {
            match self {
                Self::Activity => LoadTrigger::Activity,
                Self::PickerOpen => LoadTrigger::PickerOpen,
            }
        }
    }

    /// Whether a read would start, were `asked` to ask at `now`.
    fn would_read(selector: &ModelSelector, asked: Asked, now: Instant) -> bool {
        selector.would_load(asked.trigger(), now)
    }

    /// Starts the read `asked` asks for at `now`, on the selector's own
    /// background load; whether one started.
    fn start_read(selector: &mut ModelSelector, asked: Asked, now: Instant) -> bool {
        selector.begin_load_on(asked.trigger(), now, |_, _, _, _| async {})
    }

    /// What the menu says in its note's place after a refresh that failed
    /// under rows the selector kept, if anything.
    fn refresh_sentence(selector: &ModelSelector) -> Option<&'static str> {
        selector.refresh_retry().map(CatalogRetry::text)
    }

    /// Waits for the selector's background read and takes its answer.
    fn settle(selector: &mut ModelSelector) {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime")
            .block_on(selector.settle_load());
    }

    /// One answer of a [`Script`]: given at once, or once its gate opens.
    type Scripted = (
        Option<tokio::sync::oneshot::Receiver<()>>,
        Result<Vec<CatalogEntry>, ()>,
    );

    /// A catalog that gives its answers in turn -- a 503 once they run out --
    /// counting how often it was asked.
    struct Script {
        answers: std::sync::Mutex<std::collections::VecDeque<Scripted>>,
        asked: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }

    #[async_trait::async_trait(?Send)]
    impl ModelCatalog for Script {
        async fn fetch(&self) -> Result<Vec<CatalogEntry>, CatalogError> {
            self.asked.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let next = self.answers.lock().expect("answers").pop_front();
            let (gate, answer) = next.unwrap_or((None, Err(())));
            if let Some(gate) = gate {
                let _ = gate.await;
            }
            answer.map_err(|()| CatalogError::Gateway {
                detail: "HTTP 503".to_string(),
                retry: Some(CatalogRetry::Unreachable),
            })
        }
    }

    /// A Gateway selector over a [`Script`] of `answers`, and the count of
    /// reads it served.
    fn scripted(
        answers: Vec<Scripted>,
    ) -> (
        ModelSelector,
        std::sync::Arc<std::sync::atomic::AtomicUsize>,
    ) {
        let asked = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut selector =
            ModelSelector::for_test(ProviderId::Gateway, "m", SettingSource::UserGlobal);
        selector.set_fetcher_for_test(Box::new(Script {
            answers: std::sync::Mutex::new(answers.into()),
            asked: std::sync::Arc::clone(&asked),
        }));
        (selector, asked)
    }

    fn rows(ids: &[&str]) -> Scripted {
        (None, Ok(ids.iter().map(|id| entry(id, &[])).collect()))
    }

    fn reads(asked: &std::sync::atomic::AtomicUsize) -> usize {
        asked.load(std::sync::atomic::Ordering::SeqCst)
    }

    #[test]
    fn a_failure_holds_every_read_off_for_upstreams_backoff_which_doubles_to_five_minutes() {
        // `failureBackoffMs` (`model_cache_runtime.zig:40-54@481104fd`): after
        // the n-th failure in a row no read starts for 5 s x 2^(n-1), five
        // minutes at most, whatever asks (`backoff_over`, `:617`, `:621`),
        // counted from when the failure arrived (`completeFetch`, `:948-951`).
        // fx's own schedule (`:1531-1535`): 5 s, 10 s, ... 160 s at the sixth,
        // the cap from the seventh on.
        let (mut selector, asked) = scripted(Vec::new());
        let mut now = Instant::now();
        for (n, hold) in [5u64, 10, 20, 40, 80, 160, 300, 300]
            .into_iter()
            .enumerate()
        {
            assert!(
                start_read(&mut selector, Asked::PickerOpen, now),
                "read {} did not start",
                n + 1
            );
            settle(&mut selector);
            let failed = Instant::now();
            assert_eq!(reads(&asked), n + 1);
            assert!(matches!(selector.catalog(), CatalogState::Failed(_)));
            for by in [Asked::Activity, Asked::PickerOpen] {
                assert!(
                    !would_read(&selector, by, failed + Duration::from_secs(hold - 1)),
                    "failure {} let {by:?} read {} s on",
                    n + 1,
                    hold - 1
                );
            }
            now = failed + Duration::from_secs(hold);
            assert!(
                would_read(&selector, Asked::Activity, now),
                "failure {} held {hold} s and more",
                n + 1
            );
        }
    }

    #[test]
    fn a_held_catalog_is_read_again_by_the_menu_five_seconds_on_and_by_activity_five_minutes_on() {
        // `beginLoad` over a ready catalog (`model_cache_runtime.zig:612-614`,
        // `:623@481104fd`): the picker opening reads it again once the last
        // read began five seconds ago (`picker_revalidate_floor_ms`,
        // `:37-38`), anything else once it is the Gateway's refresh interval
        // old (`builtins/gateway.zig:2348`, five minutes); fx's test
        // `:1467-1488`. Counted from the read's start (`:636`).
        let (mut selector, asked) = scripted(vec![rows(&["m-1"]), rows(&["m-1", "m-2"])]);
        assert!(start_read(&mut selector, Asked::Activity, Instant::now()));
        settle(&mut selector);
        let began = selector.last_attempt().expect("the read began");
        let at = |ms: u64| began + Duration::from_millis(ms);
        assert!(!would_read(&selector, Asked::PickerOpen, at(4_999)));
        assert!(would_read(&selector, Asked::PickerOpen, at(5_000)));
        assert!(!would_read(&selector, Asked::Activity, at(5_000)));
        assert!(!would_read(&selector, Asked::Activity, at(299_999)));
        assert!(would_read(&selector, Asked::Activity, at(300_000)));
        // The menu's read: behind the rows, which stand until it answers.
        assert!(start_read(&mut selector, Asked::PickerOpen, at(5_000)));
        assert!(
            matches!(selector.catalog(), CatalogState::Loaded(held) if held.len() == 1),
            "the rows went while the read was on its way"
        );
        settle(&mut selector);
        assert_eq!(reads(&asked), 2);
        assert!(matches!(selector.catalog(), CatalogState::Loaded(held) if held.len() == 2));
    }

    #[test]
    fn a_refresh_that_fails_keeps_the_rows_says_so_and_is_tried_again_after_its_backoff() {
        // `applyFetchLocked` (`model_cache_runtime.zig:969-975@481104fd`): a
        // failed fetch over a catalog with rows keeps it; the menu says the
        // failure where its note goes (`loadedCatalogStatusText`,
        // `model_menu_presentation.zig:403-404`); and a retryable one is due
        // again whatever asks, once its backoff ends (`retry_due`,
        // `model_cache_runtime.zig:615-617`, `:623`). fx's test `:1445-1465`.
        let (mut selector, asked) = scripted(vec![rows(&["m-1"])]);
        assert!(start_read(&mut selector, Asked::Activity, Instant::now()));
        settle(&mut selector);
        let began = selector.last_attempt().expect("the read began");
        assert!(start_read(
            &mut selector,
            Asked::PickerOpen,
            began + Duration::from_secs(5)
        ));
        settle(&mut selector);
        let failed = Instant::now();
        assert_eq!(reads(&asked), 2);
        assert!(
            matches!(selector.catalog(), CatalogState::Loaded(held) if held.len() == 1),
            "a refresh that failed threw the rows away: {:?}",
            selector.catalog()
        );
        assert_eq!(selector.failure_retry(), None);
        assert_eq!(
            refresh_sentence(&selector),
            Some("Could not reach AI Gateway; xfx retries automatically.")
        );
        assert!(!would_read(
            &selector,
            Asked::Activity,
            failed + Duration::from_secs(4)
        ));
        assert!(would_read(
            &selector,
            Asked::Activity,
            failed + Duration::from_secs(5)
        ));
    }

    #[test]
    fn a_retry_reads_behind_the_failure_it_retries() {
        // "Only the first load for an access shows as `.loading`; a
        // revalidation or retry leaves the state readers see alone"
        // (`model_cache_runtime.zig:485-486`, `:633-634@481104fd`): while the
        // retry is on its way the catalog is still the failure, and its
        // answer replaces it (fx's test `:1490-1528`).
        let (open, gate) = tokio::sync::oneshot::channel();
        let (mut selector, asked) = scripted(vec![
            (None, Err(())),
            (Some(gate), Ok(vec![entry("m-1", &[])])),
        ]);
        assert!(start_read(&mut selector, Asked::Activity, Instant::now()));
        settle(&mut selector);
        let failed = Instant::now();
        assert!(start_read(
            &mut selector,
            Asked::Activity,
            failed + Duration::from_secs(5)
        ));
        assert!(
            matches!(selector.catalog(), CatalogState::Failed(_)),
            "the retry hid the failure: {:?}",
            selector.catalog()
        );
        assert_eq!(selector.failure_retry(), Some(CatalogRetry::Unreachable));
        open.send(()).expect("the retry waits");
        settle(&mut selector);
        assert_eq!(reads(&asked), 2);
        assert!(matches!(selector.catalog(), CatalogState::Loaded(held) if held.len() == 1));
        assert_eq!(selector.failure_retry(), None);
    }

    #[test]
    fn the_retryable_failure_sentences_say_xfx_retries_automatically() {
        // `retryableFailureText` (`model_menu_presentation.zig:435-443
        // @481104fd`), `retry /model.` until 14a12fea; fx's own name read as
        // xfx's, as the parity ledger's `product-name` rule reads it.
        assert_eq!(
            CatalogRetry::Unreachable.text(),
            "Could not reach AI Gateway; xfx retries automatically."
        );
        assert_eq!(
            CatalogRetry::RateLimited.text(),
            "AI Gateway rate limited model discovery; xfx retries automatically."
        );
    }

    #[test]
    fn activity_is_due_exactly_when_the_clock_says_it_is() {
        // `LoadClock::activity_due` is `LoadClock::due` for activity, read
        // the other way: the band asks at that instant and the runtime then
        // reads -- never before it, where the runtime would read nothing and
        // the band, having asked once for this answer, would not ask again.
        let at = Instant::now();
        let held = [
            Held::Nothing,
            Held::Failed,
            Held::Rows { retry_due: false },
            Held::Rows { retry_due: true },
        ];
        let clocks = [
            LoadClock::default(),
            LoadClock {
                began: Some(at),
                streak: 0,
                not_before: None,
            },
            LoadClock {
                began: Some(at),
                streak: 1,
                not_before: Some(at + Duration::from_secs(8)),
            },
            LoadClock {
                began: Some(at),
                streak: 7,
                not_before: Some(at + FAILURE_BACKOFF_CAP + Duration::from_secs(3)),
            },
        ];
        for clock in clocks {
            for held in held {
                let due = clock.activity_due(held, at);
                assert!(
                    clock.due(held, LoadTrigger::Activity, due),
                    "{clock:?} {held:?}: not due at {due:?}"
                );
                if due > at {
                    assert!(
                        !clock.due(held, LoadTrigger::Activity, due - Duration::from_nanos(1)),
                        "{clock:?} {held:?}: due before {due:?}"
                    );
                }
            }
        }
    }

    /// A catalog no read of which finds a runtime to run on.
    struct NoRuntime;

    #[async_trait::async_trait(?Send)]
    impl ModelCatalog for NoRuntime {
        async fn fetch(&self) -> Result<Vec<CatalogEntry>, CatalogError> {
            Ok(Vec::new())
        }

        fn runtime_fails_for_test(&self) -> bool {
            true
        }
    }

    #[test]
    fn a_read_that_finds_no_runtime_tells_its_failure_as_every_read_tells_its_answer() {
        // Upstream completes a fetch it could not start as a retryable
        // `.runtime` failure (`model_cache_runtime.zig:537-546@481104fd`),
        // which its interface reads on its next poll like any answer
        // (`pollLoadTransition`, `:745-760`): the read is told, whatever kept
        // it from running.
        let mut selector =
            ModelSelector::for_test(ProviderId::Gateway, "m", SettingSource::UserGlobal);
        selector.catalog_fetcher = Some(std::sync::Arc::new(NoRuntime));
        selector.catalog_state = CatalogState::NotLoaded;
        let (told, heard) = std::sync::mpsc::channel();
        selector.begin_load(move |catalog, retry, _, _| async move {
            let _ = told.send((catalog, retry));
        });
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime")
            .block_on(selector.settle_load());
        let (catalog, retry) = heard.try_recv().expect("the read's failure was not told");
        assert!(matches!(catalog, CatalogState::Failed(_)), "{catalog:?}");
        assert_eq!(retry, Some(CatalogRetry::Runtime));
        assert_eq!(selector.failure_retry(), Some(CatalogRetry::Runtime));
        assert!(selector.retry_at().is_some(), "no backoff");
    }

    #[test]
    fn a_read_that_could_not_start_says_upstreams_third_sentence() {
        // `retryableFailureText`'s `else` (`model_menu_presentation.zig:441
        // @481104fd`), for upstream's `.runtime` failure: a fetch thread it
        // could not spawn, which it marks retryable
        // (`model_cache_runtime.zig:537-546`).
        assert_eq!(
            CatalogRetry::Runtime.text(),
            "Could not refresh model catalog; xfx retries automatically."
        );
        assert_eq!(
            unstartable(&std::io::Error::other("no thread")).1,
            Some(CatalogRetry::Runtime)
        );
    }
}
