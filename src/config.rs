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
/// The settings object holding which of the footer's optional segments are
/// shown, upstream's spelling (`config_runtime.zig:1807-1826`, `c1db919`).
pub const STATUS_LINE_KEY: &str = "statusLine";
/// Whether the slash popup shows each command's category, upstream's key
/// (`config_runtime.zig:63`, written by `/settings`).
pub const SLASH_MENU_CATEGORIES_KEY: &str = "slash_menu_categories";
/// Whether a tool group is drawn as its header alone, upstream's key
/// (`settings_store.zig:108`, written by `/settings`).
pub const COLLAPSE_TOOL_CALLS_KEY: &str = "collapse_tool_calls";
/// Whether a launch pushes the shell's output above the cursor into
/// scrollback and starts on the screen's first row, upstream's key
/// (`config_runtime.zig:1778-1782`, written by `/settings`).
pub const STARTUP_SCROLLBACK_KEY: &str = "startup_scrollback";
/// Whether a fresh session's first prompt asks a model for the session's
/// title, upstream's key (`config_runtime.zig:1753-1757`, written by
/// `/settings`).
pub const SESSION_TITLES_KEY: &str = "session_titles";
/// The object holding whether submitted lines are kept for recall:
/// `prompt_history.enabled`, upstream's shape (`settings_store.zig:1076-1085`).
pub const PROMPT_HISTORY_KEY: &str = "prompt_history";

/// The session's reasoning effort, upstream's key (`config_runtime.zig:1798-1805`,
/// `c1db919`): `"auto"` or a level the catalog names. Profile-only, as
/// upstream's is (`isProfileOnlySettingKey`, `:834-861`): how hard a person
/// wants their model to think is theirs, not a repository's.
pub const EFFORT_KEY: &str = "effort";

/// Fast mode, upstream's key: a `bool`. Profile-only, as upstream's is.
pub const FAST_MODE_KEY: &str = "fast_mode";

/// Ultra mode, upstream's key: a `bool`. Profile-only, as upstream's is.
pub const ULTRAFAST_MODE_KEY: &str = "ultrafast_mode";

/// Whether the profile's Fast mode was chosen for the model it holds,
/// upstream's key (`resolveStartupFastMode`, `app_lifecycle.zig:827-844`).
/// Upstream writes it `true` beside every model, effort or speed change; what
/// it governs is the footer's `⚡︎`, which marks a Fast mode that is bound and
/// on. A `fast_mode` an operator wrote by hand without it is on and sent, and
/// not marked.
pub const FAST_MODE_MODEL_BOUND_KEY: &str = "fast_mode_model_bound";

/// The configured permission table (`config_runtime.zig:1845-1850`): an
/// action for everything, or categories to actions or to patterns to actions
/// (`crate::permission::rules::RuleSet::parse`). Profile-only, as upstream's is
/// (`isProfileOnlySettingKey`, `:834-866`): a repository that could allowlist
/// commands for whoever clones it would be choosing what runs on their machine.
pub const PERMISSION_KEY: &str = "permission";

/// The two places a permission table can live, and which one is in force.
///
/// Upstream keeps the same three facts (`PermissionSourceViews`,
/// `config_runtime.zig:994-1005`): the profile's own table (`user`), this
/// workspace's entry in the same file (`local`), and whether the second exists
/// at all. **Whole-table replacement, not a merge**: a workspace entry with a
/// `permission` key -- even `{}` -- puts every user rule out of force here, and
/// `/allowlist` says so.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PermissionTables {
    /// `permission` at the top of `~/.xfx/settings.json`.
    pub user: crate::permission::rules::RuleSet,
    /// `workspaces["<this workspace>"].permission`.
    pub local: crate::permission::rules::RuleSet,
    /// This workspace's entry carries a readable `permission` key, so `local`
    /// is the table in force and `user` is not consulted.
    pub user_shadowed_by_local: bool,
}

impl PermissionTables {
    /// The table every decision in this workspace reads.
    pub fn effective(&self) -> &crate::permission::rules::RuleSet {
        if self.user_shadowed_by_local {
            &self.local
        } else {
            &self.user
        }
    }
}

/// The environment override for the model.
///
/// Public so `xfx setup llmux` can *name* it: setup writes the profile, and an
/// operator whose shell overrides what was just written has to be told which
/// variable is doing it.
pub const ENV_MODEL: &str = "XFX_MODEL";
const ENV_PERMISSION_MODE: &str = "XFX_PERMISSION_MODE";
const ENV_MAX_AGENT_STEPS: &str = "XFX_MAX_AGENT_STEPS";

/// The environment override for `/sound`'s start value, upstream's `FX_SOUND`
/// (`vercel-labs/fx@c1db9191 src/core/app/app_lifecycle.zig:1714-1725`).
///
/// `0`, `false` or `off` (the last two in any case) start the session with
/// sound off; any other value that is not blank starts it on. It decides where
/// the session **starts** and nothing else: `/sound` still moves it, and still
/// saves what it moved it to. A harness that must stay silent sets
/// `XFX_SOUND=off`, as fx's own sets `FX_SOUND=0`.
pub const ENV_SOUND: &str = "XFX_SOUND";
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
    COLLAPSE_TOOL_CALLS_KEY,
    "credential_source",
    EFFORT_KEY,
    FAST_MODE_KEY,
    FAST_MODE_MODEL_BOUND_KEY,
    ULTRAFAST_MODE_KEY,
    "llmux_url",
    "model",
    "models",
    "notifications",
    PERMISSION_KEY,
    "permission_mode",
    PROMPT_HISTORY_KEY,
    "provider",
    SESSION_TITLES_KEY,
    SLASH_MENU_CATEGORIES_KEY,
    STARTUP_SCROLLBACK_KEY,
    STATUS_LINE_KEY,
    "workspaces",
    "yolo_acknowledged",
];

/// Which of the footer's three optional segments are shown
/// (`statusLine.{context,session,workspace}`).
///
/// **All three off unless a profile turns one on**, as upstream's are
/// (`settings_catalog.zig:58-60`, `c1db919`): the session title, the context
/// meter and the workspace path are things a user asks the footer for, not
/// things it says by default. Profile-only ([`PROFILE_ONLY_KEYS`]), because
/// what a person's footer shows is theirs and not a repository's to choose --
/// upstream lists `statusLine` there too (`config_runtime.zig:853`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StatusLine {
    /// How much of the context window the conversation has spent.
    pub context: bool,
    /// The session's title.
    pub session: bool,
    /// The workspace's path and its Git branch.
    pub workspace: bool,
}

impl StatusLine {
    /// Whether `item` is on.
    pub fn get(self, item: StatusLineItem) -> bool {
        match item {
            StatusLineItem::Context => self.context,
            StatusLineItem::Session => self.session,
            StatusLineItem::Workspace => self.workspace,
        }
    }

    /// Turns `item` on or off.
    pub fn set(&mut self, item: StatusLineItem, on: bool) {
        match item {
            StatusLineItem::Context => self.context = on,
            StatusLineItem::Session => self.session = on,
            StatusLineItem::Workspace => self.workspace = on,
        }
    }
}

/// One of the footer's optional segments, by the name `/statusline` takes and
/// the profile keeps it under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusLineItem {
    Context,
    Session,
    Workspace,
}

impl StatusLineItem {
    /// Every item, in upstream's order (`settings_catalog.zig:118-134`).
    pub const ALL: [Self; 3] = [Self::Context, Self::Session, Self::Workspace];

    /// The word `/statusline` takes, the notice says and the key is spelled
    /// with (`@tagName(item)`).
    pub fn name(self) -> &'static str {
        match self {
            Self::Context => "context",
            Self::Session => "session",
            Self::Workspace => "workspace",
        }
    }

    /// The item an argument names, or `None`.
    ///
    /// **Exact and case-sensitive** once spaces and tabs are off its ends, as
    /// upstream's is (`parseStatuslineItem`, `app_commands.zig:3863-3869`) --
    /// unlike `/settings` and `/allowlist`, which fold case. `Context` and
    /// `context please` are both a usage error.
    pub fn parse(argument: &str) -> Option<Self> {
        let word = argument.trim_matches([' ', '\t']);
        Self::ALL.into_iter().find(|item| item.name() == word)
    }

    /// Where the profile keeps it: `statusLine.<name>`.
    pub fn key_path(self) -> [&'static str; 2] {
        [STATUS_LINE_KEY, self.name()]
    }
}

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

    /// The word a person reads for this mode: upstream's
    /// (`permissionModeDisplayLabel`, `vercel-labs/fx@c1db9191
    /// src/core/permissions/permissions.zig`), where the mode that skips every
    /// check is `full access` -- on the hint row, in `/permissions`, and in
    /// the notice that sets it.
    ///
    /// **Not [`Self::label`]**, which stays the wire word: a session log and
    /// the settings file record `yolo`, as upstream's own settings file does
    /// (`"permission_mode":"yolo"`), and a reader of either must be able to
    /// read back what an older xfx wrote.
    pub fn display_label(self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::Auto => "auto",
            Self::Yolo => "full access",
        }
    }

    /// A mode as a person types it: upstream's grammar
    /// (`PermissionMode.parse`, `vercel-labs/fx@c1db9191
    /// src/core/shared/types.zig:2396-2403`) -- `ask`, `auto`, and `full-access`,
    /// `full access` or `yolo` for the third, in any case.
    ///
    /// The wire grammar is [`Self::parse`] and stays exact: a session log is
    /// something xfx wrote, and one that says `Full Access` is not one it
    /// wrote. This is what an argument to `/permissions` is read with, and it
    /// ends in that same table rather than in one of its own.
    pub fn parse_choice(raw: &str) -> Option<Self> {
        let word = raw.trim().to_ascii_lowercase();
        match word.as_str() {
            "full-access" | "full access" => Some(Self::Yolo),
            other => Self::parse(other),
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

/// The two switches `/sound` moves, as a session starts with them.
///
/// Upstream keeps them apart in its settings (`"notifications": {"turn_end",
/// "attention_required"}`, `vercel-labs/fx@c1db9191
/// src/core/config/config_runtime.zig:1829-1843`) and reads them apart, so a
/// file that sets only one keeps the other at its default -- and so does this.
/// `/sound` always writes both to the same value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Notifications {
    /// Ring when a turn finishes, whether it answered or failed.
    pub turn_end: bool,
    /// Ring when the session stops to ask the user something.
    pub attention_required: bool,
}

impl Notifications {
    /// Both switches at once, which is the only way `/sound` sets them.
    pub fn both(on: bool) -> Self {
        Self {
            turn_end: on,
            attention_required: on,
        }
    }
}

impl Default for Notifications {
    /// Upstream's compiled default: on where it has an audio player to play
    /// through (macOS), off elsewhere, where all it has is the terminal bell
    /// (`vercel-labs/fx@c1db9191 src/core/notifications/sound.zig:9-11`). xfx
    /// only ever rings the bell, so it keeps the same split rather than
    /// inventing a third rule: a Linux user gets what fx gives them there.
    fn default() -> Self {
        Self::both(cfg!(target_os = "macos"))
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
    /// Where `fast_mode` and `fast_mode_model_bound` came from: the binding
    /// holds only when they and the model came from one layer
    /// (`resolveStartupFastMode`, `app_lifecycle.zig:827-844`).
    pub fast_mode: SettingSource,
    pub fast_mode_model_bound: SettingSource,
    /// Where `effort` and `ultrafast_mode` came from: they decide nothing at
    /// launch, and are kept only so a save can say that a layer above the
    /// profile -- this workspace's entry -- will still decide them
    /// (`appendShadowedUserSources`, `session_commands.zig:182-184`).
    pub effort: SettingSource,
    pub ultrafast_mode: SettingSource,
}

impl Default for Sources {
    fn default() -> Self {
        Self {
            model: SettingSource::CompiledDefault,
            permission_mode: SettingSource::CompiledDefault,
            max_agent_steps: SettingSource::CompiledDefault,
            provider: SettingSource::CompiledDefault,
            llmux_url: SettingSource::CompiledDefault,
            fast_mode: SettingSource::CompiledDefault,
            fast_mode_model_bound: SettingSource::CompiledDefault,
            effort: SettingSource::CompiledDefault,
            ultrafast_mode: SettingSource::CompiledDefault,
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
            ENV_SOUND,
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
    /// Whether the person at this profile has already been shown the
    /// full-access warning once (`yolo_acknowledged`).
    ///
    /// Upstream shows the warning on the hint row the first time a session is
    /// in that mode and records that it did, so it is shown once per user,
    /// ever (`app_permission_runtime.zig:99-111`, `:168-176`). Profile-only, like
    /// the mode it is about: a repository cannot decide that its cloner has
    /// already been warned.
    pub yolo_acknowledged: bool,
    pub max_agent_steps: u32,
    /// What `/sound` starts at: [`ENV_SOUND`], else the profile's
    /// `notifications` (each switch on its own), else the compiled default.
    pub notifications: Notifications,
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
    /// Which of the footer's optional segments the profile turned on.
    pub statusline: StatusLine,
    /// Whether the slash popup shows each command's category: on unless the
    /// profile turns it off (`settings_catalog.zig:61`, `app_lifecycle.zig:785`).
    pub slash_menu_categories: bool,
    /// Whether a tool group is drawn as its header alone: off unless the
    /// profile turns it on (`settings_catalog.zig:62`, `app_lifecycle.zig:786`).
    pub collapse_tool_calls: bool,
    /// Whether a launch pushes the shell's output into scrollback and starts on
    /// the first row: on unless the profile turns it off
    /// (`settings_catalog.zig:64`, `app_lifecycle.zig:789`). Off, the session
    /// starts under that output (`prepareStartupViewport`,
    /// `app_lifecycle.zig:966-988`; [`crate::tui`]'s launch).
    pub startup_scrollback: bool,
    /// Whether a fresh session's first prompt asks the provider for a title
    /// (`crate::tui`'s `titles`): on unless the profile turns it off
    /// (`settings_catalog.zig:63`, `app_lifecycle.zig:811`).
    pub session_titles: bool,
    /// Whether submitted lines are kept for the composer's recall and written
    /// to `~/.xfx/history.jsonl`: on unless the profile turns it off
    /// (`settings_catalog.zig:65`).
    pub prompt_history: bool,
    /// The reasoning effort a session starts in ([`EFFORT_KEY`]), the default
    /// when the profile names none. Whether a given model *takes* it is the
    /// catalog's question, asked per request
    /// (`crate::provider::effort::Effort::for_request`).
    pub effort: crate::provider::effort::Effort,
    /// Fast mode as the profile left it ([`FAST_MODE_KEY`]); off when Ultra is
    /// on, because the two are never on together.
    pub fast_mode: bool,
    /// Whether that Fast mode is bound to the model ([`FAST_MODE_MODEL_BOUND_KEY`]).
    pub fast_mode_model_bound: bool,
    /// Ultra mode as the profile left it ([`ULTRAFAST_MODE_KEY`]).
    pub ultrafast_mode: bool,
    /// The directories `/workspace` saved for this workspace, as the profile
    /// spells them: `workspaces["<workspace>"].additional_directories`
    /// (`crate::workspace::access`). Profile-only, because `workspaces` is: a
    /// repository cannot add a directory to its cloner's session.
    pub additional_directories: Vec<String>,
    /// The configured permission tables ([`PERMISSION_KEY`]), user and this
    /// workspace's, and which one is in force.
    pub permission: PermissionTables,
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
        let mut additional_directories = Vec::new();
        let mut permission = PermissionTables::default();
        if let Some(object) = &user.object {
            let layer = parse_layer(
                object,
                LayerKind::Profile,
                ConfigLayer::User,
                &mut diagnostics,
            );
            settings.merge(layer, SettingSource::UserGlobal, &mut sources);
            if let Some(table) = permission_table(object, &mut diagnostics) {
                permission.user = table;
            }

            if let Some(entry) = exact_workspace_entry(object, &workspace_root, &mut diagnostics) {
                let layer = parse_layer(
                    entry,
                    LayerKind::Profile,
                    ConfigLayer::User,
                    &mut diagnostics,
                );
                settings.merge(layer, SettingSource::UserWorkspace, &mut sources);
                // Only in the workspace's own entry: a directory saved for one
                // workspace is not one every workspace may use.
                match crate::workspace::access::saved_from_entry(entry) {
                    Ok(saved) => additional_directories = saved,
                    Err(_) => diagnostics.push(Diagnostic::with_key(
                        ConfigLayer::User,
                        DiagnosticCause::InvalidValue,
                        crate::workspace::access::ADDITIONAL_DIRECTORIES_KEY,
                    )),
                }
                if let Some(table) = permission_table(entry, &mut diagnostics) {
                    permission.local = table;
                    permission.user_shadowed_by_local = true;
                }
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
            yolo_acknowledged: settings.yolo_acknowledged.unwrap_or(false),
            max_agent_steps: settings.max_agent_steps.unwrap_or(DEFAULT_MAX_AGENT_STEPS),
            notifications: Notifications {
                turn_end: settings
                    .notification_turn_end
                    .unwrap_or(Notifications::default().turn_end),
                attention_required: settings
                    .notification_attention_required
                    .unwrap_or(Notifications::default().attention_required),
            },
            provider,
            provider_rejected,
            llmux_url: settings.llmux_url,
            models: settings.models,
            statusline: StatusLine {
                context: settings.statusline_context.unwrap_or(false),
                session: settings.statusline_session.unwrap_or(false),
                workspace: settings.statusline_workspace.unwrap_or(false),
            },
            slash_menu_categories: settings.slash_menu_categories.unwrap_or(true),
            collapse_tool_calls: settings.collapse_tool_calls.unwrap_or(false),
            startup_scrollback: settings.startup_scrollback.unwrap_or(true),
            session_titles: settings.session_titles.unwrap_or(true),
            prompt_history: settings.prompt_history.unwrap_or(true),
            effort: settings.effort.unwrap_or_default(),
            // Ultra outranks Fast, as turning it on turns Fast off.
            fast_mode: settings.fast_mode.unwrap_or(false)
                && !settings.ultrafast_mode.unwrap_or(false),
            // Bound only to a model chosen in the layer Fast was
            // (`resolveStartupFastMode`, `app_lifecycle.zig:827-844`).
            fast_mode_model_bound: settings.fast_mode.unwrap_or(false)
                && settings.fast_mode_model_bound.unwrap_or(false)
                && sources.model == sources.fast_mode
                && sources.fast_mode == sources.fast_mode_model_bound,
            ultrafast_mode: settings.ultrafast_mode.unwrap_or(false),
            additional_directories,
            permission,
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
    yolo_acknowledged: Option<bool>,
    max_agent_steps: Option<u32>,
    /// `notifications.turn_end`, and the override's word for it.
    notification_turn_end: Option<bool>,
    /// `notifications.attention_required`, and the override's word for it.
    notification_attention_required: Option<bool>,
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
    /// `statusLine.context`, `.session` and `.workspace`, each merged on its
    /// own: a layer that sets one says nothing about the other two.
    statusline_context: Option<bool>,
    statusline_session: Option<bool>,
    statusline_workspace: Option<bool>,
    /// `slash_menu_categories`.
    slash_menu_categories: Option<bool>,
    /// `collapse_tool_calls`.
    collapse_tool_calls: Option<bool>,
    /// `startup_scrollback`.
    startup_scrollback: Option<bool>,
    /// `session_titles`.
    session_titles: Option<bool>,
    /// `prompt_history.enabled`.
    prompt_history: Option<bool>,
    /// `effort`, when a layer wrote a readable one.
    effort: Option<crate::provider::effort::Effort>,
    /// `fast_mode`, `fast_mode_model_bound` and `ultrafast_mode`, each merged
    /// on its own.
    fast_mode: Option<bool>,
    fast_mode_model_bound: Option<bool>,
    ultrafast_mode: Option<bool>,
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
        // Per item, as upstream merges them (`config_runtime.zig:1999-2001`).
        // No provenance is kept: nothing reports which layer turned a segment
        // on, and nothing decides anything by it.
        if let Some(on) = incoming.statusline_context {
            self.statusline_context = Some(on);
        }
        if let Some(on) = incoming.statusline_session {
            self.statusline_session = Some(on);
        }
        if let Some(on) = incoming.statusline_workspace {
            self.statusline_workspace = Some(on);
        }
        if let Some(on) = incoming.fast_mode {
            self.fast_mode = Some(on);
            sources.fast_mode = source;
        }
        if let Some(on) = incoming.fast_mode_model_bound {
            self.fast_mode_model_bound = Some(on);
            sources.fast_mode_model_bound = source;
        }
        if let Some(on) = incoming.ultrafast_mode {
            self.ultrafast_mode = Some(on);
            sources.ultrafast_mode = source;
        }
        // The `/settings` switches, each on its own and with no
        // provenance kept, for the reason the segments above keep none.
        if let Some(on) = incoming.slash_menu_categories {
            self.slash_menu_categories = Some(on);
        }
        if let Some(on) = incoming.collapse_tool_calls {
            self.collapse_tool_calls = Some(on);
        }
        if let Some(on) = incoming.startup_scrollback {
            self.startup_scrollback = Some(on);
        }
        if let Some(on) = incoming.session_titles {
            self.session_titles = Some(on);
        }
        if let Some(on) = incoming.prompt_history {
            self.prompt_history = Some(on);
        }
        if let Some(effort) = incoming.effort {
            self.effort = Some(effort);
            sources.effort = source;
        }
        if let Some(mode) = incoming.permission_mode {
            self.permission_mode = Some(mode);
            sources.permission_mode = source;
        }
        if let Some(acknowledged) = incoming.yolo_acknowledged {
            self.yolo_acknowledged = Some(acknowledged);
        }
        if let Some(steps) = incoming.max_agent_steps {
            self.max_agent_steps = Some(steps);
            sources.max_agent_steps = source;
        }
        // Field by field, as upstream merges them (`config_runtime.zig:2002-2004`):
        // a workspace entry that sets one switch leaves the other where the
        // global file put it. No provenance is kept, because nothing reports it.
        if let Some(on) = incoming.notification_turn_end {
            self.notification_turn_end = Some(on);
        }
        if let Some(on) = incoming.notification_attention_required {
            self.notification_attention_required = Some(on);
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
        if let Some(value) = object.get("yolo_acknowledged") {
            match value.as_bool() {
                Some(acknowledged) => settings.yolo_acknowledged = Some(acknowledged),
                None => diagnostics.push(Diagnostic::with_key(
                    layer,
                    DiagnosticCause::InvalidValue,
                    "yolo_acknowledged",
                )),
            }
        }
        if let Some(value) = object.get(STATUS_LINE_KEY) {
            parse_statusline(value, layer, &mut settings, diagnostics);
        }
        // Upstream's reading (`config_runtime.zig:1798-1805`): a string it can
        // parse, or `null` for the default. Anything else is a diagnostic and
        // the effort an earlier layer chose stands.
        // The speed switches: each a `bool`, and a value of any other type a
        // diagnostic that leaves what an earlier layer said.
        for (key, slot) in [
            (FAST_MODE_KEY, &mut settings.fast_mode),
            (
                FAST_MODE_MODEL_BOUND_KEY,
                &mut settings.fast_mode_model_bound,
            ),
            (ULTRAFAST_MODE_KEY, &mut settings.ultrafast_mode),
        ] {
            if let Some(value) = object.get(key) {
                match value.as_bool() {
                    Some(on) => *slot = Some(on),
                    None => diagnostics.push(Diagnostic::with_key(
                        layer,
                        DiagnosticCause::InvalidValue,
                        key,
                    )),
                }
            }
        }
        if let Some(value) = object.get(EFFORT_KEY) {
            let effort = match value {
                Value::Null => Some(crate::provider::effort::Effort::Auto),
                Value::String(raw) => crate::provider::effort::Effort::parse(raw),
                _ => None,
            };
            match effort {
                Some(effort) => settings.effort = Some(effort),
                None => diagnostics.push(Diagnostic::with_key(
                    layer,
                    DiagnosticCause::InvalidValue,
                    EFFORT_KEY,
                )),
            }
        }
        if let Some(value) = object.get(NOTIFICATIONS_KEY) {
            parse_notifications(value, &mut settings, layer, diagnostics);
        }
        for (key, slot) in [
            (
                SLASH_MENU_CATEGORIES_KEY,
                &mut settings.slash_menu_categories,
            ),
            (COLLAPSE_TOOL_CALLS_KEY, &mut settings.collapse_tool_calls),
            (STARTUP_SCROLLBACK_KEY, &mut settings.startup_scrollback),
            (SESSION_TITLES_KEY, &mut settings.session_titles),
        ] {
            if let Some(value) = object.get(key) {
                match value.as_bool() {
                    Some(on) => *slot = Some(on),
                    None => diagnostics.push(Diagnostic::with_key(
                        layer,
                        DiagnosticCause::InvalidValue,
                        key,
                    )),
                }
            }
        }
        if let Some(value) = object.get(PROMPT_HISTORY_KEY) {
            parse_prompt_history(value, layer, &mut settings, diagnostics);
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

/// Reads one layer's `statusLine` object into `settings`.
///
/// Upstream's reading (`config_runtime.zig:1807-1826`): the value is an object,
/// each of the three items in it is a `bool`, and a key inside it that names no
/// item -- upstream's own retired `sandbox`, say -- is left alone rather than
/// reported, because a profile a newer build wrote is not malformed. A value of
/// the wrong type is a diagnostic naming the item, and the item keeps whatever
/// an earlier layer said: a typo must not turn a segment the user can see into
/// one they cannot.
fn parse_statusline(
    value: &Value,
    layer: ConfigLayer,
    settings: &mut Settings,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(items) = value.as_object() else {
        diagnostics.push(Diagnostic::with_key(
            layer,
            DiagnosticCause::InvalidValue,
            STATUS_LINE_KEY,
        ));
        return;
    };
    for (item, slot) in [
        ("context", &mut settings.statusline_context),
        ("session", &mut settings.statusline_session),
        ("workspace", &mut settings.statusline_workspace),
    ] {
        let Some(on) = items.get(item) else {
            continue;
        };
        match on.as_bool() {
            Some(on) => *slot = Some(on),
            None => diagnostics.push(Diagnostic::with_key(
                layer,
                DiagnosticCause::InvalidValue,
                &format!("{STATUS_LINE_KEY}.{item}"),
            )),
        }
    }
}

/// Reads one layer's `prompt_history` object into `settings`.
///
/// Upstream's shape (`settings_store.zig:1076-1085`, `:2011`): an object whose
/// `enabled` is a `bool`, other keys in it left alone for a build that knows
/// them. A value of the wrong type is a diagnostic and keeps whatever an
/// earlier layer said, as a mistyped `statusLine` item does.
fn parse_prompt_history(
    value: &Value,
    layer: ConfigLayer,
    settings: &mut Settings,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let enabled = value.as_object().map(|object| object.get("enabled"));
    match enabled {
        Some(None) => {}
        Some(Some(Value::Bool(on))) => settings.prompt_history = Some(*on),
        _ => diagnostics.push(Diagnostic::with_key(
            layer,
            DiagnosticCause::InvalidValue,
            &format!("{PROMPT_HISTORY_KEY}.enabled"),
        )),
    }
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
    if let Some(raw) = env.nonblank(ENV_SOUND) {
        // Upstream's reading, word for word (`soundEnvOverride`,
        // `app_lifecycle.zig:1714-1725`): there is no invalid value, only the
        // three that mean off. `max` is one of the others, and on this build
        // that means on -- xfx has no `max` to start at.
        let off =
            raw == "0" || raw.eq_ignore_ascii_case("false") || raw.eq_ignore_ascii_case("off");
        settings.notification_turn_end = Some(!off);
        settings.notification_attention_required = Some(!off);
    }

    settings
}

/// The settings key `/sound` writes under.
pub const NOTIFICATIONS_KEY: &str = "notifications";

/// Reads `notifications` out of one profile layer.
///
/// Upstream refuses the whole file over a value of the wrong type
/// (`config_runtime.zig:1829-1843`); xfx's rule for every key is a diagnostic
/// and the value dropped, so a mistyped switch falls back to whatever the layer
/// below said rather than taking the profile's other keys down with it.
/// `max` is upstream's and is not read: xfx has nothing for it to turn on.
fn parse_notifications(
    value: &Value,
    settings: &mut Settings,
    layer: ConfigLayer,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(object) = value.as_object() else {
        diagnostics.push(Diagnostic::with_key(
            layer,
            DiagnosticCause::InvalidValue,
            NOTIFICATIONS_KEY,
        ));
        return;
    };
    for (key, slot) in [
        ("turn_end", &mut settings.notification_turn_end),
        (
            "attention_required",
            &mut settings.notification_attention_required,
        ),
    ] {
        match object.get(key) {
            None => {}
            Some(Value::Bool(on)) => *slot = Some(*on),
            Some(_) => diagnostics.push(Diagnostic::with_key(
                layer,
                DiagnosticCause::InvalidValue,
                NOTIFICATIONS_KEY,
            )),
        }
    }
}

/// The permission tables the profile at `settings` holds for the workspace
/// at `workspace_root`, read afresh -- what `/allowlist` puts in force after
/// it has written the file (`loadDetailedSettingsForNotice`,
/// `session_commands.zig:137-142`).
///
/// No file is no tables. A file that cannot be read, or a table in it that
/// does not parse, is an error naming why: upstream's post-commit resolution
/// treats both as a reload that failed (`postCommitResolutionError`,
/// `:144-165`), and the caller says so rather than putting half a table in
/// force.
pub fn read_permission_tables(
    settings: &Path,
    workspace_root: &Path,
) -> Result<PermissionTables, String> {
    let (tables, diagnostics) = read_permission_tables_as_loaded(settings, workspace_root);
    match diagnostics.first() {
        Some(diagnostic) => Err(diagnostic.detail()),
        None => Ok(tables),
    }
}

/// The tables a launch would load from the profile at `settings` -- a file or
/// a table that does not read contributes nothing, as at launch -- and the
/// diagnostics that say what was dropped.
pub fn read_permission_tables_as_loaded(
    settings: &Path,
    workspace_root: &Path,
) -> (PermissionTables, Vec<Diagnostic>) {
    let mut diagnostics = Vec::new();
    let read = read_layer(settings, ConfigLayer::User, &mut diagnostics);
    let mut tables = PermissionTables::default();
    if let Some(object) = &read.object {
        if let Some(table) = permission_table(object, &mut diagnostics) {
            tables.user = table;
        }
        if let Some(entry) = exact_workspace_entry(object, workspace_root, &mut diagnostics) {
            if let Some(table) = permission_table(entry, &mut diagnostics) {
                tables.local = table;
                tables.user_shadowed_by_local = true;
            }
        }
    }
    (tables, diagnostics)
}

/// One profile layer's permission table, when it has a readable one.
///
/// A table that does not parse is a diagnostic naming the key, and the layer
/// contributes **no** table -- which is what upstream's atomic layer parse
/// makes of it too (`mergeDetailedSettingsLayer`, `config_runtime.zig:966-1027`:
/// a layer that fails to parse sets no `permission_sources` entry). In
/// particular a broken workspace table does not shadow the user's: the rules
/// in force are the ones a file that reads cleanly says.
fn permission_table(
    object: &serde_json::Map<String, Value>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<crate::permission::rules::RuleSet> {
    let value = object.get(PERMISSION_KEY)?;
    match crate::permission::rules::RuleSet::parse(value) {
        Ok(table) => Some(table),
        Err(err) => {
            diagnostics.push(Diagnostic::with_note(
                ConfigLayer::User,
                DiagnosticCause::InvalidValue,
                PERMISSION_KEY,
                err.to_string(),
            ));
            None
        }
    }
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
    fn a_typed_mode_reads_upstreams_words_in_any_case() {
        // `types.zig:2396-2403`: `full-access`, `full access` and `yolo` are
        // the third mode, case-insensitively, and so are the other two.
        for (typed, mode) in [
            ("ask", PermissionMode::Ask),
            ("ASK", PermissionMode::Ask),
            ("auto", PermissionMode::Auto),
            ("Auto", PermissionMode::Auto),
            ("full-access", PermissionMode::Yolo),
            ("Full-Access", PermissionMode::Yolo),
            ("full access", PermissionMode::Yolo),
            ("yolo", PermissionMode::Yolo),
            ("YOLO", PermissionMode::Yolo),
            ("  auto ", PermissionMode::Auto),
        ] {
            assert_eq!(PermissionMode::parse_choice(typed), Some(mode), "{typed:?}");
        }
        for typed in [
            "",
            "reset",
            "full",
            "fullaccess",
            "full  access",
            "remember",
        ] {
            assert_eq!(PermissionMode::parse_choice(typed), None, "{typed:?}");
        }
        // The wire grammar is not widened by it: a log or a settings file
        // that says `Full Access` is not one xfx wrote.
        assert_eq!(PermissionMode::parse("full-access"), None);
        assert_eq!(PermissionMode::parse("YOLO"), None);
    }

    #[test]
    fn the_mode_that_skips_every_check_reads_full_access_and_records_yolo() {
        assert_eq!(PermissionMode::Ask.display_label(), "ask");
        assert_eq!(PermissionMode::Auto.display_label(), "auto");
        assert_eq!(PermissionMode::Yolo.display_label(), "full access");
        assert_eq!(PermissionMode::Yolo.label(), "yolo");
    }

    #[test]
    fn the_preferences_the_shell_writes_are_read_back_from_the_profile_only() {
        // The round trip `/permissions` depends on: what the profile writer
        // puts down is what the next launch resolves, through the loader and
        // not through a second reading of the file.
        let home = tempfile::tempdir().expect("home");
        let workspace = tempfile::tempdir().expect("workspace");
        let env = Environment::new(Some(home.path().to_path_buf()), BTreeMap::new());
        let fresh = RuntimeConfig::load_with(&env, workspace.path()).expect("load");
        assert_eq!(fresh.permission_mode, PermissionMode::Auto);
        assert!(!fresh.yolo_acknowledged);

        let path = fresh.user_settings_path.clone().expect("a profile path");
        crate::provider::profile::write_preference(
            &path,
            &["permission_mode"],
            Value::String(PermissionMode::Yolo.label().to_string()),
        )
        .expect("write the mode");
        crate::provider::profile::write_preference(
            &path,
            &["yolo_acknowledged"],
            Value::Bool(true),
        )
        .expect("write the acknowledgement");
        let reloaded = RuntimeConfig::load_with(&env, workspace.path()).expect("reload");
        assert_eq!(reloaded.permission_mode, PermissionMode::Yolo);
        assert_eq!(reloaded.sources.permission_mode, SettingSource::UserGlobal);
        assert!(reloaded.yolo_acknowledged);
        assert!(
            reloaded.diagnostics.is_empty(),
            "{:?}",
            reloaded.diagnostics
        );

        // A repository cannot say its cloner has been warned.
        std::fs::write(
            workspace.path().join(PROJECT_SETTINGS_FILE),
            r#"{"yolo_acknowledged":false}"#,
        )
        .expect("project file");
        let shadowed = RuntimeConfig::load_with(&env, workspace.path()).expect("reload");
        assert!(shadowed.yolo_acknowledged);
        assert!(shadowed
            .diagnostics
            .iter()
            .any(|d| d.ignored_setting_key() == Some("yolo_acknowledged")));

        // And a value of the wrong type is a diagnostic, not a yes.
        std::fs::write(&path, r#"{"yolo_acknowledged":"yes"}"#).expect("rewrite");
        let refused = RuntimeConfig::load_with(&env, workspace.path()).expect("reload");
        assert!(!refused.yolo_acknowledged);
        assert!(refused.diagnostics.iter().any(|d| {
            d.cause == DiagnosticCause::InvalidValue
                && d.setting_key.as_deref() == Some("yolo_acknowledged")
        }));
    }

    #[test]
    fn the_settings_menus_switches_round_trip_through_the_profile_only() {
        // `/settings`' `Slash menu categories`, `Collapse tool calls`,
        // `Startup scrollback` and `Prompt history`: upstream's keys and
        // defaults (`settings_catalog.zig:61-65`), written by the profile
        // writer and read back by the next launch.
        let home = tempfile::tempdir().expect("home");
        let workspace = tempfile::tempdir().expect("workspace");
        let env = Environment::new(Some(home.path().to_path_buf()), BTreeMap::new());
        let fresh = RuntimeConfig::load_with(&env, workspace.path()).expect("load");
        assert!(fresh.slash_menu_categories);
        assert!(!fresh.collapse_tool_calls);
        assert!(fresh.startup_scrollback);
        assert!(fresh.prompt_history);

        let path = fresh.user_settings_path.clone().expect("a profile path");
        for (key, on) in [
            (&[SLASH_MENU_CATEGORIES_KEY][..], false),
            (&[COLLAPSE_TOOL_CALLS_KEY][..], true),
            (&[STARTUP_SCROLLBACK_KEY][..], false),
            (&[PROMPT_HISTORY_KEY, "enabled"][..], false),
        ] {
            crate::provider::profile::write_preference(&path, key, Value::Bool(on)).expect("write");
        }
        let written: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
        assert_eq!(
            written,
            serde_json::json!({
                "slash_menu_categories": false,
                "collapse_tool_calls": true,
                "startup_scrollback": false,
                "prompt_history": {"enabled": false}
            })
        );
        let reloaded = RuntimeConfig::load_with(&env, workspace.path()).expect("reload");
        assert!(!reloaded.slash_menu_categories);
        assert!(reloaded.collapse_tool_calls);
        assert!(!reloaded.startup_scrollback);
        assert!(!reloaded.prompt_history);
        assert!(
            reloaded.diagnostics.is_empty(),
            "{:?}",
            reloaded.diagnostics
        );

        // A repository cannot choose them for whoever clones it.
        std::fs::write(
            workspace.path().join(PROJECT_SETTINGS_FILE),
            r#"{"slash_menu_categories":true,"collapse_tool_calls":false,"startup_scrollback":true,"prompt_history":{"enabled":true}}"#,
        )
        .expect("project file");
        let shadowed = RuntimeConfig::load_with(&env, workspace.path()).expect("reload");
        assert!(!shadowed.slash_menu_categories);
        assert!(shadowed.collapse_tool_calls);
        assert!(!shadowed.startup_scrollback);
        assert!(!shadowed.prompt_history);
        for key in [
            SLASH_MENU_CATEGORIES_KEY,
            COLLAPSE_TOOL_CALLS_KEY,
            STARTUP_SCROLLBACK_KEY,
            PROMPT_HISTORY_KEY,
        ] {
            assert!(
                shadowed
                    .diagnostics
                    .iter()
                    .any(|d| d.ignored_setting_key() == Some(key)),
                "{key}: {:?}",
                shadowed.diagnostics
            );
        }

        // A value of the wrong type is a diagnostic and the default.
        std::fs::remove_file(workspace.path().join(PROJECT_SETTINGS_FILE)).expect("remove");
        std::fs::write(
            &path,
            r#"{"slash_menu_categories":"no","collapse_tool_calls":1,"startup_scrollback":"off","prompt_history":false}"#,
        )
        .expect("rewrite");
        let refused = RuntimeConfig::load_with(&env, workspace.path()).expect("reload");
        assert!(refused.slash_menu_categories);
        assert!(!refused.collapse_tool_calls);
        assert!(refused.startup_scrollback);
        assert!(refused.prompt_history);
        for key in [
            SLASH_MENU_CATEGORIES_KEY,
            COLLAPSE_TOOL_CALLS_KEY,
            STARTUP_SCROLLBACK_KEY,
            "prompt_history.enabled",
        ] {
            assert!(
                refused.diagnostics.iter().any(|d| {
                    d.cause == DiagnosticCause::InvalidValue
                        && d.setting_key.as_deref() == Some(key)
                }),
                "{key}: {:?}",
                refused.diagnostics
            );
        }
        // Keys upstream keeps beside `enabled` are not this reader's to judge.
        std::fs::write(&path, r#"{"prompt_history":{"future":1}}"#).expect("rewrite");
        let future = RuntimeConfig::load_with(&env, workspace.path()).expect("reload");
        assert!(future.prompt_history);
        assert!(future.diagnostics.is_empty(), "{:?}", future.diagnostics);
    }

    #[test]
    fn session_titles_are_on_unless_the_profile_turns_them_off() {
        // `/settings`' `Session titles`: upstream's key and default
        // (`settings_catalog.zig:63`, `app_lifecycle.zig:811`), profile-only
        // as upstream's is (`isProfileOnlySettingKey`, `config_runtime.zig:850`).
        let home = tempfile::tempdir().expect("home");
        let workspace = tempfile::tempdir().expect("workspace");
        let env = Environment::new(Some(home.path().to_path_buf()), BTreeMap::new());
        let fresh = RuntimeConfig::load_with(&env, workspace.path()).expect("load");
        assert!(fresh.session_titles);

        let path = fresh.user_settings_path.clone().expect("a profile path");
        crate::provider::profile::write_preference(
            &path,
            &[SESSION_TITLES_KEY],
            Value::Bool(false),
        )
        .expect("write");
        assert_eq!(
            std::fs::read_to_string(&path).expect("read"),
            "{\n  \"session_titles\": false\n}\n"
        );
        let off = RuntimeConfig::load_with(&env, workspace.path()).expect("reload");
        assert!(!off.session_titles);
        assert!(off.diagnostics.is_empty(), "{:?}", off.diagnostics);

        // A repository cannot turn them back on for whoever clones it.
        std::fs::write(
            workspace.path().join(PROJECT_SETTINGS_FILE),
            r#"{"session_titles":true}"#,
        )
        .expect("project file");
        let shadowed = RuntimeConfig::load_with(&env, workspace.path()).expect("reload");
        assert!(!shadowed.session_titles);
        assert!(shadowed
            .diagnostics
            .iter()
            .any(|d| d.ignored_setting_key() == Some(SESSION_TITLES_KEY)));

        // A value of the wrong type is a diagnostic and the default.
        std::fs::remove_file(workspace.path().join(PROJECT_SETTINGS_FILE)).expect("remove");
        std::fs::write(&path, r#"{"session_titles":"no"}"#).expect("rewrite");
        let refused = RuntimeConfig::load_with(&env, workspace.path()).expect("reload");
        assert!(refused.session_titles);
        assert!(refused.diagnostics.iter().any(|d| {
            d.cause == DiagnosticCause::InvalidValue
                && d.setting_key.as_deref() == Some(SESSION_TITLES_KEY)
        }));
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
            yolo_acknowledged: Some(true),
            max_agent_steps: Some(1),
            notification_turn_end: Some(true),
            notification_attention_required: Some(false),
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
            statusline_context: Some(true),
            statusline_session: None,
            statusline_workspace: None,
            slash_menu_categories: Some(false),
            collapse_tool_calls: Some(true),
            startup_scrollback: Some(false),
            session_titles: Some(false),
            prompt_history: Some(false),
            effort: Some(crate::provider::effort::Effort::Named("low".to_string())),
            fast_mode: Some(true),
            fast_mode_model_bound: None,
            ultrafast_mode: None,
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
        assert_eq!(settings.yolo_acknowledged, Some(true));
        assert_eq!(settings.max_agent_steps, Some(2));
        assert_eq!(settings.notification_turn_end, Some(true));
        assert_eq!(settings.notification_attention_required, Some(false));
        assert_eq!(settings.backend, Some(ProviderId::Llmux));
        assert_eq!(
            settings.llmux_url.as_deref(),
            Some("http://127.0.0.1:3456"),
            "a layer that says nothing about the backend leaves its url alone"
        );
        assert_eq!(settings.statusline_context, Some(true));
        assert_eq!(settings.slash_menu_categories, Some(false));
        assert_eq!(settings.collapse_tool_calls, Some(true));
        assert_eq!(settings.startup_scrollback, Some(false));
        assert_eq!(settings.session_titles, Some(false));
        assert_eq!(settings.prompt_history, Some(false));
        assert_eq!(
            settings.effort,
            Some(crate::provider::effort::Effort::Named("low".to_string()))
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

    /// A configuration loaded from `home` (with `settings` written into its
    /// profile, when there are any) and a workspace that holds nothing.
    fn loaded_with(settings: Option<&str>, vars: &[(&str, &str)]) -> RuntimeConfig {
        let home = tempfile::tempdir().expect("home");
        let workspace = tempfile::tempdir().expect("workspace");
        if let Some(settings) = settings {
            let profile = home.path().join(PROFILE_DIR_NAME);
            fs::create_dir_all(&profile).expect("profile");
            fs::write(profile.join(USER_SETTINGS_FILE), settings).expect("settings");
        }
        let vars = vars
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect();
        RuntimeConfig::load_with(
            &Environment::new(Some(home.path().to_path_buf()), vars),
            workspace.path(),
        )
        .expect("load")
    }

    #[test]
    fn sound_starts_at_upstreams_platform_default_when_nothing_sets_it() {
        // `sound.zig:9-11`: on where fx has a player (macOS), off elsewhere.
        let config = loaded_with(None, &[]);
        assert_eq!(
            config.notifications,
            Notifications::both(cfg!(target_os = "macos"))
        );
        assert_eq!(Notifications::default(), config.notifications);
    }

    #[test]
    fn the_profile_sets_each_sound_switch_on_its_own() {
        let both_off = loaded_with(
            Some(r#"{"notifications":{"turn_end":false,"attention_required":false}}"#),
            &[],
        );
        assert_eq!(both_off.notifications, Notifications::both(false));
        assert!(
            both_off.diagnostics.is_empty(),
            "{:?}",
            both_off.diagnostics
        );
        // One switch set, and the other left at the compiled default; `max`
        // is upstream's and is neither read nor refused.
        let one = loaded_with(
            Some(r#"{"notifications":{"attention_required":true,"max":true}}"#),
            &[],
        );
        assert_eq!(
            one.notifications,
            Notifications {
                turn_end: Notifications::default().turn_end,
                attention_required: true,
            }
        );
        assert!(one.diagnostics.is_empty(), "{:?}", one.diagnostics);
    }

    #[test]
    fn a_workspace_entry_overrides_one_sound_switch_and_keeps_the_other() {
        let home = tempfile::tempdir().expect("home");
        let workspace = tempfile::tempdir().expect("workspace");
        let root = workspace.path().canonicalize().expect("canonical");
        let profile = home.path().join(PROFILE_DIR_NAME);
        fs::create_dir_all(&profile).expect("profile");
        fs::write(
            profile.join(USER_SETTINGS_FILE),
            serde_json::json!({
                "notifications": {"turn_end": true, "attention_required": false},
                "workspaces": {root.to_str().unwrap(): {"notifications": {"attention_required": true}}}
            })
            .to_string(),
        )
        .expect("settings");
        let config = RuntimeConfig::load_with(
            &Environment::new(Some(home.path().to_path_buf()), BTreeMap::new()),
            workspace.path(),
        )
        .expect("load");
        assert_eq!(config.notifications, Notifications::both(true));
    }

    #[test]
    fn a_mistyped_sound_switch_is_a_diagnostic_and_falls_back() {
        for body in [
            r#"{"notifications":true}"#,
            r#"{"notifications":{"turn_end":1}}"#,
            r#"{"notifications":{"attention_required":"yes"}}"#,
        ] {
            let config = loaded_with(Some(body), &[]);
            assert_eq!(config.notifications, Notifications::default(), "{body}");
            assert_eq!(
                config.diagnostics,
                vec![Diagnostic::with_key(
                    ConfigLayer::User,
                    DiagnosticCause::InvalidValue,
                    NOTIFICATIONS_KEY
                )],
                "{body}"
            );
        }
    }

    #[test]
    fn the_sound_override_decides_where_the_session_starts() {
        // `soundEnvOverride`, `app_lifecycle.zig:1714-1725`: three spellings of
        // off, and every other word that is not blank is on -- over whatever
        // the profile says.
        let profile = Some(r#"{"notifications":{"turn_end":true,"attention_required":true}}"#);
        for off in ["0", "false", "FALSE", "off", "Off", " off "] {
            assert_eq!(
                loaded_with(profile, &[(ENV_SOUND, off)]).notifications,
                Notifications::both(false),
                "{off:?}"
            );
        }
        let quiet = Some(r#"{"notifications":{"turn_end":false,"attention_required":false}}"#);
        for on in ["1", "on", "true", "max", "yes"] {
            assert_eq!(
                loaded_with(quiet, &[(ENV_SOUND, on)]).notifications,
                Notifications::both(true),
                "{on:?}"
            );
        }
        // Blank is absent, as every override here is.
        assert_eq!(
            loaded_with(quiet, &[(ENV_SOUND, "  ")]).notifications,
            Notifications::both(false)
        );
    }

    #[test]
    fn a_project_file_cannot_choose_the_sound() {
        // Upstream lists `notifications` among its profile-only keys
        // (`config_runtime.zig:854`).
        let home = tempfile::tempdir().expect("home");
        let workspace = tempfile::tempdir().expect("workspace");
        fs::write(
            workspace.path().join(PROJECT_SETTINGS_FILE),
            r#"{"notifications":{"turn_end":false,"attention_required":false}}"#,
        )
        .expect("project");
        // On every platform the default is the answer, whatever it is there.
        let config = RuntimeConfig::load_with(
            &Environment::new(Some(home.path().to_path_buf()), BTreeMap::new()),
            workspace.path(),
        )
        .expect("load");
        assert_eq!(config.notifications, Notifications::default());
        let ignored: Vec<&str> = config
            .diagnostics
            .iter()
            .filter_map(Diagnostic::ignored_setting_key)
            .collect();
        assert_eq!(ignored, [NOTIFICATIONS_KEY]);
    }

    #[test]
    fn what_write_preference_writes_is_what_the_next_load_reads() {
        // The round trip `/sound` depends on, through the two real functions:
        // the shared writer, and the one reader that decides what a settings
        // file means.
        let home = tempfile::tempdir().expect("home");
        let workspace = tempfile::tempdir().expect("workspace");
        let env = Environment::new(Some(home.path().to_path_buf()), BTreeMap::new());
        let path = RuntimeConfig::load_with(&env, workspace.path())
            .expect("load")
            .user_settings_path
            .expect("a home has a settings path");
        for on in [true, false, true] {
            for key in ["turn_end", "attention_required"] {
                crate::provider::profile::write_preference(
                    &path,
                    &[NOTIFICATIONS_KEY, key],
                    Value::Bool(on),
                )
                .expect("write");
            }
            let reloaded = RuntimeConfig::load_with(&env, workspace.path()).expect("reload");
            assert_eq!(reloaded.notifications, Notifications::both(on));
            assert!(
                reloaded.diagnostics.is_empty(),
                "{:?}",
                reloaded.diagnostics
            );
        }
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

    /// A configuration loaded from `home` with `profile` as its settings file,
    /// for a workspace beside it.
    fn loaded_with_profile(profile: &str) -> (tempfile::TempDir, RuntimeConfig) {
        let root = tempfile::tempdir().expect("a root");
        let home = root.path().join("home");
        let workspace = root.path().join("workspace");
        fs::create_dir_all(home.join(PROFILE_DIR_NAME)).expect("a profile home");
        fs::create_dir_all(&workspace).expect("a workspace");
        fs::write(
            home.join(PROFILE_DIR_NAME).join(USER_SETTINGS_FILE),
            profile,
        )
        .expect("a profile");
        let config =
            RuntimeConfig::load_with(&Environment::new(Some(home), BTreeMap::new()), &workspace)
                .expect("load");
        (root, config)
    }

    #[test]
    fn every_statusline_segment_is_off_unless_the_profile_turns_it_on() {
        // `settings_catalog.zig:58-60`: all three default to off, upstream's
        // choice and therefore this one's.
        let (_root, config) = loaded_with_profile("{}");
        assert_eq!(config.statusline, StatusLine::default());
        assert_eq!(
            StatusLine::default(),
            StatusLine {
                context: false,
                session: false,
                workspace: false
            }
        );

        let (_root, config) = loaded_with_profile(
            r#"{"statusLine":{"session":false,"context":true,"workspace":true}}"#,
        );
        assert_eq!(
            config.statusline,
            StatusLine {
                context: true,
                session: false,
                workspace: true
            }
        );
        assert!(config.diagnostics.is_empty(), "{:?}", config.diagnostics);
    }

    #[test]
    fn a_statusline_item_of_the_wrong_type_is_reported_and_an_unknown_one_is_left_alone() {
        // Upstream's own retired `sandbox` item is in profiles it wrote
        // (`tui-slash-menu.test.ts:1777`), and it is not a mistake.
        let (_root, config) =
            loaded_with_profile(r#"{"statusLine":{"sandbox":false,"context":1,"session":true}}"#);
        assert_eq!(
            config.statusline,
            StatusLine {
                context: false,
                session: true,
                workspace: false
            }
        );
        assert_eq!(
            config.diagnostics,
            vec![Diagnostic::with_key(
                ConfigLayer::User,
                DiagnosticCause::InvalidValue,
                "statusLine.context"
            )]
        );

        let (_root, config) = loaded_with_profile(r#"{"statusLine":7}"#);
        assert_eq!(config.statusline, StatusLine::default());
        assert_eq!(
            config.diagnostics,
            vec![Diagnostic::with_key(
                ConfigLayer::User,
                DiagnosticCause::InvalidValue,
                "statusLine"
            )]
        );
    }

    /// The configuration for a workspace beside `home`, with `profile` as its
    /// settings and `project` as its `.xfx.json` when there is one.
    fn loaded_in(
        root: &Path,
        profile: impl Fn(&str) -> Value,
        project: Option<Value>,
    ) -> RuntimeConfig {
        let home = root.join("home");
        let workspace = root.join("workspace");
        fs::create_dir_all(home.join(PROFILE_DIR_NAME)).expect("a profile home");
        fs::create_dir_all(&workspace).expect("a workspace");
        let canonical = workspace.canonicalize().expect("canonical");
        fs::write(
            home.join(PROFILE_DIR_NAME).join(USER_SETTINGS_FILE),
            profile(&canonical.to_string_lossy()).to_string(),
        )
        .expect("a profile");
        if let Some(project) = project {
            fs::write(workspace.join(PROJECT_SETTINGS_FILE), project.to_string())
                .expect("a project file");
        }
        RuntimeConfig::load_with(&Environment::new(Some(home), BTreeMap::new()), &workspace)
            .expect("load")
    }

    #[test]
    fn the_saved_directories_are_this_workspaces_entry_and_nobody_elses() {
        let root = tempfile::tempdir().expect("a root");
        let config = loaded_in(
            root.path(),
            |key| {
                serde_json::json!({
                    "additional_directories": ["/global"],
                    "workspaces": {
                        key: {"additional_directories": ["/one", "/two"]},
                        "/some/other/workspace": {"additional_directories": ["/three"]}
                    }
                })
            },
            None,
        );
        assert_eq!(config.additional_directories, ["/one", "/two"]);
        assert!(config.diagnostics.is_empty(), "{:?}", config.diagnostics);
    }

    #[test]
    fn a_saved_directory_list_out_of_upstreams_shape_is_reported_and_unused() {
        let root = tempfile::tempdir().expect("a root");
        let config = loaded_in(
            root.path(),
            |key| serde_json::json!({"workspaces": {key: {"additional_directories": ["relative"]}}}),
            None,
        );
        assert!(config.additional_directories.is_empty());
        assert!(config.diagnostics.iter().any(|diagnostic| {
            diagnostic.cause == DiagnosticCause::InvalidValue
                && diagnostic.setting_key.as_deref() == Some("additional_directories")
        }));
    }

    #[test]
    fn a_project_file_cannot_add_a_directory_to_its_cloners_session() {
        let root = tempfile::tempdir().expect("a root");
        let config = loaded_in(
            root.path(),
            |_| serde_json::json!({}),
            Some(serde_json::json!({
                "workspaces": {"/anything": {"additional_directories": ["/etc"]}},
                "additional_directories": ["/etc"]
            })),
        );
        assert!(config.additional_directories.is_empty());
        assert!(config
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.ignored_setting_key() == Some("workspaces")));
    }

    #[test]
    fn a_workspace_entry_overrides_one_statusline_item_and_leaves_the_others() {
        // Merged per item, the way `permission_mode` is read: the exact
        // workspace entry outranks the global object, key by key.
        let root = tempfile::tempdir().expect("a root");
        let home = root.path().join("home");
        let workspace = root.path().join("workspace");
        fs::create_dir_all(home.join(PROFILE_DIR_NAME)).expect("a profile home");
        fs::create_dir_all(&workspace).expect("a workspace");
        let canonical = workspace.canonicalize().expect("canonical");
        let profile = serde_json::json!({
            "statusLine": {"context": true, "session": true},
            "workspaces": {canonical.to_string_lossy(): {"statusLine": {"session": false}}}
        });
        fs::write(
            home.join(PROFILE_DIR_NAME).join(USER_SETTINGS_FILE),
            profile.to_string(),
        )
        .expect("a profile");
        let config =
            RuntimeConfig::load_with(&Environment::new(Some(home), BTreeMap::new()), &workspace)
                .expect("load");
        assert_eq!(
            config.statusline,
            StatusLine {
                context: true,
                session: false,
                workspace: false
            }
        );
    }

    #[test]
    fn a_project_file_cannot_choose_what_a_users_footer_shows() {
        let object = serde_json::from_str::<Value>(r#"{"statusLine":{"context":true}}"#).unwrap();
        let object = object.as_object().unwrap();
        let mut diagnostics = Vec::new();
        report_ignored_profile_keys(object, &mut diagnostics);
        let settings = parse_layer(
            object,
            LayerKind::Project,
            ConfigLayer::Project,
            &mut diagnostics,
        );
        assert_eq!(settings.statusline_context, None);
        assert_eq!(
            diagnostics,
            vec![Diagnostic::with_key(
                ConfigLayer::Project,
                DiagnosticCause::IgnoredProjectProfileSetting,
                "statusLine"
            )]
        );
    }

    #[test]
    fn the_effort_is_the_profiles_and_the_default_unless_it_names_one() {
        use crate::provider::effort::Effort;
        let (_root, config) = loaded_with_profile("{}");
        assert_eq!(config.effort, Effort::Auto);
        let (_root, config) = loaded_with_profile(r#"{"effort":"xhigh"}"#);
        assert_eq!(config.effort, Effort::Named("xhigh".to_string()));
        // `null` and any spelling of the default are the default
        // (`config_runtime.zig:1798-1805`).
        for profile in [r#"{"effort":null}"#, r#"{"effort":"auto"}"#] {
            let (_root, config) = loaded_with_profile(profile);
            assert_eq!(config.effort, Effort::Auto, "{profile}");
        }
        // A value upstream would not parse is reported and does not become an
        // effort anybody asked for.
        for profile in [r#"{"effort":7}"#, r#"{"effort":"very high"}"#] {
            let (_root, config) = loaded_with_profile(profile);
            assert_eq!(config.effort, Effort::Auto, "{profile}");
            assert!(
                config
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.detail()
                        == "user config diagnostic: invalid_value key=effort"),
                "{profile}: {:?}",
                config.diagnostics
            );
        }
    }

    #[test]
    fn fast_is_bound_only_to_a_model_chosen_where_fast_was() {
        // `resolveStartupFastMode` (`app_lifecycle.zig:827-844`): the binding
        // holds only when the model, `fast_mode` and the binding all come from
        // the same layer -- a model an `XFX_MODEL` names, or the compiled
        // default, was not the one Fast was chosen for, so the footer does not
        // mark it (Fast is still on, and still sent).
        let profile = r#"{"models":{"gateway":"anthropic/claude-opus-5"},"provider":"gateway","fast_mode":true,"fast_mode_model_bound":true}"#;
        let (_root, config) = loaded_with_profile(profile);
        assert!(config.fast_mode && config.fast_mode_model_bound);
        let root = tempfile::tempdir().expect("a root");
        let home = root.path().join("home");
        let workspace = root.path().join("workspace");
        fs::create_dir_all(home.join(PROFILE_DIR_NAME)).expect("a profile home");
        fs::create_dir_all(&workspace).expect("a workspace");
        fs::write(
            home.join(PROFILE_DIR_NAME).join(USER_SETTINGS_FILE),
            profile,
        )
        .expect("a profile");
        let env = Environment::new(
            Some(home),
            BTreeMap::from([(ENV_MODEL.to_string(), "openai/gpt-6-astra".to_string())]),
        );
        let config = RuntimeConfig::load_with(&env, &workspace).expect("load");
        assert!(config.fast_mode, "Fast is still on");
        assert!(
            !config.fast_mode_model_bound,
            "bound to a model the environment replaced"
        );
        // The compiled default model is not the profile's either.
        let (_root, config) =
            loaded_with_profile(r#"{"fast_mode":true,"fast_mode_model_bound":true}"#);
        assert!(config.fast_mode && !config.fast_mode_model_bound);
    }

    #[test]
    fn the_speed_switches_are_the_profiles_and_ultra_outranks_fast() {
        let (_root, config) = loaded_with_profile("{}");
        assert!(!config.fast_mode && !config.ultrafast_mode && !config.fast_mode_model_bound);
        let (_root, config) = loaded_with_profile(
            r#"{"model":"acme/one","fast_mode":true,"ultrafast_mode":false,"fast_mode_model_bound":true}"#,
        );
        assert!(config.fast_mode && config.fast_mode_model_bound && !config.ultrafast_mode);
        // Fast written by hand without the binding is on, and not bound.
        let (_root, config) = loaded_with_profile(r#"{"fast_mode":true}"#);
        assert!(config.fast_mode && !config.fast_mode_model_bound);
        // Never both.
        let (_root, config) = loaded_with_profile(r#"{"fast_mode":true,"ultrafast_mode":true}"#);
        assert!(config.ultrafast_mode && !config.fast_mode);
        let (_root, config) = loaded_with_profile(r#"{"fast_mode":"yes"}"#);
        assert!(!config.fast_mode);
        assert!(
            config
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.detail()
                    == "user config diagnostic: invalid_value key=fast_mode"),
            "{:?}",
            config.diagnostics
        );
        // A repository does not choose it.
        let object = serde_json::from_str::<Value>(r#"{"ultrafast_mode":true}"#).unwrap();
        let mut diagnostics = Vec::new();
        report_ignored_profile_keys(object.as_object().unwrap(), &mut diagnostics);
        assert_eq!(
            diagnostics,
            vec![Diagnostic::with_key(
                ConfigLayer::Project,
                DiagnosticCause::IgnoredProjectProfileSetting,
                ULTRAFAST_MODE_KEY
            )]
        );
    }

    #[test]
    fn a_project_file_cannot_choose_how_hard_a_users_model_thinks() {
        let object = serde_json::from_str::<Value>(r#"{"effort":"max"}"#).unwrap();
        let object = object.as_object().unwrap();
        let mut diagnostics = Vec::new();
        report_ignored_profile_keys(object, &mut diagnostics);
        let settings = parse_layer(
            object,
            LayerKind::Project,
            ConfigLayer::Project,
            &mut diagnostics,
        );
        assert_eq!(settings.effort, None);
        assert_eq!(
            diagnostics,
            vec![Diagnostic::with_key(
                ConfigLayer::Project,
                DiagnosticCause::IgnoredProjectProfileSetting,
                EFFORT_KEY
            )]
        );
    }

    /// A home with `profile` written into it (the workspace path substituted
    /// for `{ROOT}`, canonical, as the loader keys the entry), a workspace with
    /// `project` as its `.xfx.json` when given, and what loading made of them.
    fn loaded_tables(profile: &str, project: Option<&str>) -> RuntimeConfig {
        let root = tempfile::tempdir().expect("a root");
        let home = root.path().join("home");
        let workspace = root.path().join("workspace");
        fs::create_dir_all(home.join(PROFILE_DIR_NAME)).expect("a profile home");
        fs::create_dir_all(&workspace).expect("a workspace");
        let canonical = workspace.canonicalize().expect("canonical");
        let profile = profile.replace("{ROOT}", canonical.to_str().expect("utf-8"));
        fs::write(
            home.join(PROFILE_DIR_NAME).join(USER_SETTINGS_FILE),
            profile,
        )
        .expect("a profile");
        if let Some(project) = project {
            fs::write(workspace.join(PROJECT_SETTINGS_FILE), project).expect("a project file");
        }
        RuntimeConfig::load_with(&Environment::new(Some(home), BTreeMap::new()), &workspace)
            .expect("load")
    }

    fn table(
        rules: &[(&str, &str, crate::permission::rules::RuleAction)],
    ) -> crate::permission::rules::RuleSet {
        crate::permission::rules::RuleSet::new(
            rules
                .iter()
                .map(|(permission, pattern, action)| {
                    crate::permission::rules::ConfiguredRule::new(*permission, *pattern, *action)
                })
                .collect(),
        )
    }

    #[test]
    fn the_user_table_is_in_force_until_this_workspace_names_one() {
        use crate::permission::rules::RuleAction::{Allow, Deny};
        let config = loaded_tables(
            r#"{"permission":{"bash":{"git *":"allow"},"edit":"deny"}}"#,
            None,
        );
        let user = table(&[("bash", "git *", Allow), ("edit", "*", Deny)]);
        assert_eq!(config.permission.user, user);
        assert!(!config.permission.user_shadowed_by_local);
        assert_eq!(config.permission.effective(), &user);
        assert!(config.diagnostics.is_empty(), "{:?}", config.diagnostics);
    }

    #[test]
    fn a_workspace_table_replaces_the_user_table_whole() {
        // `later permission layers replace earlier rules`
        // (`config_runtime.zig:3092-3117`): not a merge.
        use crate::permission::rules::RuleAction::{Allow, Deny};
        let config = loaded_tables(
            r#"{"permission":{"edit":"allow"},"workspaces":{"{ROOT}":{"permission":{"bash":"deny"}}}}"#,
            None,
        );
        assert_eq!(config.permission.user, table(&[("edit", "*", Allow)]));
        assert_eq!(config.permission.local, table(&[("bash", "*", Deny)]));
        assert!(config.permission.user_shadowed_by_local);
        assert_eq!(
            config.permission.effective(),
            &table(&[("bash", "*", Deny)])
        );
    }

    #[test]
    fn an_empty_workspace_table_still_shadows_the_user_table() {
        // `empty workspace override permission clears earlier rules`
        // (`config_runtime.zig:3119-3140`).
        let config = loaded_tables(
            r#"{"permission":{"edit":"allow"},"workspaces":{"{ROOT}":{"permission":{}}}}"#,
            None,
        );
        assert!(config.permission.user_shadowed_by_local);
        assert!(config.permission.effective().is_empty());
        // Another workspace's entry is not this one's.
        let other = loaded_tables(
            r#"{"permission":{"edit":"allow"},"workspaces":{"/elsewhere":{"permission":{}}}}"#,
            None,
        );
        assert!(!other.permission.user_shadowed_by_local);
        assert!(!other.permission.effective().is_empty());
    }

    #[test]
    fn a_table_keeps_the_order_the_file_wrote_it_in() {
        // Last match wins, so the order is the verdict; a sorted reading would
        // turn this deny-everything table into one that allows `rm`.
        use crate::permission::rules::RuleAction::{Allow, Deny};
        let config = loaded_tables(
            r#"{"permission":{"bash":{"rm*":"allow","*":"deny"}}}"#,
            None,
        );
        assert_eq!(
            config.permission.effective(),
            &table(&[("bash", "rm*", Allow), ("bash", "*", Deny)])
        );
        assert_eq!(
            config
                .permission
                .effective()
                .decide(crate::permission::rules::BASH, None, "rm x"),
            Some(Deny)
        );
    }

    #[test]
    fn a_repository_cannot_write_a_permission_table() {
        // Trap (a) of the research: "local" is the profile's entry for this
        // workspace, never a file in it.
        let config = loaded_tables("{}", Some(r#"{"permission":{"bash":"allow"}}"#));
        assert!(config.permission.effective().is_empty());
        assert!(config
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.ignored_setting_key() == Some(PERMISSION_KEY)));
    }

    #[test]
    fn a_table_that_does_not_read_is_reported_and_contributes_nothing() {
        use crate::permission::rules::RuleAction::Allow;
        // A broken user table: no rules at all, and a diagnostic that says why.
        let broken = loaded_tables(
            r#"{"permission":{"bash":{"ls":"allow","rm":"sure"}}}"#,
            None,
        );
        assert!(broken.permission.effective().is_empty());
        let diagnostic = broken
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.setting_key.as_deref() == Some(PERMISSION_KEY))
            .expect("a permission diagnostic");
        assert_eq!(diagnostic.cause, DiagnosticCause::InvalidValue);
        assert!(diagnostic
            .detail()
            .contains("`sure` is not a permission action"));
        // A broken workspace table does not shadow a readable user table.
        let shadow = loaded_tables(
            r#"{"permission":{"read":"allow"},"workspaces":{"{ROOT}":{"permission":["x"]}}}"#,
            None,
        );
        assert!(!shadow.permission.user_shadowed_by_local);
        assert_eq!(
            shadow.permission.effective(),
            &table(&[("read", "*", Allow)])
        );
    }
}
