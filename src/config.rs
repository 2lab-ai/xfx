//! Configuration discovery, precedence, credentials, and diagnostics.
//!
//! Four layers contribute, lowest precedence first:
//!
//! 1. compiled defaults
//! 2. the project file `<workspace>/.fxr.json`
//! 3. the profile global file `~/.fxr/settings.json`
//! 4. the exact-workspace entry `workspaces["<workspace>"]` inside that same file
//! 5. the process environment (`FXR_MODEL`, `FXR_PERMISSION_MODE`, `FXR_MAX_AGENT_STEPS`)
//!
//! This mirrors the upstream merge order in
//! `vercel-labs/fx@580a0c5d src/core/config/config_runtime.zig:341-455`, where a
//! later layer overwrites an earlier one only for the keys it actually sets.
//!
//! Loading is strictly read-only. `status` and `doctor` must work on a machine
//! that has never run fxr, so nothing here creates `~/.fxr`.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// The model fxr requests when nothing selects one.
///
/// Matches upstream's Gateway default
/// (`vercel-labs/fx@580a0c5d src/builtins/gateway.zig:40`).
pub const DEFAULT_MODEL: &str = "zai/glm-5.2";

/// The compiled ceiling on model steps within one turn.
///
/// Upstream compiles in `0`, meaning unbounded
/// (`vercel-labs/fx@580a0c5d src/core/config/agent_steps.zig:3`). fxr deliberately
/// ships a bound instead; `0` remains the explicit unbounded opt-out so the
/// configured semantics still match upstream.
pub const DEFAULT_MAX_AGENT_STEPS: u32 = 25;

/// The largest settings file fxr will read, matching upstream's 64 KiB ceiling
/// (`vercel-labs/fx@580a0c5d src/core/config/config_runtime.zig:17`).
pub const MAX_SETTINGS_BYTES: usize = 64 * 1024;

/// Directory name of the profile home under `$HOME`.
pub const PROFILE_DIR_NAME: &str = ".fxr";

/// File name of the project-scoped settings file.
pub const PROJECT_SETTINGS_FILE: &str = ".fxr.json";

const USER_SETTINGS_FILE: &str = "settings.json";

const ENV_MODEL: &str = "FXR_MODEL";
const ENV_PERMISSION_MODE: &str = "FXR_PERMISSION_MODE";
const ENV_MAX_AGENT_STEPS: &str = "FXR_MAX_AGENT_STEPS";
const ENV_OIDC_TOKEN: &str = "VERCEL_OIDC_TOKEN";
const ENV_GATEWAY_KEY: &str = "AI_GATEWAY_API_KEY";

/// Settings keys that only the profile may set.
///
/// A repository is shared, so it must not be able to choose the model, the
/// permission mode, or the credential for whoever clones it. Upstream draws the
/// same line in `src/core/config/config_runtime.zig:548-576`; fxr keeps the
/// subset that its own settings surface actually supports.
const PROFILE_ONLY_KEYS: &[&str] = &[
    "model",
    "permission_mode",
    "credential_source",
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

    fn parse(raw: &str) -> Option<Self> {
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// The provenance of every setting fxr resolves.
#[derive(Debug, Clone, Copy)]
pub struct Sources {
    pub model: SettingSource,
    pub permission_mode: SettingSource,
    pub max_agent_steps: SettingSource,
}

impl Default for Sources {
    fn default() -> Self {
        Self {
            model: SettingSource::CompiledDefault,
            permission_mode: SettingSource::CompiledDefault,
            max_agent_steps: SettingSource::CompiledDefault,
        }
    }
}

/// Which file a diagnostic came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigLayer {
    /// `~/.fxr/settings.json`, including its `workspaces` entries.
    User,
    /// `<workspace>/.fxr.json`.
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
    /// A key was present with a value fxr cannot interpret.
    InvalidValue,
}

impl DiagnosticCause {
    pub fn label(self) -> &'static str {
        match self {
            Self::MalformedSettings => "malformed_settings",
            Self::SettingsTooLarge => "settings_too_large",
            Self::UnreadableSettings => "unreadable_settings",
            Self::IgnoredProjectProfileSetting => "ignored_project_profile_setting",
            Self::InvalidValue => "invalid_value",
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
}

impl Diagnostic {
    fn new(layer: ConfigLayer, cause: DiagnosticCause) -> Self {
        Self {
            layer,
            cause,
            setting_key: None,
        }
    }

    fn with_key(layer: ConfigLayer, cause: DiagnosticCause, key: &str) -> Self {
        Self {
            layer,
            cause,
            setting_key: Some(key.to_string()),
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
        detail
    }
}

/// Which environment variable supplied the active credential.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialSource {
    VercelOidcToken,
    AiGatewayApiKey,
}

impl CredentialSource {
    /// The label snapshots print. It names the variable, never the value.
    pub fn label(self) -> &'static str {
        match self {
            Self::VercelOidcToken => ENV_OIDC_TOKEN,
            Self::AiGatewayApiKey => ENV_GATEWAY_KEY,
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

/// The variables and home directory fxr is allowed to observe.
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

    /// Reads the real process environment, taking only the variables fxr uses.
    pub fn from_process() -> Self {
        let mut vars = BTreeMap::new();
        for key in [
            ENV_MODEL,
            ENV_PERMISSION_MODE,
            ENV_MAX_AGENT_STEPS,
            ENV_OIDC_TOKEN,
            ENV_GATEWAY_KEY,
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
    /// `~/.fxr`, whether or not it exists. Loading never creates it.
    pub profile_dir: Option<PathBuf>,
    /// `~/.fxr/settings.json`, whether or not it exists.
    pub user_settings_path: Option<PathBuf>,
    /// `<workspace>/.fxr.json`, whether or not it exists.
    pub project_settings_path: PathBuf,
    /// A `~/.fxr/settings.json` entry exists on disk, whether or not it was
    /// usable. Presence and usability are separate facts: a file that exists but
    /// cannot be parsed must never be reported as "no config files found".
    pub user_settings_present: bool,
    /// A `<workspace>/.fxr.json` entry exists on disk, whether or not it was
    /// usable.
    pub project_settings_present: bool,
    /// The profile settings file was parsed and merged.
    pub user_settings_loaded: bool,
    /// The project settings file was parsed and merged.
    pub project_settings_loaded: bool,
    pub model: String,
    pub permission_mode: PermissionMode,
    pub max_agent_steps: u32,
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

        Ok(Self {
            workspace_root,
            profile_dir,
            user_settings_path,
            project_settings_path,
            user_settings_present,
            project_settings_present,
            user_settings_loaded,
            project_settings_loaded,
            model: settings.model.unwrap_or_else(|| DEFAULT_MODEL.to_string()),
            permission_mode: settings.permission_mode.unwrap_or_default(),
            max_agent_steps: settings.max_agent_steps.unwrap_or(DEFAULT_MAX_AGENT_STEPS),
            sources,
            diagnostics,
            credential: resolve_credential(env),
        })
    }

    /// Whether a settings file exists on disk, usable or not.
    pub fn has_settings_file(&self) -> bool {
        self.user_settings_present || self.project_settings_present
    }

    /// Whether a settings file exists that fxr could not use.
    ///
    /// This is the case `doctor` must not describe as "no config files found":
    /// the user wrote a file, fxr ignored it, and silence would read as consent.
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
/// "the user has settings fxr threw away", and only the second one needs to be
/// shouted about.
struct LayerRead {
    /// Something exists at the path, whatever its content.
    ///
    /// Set for every outcome that produced a diagnostic, so a layer fxr has
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
            // is wrong with the path; either way fxr owes the user a report.
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
        assert_eq!(env.nonblank("FXR_ABSENT"), None);
    }

    #[test]
    fn a_later_layer_only_overrides_the_keys_it_sets() {
        let mut settings = Settings {
            model: Some("first".to_string()),
            permission_mode: Some(PermissionMode::Ask),
            max_agent_steps: Some(1),
        };
        let mut sources = Sources::default();
        settings.merge(
            Settings {
                model: None,
                permission_mode: None,
                max_agent_steps: Some(2),
            },
            SettingSource::UserGlobal,
            &mut sources,
        );
        assert_eq!(settings.model.as_deref(), Some("first"));
        assert_eq!(settings.permission_mode, Some(PermissionMode::Ask));
        assert_eq!(settings.max_agent_steps, Some(2));
        assert_eq!(sources.max_agent_steps, SettingSource::UserGlobal);
        assert_eq!(sources.model, SettingSource::CompiledDefault);
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
