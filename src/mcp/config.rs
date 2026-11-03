//! Where MCP servers are configured, and what a configured server is.
//!
//! Two files say which servers exist, as upstream's two do:
//!
//! - **The profile's** `~/.xfx/mcp.json`, upstream's `~/.fx/mcp.json` in
//!   upstream's shape (`{"mcp":{"<name>":{...}}}`, the Claude-style
//!   `mcpServers` key read as an alias), written only by `/mcp add` and
//!   `/mcp remove` under `~/.xfx/mcp.lock` (`vercel-labs/fx@c1db9191
//!   src/builtins/mcp.zig:969-1180`). Trusted: the person wrote it or typed the
//!   command that did.
//! - **The workspace's** `.mcp.json`, Claude Code's file and key
//!   (`mcpServers`, `src/core/mcp/workspace_config.zig:19`,
//!   `project_config.zig:421-500`). Not trusted: a repository a person cloned
//!   can carry one, so none of its servers starts until the person says so,
//!   and what they said is kept in `~/.xfx/settings.json` under
//!   `workspaces["<root>"]` with Claude Code's own key names
//!   (`enabledMcpjsonServers`, `disabledMcpjsonServers`;
//!   `project_config.zig:74-76`, `:723-832`). **An approval is for the entry
//!   the person was shown**, not for its name: beside each approved name is
//!   the [`entry_digest`] of what was approved (`enabledMcpjsonServerDigests`),
//!   and an entry that no longer matches it -- a command, an address or a
//!   credential source changed by anyone who can write the repository's file,
//!   the model included -- asks again. Upstream keys the approval by name
//!   alone (`resolveAdmission`, `project_config.zig:825-832`); see
//!   `docs/parity.md`.
//!
//! Parsing is upstream's `parseServerEntry` (`project_config.zig:944-1037`)
//! rule for rule, with its error names, because those names are what a person
//! reads: `/mcp add` says `Failed to save MCP server config: <name>.`
//! (`mcp.zig:296-300`).

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Map, Value};

/// `startup_timeout_ms`'s default (`mcp_contract.zig:5`).
pub const DEFAULT_STARTUP_TIMEOUT_MS: u32 = 30_000;
/// `operation_timeout_ms`'s default (`mcp_contract.zig:6`).
pub const DEFAULT_OPERATION_TIMEOUT_MS: u32 = 60_000;
/// `restart_limit`'s default (`mcp_contract.zig:8`).
pub const DEFAULT_RESTART_LIMIT: u8 = 1;

/// The profile file's name, in the profile directory.
pub const PROFILE_FILE: &str = "mcp.json";
/// The lock every profile write takes (`acquireProfileMutationLock`,
/// `mcp.zig:991-1001`).
pub const PROFILE_LOCK: &str = "mcp.lock";
/// The workspace's file (`workspace_config.zig:19`).
pub const WORKSPACE_FILE: &str = ".mcp.json";

/// The keys a workspace entry keeps the person's choices under: upstream's
/// two lists, and the digest each approval is for.
pub const ENABLED_KEY: &str = "enabledMcpjsonServers";
pub const DISABLED_KEY: &str = "disabledMcpjsonServers";
pub const DIGESTS_KEY: &str = "enabledMcpjsonServerDigests";
/// Upstream's "every server, whatever it says" switch. Read by no xfx that
/// binds approvals to content, and dropped from the entry at its next write:
/// it approved entries nobody was shown, so it admits nothing.
pub const ENABLE_ALL_KEY: &str = "enableAllProjectMcpServers";

/// The fields of a `.mcp.json` entry that say when and how often it runs,
/// not what runs, where it connects or what it sends: changing one keeps the
/// approval ([`entry_digest`]).
const SCHEDULING_FIELDS: [&str; 5] = [
    "enabled",
    "required",
    "startup_timeout_ms",
    "operation_timeout_ms",
    "restart_limit",
];

/// How long a profile write waits for another writer's lock: upstream's two
/// seconds (`profile_lock_deadline_ms`, `mcp.zig:26`).
const PROFILE_LOCK_DEADLINE: Duration = Duration::from_millis(2_000);

/// The most bytes of `mcp.json` or `.mcp.json` read (`mcp.zig:986`).
const MAX_CONFIG_BYTES: u64 = 1024 * 1024;

/// Which file a server came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Profile,
    Workspace,
}

/// How a server is reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    /// A child process speaking newline-delimited JSON-RPC.
    Stdio,
    /// Streamable HTTP.
    Http,
    /// The older HTTP-with-SSE transport. Parsed, kept and listed, and not
    /// connected: see `docs/parity.md`.
    Sse,
}

impl Transport {
    /// The menu's spelling (`serverMetadata`, `mcp_menu_presentation.zig:420-431`).
    pub fn label(self) -> &'static str {
        match self {
            Self::Stdio => "stdio",
            Self::Http => "HTTP",
            Self::Sse => "SSE",
        }
    }

    /// `@tagName`, which the details screen shows.
    pub fn tag(self) -> &'static str {
        match self {
            Self::Stdio => "stdio",
            Self::Http => "http",
            Self::Sse => "sse",
        }
    }
}

/// What the person said about one workspace server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    Pending,
    Approved,
    Rejected,
}

/// One configured server, whole.
#[derive(Debug, Clone, PartialEq)]
pub struct ServerConfig {
    pub name: String,
    pub source: Source,
    pub transport: Transport,
    /// The program, for a stdio server.
    pub command: String,
    pub args: Vec<String>,
    /// A stdio server's own variables, set over the few of xfx's it is given
    /// ([`super::transport::server_environment`]).
    pub env: Vec<(String, String)>,
    /// The endpoint, for an HTTP or SSE server.
    pub url: String,
    pub headers: Vec<(String, String)>,
    /// Header names and the variables whose values they are sent with
    /// (`runtime::http_headers`); kept verbatim so a profile written back
    /// keeps them.
    pub header_env: Vec<(String, String)>,
    pub bearer_token_env: Option<String>,
    pub oauth: Option<Value>,
    pub enabled: bool,
    pub required: bool,
    pub startup_timeout_ms: u32,
    pub operation_timeout_ms: u32,
    pub restart_limit: u8,
    /// Only a workspace server has one.
    pub admission: Option<Admission>,
}

impl ServerConfig {
    /// A profile stdio server as `/mcp add NAME COMMAND [ARGS...]` makes one
    /// (`mcp.zig:1040-1073`): enabled, upstream's defaults.
    pub fn local(name: &str, command: &str, args: &[String]) -> Self {
        Self {
            name: name.to_string(),
            source: Source::Profile,
            transport: Transport::Stdio,
            command: command.to_string(),
            args: args.to_vec(),
            env: Vec::new(),
            url: String::new(),
            headers: Vec::new(),
            header_env: Vec::new(),
            bearer_token_env: None,
            oauth: None,
            enabled: true,
            required: false,
            startup_timeout_ms: DEFAULT_STARTUP_TIMEOUT_MS,
            operation_timeout_ms: DEFAULT_OPERATION_TIMEOUT_MS,
            restart_limit: DEFAULT_RESTART_LIMIT,
            admission: None,
        }
    }

    /// A profile HTTP server as `/mcp add --transport http NAME URL` makes one.
    pub fn http(name: &str, url: &str) -> Self {
        Self {
            transport: Transport::Http,
            command: String::new(),
            url: url.to_string(),
            ..Self::local(name, "", &[])
        }
    }

    pub fn startup_timeout(&self) -> Duration {
        Duration::from_millis(u64::from(self.startup_timeout_ms))
    }

    pub fn operation_timeout(&self) -> Duration {
        Duration::from_millis(u64::from(self.operation_timeout_ms))
    }
}

/// Why a configuration could not be read, by upstream's error name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfigError(pub &'static str);

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for ConfigError {}

/// Server names are `[A-Za-z0-9_-]+` (`isValidServerName`,
/// `command_provider.zig:93-100`).
pub fn is_valid_server_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

/// Upstream's endpoint rule (`validateEndpoint`, `streamable_http.zig:76-97`):
/// `https` anywhere, plain `http` only to a loopback host with an explicit
/// port, and never with credentials or a fragment in the URL.
pub fn validate_endpoint(url: &str) -> Result<(), ConfigError> {
    let invalid = ConfigError("McpConfigInvalidUrl");
    let (scheme, rest) = url.split_once("://").ok_or(invalid)?;
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    if authority.is_empty() || authority.contains('@') || rest.contains('#') {
        return Err(invalid);
    }
    if scheme.eq_ignore_ascii_case("https") {
        return Ok(());
    }
    if !scheme.eq_ignore_ascii_case("http") {
        return Err(invalid);
    }
    let (host, port) = if let Some(stripped) = authority.strip_prefix('[') {
        let close = stripped.find(']').ok_or(invalid)?;
        let host = &authority[..close + 2];
        (host, authority[close + 2..].strip_prefix(':'))
    } else {
        match authority.rsplit_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        }
    };
    let Some(port) = port.filter(|port| !port.is_empty() && port.parse::<u16>().is_ok()) else {
        return Err(invalid);
    };
    let _ = port;
    if host == "127.0.0.1" || host.eq_ignore_ascii_case("localhost") || host == "[::1]" {
        Ok(())
    } else {
        Err(invalid)
    }
}

// ---------------------------------------------------------------------------
// one entry
// ---------------------------------------------------------------------------

fn parse_entry(
    name: &str,
    value: &Value,
    source: Source,
    admission: Option<Admission>,
) -> Result<ServerConfig, ConfigError> {
    let object = value
        .as_object()
        .ok_or(ConfigError("McpConfigServerMustBeObject"))?;
    let kind = match object.get("type") {
        None => "local",
        Some(Value::String(kind)) => kind.as_str(),
        Some(_) => return Err(ConfigError("McpConfigInvalidType")),
    };
    let transport = match kind {
        "sse" => Transport::Sse,
        "http" => Transport::Http,
        "local" | "stdio" => Transport::Stdio,
        _ => return Err(ConfigError("McpConfigInvalidType")),
    };
    let enabled = match object.get("enabled") {
        None => true,
        Some(Value::Bool(enabled)) => *enabled,
        Some(_) => return Err(ConfigError("McpConfigInvalidEnabled")),
    };
    // A workspace server is never required: one a repository declared
    // cannot hold the person's session hostage (`force_optional`).
    let required = match object.get("required") {
        None => false,
        Some(Value::Bool(required)) => *required && source == Source::Profile,
        Some(_) => return Err(ConfigError("McpConfigInvalidRequired")),
    };
    let startup_timeout_ms = policy(
        object,
        "startup_timeout_ms",
        DEFAULT_STARTUP_TIMEOUT_MS.into(),
        1,
        u32::MAX.into(),
    )
    .map_err(|()| ConfigError("McpConfigInvalidStartupTimeout"))?
        as u32;
    let operation_timeout_ms = policy(
        object,
        "operation_timeout_ms",
        DEFAULT_OPERATION_TIMEOUT_MS.into(),
        1,
        u32::MAX.into(),
    )
    .map_err(|()| ConfigError("McpConfigInvalidOperationTimeout"))?
        as u32;
    let env = environment(object)?;
    let mut config = ServerConfig {
        name: name.to_string(),
        source,
        transport,
        command: String::new(),
        args: Vec::new(),
        env,
        url: String::new(),
        headers: Vec::new(),
        header_env: Vec::new(),
        bearer_token_env: None,
        oauth: None,
        enabled,
        required,
        startup_timeout_ms,
        operation_timeout_ms,
        restart_limit: DEFAULT_RESTART_LIMIT,
        admission,
    };
    if transport == Transport::Stdio {
        config.restart_limit = policy(
            object,
            "restart_limit",
            DEFAULT_RESTART_LIMIT.into(),
            0,
            u8::MAX.into(),
        )
        .map_err(|()| ConfigError("McpConfigInvalidRestartLimit"))?
            as u8;
        let (command, args) = command_spec(object).ok_or(ConfigError("McpConfigInvalidCommand"))?;
        config.command = command;
        config.args = args;
        return Ok(config);
    }
    config.url = match object.get("url") {
        None => return Err(ConfigError("McpConfigMissingUrl")),
        Some(Value::String(url)) => url.clone(),
        Some(_) => return Err(ConfigError("McpConfigInvalidUrl")),
    };
    validate_endpoint(&config.url)?;
    config.headers = string_map(object.get("headers"), "McpConfigInvalidHeaders")?;
    // A repository may not plant a credential header: only the profile's
    // own entries may carry `Authorization` (`allow_authorization_header`).
    if source == Source::Workspace
        && config
            .headers
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case("authorization"))
    {
        return Err(ConfigError("McpConfigInvalidHeaders"));
    }
    if !valid_static_headers(&config.headers) {
        return Err(ConfigError("McpConfigInvalidHeaders"));
    }
    // A header whose value is a variable's (`parseRemoteHeaderEnv`,
    // `project_config.zig:1110-1140`): the variable a plain name, never
    // `Authorization` (that is `bearer_token_env`'s), and the names held to
    // the same rules as static ones.
    config.header_env = string_map(object.get("header_env"), "McpConfigInvalidHeaders")?;
    let named: Vec<(String, String)> = config
        .header_env
        .iter()
        .map(|(name, _)| (name.clone(), "value".to_string()))
        .collect();
    if config.header_env.iter().any(|(name, variable)| {
        !is_env_name(variable) || name.eq_ignore_ascii_case("authorization")
    }) || !valid_static_headers(&named)
    {
        return Err(ConfigError("McpConfigInvalidHeaders"));
    }
    config.bearer_token_env = match object.get("bearer_token_env") {
        None => None,
        Some(Value::String(name)) if is_env_name(name) => Some(name.clone()),
        Some(_) => return Err(ConfigError("McpConfigInvalidBearerEnvironment")),
    };
    config.oauth = match object.get("oauth") {
        None => None,
        Some(value @ Value::Object(_)) => Some(value.clone()),
        Some(_) => return Err(ConfigError("McpConfigInvalidOAuth")),
    };
    Ok(config)
}

/// An unsigned integer field, its default when absent (`parseUnsignedPolicy`,
/// `project_config.zig:1182-1198`).
fn policy(
    object: &Map<String, Value>,
    key: &str,
    default: u64,
    min: u64,
    max: u64,
) -> Result<u64, ()> {
    match object.get(key) {
        None => Ok(default),
        Some(value) => {
            let parsed = value.as_u64().ok_or(())?;
            if parsed < min || parsed > max {
                Err(())
            } else {
                Ok(parsed)
            }
        }
    }
}

/// `command` as a string with `args` beside it, or as one array of both
/// (`parseCommandSpec`, `project_config.zig:1209-1243`).
fn command_spec(object: &Map<String, Value>) -> Option<(String, Vec<String>)> {
    match object.get("command")? {
        Value::String(command) => {
            let args = match object.get("args") {
                None => Vec::new(),
                Some(value) => string_array(value)?,
            };
            Some((command.clone(), args))
        }
        Value::Array(items) => {
            let mut words = items.iter().map(|item| item.as_str().map(str::to_string));
            let command = words.next()??;
            let args = words.collect::<Option<Vec<_>>>()?;
            Some((command, args))
        }
        _ => None,
    }
}

fn string_array(value: &Value) -> Option<Vec<String>> {
    value
        .as_array()?
        .iter()
        .map(|item| item.as_str().map(str::to_string))
        .collect()
}

/// `environment`, or failing that `env` (`parseSelectedEnvironment`,
/// `project_config.zig:1200-1206`).
fn environment(object: &Map<String, Value>) -> Result<Vec<(String, String)>, ConfigError> {
    let value = object.get("environment").or_else(|| object.get("env"));
    let pairs = string_map(value, "McpConfigInvalidEnvironment")?;
    if pairs.iter().any(|(key, _)| !is_env_name(key)) {
        return Err(ConfigError("McpConfigInvalidEnvironment"));
    }
    Ok(pairs)
}

fn string_map(
    value: Option<&Value>,
    error: &'static str,
) -> Result<Vec<(String, String)>, ConfigError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let object = value.as_object().ok_or(ConfigError(error))?;
    object
        .iter()
        .map(|(key, value)| match value {
            Value::String(value) => Ok((key.clone(), value.clone())),
            _ => Err(ConfigError(error)),
        })
        .collect()
}

/// xfx's own credentials, the two variables the gateway key is read from
/// ([`crate::config::CREDENTIAL_VARIABLES`]). A workspace server never
/// resolves them: an approval is one answer to one prompt and an HTTP server
/// runs none of the repository's code, so an approved entry naming one would
/// send the key to whatever host it names. Upstream resolves them like any
/// other variable (`docs/parity.md`).
fn is_credential_variable(name: &str) -> bool {
    crate::config::CREDENTIAL_VARIABLES.contains(&name)
}

fn is_env_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    matches!(bytes.next(), Some(first) if first.is_ascii_alphabetic() || first == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn is_token_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte)
}

/// The headers the protocol sets itself, which a configuration may not
/// (`isReservedHeader`, `streamable_http.zig:1234-1252`).
const RESERVED_HEADERS: [&str; 12] = [
    "accept",
    "accept-encoding",
    "connection",
    "content-length",
    "content-type",
    "host",
    "last-event-id",
    "mcp-method",
    "mcp-name",
    "mcp-protocol-version",
    "mcp-session-id",
    "transfer-encoding",
];

/// `validateStaticHeaders` (`streamable_http.zig:99-111`): each name an HTTP
/// token and not one the protocol sets, each value free of control bytes
/// but tab, and no name twice in any case.
fn valid_static_headers(headers: &[(String, String)]) -> bool {
    headers.iter().enumerate().all(|(index, (name, value))| {
        let lowered = name.to_ascii_lowercase();
        !name.is_empty()
            && name.bytes().all(is_token_byte)
            && value
                .bytes()
                .all(|byte| (byte >= 0x20 || byte == b'\t') && byte != 0x7f)
            && !RESERVED_HEADERS.contains(&lowered.as_str())
            && !lowered.starts_with("mcp-param-")
            && !headers[..index]
                .iter()
                .any(|(previous, _)| previous.eq_ignore_ascii_case(name))
    })
}

// ---------------------------------------------------------------------------
// the profile file
// ---------------------------------------------------------------------------

/// What the profile file holds.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Profile {
    pub servers: Vec<ServerConfig>,
}

/// `mcp.json` in `profile_dir`.
pub fn profile_path(profile_dir: &Path) -> PathBuf {
    profile_dir.join(PROFILE_FILE)
}

/// Reads the profile's servers. A file that is not there is an empty profile.
pub fn load_profile(path: &Path) -> Result<Profile, ConfigError> {
    let Some(text) = read_bounded(path).map_err(|_| ConfigError("McpConfigUnreadable"))? else {
        return Ok(Profile::default());
    };
    parse_profile(&text)
}

/// `parseProfileDocument` (`project_config.zig:241-288`): the `mcp` map, or
/// the `mcpServers` alias when there is no `mcp`.
pub fn parse_profile(text: &str) -> Result<Profile, ConfigError> {
    let document: Value =
        serde_json::from_str(text).map_err(|_| ConfigError("McpConfigInvalidJson"))?;
    let object = document
        .as_object()
        .ok_or(ConfigError("McpConfigRootMustBeObject"))?;
    let Some(servers) = object.get("mcp").or_else(|| object.get("mcpServers")) else {
        return Ok(Profile::default());
    };
    let servers = servers
        .as_object()
        .ok_or(ConfigError("McpConfigServersMustBeObject"))?;
    let servers = servers
        .iter()
        .map(|(name, value)| parse_entry(name, value, Source::Profile, None))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Profile { servers })
}

/// The profile file's bytes, in upstream's shape and key order
/// (`renderConfigJson`, `mcp.zig:1075-1180`).
pub fn render_profile(servers: &[ServerConfig]) -> String {
    let quote = |text: &str| Value::from(text).to_string();
    let mut out = String::from("{\"mcp\":{");
    for (index, config) in servers.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&quote(&config.name));
        out.push_str(":{");
        match config.transport {
            Transport::Stdio => {
                out.push_str("\"type\":\"local\",\"command\":[");
                out.push_str(&quote(&config.command));
                for arg in &config.args {
                    out.push(',');
                    out.push_str(&quote(arg));
                }
                out.push(']');
            }
            Transport::Sse | Transport::Http => {
                out.push_str("\"type\":\"");
                out.push_str(config.transport.tag());
                out.push_str("\",\"url\":");
                out.push_str(&quote(&config.url));
            }
        }
        out.push_str(&format!(",\"enabled\":{}", config.enabled));
        if config.required {
            out.push_str(",\"required\":true");
        }
        out.push_str(&format!(
            ",\"startup_timeout_ms\":{},\"operation_timeout_ms\":{}",
            config.startup_timeout_ms, config.operation_timeout_ms
        ));
        if config.transport == Transport::Stdio {
            out.push_str(&format!(",\"restart_limit\":{}", config.restart_limit));
        }
        for (key, pairs) in [
            ("headers", &config.headers),
            ("header_env", &config.header_env),
        ] {
            if pairs.is_empty() {
                continue;
            }
            out.push_str(&format!(",\"{key}\":{{"));
            for (index, (name, value)) in pairs.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&quote(name));
                out.push(':');
                out.push_str(&quote(value));
            }
            out.push('}');
        }
        if let Some(name) = &config.bearer_token_env {
            out.push_str(",\"bearer_token_env\":");
            out.push_str(&quote(name));
        }
        if let Some(oauth) = &config.oauth {
            out.push_str(",\"oauth\":");
            out.push_str(&oauth.to_string());
        }
        if !config.env.is_empty() {
            out.push_str(",\"environment\":{");
            for (index, (key, value)) in config.env.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&quote(key));
                out.push(':');
                out.push_str(&quote(value));
            }
            out.push('}');
        }
        out.push('}');
    }
    out.push_str("}}");
    out
}

/// Adds `config` to the profile at `path`, in place of a server of the same
/// name or after the others, under the profile lock.
pub fn add_profile_server(path: &Path, config: ServerConfig) -> Result<(), ConfigError> {
    edit_profile(path, |servers| {
        match servers
            .iter_mut()
            .find(|existing| existing.name == config.name)
        {
            Some(existing) => *existing = config,
            None => servers.push(config),
        }
        true
    })
    .map(|_| ())
}

/// Removes the profile server called `name`. `Ok(false)` when there is none.
pub fn remove_profile_server(path: &Path, name: &str) -> Result<bool, ConfigError> {
    edit_profile(path, |servers| {
        let before = servers.len();
        servers.retain(|server| server.name != name);
        servers.len() != before
    })
}

/// One read-modify-write of the profile under `mcp.lock`; nothing is written
/// when `edit` says it changed nothing.
fn edit_profile(
    path: &Path,
    edit: impl FnOnce(&mut Vec<ServerConfig>) -> bool,
) -> Result<bool, ConfigError> {
    let dir = path.parent().ok_or(ConfigError("McpConfigPathInvalid"))?;
    create_private_dir(dir).map_err(|_| ConfigError("McpConfigPathInvalid"))?;
    let _lock = ProfileLock::acquire(&dir.join(PROFILE_LOCK))?;
    let mut profile = load_profile(path)?;
    if !edit(&mut profile.servers) {
        return Ok(false);
    }
    let bytes = render_profile(&profile.servers);
    crate::provider::profile::replace_private_file(dir, path, bytes.as_bytes())
        .map_err(|_| ConfigError("McpConfigWriteFailed"))?;
    Ok(true)
}

fn create_private_dir(dir: &Path) -> io::Result<()> {
    if dir.is_dir() {
        return Ok(());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
    }
    #[cfg(not(unix))]
    {
        fs::create_dir_all(dir)
    }
}

/// `mcp.lock`, held for one write: an exclusive `flock`, retried until
/// upstream's deadline, released when the file closes.
struct ProfileLock {
    #[cfg(unix)]
    _file: fs::File,
}

impl ProfileLock {
    #[cfg(unix)]
    fn acquire(path: &Path) -> Result<Self, ConfigError> {
        use rustix::fs::{flock, FlockOperation};
        use std::os::unix::fs::OpenOptionsExt;
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
            .map_err(|_| ConfigError("McpConfigLockFailed"))?;
        let started = std::time::Instant::now();
        loop {
            match flock(&file, FlockOperation::NonBlockingLockExclusive) {
                Ok(()) => return Ok(Self { _file: file }),
                Err(rustix::io::Errno::WOULDBLOCK) if started.elapsed() < PROFILE_LOCK_DEADLINE => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(_) => return Err(ConfigError("McpConfigLockTimeout")),
            }
        }
    }

    #[cfg(not(unix))]
    fn acquire(_path: &Path) -> Result<Self, ConfigError> {
        Ok(Self {})
    }
}

/// A regular file's text, or `None` when there is no file.
fn read_bounded(path: &Path) -> io::Result<Option<String>> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err),
    };
    if !metadata.is_file() || metadata.len() > MAX_CONFIG_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "not a readable config",
        ));
    }
    fs::read_to_string(path).map(Some)
}

// ---------------------------------------------------------------------------
// the workspace file and the person's choices about it
// ---------------------------------------------------------------------------

/// What the person said about the workspace's servers.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Choices {
    /// Each approved server's name, with the [`entry_digest`] of the entry
    /// that was approved.
    pub approved: Vec<(String, String)>,
    pub rejected: Vec<String>,
}

impl Choices {
    /// `resolveAdmission` (`project_config.zig:825-832`) -- a rejection wins
    /// -- except that an approval admits only the entry it was given for: an
    /// entry whose `digest` is not the approved one is pending again.
    pub fn admission(&self, name: &str, digest: &str) -> Admission {
        if self.rejected.iter().any(|rejected| rejected == name) {
            Admission::Rejected
        } else if self
            .approved
            .iter()
            .any(|(approved, approved_digest)| approved == name && approved_digest == digest)
        {
            Admission::Approved
        } else {
            Admission::Pending
        }
    }
}

/// One thing the person can decide about workspace servers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrustAction {
    /// Approve the entry called this as the runtime last read it: the typed
    /// `/mcp trust approve <name>`, the one way to approve an entry that
    /// parses only once expanded (`"url": "${URL}"`). The trust prompt and
    /// the menu's details screen send [`Self::ApproveShown`] instead.
    Approve(String),
    /// Approve the entry the trust prompt or the menu's details screen
    /// showed, whose [`entry_digest`] this is: if the file changed after it
    /// was drawn, the approval is for an entry no longer there, and the new
    /// one asks.
    ApproveShown {
        name: String,
        digest: String,
    },
    Reject(String),
    /// Approve every entry the runtime last read as a pending server -- the
    /// servers there now, not ones added later, as fx v0.0.16 does
    /// (`CHANGELOG.md:165@481104fd`, `src/core/mcp_host/verbs.zig:641-655@481104fd`).
    ApproveAll,
    Reset,
}

/// The workspace's servers and the entries it could not read.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WorkspaceServers {
    pub servers: Vec<ServerConfig>,
    /// One sentence per entry that did not parse, as upstream's
    /// configuration issues say it (`renderWorkspaceDiagnostic`).
    pub issues: Vec<String>,
    /// Every entry with a valid name and its [`entry_digest`], parsed or
    /// not: what an approval of that name would be for.
    pub digests: Vec<(String, String)>,
}

/// What an approval of a `.mcp.json` entry is for: SHA-256 over the entry as
/// written -- before `${VAR}` expansion, which only an approved entry gets --
/// in a canonical form (object keys sorted, `Value`'s own number and string
/// spelling), every field but [`SCHEDULING_FIELDS`].
///
/// So what runs (`command`, `args`, `env`/`environment`), where it connects
/// and what it sends (`type`, `url`, `headers`, `header_env`,
/// `bearer_token_env`, `oauth`), and any field this version does not know are
/// all bound: a change to one is an entry the person has not approved. There
/// is no `cwd` field to bind: a stdio server starts in xfx's own working
/// directory, which the entry cannot name.
pub fn entry_digest(value: &Value) -> String {
    use sha2::{Digest, Sha256};
    let mut text = String::new();
    match value {
        Value::Object(object) => {
            let kept: Map<String, Value> = object
                .iter()
                .filter(|(key, _)| !SCHEDULING_FIELDS.contains(&key.as_str()))
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect();
            canonical(&Value::Object(kept), &mut text);
        }
        other => canonical(other, &mut text),
    }
    let digest = Sha256::digest(text.as_bytes());
    let mut out = String::from("sha256:");
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// `value` as JSON with every object's keys in sorted order.
fn canonical(value: &Value, out: &mut String) {
    match value {
        Value::Object(object) => {
            let mut keys: Vec<&String> = object.keys().collect();
            keys.sort();
            out.push('{');
            for (index, key) in keys.into_iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&Value::from(key.as_str()).to_string());
                out.push(':');
                canonical(&object[key], out);
            }
            out.push('}');
        }
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                canonical(item, out);
            }
            out.push(']');
        }
        other => out.push_str(&other.to_string()),
    }
}

/// Reads `<root>/.mcp.json` under `choices`, each entry admitted by its name
/// and its [`entry_digest`], taken over the entry as written, before any
/// expansion. An approved server's `${VAR}` and `${VAR:-default}` are then
/// expanded from `environment`, in which xfx's own credentials are never set
/// (`is_credential_variable`), and one left without a value does not start; a
/// pending or rejected one is kept as written, since it will not run
/// (`expandApprovedWorkspaceConfigs`, `project_config.zig:501-575`).
pub fn load_workspace(
    root: &Path,
    choices: &Choices,
    environment: &dyn Fn(&str) -> Option<String>,
) -> WorkspaceServers {
    let path = root.join(WORKSPACE_FILE);
    let text = match read_bounded(&path) {
        Ok(Some(text)) => text,
        Ok(None) => return WorkspaceServers::default(),
        Err(_) => {
            return WorkspaceServers {
                issues: vec![".mcp.json: McpConfigUnreadable".to_string()],
                ..WorkspaceServers::default()
            }
        }
    };
    let document: Value = match serde_json::from_str(&text) {
        Ok(document) => document,
        Err(_) => {
            return WorkspaceServers {
                issues: vec![".mcp.json: McpConfigInvalidJson".to_string()],
                ..WorkspaceServers::default()
            }
        }
    };
    let Some(servers) = document.get("mcpServers").and_then(Value::as_object) else {
        return WorkspaceServers::default();
    };
    let lookup = |variable: &str| {
        if is_credential_variable(variable) {
            None
        } else {
            environment(variable)
        }
    };
    let mut result = WorkspaceServers::default();
    for (name, value) in servers {
        if !is_valid_server_name(name) {
            result.issues.push(format!("{name}: McpInvalidServerName"));
            continue;
        }
        let digest = entry_digest(value);
        let admission = choices.admission(name, &digest);
        result.digests.push((name.clone(), digest));
        let value = if admission == Admission::Approved {
            match expand_entry(value, &lookup) {
                Ok(value) => value,
                Err(unresolved) => {
                    result.issues.push(unresolved.issue(name));
                    continue;
                }
            }
        } else {
            value.clone()
        };
        match parse_entry(name, &value, Source::Workspace, Some(admission)) {
            Ok(config) => match withheld_source(&config) {
                Some((field, variable)) => result.issues.push(withheld(name, field, variable)),
                None => result.servers.push(config),
            },
            Err(err) => result.issues.push(format!("{name}: {err}")),
        }
    }
    result
}

/// A workspace server's `header_env` or `bearer_token_env` that names one of
/// xfx's credentials, by its field. Checked after expansion, so a `${VAR}`
/// that spells one is caught too.
fn withheld_source(config: &ServerConfig) -> Option<(&'static str, &str)> {
    config
        .header_env
        .iter()
        .map(|(_, variable)| ("header_env", variable.as_str()))
        .chain(
            config
                .bearer_token_env
                .as_deref()
                .map(|variable| ("bearer_token_env", variable)),
        )
        .find(|(_, variable)| is_credential_variable(variable))
}

/// Why a workspace server that names one of xfx's credentials does not
/// start: deliberate, and what to do instead. xfx's own sentence, in the
/// shape of upstream's below.
fn withheld(server: &str, field: &str, variable: &str) -> String {
    format!(
        ".mcp.json server '{server}' field {field} uses '{variable}', which xfx withholds \
         from project servers; a server in ~/.xfx/mcp.json may use it."
    )
}

/// The most of a variable's name a sentence carries
/// (`max_rendered_environment_name_bytes`, `project_config.zig:549`).
const MAX_RENDERED_VARIABLE: usize = 128;

/// A `${VAR}` an approved workspace server has no value for: unset with no
/// default, or one of xfx's credentials, which it never gets. The server is
/// dropped, as upstream drops it (`expandApprovedWorkspaceConfigs`,
/// `project_config.zig:526-545`).
struct Unresolved {
    /// Upstream's name for the field (`WorkspaceEnvironmentField`, `:126-131`).
    field: &'static str,
    variable: String,
}

impl Unresolved {
    /// What the menu says (`renderWorkspaceDiagnostic`, `:156-176`).
    fn issue(&self, server: &str) -> String {
        let variable: String = crate::mcp::terminal_safe(&self.variable)
            .chars()
            .take(MAX_RENDERED_VARIABLE)
            .collect();
        if is_credential_variable(&self.variable) {
            return withheld(server, self.field, &variable);
        }
        format!(
            ".mcp.json server '{server}' field {} requires environment variable \
             '{variable}'; set it or use ${{{variable}:-default}}.",
            self.field
        )
    }
}

/// The field a top-level key's `${VAR}` is reported in: upstream's four
/// (`WorkspaceEnvironmentField`), `url` (which xfx expands and upstream does
/// not), and upstream's `value` for the rest.
fn expansion_field(key: &str) -> &'static str {
    match key {
        "command" => "command",
        "args" => "argument",
        "env" | "environment" => "environment",
        "headers" => "http_header",
        "url" => "url",
        _ => "value",
    }
}

/// `expand` over one entry, each field under its own name; `command` as one
/// array is the program and then its arguments (`command_spec`).
fn expand_entry(
    value: &Value,
    environment: &dyn Fn(&str) -> Option<String>,
) -> Result<Value, Unresolved> {
    let Value::Object(object) = value else {
        return Ok(value.clone());
    };
    object
        .iter()
        .map(|(key, item)| {
            let expanded = match (key.as_str(), item) {
                ("command", Value::Array(words)) => Value::Array(
                    words
                        .iter()
                        .enumerate()
                        .map(|(index, word)| {
                            let field = if index == 0 { "command" } else { "argument" };
                            expand(word, field, environment)
                        })
                        .collect::<Result<_, _>>()?,
                ),
                _ => expand(item, expansion_field(key), environment)?,
            };
            Ok((key.clone(), expanded))
        })
        .collect::<Result<Map<_, _>, _>>()
        .map(Value::Object)
}

/// `${VAR}` and `${VAR:-default}` in every string of `value`, all of it in
/// `field`.
fn expand(
    value: &Value,
    field: &'static str,
    environment: &dyn Fn(&str) -> Option<String>,
) -> Result<Value, Unresolved> {
    Ok(match value {
        Value::String(text) => Value::String(expand_text(text, field, environment)?),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| expand(item, field, environment))
                .collect::<Result<_, _>>()?,
        ),
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| Ok((key.clone(), expand(value, field, environment)?)))
                .collect::<Result<_, _>>()?,
        ),
        other => other.clone(),
    })
}

fn expand_text(
    text: &str,
    field: &'static str,
    environment: &dyn Fn(&str) -> Option<String>,
) -> Result<String, Unresolved> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find('}') else {
            out.push_str(&rest[start..]);
            return Ok(out);
        };
        let inner = &after[..end];
        let (name, default) = match inner.split_once(":-") {
            Some((name, default)) => (name, Some(default)),
            None => (inner, None),
        };
        match (
            environment(name).filter(|value| !value.is_empty() || default.is_none()),
            default,
        ) {
            (Some(value), _) => out.push_str(&value),
            (None, Some(default)) => out.push_str(default),
            (None, None) => {
                return Err(Unresolved {
                    field,
                    variable: name.to_string(),
                })
            }
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

/// The person's choices for `root` in a settings document
/// (`parseChoices`, `project_config.zig:723-773`): a name both approved and
/// rejected counts as rejected, and an approved name with no digest beside it
/// -- an approval an older xfx saved by name alone -- is no approval: that
/// server asks once more, and the answer is saved with its digest.
pub fn choices_in(document: &Map<String, Value>, root: &Path) -> Choices {
    let entry = root.to_str().and_then(|key| {
        document
            .get(crate::workspace::access::WORKSPACES_KEY)
            .and_then(Value::as_object)
            .and_then(|workspaces| workspaces.get(key))
            .and_then(Value::as_object)
    });
    let Some(entry) = entry else {
        return Choices::default();
    };
    let names = |key: &str| -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        for name in entry
            .get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            if !names.iter().any(|seen| seen == name) {
                names.push(name.to_string());
            }
        }
        names
    };
    let rejected = names(DISABLED_KEY);
    let digests = entry.get(DIGESTS_KEY).and_then(Value::as_object);
    let approved = names(ENABLED_KEY)
        .into_iter()
        .filter(|name| !rejected.contains(name))
        .filter_map(|name| {
            let digest = digests?.get(&name)?.as_str()?.to_string();
            Some((name, digest))
        })
        .collect();
    Choices { approved, rejected }
}

/// The choices in the settings file at `settings`, or none when it cannot
/// be read.
pub fn load_choices(settings: Option<&Path>, root: &Path) -> Choices {
    settings
        .and_then(|path| crate::provider::profile::read_existing(path).ok())
        .map(|document| choices_in(&document, root))
        .unwrap_or_default()
}

/// What the runtime last read, for an approval by name to bind: each entry
/// with its [`entry_digest`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Loaded {
    /// Every entry with a valid name, parsed or not
    /// ([`WorkspaceServers::digests`]): what [`TrustAction::Approve`] binds.
    pub entries: Vec<(String, String)>,
    /// The entries that parsed as pending servers: what
    /// [`TrustAction::ApproveAll`] approves. One that only parses once
    /// expanded (`"type": "${T:-stdio}"`) was shown as a configuration error,
    /// not as a server, so approving all does not approve it.
    pub pending: Vec<(String, String)>,
}

/// `applyAction` (`project_config.zig:775-823`), and whether it reduced the
/// authority in force, with `loaded` what an approval by name binds. A name
/// with no entry there has nothing to approve and is only taken off the
/// rejected list.
pub fn apply_action(current: &Choices, action: &TrustAction, loaded: &Loaded) -> (Choices, bool) {
    let mut next = current.clone();
    let approve = |next: &mut Choices, name: &str, digest: Option<&str>| {
        if let Some(digest) = digest {
            match next
                .approved
                .iter_mut()
                .find(|(approved, _)| approved == name)
            {
                Some(approval) => approval.1 = digest.to_string(),
                None => next.approved.push((name.to_string(), digest.to_string())),
            }
        }
        next.rejected.retain(|rejected| rejected != name);
    };
    let loaded_digest = |name: &str| {
        loaded
            .entries
            .iter()
            .find(|(entry, _)| entry == name)
            .map(|(_, digest)| digest.as_str())
    };
    let reduced = match action {
        TrustAction::Approve(name) => {
            approve(&mut next, name, loaded_digest(name));
            false
        }
        TrustAction::ApproveShown { name, digest } => {
            approve(&mut next, name, Some(digest));
            false
        }
        TrustAction::Reject(name) => {
            let reduced = current
                .approved
                .iter()
                .any(|(approved, _)| approved == name);
            next.approved.retain(|(approved, _)| approved != name);
            if !next.rejected.contains(name) {
                next.rejected.push(name.clone());
            }
            reduced
        }
        TrustAction::ApproveAll => {
            for (name, digest) in &loaded.pending {
                if !current.rejected.contains(name) {
                    approve(&mut next, name, Some(digest));
                }
            }
            false
        }
        TrustAction::Reset => {
            let reduced = !current.approved.is_empty();
            next = Choices::default();
            reduced
        }
    };
    (next, reduced)
}

/// Writes `action` into the settings file at `settings` for `root`, through
/// the settings writer every other profile change uses
/// ([`crate::provider::profile::edit_document`]), `loaded` as
/// [`apply_action`] takes it. An entry left with nothing leaves the file, and
/// so does an empty `workspaces`.
pub fn save_action(
    settings: &Path,
    root: &Path,
    action: &TrustAction,
    loaded: &Loaded,
) -> io::Result<bool> {
    let key = root
        .to_str()
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "the workspace path is not UTF-8",
            )
        })?
        .to_string();
    let workspaces_key = crate::workspace::access::WORKSPACES_KEY;
    crate::provider::profile::edit_document::<io::Error>(settings, |document| {
        let current = choices_in(document, root);
        let (next, _) = apply_action(&current, action, loaded);
        if next == current && !matches!(action, TrustAction::Reset) {
            return Ok(false);
        }
        let workspaces = document
            .entry(workspaces_key.to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        let Some(workspaces) = workspaces.as_object_mut() else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "the settings file's `workspaces` is not an object",
            ));
        };
        let entry = workspaces
            .entry(key.clone())
            .or_insert_with(|| Value::Object(Map::new()));
        let Some(entry) = entry.as_object_mut() else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "the settings file's workspace entry is not an object",
            ));
        };
        let before = entry.clone();
        let approved: Vec<String> = next.approved.iter().map(|(name, _)| name.clone()).collect();
        for (field, names) in [(ENABLED_KEY, &approved), (DISABLED_KEY, &next.rejected)] {
            if names.is_empty() {
                entry.shift_remove(field);
            } else {
                entry.insert(
                    field.to_string(),
                    Value::Array(names.iter().cloned().map(Value::String).collect()),
                );
            }
        }
        if next.approved.is_empty() {
            entry.shift_remove(DIGESTS_KEY);
        } else {
            entry.insert(
                DIGESTS_KEY.to_string(),
                Value::Object(
                    next.approved
                        .iter()
                        .map(|(name, digest)| (name.clone(), Value::String(digest.clone())))
                        .collect(),
                ),
            );
        }
        entry.shift_remove(ENABLE_ALL_KEY);
        let changed = *entry != before;
        if entry.is_empty() {
            workspaces.shift_remove(&key);
        }
        if workspaces.is_empty() {
            document.shift_remove(workspaces_key);
        }
        Ok(changed || before.is_empty())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `entries` read, each of them a pending server.
    fn loaded_as_pending(entries: &[(&str, &str)]) -> Loaded {
        let entries: Vec<(String, String)> = entries
            .iter()
            .map(|(name, digest)| (name.to_string(), digest.to_string()))
            .collect();
        Loaded {
            pending: entries.clone(),
            entries,
        }
    }
    use serde_json::json;

    #[test]
    fn a_profile_written_by_add_is_upstreams_bytes() {
        // `cap/fx-mcp-fixture-40x120-after-add.files`.
        let config = ServerConfig::local(
            "fixture",
            "/opt/homebrew/bin/bun",
            &["/abs/mcp-legacy-stdio.mjs".to_string()],
        );
        assert_eq!(
            render_profile(&[config]),
            r#"{"mcp":{"fixture":{"type":"local","command":["/opt/homebrew/bin/bun","/abs/mcp-legacy-stdio.mjs"],"enabled":true,"startup_timeout_ms":30000,"operation_timeout_ms":60000,"restart_limit":1}}}"#
        );
        assert_eq!(render_profile(&[]), r#"{"mcp":{}}"#);
        let http = ServerConfig::http("remote", "http://127.0.0.1:9/mcp");
        assert_eq!(
            render_profile(&[http]),
            r#"{"mcp":{"remote":{"type":"http","url":"http://127.0.0.1:9/mcp","enabled":true,"startup_timeout_ms":30000,"operation_timeout_ms":60000}}}"#
        );
    }

    #[test]
    fn a_profile_round_trips_through_its_own_parser() {
        let text = r#"{"mcp":{"a":{"type":"local","command":["node","s.js"],"enabled":false,"startup_timeout_ms":5,"operation_timeout_ms":7,"restart_limit":0,"environment":{"K":"v"}},"b":{"type":"http","url":"https://example.test/mcp","enabled":true,"startup_timeout_ms":30000,"operation_timeout_ms":60000,"headers":{"X-A":"1"}}}}"#;
        let profile = parse_profile(text).expect("parses");
        assert_eq!(render_profile(&profile.servers), text);
        assert!(!profile.servers[0].enabled);
        assert_eq!(profile.servers[0].restart_limit, 0);
    }

    #[test]
    fn every_bad_field_is_named_as_upstream_names_it() {
        for (text, expected) in [
            ("[]", "McpConfigRootMustBeObject"),
            ("{", "McpConfigInvalidJson"),
            (r#"{"mcp":[]}"#, "McpConfigServersMustBeObject"),
            (r#"{"mcp":{"a":1}}"#, "McpConfigServerMustBeObject"),
            (
                r#"{"mcp":{"a":{"type":"ws","command":"x"}}}"#,
                "McpConfigInvalidType",
            ),
            (
                r#"{"mcp":{"a":{"command":"x","enabled":"yes"}}}"#,
                "McpConfigInvalidEnabled",
            ),
            (
                r#"{"mcp":{"a":{"command":"x","startup_timeout_ms":0}}}"#,
                "McpConfigInvalidStartupTimeout",
            ),
            (
                r#"{"mcp":{"a":{"command":"x","operation_timeout_ms":"slow"}}}"#,
                "McpConfigInvalidOperationTimeout",
            ),
            (
                r#"{"mcp":{"a":{"command":"x","restart_limit":256}}}"#,
                "McpConfigInvalidRestartLimit",
            ),
            (r#"{"mcp":{"a":{"command":3}}}"#, "McpConfigInvalidCommand"),
            (r#"{"mcp":{"a":{"type":"http"}}}"#, "McpConfigMissingUrl"),
            (
                r#"{"mcp":{"a":{"type":"http","url":"http://example.test/mcp"}}}"#,
                "McpConfigInvalidUrl",
            ),
        ] {
            assert_eq!(parse_profile(text), Err(ConfigError(expected)), "{text}");
        }
    }

    #[test]
    fn headers_are_held_to_upstreams_static_rules_and_header_env_to_plain_names() {
        let remote = |fields: &str| {
            format!(
                r#"{{"mcp":{{"r":{{"type":"http","url":"https://example.test/mcp",{fields}}}}}}}"#
            )
        };
        for fields in [
            r#""headers":{"X-A":"1","x-a":"2"}"#,
            r#""headers":{"Content-Type":"text/plain"}"#,
            r#""headers":{"MCP-Session-Id":"s"}"#,
            r#""headers":{"Mcp-Param-Name":"v"}"#,
            r#""headers":{"X-A":"line\u0001"}"#,
            r#""headers":{"X A":"1"}"#,
            r#""header_env":{"X-Team":"not a name"}"#,
            r#""header_env":{"Authorization":"TOKEN"}"#,
            r#""header_env":{"Host":"HOST_VALUE"}"#,
            r#""header_env":{"X-A":"ONE","x-a":"TWO"}"#,
        ] {
            assert_eq!(
                parse_profile(&remote(fields)),
                Err(ConfigError("McpConfigInvalidHeaders")),
                "{fields}"
            );
        }
        let profile = parse_profile(&remote(
            r#""headers":{"X-A":"tab\there","Authorization":"Bearer x"},"header_env":{"X-Team":"TEAM_1"}"#,
        ))
        .expect("parses");
        assert_eq!(
            profile.servers[0].header_env,
            [("X-Team".to_string(), "TEAM_1".to_string())]
        );
    }

    #[test]
    fn the_endpoint_rule_is_https_or_loopback_with_a_port() {
        for good in [
            "https://example.test/mcp",
            "http://127.0.0.1:8080/mcp",
            "http://localhost:1",
            "http://[::1]:9/x",
        ] {
            assert_eq!(validate_endpoint(good), Ok(()), "{good}");
        }
        for bad in [
            "http://example.test:80/mcp",
            "http://127.0.0.1/mcp",
            "https://user@example.test/mcp",
            "https://example.test/mcp#frag",
            "ftp://127.0.0.1:1",
            "nonsense",
        ] {
            assert!(validate_endpoint(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_rejection_wins_and_reset_forgets_everything() {
        let document = json!({"workspaces": {"/w": {
            "enabledMcpjsonServers": ["a", "both"],
            "disabledMcpjsonServers": ["both", "b"],
            "enabledMcpjsonServerDigests": {"a": "sha256:a", "both": "sha256:both"},
        }}});
        let choices = choices_in(document.as_object().unwrap(), Path::new("/w"));
        assert_eq!(choices.admission("a", "sha256:a"), Admission::Approved);
        assert_eq!(
            choices.admission("both", "sha256:both"),
            Admission::Rejected
        );
        assert_eq!(choices.admission("c", "sha256:c"), Admission::Pending);
        let loaded = loaded_as_pending(&[("b", "sha256:b")]);
        let (next, reduced) = apply_action(&choices, &TrustAction::Reject("a".into()), &loaded);
        assert!(reduced);
        assert_eq!(next.admission("a", "sha256:a"), Admission::Rejected);
        let (next, reduced) = apply_action(&next, &TrustAction::Approve("b".into()), &loaded);
        assert!(!reduced);
        assert_eq!(next.admission("b", "sha256:b"), Admission::Approved);
        let (next, _) = apply_action(&next, &TrustAction::Reset, &loaded);
        assert_eq!(next, Choices::default());
    }

    #[test]
    fn an_approval_admits_only_the_entry_it_was_given_for() {
        let entry = json!({"command": "/bin/node", "args": ["server.js"], "env": {"K": "v"}});
        let digest = entry_digest(&entry);
        let choices = Choices {
            approved: vec![("proj".into(), digest.clone())],
            ..Choices::default()
        };
        assert_eq!(choices.admission("proj", &digest), Admission::Approved);
        // What runs, where it connects, what it sends: each change asks again.
        for changed in [
            json!({"command": "/bin/sh", "args": ["server.js"], "env": {"K": "v"}}),
            json!({"command": "/bin/node", "args": ["evil.js"], "env": {"K": "v"}}),
            json!({"command": "/bin/node", "args": ["server.js"], "env": {"K": "w"}}),
            json!({"command": "/bin/node", "args": ["server.js"], "env": {"K": "v", "L": "v"}}),
            json!({"command": "/bin/node", "args": ["server.js"], "environment": {"K": "v"}}),
            json!({"command": ["/bin/node", "server.js"], "env": {"K": "v"}}),
            json!({"command": "/bin/node", "args": ["server.js"], "env": {"K": "v"}, "x": 1}),
        ] {
            assert_eq!(
                choices.admission("proj", &entry_digest(&changed)),
                Admission::Pending,
                "{changed}"
            );
        }
        let remote = json!({"type": "http", "url": "https://a.test/mcp",
            "headers": {"X-A": "1"}, "header_env": {"X-T": "TEAM"}, "bearer_token_env": "TOKEN"});
        for changed in [
            json!({"type": "http", "url": "https://b.test/mcp",
                "headers": {"X-A": "1"}, "header_env": {"X-T": "TEAM"}, "bearer_token_env": "TOKEN"}),
            json!({"type": "http", "url": "https://a.test/mcp",
                "headers": {"X-A": "2"}, "header_env": {"X-T": "TEAM"}, "bearer_token_env": "TOKEN"}),
            json!({"type": "http", "url": "https://a.test/mcp",
                "headers": {"X-A": "1"}, "header_env": {"X-T": "OTHER"}, "bearer_token_env": "TOKEN"}),
            json!({"type": "http", "url": "https://a.test/mcp",
                "headers": {"X-A": "1"}, "header_env": {"X-T": "TEAM"}, "bearer_token_env": "OTHER"}),
            json!({"type": "sse", "url": "https://a.test/mcp",
                "headers": {"X-A": "1"}, "header_env": {"X-T": "TEAM"}, "bearer_token_env": "TOKEN"}),
        ] {
            assert_ne!(entry_digest(&changed), entry_digest(&remote), "{changed}");
        }
        // Key order, and the fields that only say when it runs, are not what
        // was approved.
        let reordered: Value = serde_json::from_str(
            r#"{"env":{"K":"v"},"args":["server.js"],"enabled":false,"startup_timeout_ms":5,"operation_timeout_ms":5,"restart_limit":0,"required":true,"command":"/bin/node"}"#,
        )
        .unwrap();
        assert_eq!(entry_digest(&reordered), digest);
        assert!(
            digest.starts_with("sha256:") && digest.len() == 7 + 64,
            "{digest}"
        );
    }

    #[test]
    fn an_approval_saved_by_name_alone_or_for_every_server_asks_again() {
        // What an xfx that keyed approvals by name wrote: no digest, or the
        // switch that approved every entry whatever it said.
        let document = json!({"workspaces": {"/w": {
            "enabledMcpjsonServers": ["a"],
            "enableAllProjectMcpServers": true,
        }}});
        let choices = choices_in(document.as_object().unwrap(), Path::new("/w"));
        let digest = entry_digest(&json!({"command": "x"}));
        assert_eq!(choices.admission("a", &digest), Admission::Pending);
        assert_eq!(choices.admission("b", &digest), Admission::Pending);
        assert_eq!(choices, Choices::default());
    }

    #[test]
    fn approve_all_approves_the_entries_there_now_and_not_later_ones() {
        let current = Choices {
            rejected: vec!["no".into()],
            ..Choices::default()
        };
        let loaded = Loaded {
            pending: loaded_as_pending(&[("a", "sha256:a"), ("no", "sha256:no")]).pending,
            // Read, and shown only as a configuration error: not approved.
            entries: loaded_as_pending(&[
                ("a", "sha256:a"),
                ("no", "sha256:no"),
                ("unparsed", "sha256:u"),
            ])
            .entries,
        };
        let (next, _) = apply_action(&current, &TrustAction::ApproveAll, &loaded);
        assert_eq!(next.admission("a", "sha256:a"), Admission::Approved);
        assert_eq!(next.admission("a", "sha256:changed"), Admission::Pending);
        assert_eq!(next.admission("no", "sha256:no"), Admission::Rejected);
        assert_eq!(next.admission("later", "sha256:later"), Admission::Pending);
        assert_eq!(next.admission("unparsed", "sha256:u"), Admission::Pending);
        // The prompt's approval is for what it showed, whatever was read since.
        let shown = TrustAction::ApproveShown {
            name: "a".into(),
            digest: "sha256:shown".into(),
        };
        let (next, _) = apply_action(&next, &shown, &loaded);
        assert_eq!(
            next.approved,
            [("a".to_string(), "sha256:shown".to_string())]
        );
        // A name not in the file has nothing to approve.
        let (next, _) = apply_action(
            &Choices::default(),
            &TrustAction::Approve("ghost".into()),
            &loaded,
        );
        assert_eq!(next, Choices::default());
    }

    #[test]
    fn the_choices_land_in_settings_as_upstreams_keys_and_leave_when_reset() {
        let dir = tempfile::tempdir().expect("dir");
        let settings = dir.path().join("settings.json");
        let root = Path::new("/w/x");
        // The document upstream leaves (`cap/fx-mcp-fixture-40x120-{after-add,end}.files`),
        // in the layout xfx's settings writer gives every profile, with the
        // digest each approval is for beside upstream's list.
        let read =
            || serde_json::from_str::<Value>(&fs::read_to_string(&settings).unwrap()).unwrap();
        let loaded = loaded_as_pending(&[("proj", "sha256:p")]);
        save_action(
            &settings,
            root,
            &TrustAction::Reject("proj".into()),
            &loaded,
        )
        .expect("saved");
        assert_eq!(
            read(),
            json!({"workspaces": {"/w/x": {"disabledMcpjsonServers": ["proj"]}}})
        );
        save_action(
            &settings,
            root,
            &TrustAction::Approve("proj".into()),
            &loaded,
        )
        .expect("saved");
        assert_eq!(
            read(),
            json!({"workspaces": {"/w/x": {
                "enabledMcpjsonServers": ["proj"],
                "enabledMcpjsonServerDigests": {"proj": "sha256:p"},
            }}})
        );
        save_action(&settings, root, &TrustAction::Reset, &loaded).expect("saved");
        assert_eq!(read(), json!({}));
        // The switch an older xfx wrote leaves at the next write.
        fs::write(
            &settings,
            json!({"workspaces": {"/w/x": {"enableAllProjectMcpServers": true}}}).to_string(),
        )
        .unwrap();
        save_action(&settings, root, &TrustAction::ApproveAll, &loaded).expect("saved");
        assert_eq!(
            read(),
            json!({"workspaces": {"/w/x": {
                "enabledMcpjsonServers": ["proj"],
                "enabledMcpjsonServerDigests": {"proj": "sha256:p"},
            }}})
        );
    }

    #[test]
    fn a_workspace_file_is_read_under_the_choices_and_expanded_only_once_approved() {
        let dir = tempfile::tempdir().expect("dir");
        fs::write(
            dir.path().join(WORKSPACE_FILE),
            r#"{"mcpServers":{"ok":{"command":"${CMD}","args":["${MISSING:-fallback}"]},"bad":{"command":3},"remote":{"type":"http","url":"https://example.test/mcp","headers":{"Authorization":"x"}}}}"#,
        )
        .unwrap();
        let env = |name: &str| (name == "CMD").then(|| "/bin/node".to_string());
        let pending = load_workspace(dir.path(), &Choices::default(), &env);
        assert_eq!(pending.servers.len(), 1);
        assert_eq!(pending.servers[0].command, "${CMD}");
        assert_eq!(pending.servers[0].admission, Some(Admission::Pending));
        assert_eq!(pending.issues.len(), 2, "{:?}", pending.issues);
        // Every entry with a valid name has a digest, parsed or not; the
        // digest is of the entry as written, `${CMD}` and all.
        let names: Vec<&str> = pending
            .digests
            .iter()
            .map(|(name, _)| name.as_str())
            .collect();
        assert_eq!(names, ["ok", "bad", "remote"]);
        assert_eq!(
            pending.digests[0].1,
            entry_digest(&json!({"command": "${CMD}", "args": ["${MISSING:-fallback}"]}))
        );
        let approved = Choices {
            approved: vec![("ok".into(), pending.digests[0].1.clone())],
            ..Choices::default()
        };
        let servers = load_workspace(dir.path(), &approved, &env).servers;
        assert_eq!(servers[0].admission, Some(Admission::Approved));
        assert_eq!(servers[0].command, "/bin/node");
        assert_eq!(servers[0].args, ["fallback"]);
    }

    #[test]
    fn profile_writes_keep_order_replace_by_name_and_say_when_nothing_was_removed() {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join("p").join(PROFILE_FILE);
        add_profile_server(&path, ServerConfig::local("a", "x", &[])).unwrap();
        add_profile_server(&path, ServerConfig::local("b", "y", &[])).unwrap();
        add_profile_server(&path, ServerConfig::local("a", "z", &[])).unwrap();
        let names: Vec<_> = load_profile(&path)
            .unwrap()
            .servers
            .into_iter()
            .map(|server| (server.name, server.command))
            .collect();
        assert_eq!(names, [("a".into(), "z".into()), ("b".into(), "y".into())]);
        assert!(remove_profile_server(&path, "a").unwrap());
        assert!(!remove_profile_server(&path, "nope").unwrap());
        assert!(dir.path().join("p").join(PROFILE_LOCK).exists());
    }
}
