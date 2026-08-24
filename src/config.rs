//! Configuration discovery, precedence, credentials, and diagnostics.
//!
//! Four layers contribute, lowest precedence first:
//!
//! 1. compiled defaults
//! 2. the project file `<workspace>/.xfx.json`
//! 3. the profile global file `~/.xfx/settings.json`
//! 4. the exact-workspace entry `workspaces["<workspace>"]` inside that same file
//! 5. the process environment (`XFX_MODEL`, `XFX_PERMISSION_MODE`, `XFX_MAX_AGENT_STEPS`)
//!
//! This mirrors the upstream merge order in
//! `vercel-labs/fx@580a0c5d src/core/config/config_runtime.zig:341-455`, where a
//! later layer overwrites an earlier one only for the keys it actually sets.
//!
//! Loading is strictly read-only. `status` and `doctor` must work on a machine
//! that has never run xfx, so nothing here creates `~/.xfx`.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::provider::ProviderId;

/// The model xfx requests when nothing selects one.
///
/// Matches upstream's Gateway default
/// (`vercel-labs/fx@580a0c5d src/builtins/gateway.zig:40`).
pub const DEFAULT_MODEL: &str = "zai/glm-5.2";

/// The compiled ceiling on model steps within one turn.
///
/// Upstream compiles in `0`, meaning unbounded
/// (`vercel-labs/fx@580a0c5d src/core/config/agent_steps.zig:3`). xfx deliberately
/// ships a bound instead; `0` remains the explicit unbounded opt-out so the
/// configured semantics still match upstream.
pub const DEFAULT_MAX_AGENT_STEPS: u32 = 25;

/// The largest settings file xfx will read, matching upstream's 64 KiB ceiling
/// (`vercel-labs/fx@580a0c5d src/core/config/config_runtime.zig:17`).
pub const MAX_SETTINGS_BYTES: usize = 64 * 1024;

/// Directory name of the profile home under `$HOME`.
pub const PROFILE_DIR_NAME: &str = ".xfx";

/// File name of the project-scoped settings file.
pub const PROJECT_SETTINGS_FILE: &str = ".xfx.json";

const USER_SETTINGS_FILE: &str = "settings.json";

/// The settings key that names the provider directly.
pub const PROVIDER_KEY: &str = "provider";
/// The settings key holding one model preference per provider tag.
pub const MODELS_KEY: &str = "models";
/// The v0.1.0 key that selected a backend, still read and still written.
pub const BACKEND_KEY: &str = "backend";

/// The environment override for the model.
///
/// Public so `xfx setup llmux` can *name* it: setup writes the profile, and an
/// operator whose shell overrides what was just written has to be told which
/// variable is doing it.
pub const ENV_MODEL: &str = "XFX_MODEL";
const ENV_PERMISSION_MODE: &str = "XFX_PERMISSION_MODE";
const ENV_MAX_AGENT_STEPS: &str = "XFX_MAX_AGENT_STEPS";
const ENV_OIDC_TOKEN: &str = "VERCEL_OIDC_TOKEN";
const ENV_GATEWAY_KEY: &str = "AI_GATEWAY_API_KEY";

/// The variable llmux uses to name its own configuration file.
///
/// Public so [`crate::llmux::setup`] reads one spelling of it rather than two.
/// It is not an xfx knob: no configuration layer reads it, and nothing here sets
/// it. `xfx setup llmux` consults it only to find out which port to talk to.
pub const ENV_LLMUX_CONFIG: &str = "LLMUX_CONFIG";

/// The XDG base directory that holds `llmux.json` when nothing names it outright.
pub const ENV_XDG_CONFIG_HOME: &str = "XDG_CONFIG_HOME";

/// Settings keys that only the profile may set.
///
/// A repository is shared, so it must not be able to choose the model, the
/// permission mode, or the credential for whoever clones it. Upstream draws the
/// same line in `src/core/config/config_runtime.zig:548-576`; xfx keeps the
/// subset that its own settings surface actually supports.
const PROFILE_ONLY_KEYS: &[&str] = &[
    "backend",
    "credential_source",
    "llmux_url",
    "model",
    "models",
    "permission_mode",
    "provider",
    "workspaces",
];

/// How much authority the agent has before it must ask.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionMode {
    /// Mutations and commands require an interactive approval.
    Ask,
    /// Reads run directly; reversible writes and an allowlisted command grammar
    /// run after structural validation.
    Auto,
    /// Policy checks are skipped entirely.
    Yolo,
}

impl PermissionMode {
    /// The stable wire label used by every renderer.
    pub fn label(self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::Auto => "auto",
            Self::Yolo => "yolo",
        }
    }

    /// The inverse of [`Self::label`], tolerating surrounding whitespace.
    ///
    /// Public because a persisted session records the label and has to be able
    /// to read it back; a second parser in the session module would be a second
    /// thing to keep in step with the three modes.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim() {
            "ask" => Some(Self::Ask),
            "auto" => Some(Self::Auto),
            "yolo" => Some(Self::Yolo),
            _ => None,
        }
    }
}

impl Default for PermissionMode {
    /// Upstream compiles in `auto`
    /// (`vercel-labs/fx@580a0c5d src/core/config/config_runtime.zig:18`).
    fn default() -> Self {
        Self::Auto
    }
}

/// Which layer supplied the effective value of one setting.
///
/// Ordered: a later variant outranks an earlier one. The order is the layer order, which is
/// what lets two keys competing for one setting be compared by where each came from rather
/// than by which was parsed last.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SettingSource {
    CompiledDefault,
    Project,
    UserGlobal,
    UserWorkspace,
    ProcessOverride,
}

impl SettingSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::CompiledDefault => "compiled_default",
            Self::Project => "project",
            Self::UserGlobal => "user_global",
            Self::UserWorkspace => "user_workspace",
            Self::ProcessOverride => "process_override",
        }
    }
}

/// The provenance of every setting xfx resolves.
#[derive(Debug, Clone, Copy)]
pub struct Sources {
    pub model: SettingSource,
    pub permission_mode: SettingSource,
    pub max_agent_steps: SettingSource,
    pub provider: SettingSource,
    /// Where the daemon URL came from.
    ///
    /// It has its own entry rather than riding on `provider`'s, because a
    /// workspace entry can pin the URL without touching the provider -- and
    /// `setup` has to be able to tell the operator that the file it just wrote
    /// is not the layer their next turn will read.
    pub llmux_url: SettingSource,
}

impl Default for Sources {
    fn default() -> Self {
        Self {
            model: SettingSource::CompiledDefault,
            permission_mode: SettingSource::CompiledDefault,
            max_agent_steps: SettingSource::CompiledDefault,
            provider: SettingSource::CompiledDefault,
            llmux_url: SettingSource::CompiledDefault,
        }
    }
}

/// Which file a diagnostic came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigLayer {
    /// `~/.xfx/settings.json`, including its `workspaces` entries.
    User,
    /// `<workspace>/.xfx.json`.
    Project,
    /// The process environment.
    Process,
}

impl ConfigLayer {
    pub fn label(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Project => "project",
            Self::Process => "process",
        }
    }
}

/// Why a layer or a key was not applied as written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticCause {
    /// The file exists but is not a JSON object.
    MalformedSettings,
    /// The file is larger than [`MAX_SETTINGS_BYTES`].
    SettingsTooLarge,
    /// The file could not be read at all.
    UnreadableSettings,
    /// A profile-only key appeared in the project file and was dropped.
    IgnoredProjectProfileSetting,
    /// A key was present with a value xfx cannot interpret.
    InvalidValue,
    /// Two keys select a provider and they disagree.
    ConflictingProviderSelection,
}

impl DiagnosticCause {
    pub fn label(self) -> &'static str {
        match self {
            Self::MalformedSettings => "malformed_settings",
            Self::SettingsTooLarge => "settings_too_large",
            Self::UnreadableSettings => "unreadable_settings",
            Self::IgnoredProjectProfileSetting => "ignored_project_profile_setting",
            Self::InvalidValue => "invalid_value",
            Self::ConflictingProviderSelection => "conflicting_provider_selection",
        }
    }
}

/// One non-fatal configuration problem, reported rather than thrown.
///
/// `status` and `doctor` must still describe the machine when its settings are
/// broken, so nothing in this module turns a bad file into a failed command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub layer: ConfigLayer,
    pub cause: DiagnosticCause,
    pub setting_key: Option<String>,
    /// The facts a reader needs that the cause alone does not carry -- today,
    /// the two values that disagree. Rendered into `detail`, so a `doctor`
    /// `config` check names them rather than telling an operator that something
    /// somewhere conflicts.
    pub note: Option<String>,
}

impl Diagnostic {
    fn new(layer: ConfigLayer, cause: DiagnosticCause) -> Self {
        Self {
            layer,
            cause,
            setting_key: None,
            note: None,
        }
    }

    fn with_key(layer: ConfigLayer, cause: DiagnosticCause, key: &str) -> Self {
        Self {
            layer,
            cause,
            setting_key: Some(key.to_string()),
            note: None,
        }
    }

    fn with_note(layer: ConfigLayer, cause: DiagnosticCause, key: &str, note: String) -> Self {
        Self {
            layer,
            cause,
            setting_key: Some(key.to_string()),
            note: Some(note),
        }
    }

    /// The key that was dropped for being profile-only, if that is what happened.
    pub fn ignored_setting_key(&self) -> Option<&str> {
        if self.cause == DiagnosticCause::IgnoredProjectProfileSetting {
            self.setting_key.as_deref()
        } else {
            None
        }
    }

    /// Whether this diagnostic reports a layer that exceeded the size ceiling.
    pub fn is_too_large(&self) -> bool {
        self.cause == DiagnosticCause::SettingsTooLarge
    }

    /// A single-line human description, used as a `doctor` check detail.
    pub fn detail(&self) -> String {
        let mut detail = format!(
            "{} config diagnostic: {}",
            self.layer.label(),
            self.cause.label()
        );
        if let Some(key) = &self.setting_key {
            detail.push_str(" key=");
            detail.push_str(key);
        }
        if let Some(note) = &self.note {
            detail.push(' ');
            detail.push_str(note);
        }
        detail
    }
}

/// A provider selection xfx could not read, and which key wrote it.
///
/// Kept rather than discarded, because it must **poison the turn**. Falling back
/// to the compiled default would send the prompt and the Gateway credential to a
/// remote paid endpoint because a settings value was mistyped. The key travels
/// with the value because the refusal has to name the key the operator actually
/// wrote -- there are two that select a provider, and quoting the wrong one
/// sends them to edit a line that is fine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderRejection {
    pub key: &'static str,
    pub value: String,
}

/// Which environment variable supplied the active credential.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialSource {
    VercelOidcToken,
    AiGatewayApiKey,
    /// llmux's keyless loopback arrangement: authenticated-but-tokenless.
    ///
    /// Not "the daemon is up" -- that is a reachability probe wearing a
    /// credential's clothes -- and not an empty secret passed through a guard
    /// written for a non-empty one, which is how this becomes a confusing 401
    /// later. It resolves from configuration alone.
    LlmuxLoopback,
}

impl CredentialSource {
    /// The label snapshots print. It names the variable, never the value.
    pub fn label(self) -> &'static str {
        match self {
            Self::VercelOidcToken => ENV_OIDC_TOKEN,
            Self::AiGatewayApiKey => ENV_GATEWAY_KEY,
            Self::LlmuxLoopback => crate::provider::LLMUX_LOOPBACK_LABEL,
        }
    }
}

/// A resolved bearer credential.
///
/// The secret is private and `Debug` is redacted, so a credential cannot reach a
/// log, a snapshot, or a panic message by accident.
#[derive(Clone)]
pub struct Credential {
    source: CredentialSource,
    secret: String,
}

impl Credential {
    pub fn source(&self) -> CredentialSource {
        self.source
    }

    pub fn source_label(&self) -> &'static str {
        self.source.label()
    }

    /// The bearer value. Only the transport may call this.
    pub fn secret(&self) -> &str {
        &self.secret
    }
}

impl fmt::Debug for Credential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credential")
            .field("source", &self.source)
            .field("secret", &"<redacted>")
            .finish()
    }
}

/// The variables and home directory xfx is allowed to observe.
///
/// Passing this explicitly keeps configuration a pure function of its inputs, so
/// tests never mutate the process environment and never race each other.
#[derive(Debug, Clone, Default)]
pub struct Environment {
    home: Option<PathBuf>,
    vars: BTreeMap<String, String>,
}

impl Environment {
    pub fn new(home: Option<PathBuf>, vars: BTreeMap<String, String>) -> Self {
        Self { home, vars }
    }

    /// Reads the real process environment, taking only the variables xfx uses.
    pub fn from_process() -> Self {
        let mut vars = BTreeMap::new();
        for key in [
            ENV_MODEL,
            ENV_PERMISSION_MODE,
            ENV_MAX_AGENT_STEPS,
            ENV_OIDC_TOKEN,
            ENV_GATEWAY_KEY,
            // Not xfx knobs and not read by any layer below: `xfx setup llmux`
            // consults them to find *llmux's* configuration file, which is where
            // the daemon's port is written. They are captured here so that
            // reading is a pure function of this struct like everything else,
            // rather than a second, untestable read of the process environment.
            ENV_LLMUX_CONFIG,
            ENV_XDG_CONFIG_HOME,
        ] {
            if let Ok(value) = std::env::var(key) {
                vars.insert(key.to_string(), value);
            }
        }
        Self {
            home: std::env::var_os("HOME")
                .map(PathBuf::from)
                .filter(|p| !p.as_os_str().is_empty()),
            vars,
        }
    }

    pub fn home(&self) -> Option<&Path> {
        self.home.as_deref()
    }

    pub fn var(&self, key: &str) -> Option<&str> {
        self.vars.get(key).map(String::as_str)
    }

    /// A variable that is present and not only whitespace.
    ///
    /// Upstream ignores a blank override rather than letting it displace a
    /// configured value (`src/core/config/config_runtime.zig:449-453`).
    fn nonblank(&self, key: &str) -> Option<&str> {
        let value = self.var(key)?.trim();
        (!value.is_empty()).then_some(value)
    }
}

/// A fatal configuration failure. Malformed content is never fatal; only losing
/// the workspace itself is.
#[derive(Debug)]
pub enum ConfigError {
    /// The current directory could not be resolved.
    Workspace(io::Error),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Workspace(err) => write!(f, "cannot resolve the workspace directory: {err}"),
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Workspace(err) => Some(err),
        }
    }
}

/// The fully resolved runtime configuration for one invocation.
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub workspace_root: PathBuf,
    /// `~/.xfx`, whether or not it exists. Loading never creates it.
    pub profile_dir: Option<PathBuf>,
    /// `~/.xfx/settings.json`, whether or not it exists.
    pub user_settings_path: Option<PathBuf>,
    /// `<workspace>/.xfx.json`, whether or not it exists.
    pub project_settings_path: PathBuf,
    /// A `~/.xfx/settings.json` entry exists on disk, whether or not it was
    /// usable. Presence and usability are separate facts: a file that exists but
    /// cannot be parsed must never be reported as "no config files found".
    pub user_settings_present: bool,
    /// A `<workspace>/.xfx.json` entry exists on disk, whether or not it was
    /// usable.
    pub project_settings_present: bool,
    /// The profile settings file was parsed and merged.
    pub user_settings_loaded: bool,
    /// The project settings file was parsed and merged.
    pub project_settings_loaded: bool,
    pub model: String,
    pub permission_mode: PermissionMode,
    pub max_agent_steps: u32,
    /// The provider a turn will talk to.
    ///
    /// This is the *configured* provider, never silently downgraded. When it is
    /// [`ProviderId::Llmux`] and [`Self::llmux_url`] is `None`, xfx refuses the turn
    /// and names `xfx setup llmux` rather than falling back to the Gateway: an
    /// operator who configured a local daemon must not have their prompt sent to
    /// a remote paid endpoint because the URL was missing or mistyped.
    pub provider: ProviderId,
    /// A provider selection xfx could not read, when a layer wrote one.
    ///
    /// Kept rather than discarded, because it must **poison the turn**. Falling
    /// back to the compiled default here would send the prompt and the Gateway
    /// credential to a remote paid endpoint because a settings value was
    /// mistyped -- the same silent redirection the `llmux_url` rules exist to
    /// prevent. `status` and `doctor` still render, because describing a broken
    /// machine is their whole job; `ask` and the shell refuse and quote it.
    pub provider_rejected: Option<ProviderRejection>,
    /// The llmux daemon's base URL, present only when a settings layer named one
    /// that passes the endpoint rule. A refused URL is a diagnostic and a `None`,
    /// never a value.
    pub llmux_url: Option<String>,
    /// Model preferences by provider tag, as merged. Exposed so a writer can preserve the
    /// entries for providers this invocation is not using.
    pub models: BTreeMap<String, String>,
    pub sources: Sources,
    pub diagnostics: Vec<Diagnostic>,
    pub credential: Option<Credential>,
}

impl RuntimeConfig {
    /// Loads configuration for `workspace` from the real process environment.
    pub fn load(workspace: &Path) -> Result<Self, ConfigError> {
        Self::load_with(&Environment::from_process(), workspace)
    }

    /// Loads configuration for `workspace` from an explicit environment.
    pub fn load_with(env: &Environment, workspace: &Path) -> Result<Self, ConfigError> {
        // Canonicalize before anything else: the workspace path is both the
        // reported identity and the key of the exact-workspace settings entry,
        // so the two must be derived from the same string.
        let workspace_root = workspace.canonicalize().map_err(ConfigError::Workspace)?;

        let profile_dir = env.home().map(|home| home.join(PROFILE_DIR_NAME));
        let user_settings_path = profile_dir.as_ref().map(|dir| dir.join(USER_SETTINGS_FILE));
        let project_settings_path = workspace_root.join(PROJECT_SETTINGS_FILE);

        let mut diagnostics = Vec::new();
        let mut settings = Settings::default();
        let mut sources = Sources::default();

        let project = read_layer(
            &project_settings_path,
            ConfigLayer::Project,
            &mut diagnostics,
        );
        let project_settings_present = project.present;
        let project_settings_loaded = project.object.is_some();
        if let Some(object) = &project.object {
            report_ignored_profile_keys(object, &mut diagnostics);
            let layer = parse_layer(
                object,
                LayerKind::Project,
                ConfigLayer::Project,
                &mut diagnostics,
            );
            settings.merge(layer, SettingSource::Project, &mut sources);
        }

        let user = match user_settings_path.as_ref() {
            Some(path) => read_layer(path, ConfigLayer::User, &mut diagnostics),
            None => LayerRead::absent(),
        };
        let user_settings_present = user.present;
        let user_settings_loaded = user.object.is_some();
        if let Some(object) = &user.object {
            let layer = parse_layer(
                object,
                LayerKind::Profile,
                ConfigLayer::User,
                &mut diagnostics,
            );
            settings.merge(layer, SettingSource::UserGlobal, &mut sources);

            if let Some(entry) = exact_workspace_entry(object, &workspace_root, &mut diagnostics) {
                let layer = parse_layer(
                    entry,
                    LayerKind::Profile,
                    ConfigLayer::User,
                    &mut diagnostics,
                );
                settings.merge(layer, SettingSource::UserWorkspace, &mut sources);
            }
        }

        settings.merge(
            parse_environment(env, &mut diagnostics),
            SettingSource::ProcessOverride,
            &mut sources,
        );

        // Rule 1 and the coexistence rule with layer provenance: compare sources FIRST
        // (newest layer wins, including newest rejected values that poison), then use
        // `provider` wins over `backend` precedence ONLY when sources are equal.
        // An unreadable value of whichever key is newest poisons the turn rather than
        // being replaced by a default, and a disagreement is reported rather than resolved
        // silently.
        let (provider, provider_rejected) = {
            // Determine which key's source is newest. Both readable and rejected values
            // carry source information.
            let provider_source = settings
                .provider_source
                .or(settings.provider_rejected_source);
            let backend_source = settings.backend_source.or(settings.backend_rejected_source);

            // Compare sources: newer layer wins. Use a helper to determine which key wins.
            match (
                provider_source.cmp(&backend_source),
                provider_source,
                backend_source,
            ) {
                (std::cmp::Ordering::Greater, _, _) => {
                    // Provider source is newer: use provider (readable or rejected).
                    if let Some(rejected) = &settings.provider_rejected {
                        (
                            ProviderId::default(),
                            Some(ProviderRejection {
                                key: PROVIDER_KEY,
                                value: rejected.clone(),
                            }),
                        )
                    } else {
                        (settings.provider.unwrap_or_default(), None)
                    }
                }
                (std::cmp::Ordering::Less, _, _) => {
                    // Backend source is newer: use backend (readable or rejected).
                    if let Some(rejected) = &settings.backend_rejected {
                        (
                            ProviderId::default(),
                            Some(ProviderRejection {
                                key: BACKEND_KEY,
                                value: rejected.clone(),
                            }),
                        )
                    } else {
                        (settings.backend.unwrap_or_default(), None)
                    }
                }
                (std::cmp::Ordering::Equal, Some(_), Some(_)) => {
                    // Equal sources: provider wins over backend within one layer.
                    if let Some(rejected) = &settings.provider_rejected {
                        (
                            ProviderId::default(),
                            Some(ProviderRejection {
                                key: PROVIDER_KEY,
                                value: rejected.clone(),
                            }),
                        )
                    } else if let Some(provider) = settings.provider {
                        (provider, None)
                    } else if let Some(rejected) = &settings.backend_rejected {
                        (
                            ProviderId::default(),
                            Some(ProviderRejection {
                                key: BACKEND_KEY,
                                value: rejected.clone(),
                            }),
                        )
                    } else {
                        (settings.backend.unwrap_or_default(), None)
                    }
                }
                (std::cmp::Ordering::Equal, Some(_), None) => {
                    // Only provider source: use it.
                    if let Some(rejected) = &settings.provider_rejected {
                        (
                            ProviderId::default(),
                            Some(ProviderRejection {
                                key: PROVIDER_KEY,
                                value: rejected.clone(),
                            }),
                        )
                    } else {
                        (settings.provider.unwrap_or_default(), None)
                    }
                }
                (std::cmp::Ordering::Equal, None, Some(_)) => {
                    // Only backend source: use it.
                    if let Some(rejected) = &settings.backend_rejected {
                        (
                            ProviderId::default(),
                            Some(ProviderRejection {
                                key: BACKEND_KEY,
                                value: rejected.clone(),
                            }),
                        )
                    } else {
                        (settings.backend.unwrap_or_default(), None)
                    }
                }
                (std::cmp::Ordering::Equal, None, None) => (ProviderId::default(), None),
            }
        };

        // Report disagreement if both keys have readable values that differ.
        // Any such desync is operator-visible and must be diagnosed, regardless of layer.
        // The note explains which one wins based on source precedence.
        if let (Some(provider_val), Some(backend_val)) = (settings.provider, settings.backend) {
            if provider_val != backend_val {
                let provider_source = settings.provider_source;
                let backend_source = settings.backend_source;

                // Determine the explanation of precedence
                let explanation = match (provider_source, backend_source) {
                    (Some(p_src), Some(b_src)) if p_src > b_src => format!(
                        "provider={} backend={}; layer order decides: `provider` source ({}) is \
                         newer than `backend` source ({})",
                        provider_val.label(),
                        backend_val.label(),
                        p_src.label(),
                        b_src.label()
                    ),
                    (Some(p_src), Some(b_src)) if b_src > p_src => format!(
                        "provider={} backend={}; layer order decides: `backend` source ({}) is \
                         newer than `provider` source ({})",
                        provider_val.label(),
                        backend_val.label(),
                        b_src.label(),
                        p_src.label()
                    ),
                    (Some(_), Some(_)) => format!(
                        "provider={} backend={}; same layer: `provider` key takes precedence, \
                         and an older binary reading this profile would use `backend`",
                        provider_val.label(),
                        backend_val.label()
                    ),
                    _ => format!(
                        "provider={} backend={}; incomplete source information",
                        provider_val.label(),
                        backend_val.label()
                    ),
                };

                diagnostics.push(Diagnostic::with_note(
                    ConfigLayer::User,
                    DiagnosticCause::ConflictingProviderSelection,
                    PROVIDER_KEY,
                    explanation,
                ));
            }
        }

        // The per-provider preference wins inside one layer, because the newer
        // key is the one a newer binary wrote deliberately; across layers the
        // layer order still decides, because a workspace entry that pins this
        // directory's model must not stop working when the global file grows a
        // `models` object. Comparing the two sources is the whole rule.
        let tag = provider.label();
        let flat_source = sources.model;

        // Seed the flat model into the models map if it was explicitly set by a layer.
        // Update the entry if the flat source is newer; keep it if sources are equal.
        if flat_source != SettingSource::CompiledDefault {
            if let Some(model) = &settings.model {
                let tag_str = tag.to_string();
                match settings.models_sources.get(&tag_str) {
                    Some(&entry_source) if flat_source > entry_source => {
                        // Flat source is newer: update the entry.
                        settings.models.insert(tag_str.clone(), model.clone());
                        settings.models_sources.insert(tag_str, flat_source);
                    }
                    None => {
                        // No existing entry: seed it.
                        settings.models.insert(tag_str.clone(), model.clone());
                        settings.models_sources.insert(tag_str, flat_source);
                    }
                    _ => {
                        // Entry exists with equal or higher source: keep it.
                    }
                }
            }
        }

        let entry = settings
            .models
            .get(tag)
            .zip(settings.models_sources.get(tag).copied())
            // `>=` and not `>`: inside one layer the two sources are equal and
            // the per-provider key is the deliberate one. When no layer wrote a
            // flat `model`, `flat_source` is `CompiledDefault`, which every real
            // layer outranks -- so this one comparison covers that case too and
            // there is no second condition to keep in step with it.
            .filter(|(_, entry_source)| *entry_source >= flat_source);
        let model = match entry {
            Some((model, entry_source)) => {
                sources.model = entry_source;
                model.clone()
            }
            None => settings
                .model
                .clone()
                .unwrap_or_else(|| DEFAULT_MODEL.to_string()),
        };

        Ok(Self {
            workspace_root,
            profile_dir,
            user_settings_path,
            project_settings_path,
            user_settings_present,
            project_settings_present,
            user_settings_loaded,
            project_settings_loaded,
            model,
            permission_mode: settings.permission_mode.unwrap_or_default(),
            max_agent_steps: settings.max_agent_steps.unwrap_or(DEFAULT_MAX_AGENT_STEPS),
            provider,
            provider_rejected,
            llmux_url: settings.llmux_url,
            models: settings.models,
            sources,
            diagnostics,
            credential: resolve_credential(env),
        })
    }

    /// Whether a settings file exists on disk, usable or not.
    pub fn has_settings_file(&self) -> bool {
        self.user_settings_present || self.project_settings_present
    }

    /// Whether a settings file exists that xfx could not use.
    ///
    /// This is the case `doctor` must not describe as "no config files found":
    /// the user wrote a file, xfx ignored it, and silence would read as consent.
    pub fn has_unusable_settings_file(&self) -> bool {
        (self.user_settings_present && !self.user_settings_loaded)
            || (self.project_settings_present && !self.project_settings_loaded)
    }
}

/// One layer's contribution: `None` means "this layer does not set that key".
#[derive(Debug, Default)]
struct Settings {
    model: Option<String>,
    permission_mode: Option<PermissionMode>,
    max_agent_steps: Option<u32>,
    backend: Option<ProviderId>,
    backend_source: Option<SettingSource>,
    /// The raw `backend` value a layer wrote that could not be read.
    backend_rejected: Option<String>,
    backend_rejected_source: Option<SettingSource>,
    /// A layer's `provider` value, when it wrote a readable one.
    provider: Option<ProviderId>,
    provider_source: Option<SettingSource>,
    /// The raw `provider` value a layer wrote that could not be read.
    provider_rejected: Option<String>,
    provider_rejected_source: Option<SettingSource>,
    /// Model preferences by provider tag, merged per entry.
    models: BTreeMap<String, String>,
    /// Where each surviving `models` entry came from.
    models_sources: BTreeMap<String, SettingSource>,
    llmux_url: Option<String>,
    /// This layer wrote an `llmux_url` the endpoint policy refused.
    llmux_url_rejected: bool,
}

impl Settings {
    /// Applies `incoming` over `self`, key by key, recording provenance.
    ///
    /// A key absent from `incoming` leaves both the value and its source alone;
    /// that is what makes the layer order a precedence order.
    fn merge(&mut self, incoming: Settings, source: SettingSource, sources: &mut Sources) {
        if let Some(model) = incoming.model {
            self.model = Some(model);
            sources.model = source;
        }
        if let Some(mode) = incoming.permission_mode {
            self.permission_mode = Some(mode);
            sources.permission_mode = source;
        }
        if let Some(steps) = incoming.max_agent_steps {
            self.max_agent_steps = Some(steps);
            sources.max_agent_steps = source;
        }
        if let Some(backend) = incoming.backend {
            self.backend = Some(backend);
            self.backend_source = Some(source);
            self.backend_rejected = None;
            self.backend_rejected_source = None;
            sources.provider = source;
        }
        // A later layer that writes an unreadable value replaces a readable one:
        // the operator's most recent word is what they meant, and quietly using
        // an older layer's backend would be the fallback this exists to stop.
        if let Some(rejected) = incoming.backend_rejected {
            self.backend = None;
            self.backend_source = None;
            self.backend_rejected = Some(rejected);
            self.backend_rejected_source = Some(source);
            sources.provider = source;
        }
        if let Some(provider) = incoming.provider {
            self.provider = Some(provider);
            self.provider_source = Some(source);
            self.provider_rejected = None;
            self.provider_rejected_source = None;
            sources.provider = source;
        }
        // A later layer that writes an unreadable value replaces a readable one:
        // the operator's most recent word is what they meant, and quietly using
        // an older layer's provider would be the fallback this exists to stop.
        if let Some(rejected) = incoming.provider_rejected {
            self.provider = None;
            self.provider_source = None;
            self.provider_rejected = Some(rejected);
            self.provider_rejected_source = Some(source);
            sources.provider = source;
        }
        // Per entry rather than wholesale: a profile that names a model for one
        // provider and a workspace entry that names one for another are two
        // different settings, and replacing the map would silently drop the one
        // the operator is not currently using.
        for (tag, model) in incoming.models {
            self.models.insert(tag.clone(), model);
            self.models_sources.insert(tag, source);
        }
        if let Some(url) = incoming.llmux_url {
            self.llmux_url = Some(url);
            self.llmux_url_rejected = false;
            sources.llmux_url = source;
        }
        // A later layer whose URL was refused **clears** the accumulated one,
        // exactly as a rejected `backend` does. Deferring to an earlier layer
        // would mean the operator's most recent word about where the prompt goes
        // was dropped and an older one silently governed instead -- which is the
        // fallback this whole mechanism exists to refuse.
        if incoming.llmux_url_rejected {
            self.llmux_url = None;
            self.llmux_url_rejected = true;
            sources.llmux_url = source;
        }
    }
}

/// Whether a layer is allowed to set profile-only keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LayerKind {
    Profile,
    Project,
}

fn parse_layer(
    object: &serde_json::Map<String, Value>,
    kind: LayerKind,
    layer: ConfigLayer,
    diagnostics: &mut Vec<Diagnostic>,
) -> Settings {
    let mut settings = Settings::default();

    if kind == LayerKind::Profile {
        if let Some(value) = object.get("model") {
            match value.as_str().map(str::trim) {
                Some(model) if !model.is_empty() => settings.model = Some(model.to_string()),
                _ => diagnostics.push(Diagnostic::with_key(
                    layer,
                    DiagnosticCause::InvalidValue,
                    "model",
                )),
            }
        }
        if let Some(value) = object.get("permission_mode") {
            match value.as_str().and_then(PermissionMode::parse) {
                Some(mode) => settings.permission_mode = Some(mode),
                None => diagnostics.push(Diagnostic::with_key(
                    layer,
                    DiagnosticCause::InvalidValue,
                    "permission_mode",
                )),
            }
        }
        if let Some(value) = object.get("backend") {
            match value.as_str().and_then(ProviderId::parse) {
                Some(backend) => settings.backend = Some(backend),
                None => {
                    // The raw value is carried forward so the refusal can quote
                    // it. A value of the wrong *type* has no spelling worth
                    // quoting, so it is reported as the empty string.
                    settings.backend_rejected =
                        Some(value.as_str().unwrap_or_default().trim().to_string());
                    diagnostics.push(Diagnostic::with_key(
                        layer,
                        DiagnosticCause::InvalidValue,
                        "backend",
                    ));
                }
            }
        }
        if let Some(value) = object.get(PROVIDER_KEY) {
            match value.as_str().and_then(ProviderId::parse) {
                Some(provider) => settings.provider = Some(provider),
                None => {
                    settings.provider_rejected =
                        Some(value.as_str().unwrap_or_default().trim().to_string());
                    diagnostics.push(Diagnostic::with_key(
                        layer,
                        DiagnosticCause::InvalidValue,
                        PROVIDER_KEY,
                    ));
                }
            }
        }
        if let Some(value) = object.get(MODELS_KEY) {
            match value.as_object() {
                Some(entries) => {
                    for (tag, model) in entries {
                        // An entry keyed by a provider this build cannot reach is
                        // kept, not dropped: it belongs to a provider a newer
                        // binary selects, and deleting other people's settings is
                        // not this reader's job. It is simply never selected.
                        match model.as_str().map(str::trim) {
                            Some(model) if !model.is_empty() => {
                                settings.models.insert(tag.clone(), model.to_string());
                            }
                            _ => diagnostics.push(Diagnostic::with_key(
                                layer,
                                DiagnosticCause::InvalidValue,
                                MODELS_KEY,
                            )),
                        }
                    }
                }
                None => diagnostics.push(Diagnostic::with_key(
                    layer,
                    DiagnosticCause::InvalidValue,
                    MODELS_KEY,
                )),
            }
        }
        if let Some(value) = object.get("llmux_url") {
            // The llmux module owns what one of its URLs is allowed to be,
            // because that policy is what makes the keyless story true: the
            // request carries the prompt and no credential, so the endpoint has
            // to be on this machine. A refused one is dropped rather than kept
            // as a string somebody downstream might decide to trust.
            match value
                .as_str()
                .and_then(|raw| crate::llmux::endpoint(raw, crate::llmux::URL_KEY).ok())
            {
                Some(endpoint) => settings.llmux_url = Some(endpoint.url().to_string()),
                None => {
                    settings.llmux_url_rejected = true;
                    diagnostics.push(Diagnostic::with_key(
                        layer,
                        DiagnosticCause::InvalidValue,
                        crate::llmux::URL_KEY,
                    ));
                }
            }
        }
    }

    if let Some(value) = object.get("max_agent_steps") {
        match value.as_u64().and_then(|steps| u32::try_from(steps).ok()) {
            Some(steps) => settings.max_agent_steps = Some(steps),
            None => diagnostics.push(Diagnostic::with_key(
                layer,
                DiagnosticCause::InvalidValue,
                "max_agent_steps",
            )),
        }
    }

    settings
}

fn parse_environment(env: &Environment, diagnostics: &mut Vec<Diagnostic>) -> Settings {
    let mut settings = Settings::default();

    if let Some(model) = env.nonblank(ENV_MODEL) {
        settings.model = Some(model.to_string());
    }
    if let Some(raw) = env.nonblank(ENV_PERMISSION_MODE) {
        match PermissionMode::parse(raw) {
            Some(mode) => settings.permission_mode = Some(mode),
            None => diagnostics.push(Diagnostic::with_key(
                ConfigLayer::Process,
                DiagnosticCause::InvalidValue,
                ENV_PERMISSION_MODE,
            )),
        }
    }
    if let Some(raw) = env.nonblank(ENV_MAX_AGENT_STEPS) {
        match raw.parse::<u32>() {
            Ok(steps) => settings.max_agent_steps = Some(steps),
            Err(_) => diagnostics.push(Diagnostic::with_key(
                ConfigLayer::Process,
                DiagnosticCause::InvalidValue,
                ENV_MAX_AGENT_STEPS,
            )),
        }
    }

    settings
}

/// Records one diagnostic per profile-only key found in the project file.
///
/// The key is dropped, not applied: a checked-in repository must not be able to
/// choose the model, the permission mode, or the credential of whoever runs it.
fn report_ignored_profile_keys(
    object: &serde_json::Map<String, Value>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for key in object.keys() {
        if PROFILE_ONLY_KEYS.contains(&key.as_str()) {
            diagnostics.push(Diagnostic::with_key(
                ConfigLayer::Project,
                DiagnosticCause::IgnoredProjectProfileSetting,
                key,
            ));
        }
    }
}

/// The `workspaces` entry whose key is exactly this workspace root.
///
/// Exact match only: a prefix match would let a parent directory's entry govern
/// an unrelated nested repository.
fn exact_workspace_entry<'a>(
    object: &'a serde_json::Map<String, Value>,
    workspace_root: &Path,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<&'a serde_json::Map<String, Value>> {
    let workspaces = object.get("workspaces")?;
    let Some(map) = workspaces.as_object() else {
        diagnostics.push(Diagnostic::with_key(
            ConfigLayer::User,
            DiagnosticCause::InvalidValue,
            "workspaces",
        ));
        return None;
    };
    let entry = map.get(workspace_root.to_str()?)?;
    match entry.as_object() {
        Some(object) => Some(object),
        None => {
            diagnostics.push(Diagnostic::with_key(
                ConfigLayer::User,
                DiagnosticCause::InvalidValue,
                "workspaces",
            ));
            None
        }
    }
}

/// The outcome of consulting one settings file.
///
/// Presence and usability are kept apart deliberately. Collapsing them into a
/// single `Option` loses the difference between "the user has no settings" and
/// "the user has settings xfx threw away", and only the second one needs to be
/// shouted about.
struct LayerRead {
    /// Something exists at the path, whatever its content.
    ///
    /// Set for every outcome that produced a diagnostic, so a layer xfx has
    /// something to say about can never be reported as absent.
    present: bool,
    /// The parsed object, when the file was usable.
    object: Option<serde_json::Map<String, Value>>,
}

impl LayerRead {
    fn absent() -> Self {
        Self {
            present: false,
            object: None,
        }
    }

    /// Present but unusable; the caller has already recorded why.
    fn rejected() -> Self {
        Self {
            present: true,
            object: None,
        }
    }

    fn loaded(object: serde_json::Map<String, Value>) -> Self {
        Self {
            present: true,
            object: Some(object),
        }
    }
}

/// Reads one settings file, recording why it was rejected and whether it was
/// there at all.
fn read_layer(path: &Path, layer: ConfigLayer, diagnostics: &mut Vec<Diagnostic>) -> LayerRead {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return LayerRead::absent(),
        Err(_) => {
            // The entry could not be stat'ed. Something is there, or something
            // is wrong with the path; either way xfx owes the user a report.
            diagnostics.push(Diagnostic::new(layer, DiagnosticCause::UnreadableSettings));
            return LayerRead::rejected();
        }
    };
    if !metadata.is_file() {
        diagnostics.push(Diagnostic::new(layer, DiagnosticCause::MalformedSettings));
        return LayerRead::rejected();
    }
    if metadata.len() > MAX_SETTINGS_BYTES as u64 {
        diagnostics.push(Diagnostic::new(layer, DiagnosticCause::SettingsTooLarge));
        return LayerRead::rejected();
    }

    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(_) => {
            diagnostics.push(Diagnostic::new(layer, DiagnosticCause::UnreadableSettings));
            return LayerRead::rejected();
        }
    };
    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(object)) => LayerRead::loaded(object),
        Ok(_) | Err(_) => {
            diagnostics.push(Diagnostic::new(layer, DiagnosticCause::MalformedSettings));
            LayerRead::rejected()
        }
    }
}

/// Resolves the bearer credential from the environment.
///
/// Only these two variables are consulted, in this order, and only when nonblank
/// (design: "Credential precedence is nonblank `VERCEL_OIDC_TOKEN`, then nonblank
/// `AI_GATEWAY_API_KEY`").
fn resolve_credential(env: &Environment) -> Option<Credential> {
    for (key, source) in [
        (ENV_OIDC_TOKEN, CredentialSource::VercelOidcToken),
        (ENV_GATEWAY_KEY, CredentialSource::AiGatewayApiKey),
    ] {
        if let Some(secret) = env.nonblank(key) {
            return Some(Credential {
                source,
                secret: secret.to_string(),
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_mode_labels_round_trip() {
        for mode in [
            PermissionMode::Ask,
            PermissionMode::Auto,
            PermissionMode::Yolo,
        ] {
            assert_eq!(PermissionMode::parse(mode.label()), Some(mode));
        }
        assert_eq!(PermissionMode::parse("keychain"), None);
        assert_eq!(PermissionMode::parse(""), None);
    }

    #[test]
    fn permission_mode_parsing_tolerates_surrounding_whitespace() {
        assert_eq!(
            PermissionMode::parse("  yolo \n"),
            Some(PermissionMode::Yolo)
        );
    }

    #[test]
    fn a_credential_never_renders_its_secret() {
        let credential = Credential {
            source: CredentialSource::VercelOidcToken,
            secret: "super-secret".to_string(),
        };
        let rendered = format!("{credential:?}");
        assert!(!rendered.contains("super-secret"), "{rendered}");
        assert!(rendered.contains("VercelOidcToken"), "{rendered}");
    }

    #[test]
    fn blank_environment_values_are_treated_as_absent() {
        let env = Environment::new(
            None,
            BTreeMap::from([
                (ENV_MODEL.to_string(), "  \t ".to_string()),
                (ENV_GATEWAY_KEY.to_string(), "value".to_string()),
            ]),
        );
        assert_eq!(env.nonblank(ENV_MODEL), None);
        assert_eq!(env.nonblank(ENV_GATEWAY_KEY), Some("value"));
        assert_eq!(env.nonblank("XFX_ABSENT"), None);
    }

    #[test]
    fn a_later_layer_only_overrides_the_keys_it_sets() {
        let mut settings = Settings {
            model: Some("first".to_string()),
            permission_mode: Some(PermissionMode::Ask),
            max_agent_steps: Some(1),
            backend: Some(ProviderId::Llmux),
            backend_source: Some(SettingSource::UserGlobal),
            backend_rejected: None,
            backend_rejected_source: None,
            provider: None,
            provider_source: None,
            provider_rejected: None,
            provider_rejected_source: None,
            models: BTreeMap::new(),
            models_sources: BTreeMap::new(),
            llmux_url: Some("http://127.0.0.1:3456".to_string()),
            llmux_url_rejected: false,
        };
        let mut sources = Sources::default();
        settings.merge(
            Settings {
                max_agent_steps: Some(2),
                ..Settings::default()
            },
            SettingSource::UserGlobal,
            &mut sources,
        );
        assert_eq!(settings.model.as_deref(), Some("first"));
        assert_eq!(settings.permission_mode, Some(PermissionMode::Ask));
        assert_eq!(settings.max_agent_steps, Some(2));
        assert_eq!(settings.backend, Some(ProviderId::Llmux));
        assert_eq!(
            settings.llmux_url.as_deref(),
            Some("http://127.0.0.1:3456"),
            "a layer that says nothing about the backend leaves its url alone"
        );
        assert_eq!(sources.max_agent_steps, SettingSource::UserGlobal);
        assert_eq!(sources.model, SettingSource::CompiledDefault);
        assert_eq!(sources.provider, SettingSource::CompiledDefault);
    }

    #[test]
    fn an_explicit_zero_step_limit_is_a_value_not_an_absence() {
        // `0` means unbounded, so the merge must key off presence rather than
        // truthiness; a `if steps > 0` shortcut here would silently restore the
        // compiled bound and cap a turn the user asked to leave uncapped.
        let mut settings = Settings {
            max_agent_steps: Some(9),
            ..Settings::default()
        };
        let mut sources = Sources::default();
        settings.merge(
            Settings {
                max_agent_steps: Some(0),
                ..Settings::default()
            },
            SettingSource::ProcessOverride,
            &mut sources,
        );
        assert_eq!(settings.max_agent_steps, Some(0));
        assert_eq!(sources.max_agent_steps, SettingSource::ProcessOverride);
    }

    #[test]
    fn a_zero_step_limit_survives_parsing_from_every_layer() {
        let object = serde_json::from_str::<Value>(r#"{"max_agent_steps":0}"#).unwrap();
        let object = object.as_object().unwrap();
        let mut diagnostics = Vec::new();
        for kind in [LayerKind::Project, LayerKind::Profile] {
            let settings = parse_layer(object, kind, ConfigLayer::Project, &mut diagnostics);
            assert_eq!(settings.max_agent_steps, Some(0), "{kind:?}");
        }

        let env = Environment::new(
            None,
            BTreeMap::from([(ENV_MAX_AGENT_STEPS.to_string(), "0".to_string())]),
        );
        let settings = parse_environment(&env, &mut diagnostics);
        assert_eq!(settings.max_agent_steps, Some(0));
        assert!(diagnostics.is_empty(), "0 is valid, not a diagnostic");
    }

    #[test]
    fn an_unreadable_backend_is_carried_forward_so_a_turn_can_quote_it() {
        for (body, expected) in [
            (r#"{"backend":"anthropic"}"#, "anthropic"),
            (r#"{"backend":"  weird  "}"#, "weird"),
            // A value of the wrong type has no spelling worth quoting.
            (r#"{"backend":7}"#, ""),
        ] {
            let object = serde_json::from_str::<Value>(body).unwrap();
            let mut diagnostics = Vec::new();
            let settings = parse_layer(
                object.as_object().unwrap(),
                LayerKind::Profile,
                ConfigLayer::User,
                &mut diagnostics,
            );
            assert_eq!(settings.backend, None, "for {body}");
            assert_eq!(
                settings.backend_rejected.as_deref(),
                Some(expected),
                "for {body}"
            );
            assert_eq!(diagnostics.len(), 1, "for {body}");
        }
    }

    #[test]
    fn a_later_layer_that_cannot_be_read_does_not_leave_an_earlier_one_in_charge() {
        // Otherwise a mistyped `backend` in the workspace entry would silently
        // run under whatever the profile said, which is the fallback this whole
        // mechanism exists to refuse.
        let mut settings = Settings {
            backend: Some(ProviderId::Llmux),
            ..Settings::default()
        };
        let mut sources = Sources::default();
        settings.merge(
            Settings {
                backend_rejected: Some("nonsense".to_string()),
                ..Settings::default()
            },
            SettingSource::UserWorkspace,
            &mut sources,
        );
        assert_eq!(settings.backend, None);
        assert_eq!(settings.backend_rejected.as_deref(), Some("nonsense"));
        assert_eq!(sources.provider, SettingSource::UserWorkspace);
    }

    #[test]
    fn the_compiled_default_provider_is_the_gateway() {
        assert_eq!(ProviderId::default(), ProviderId::Gateway);
    }

    #[test]
    fn a_profile_layer_selects_the_provider_and_its_url() {
        let object = serde_json::from_str::<Value>(
            r#"{"backend":"llmux","llmux_url":"http://127.0.0.1:3456"}"#,
        )
        .unwrap();
        let mut diagnostics = Vec::new();
        let settings = parse_layer(
            object.as_object().unwrap(),
            LayerKind::Profile,
            ConfigLayer::User,
            &mut diagnostics,
        );
        assert_eq!(settings.backend, Some(ProviderId::Llmux));
        assert_eq!(settings.llmux_url.as_deref(), Some("http://127.0.0.1:3456"));
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn a_project_layer_cannot_choose_the_provider_or_its_url() {
        // A checked-in repository must not be able to redirect whoever clones
        // it at an endpoint of its choosing.
        let object = serde_json::from_str::<Value>(
            r#"{"backend":"llmux","llmux_url":"http://127.0.0.1:3456"}"#,
        )
        .unwrap();
        let object = object.as_object().unwrap();
        let mut diagnostics = Vec::new();
        let settings = parse_layer(
            object,
            LayerKind::Project,
            ConfigLayer::Project,
            &mut diagnostics,
        );
        assert_eq!(settings.backend, None);
        assert_eq!(settings.llmux_url, None);

        report_ignored_profile_keys(object, &mut diagnostics);
        let ignored: Vec<&str> = diagnostics
            .iter()
            .filter_map(Diagnostic::ignored_setting_key)
            .collect();
        assert_eq!(ignored, ["backend", "llmux_url"]);
    }

    #[test]
    fn an_unreadable_provider_or_url_is_a_diagnostic_rather_than_a_value() {
        // The URL goes through the same transport rule as the environment
        // override: a plaintext http endpoint that is not loopback would carry
        // the whole prompt to a machine the operator did not mean to name.
        for (body, key) in [
            (r#"{"backend":"anthropic"}"#, "backend"),
            (r#"{"backend":7}"#, "backend"),
            (r#"{"llmux_url":"http://example.com:80"}"#, "llmux_url"),
            (r#"{"llmux_url":"http://127.0.0.1"}"#, "llmux_url"),
            (r#"{"llmux_url":"ftp://127.0.0.1:3456"}"#, "llmux_url"),
            (r#"{"llmux_url":"  "}"#, "llmux_url"),
        ] {
            let object = serde_json::from_str::<Value>(body).unwrap();
            let mut diagnostics = Vec::new();
            let settings = parse_layer(
                object.as_object().unwrap(),
                LayerKind::Profile,
                ConfigLayer::User,
                &mut diagnostics,
            );
            assert_eq!(settings.backend, None, "for {body}");
            assert_eq!(settings.llmux_url, None, "for {body}");
            assert_eq!(
                diagnostics,
                vec![Diagnostic::with_key(
                    ConfigLayer::User,
                    DiagnosticCause::InvalidValue,
                    key
                )],
                "for {body}"
            );
        }
    }

    #[test]
    fn a_project_layer_cannot_set_profile_only_keys() {
        let object = serde_json::from_str::<Value>(
            r#"{"model":"x/y","permission_mode":"yolo","max_agent_steps":4}"#,
        )
        .unwrap();
        let object = object.as_object().unwrap();
        let mut diagnostics = Vec::new();
        let settings = parse_layer(
            object,
            LayerKind::Project,
            ConfigLayer::Project,
            &mut diagnostics,
        );
        assert_eq!(settings.model, None);
        assert_eq!(settings.permission_mode, None);
        assert_eq!(settings.max_agent_steps, Some(4));
    }

    #[test]
    fn diagnostic_details_name_the_layer_cause_and_key() {
        let diagnostic = Diagnostic::with_key(
            ConfigLayer::Project,
            DiagnosticCause::IgnoredProjectProfileSetting,
            "model",
        );
        assert_eq!(
            diagnostic.detail(),
            "project config diagnostic: ignored_project_profile_setting key=model"
        );
        assert_eq!(diagnostic.ignored_setting_key(), Some("model"));
        assert!(!diagnostic.is_too_large());
    }
}
