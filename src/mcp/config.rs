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
//!
//! fx v0.0.16 (`481104fd`) changed four things here, and this file has them:
//!
//! - **An entry xfx can't read is skipped with a note** in fx's words, rather
//!   than the whole profile refused (`parseProfileServerMap`,
//!   `src/core/mcp/project_config.zig:373-401@481104fd`;
//!   `renderWorkspaceDiagnostic`, `:161-215`): [`Profile::notes`],
//!   [`WorkspaceServers::issues`].
//! - **Both files are edited in place** ([`update`]; `config_file.zig@481104fd`):
//!   one entry added to or removed from the file's JSON, every other key, its
//!   order and the text of its numbers kept, two-space pretty JSON, an entry
//!   in Claude's shape (`{"command","args","env"}`,
//!   `{"type":"http","url","headers"}`). An old `{"mcp":…}` profile keeps its
//!   `mcp` key. The `/mcp` verbs call it (`crate::mcp::command`); c1db9191's
//!   whole-file writers it replaced, `add_profile_server` and
//!   `remove_profile_server`, are left only for tests to seed a profile with.
//! - **`--project` edits the workspace's `.mcp.json`**, never through a link
//!   or a file with another hard link, keeping its mode.
//! - **Before any server starts** ([`prepare_to_start`]; `loadHost`,
//!   `src/builtins/mcp.zig:121-134@481104fd`), a profile server's `${VAR}`
//!   is expanded, and a server whose name a terminal can't show is skipped,
//!   each with a note. The file keeps `${VAR}`; the values go to the server
//!   they start or reach, and to a screen only as that server's own stderr.

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
///
/// Once through [`prepare_to_start`], a profile server's command, arguments,
/// `env` values and header values are what its `${VAR}`s expanded to, and an
/// approved workspace server's are too ([`load_workspace`]): values from the
/// environment, a credential among them, for the server: no screen of xfx's
/// shows one, though a server's own stderr, quoted in its failure, may. What
/// a person is shown of an entry is its file's text ([`ServerConfig::written`]), and
/// `{:?}` names the server and only counts what it runs and sends (the
/// `Debug` below).
#[derive(Clone, PartialEq)]
pub struct ServerConfig {
    pub name: String,
    pub source: Source,
    pub transport: Transport,
    /// The program, for a stdio server.
    pub command: String,
    pub args: Vec<String>,
    /// Its command, arguments and URL as its file has them, kept before an
    /// expansion fills them -- a profile server's by [`prepare_to_start`],
    /// an approved workspace server's by [`load_workspace`]: what a screen
    /// shows of the server ([`Self::shown_command`], [`Self::shown_url`]).
    /// `None` for a server nothing expanded.
    pub written: Option<Written>,
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

/// The server's name, where it came from, how it is reached and when it
/// runs; of every field an expansion can fill -- a profile server's command,
/// arguments, `env` and header values ([`prepare_to_start`]), and an approved
/// workspace server's every string, its URL, `header_env`, `bearer_token_env`
/// and `oauth` included ([`load_workspace`]) -- only whether it is set or how
/// many there are. A `{:?}` in a panic, an assertion or a trace never carries
/// a value from the environment, nor one from the file.
impl fmt::Debug for ServerConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let set = |is_set: bool| {
            if is_set {
                "<redacted>"
            } else {
                "<none>"
            }
        };
        let count = |n: usize| format!("<{n} redacted>");
        f.debug_struct("ServerConfig")
            .field("name", &self.name)
            .field("source", &self.source)
            .field("transport", &self.transport)
            .field(
                "command",
                &format_args!("{}", set(!self.command.is_empty())),
            )
            .field("args", &format_args!("{}", count(self.args.len())))
            .field("env", &format_args!("{}", count(self.env.len())))
            .field("url", &format_args!("{}", set(!self.url.is_empty())))
            .field("headers", &format_args!("{}", count(self.headers.len())))
            .field(
                "header_env",
                &format_args!("{}", count(self.header_env.len())),
            )
            .field(
                "bearer_token_env",
                &format_args!("{}", set(self.bearer_token_env.is_some())),
            )
            .field("oauth", &format_args!("{}", set(self.oauth.is_some())))
            .field("enabled", &self.enabled)
            .field("required", &self.required)
            .field("startup_timeout_ms", &self.startup_timeout_ms)
            .field("operation_timeout_ms", &self.operation_timeout_ms)
            .field("restart_limit", &self.restart_limit)
            .field("admission", &self.admission)
            .finish()
    }
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
            written: None,
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

    /// The command and arguments a screen shows (`/mcp show`, the menu's
    /// `runs`, `approve`'s sentence): as the file has them -- a value an
    /// expansion put in is for the server.
    pub fn shown_command(&self) -> (&str, &[String]) {
        match &self.written {
            Some(written) => (&written.command, &written.args),
            None => (&self.command, &self.args),
        }
    }

    /// The URL a screen shows, the same way.
    pub fn shown_url(&self) -> &str {
        match &self.written {
            Some(written) => &written.url,
            None => &self.url,
        }
    }

    pub fn startup_timeout(&self) -> Duration {
        Duration::from_millis(u64::from(self.startup_timeout_ms))
    }

    pub fn operation_timeout(&self) -> Duration {
        Duration::from_millis(u64::from(self.operation_timeout_ms))
    }
}

/// A server's command, arguments and URL as its file has them
/// ([`ServerConfig::written`]).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Written {
    pub command: String,
    pub args: Vec<String>,
    pub url: String,
}

impl Written {
    /// What `entry`, a server's object as its file has it, runs and reaches:
    /// a `command` string and its `args`, or a `command` list, and a `url`.
    /// What isn't a string is left empty -- nothing is shown in its place.
    fn of(entry: &Value) -> Self {
        let strings = |value: Option<&Value>| -> Vec<String> {
            value
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default()
        };
        let (command, args) = match entry.get("command") {
            Some(Value::String(command)) => (command.clone(), strings(entry.get("args"))),
            Some(Value::Array(_)) => {
                let mut words = strings(entry.get("command"));
                if words.is_empty() {
                    (String::new(), Vec::new())
                } else {
                    let command = words.remove(0);
                    (command, words)
                }
            }
            _ => (String::new(), Vec::new()),
        };
        let url = entry
            .get("url")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        Self { command, args, url }
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
        // Other clients' files spell Streamable HTTP out
        // (`project_config.zig:1029-1033@481104fd`).
        "http" | "streamable-http" => Transport::Http,
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
        written: None,
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
        // The usual mistake: a remote server copied without its type
        // (`project_config.zig:1104-1105@481104fd`).
        if !object.contains_key("type") && object.contains_key("url") {
            return Err(ConfigError("McpConfigUrlWithoutType"));
        }
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

pub(crate) fn is_env_name(name: &str) -> bool {
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
    /// One sentence per entry left out, in fx v0.0.16's words: an entry xfx
    /// can't read, or one under `mcpServers` beside an `mcp` that is read
    /// instead. They come before the workspace's.
    pub notes: Vec<String>,
}

/// `mcp.json` in `profile_dir`.
pub fn profile_path(profile_dir: &Path) -> PathBuf {
    profile_dir.join(PROFILE_FILE)
}

/// Reads the profile's servers. A file that is not there is an empty profile;
/// an entry that can't be read is left out with a note
/// (`loadNativeConfigs`, `src/builtins/mcp.zig:45-56@481104fd`). A file that
/// can't be read, or isn't a JSON object of servers, is still an error.
pub fn load_profile(path: &Path) -> Result<Profile, ConfigError> {
    let Some(text) = read_bounded(path).map_err(|_| ConfigError("McpConfigUnreadable"))? else {
        return Ok(Profile::default());
    };
    parse_profile(&text)
}

/// `parseProfileDocument` (`project_config.zig:303-371@481104fd`): the `mcp`
/// map, or the `mcpServers` alias when there is no `mcp`, each entry that
/// can't be read skipped with a note (`parseProfileServerMap`, `:373-401`).
pub fn parse_profile(text: &str) -> Result<Profile, ConfigError> {
    let (servers, alias) = profile_servers(text)?;
    let mut profile = Profile::default();
    for (name, value) in &servers {
        match parse_entry(name, value, Source::Profile, None) {
            Ok(config) => profile.servers.push(config),
            Err(err) => profile
                .notes
                .push(skipped(Source::Profile, name, Some(err))),
        }
    }
    // fx reads only `mcp`, so each name only `mcpServers` has is a note
    // (`noteIgnoredAlias`, `:403-418`).
    for name in alias.iter().flat_map(Map::keys) {
        if !servers.contains_key(name) {
            profile.notes.push(format!(
                "{} server '{}' was skipped: the file has both \"mcp\" and \"mcpServers\", \
                 and xfx reads only \"mcp\"; merge them into one.",
                Source::Profile.file(),
                note_name(name)
            ));
        }
    }
    Ok(profile)
}

/// The profile's servers as c1db9191's whole-file writer, `add_profile_server`,
/// reads them: every entry, or the first one's error. That writer renders the
/// whole profile from what it read, so an entry left out here would be an
/// entry deleted there. Tests only, as the writer is.
#[cfg(test)]
fn parse_profile_strictly(text: &str) -> Result<Vec<ServerConfig>, ConfigError> {
    let (servers, _) = profile_servers(text)?;
    servers
        .iter()
        .map(|(name, value)| parse_entry(name, value, Source::Profile, None))
        .collect()
}

/// A file's map of server names to entries.
type ServerMap = Map<String, Value>;

/// The server map the profile reads, and the `mcpServers` beside an `mcp`
/// it doesn't.
fn profile_servers(text: &str) -> Result<(ServerMap, Option<ServerMap>), ConfigError> {
    let document = read_json(text).ok_or(ConfigError("McpConfigInvalidJson"))?;
    let Value::Object(mut object) = document else {
        return Err(ConfigError("McpConfigRootMustBeObject"));
    };
    let canonical = object.shift_remove("mcp");
    let alias = object.shift_remove("mcpServers");
    let (servers, ignored) = match (canonical, alias) {
        (Some(servers), alias) => (servers, alias),
        (None, Some(servers)) => (servers, None),
        (None, None) => return Ok((Map::new(), None)),
    };
    let Value::Object(servers) = servers else {
        return Err(ConfigError("McpConfigServersMustBeObject"));
    };
    let ignored = match ignored {
        Some(Value::Object(alias)) => Some(alias),
        _ => None,
    };
    Ok((servers, ignored))
}

/// `text` as a JSON value, when it is one by fx's reading: no key twice in
/// an object (`std.json`'s default `duplicate_field_behavior`), which
/// `serde_json` would take as its last. The same reading [`update`] edits by,
/// so a file the writer refuses is not one the reader takes some other way.
fn read_json(text: &str) -> Option<Value> {
    parse_document(text).ok()?;
    serde_json::from_str(text).ok()
}

// ---------------------------------------------------------------------------
// notes
// ---------------------------------------------------------------------------

impl Source {
    /// How a note names the file (`diagnosticFile`,
    /// `project_config.zig:237-239@481104fd`).
    pub fn file(self) -> &'static str {
        match self {
            Self::Profile => "~/.xfx/mcp.json",
            Self::Workspace => WORKSPACE_FILE,
        }
    }
}

/// The most of a server's name a note carries (`renderWorkspaceDiagnostic`'s
/// `encodeTerminalSafe(.., 160)`, `project_config.zig:165-169@481104fd`).
const MAX_NOTE_NAME_BYTES: usize = 160;

/// A name from the file, spelled for a terminal row by
/// [`crate::mcp::terminal_safe`], one character at a time, and cut as it cuts
/// to [`MAX_NOTE_NAME_BYTES`].
fn note_name(name: &str) -> String {
    spelled(name, MAX_NOTE_NAME_BYTES)
}

/// `text` as `encodeTerminalSafe(.., max)` gives it
/// (`src/core/shared/text_utils.zig:353-390@481104fd`): each character
/// spelled by [`crate::mcp::terminal_safe`], and past `max` bytes cut at a
/// character's boundary to end `...` within `max`.
fn spelled(text: &str, max: usize) -> String {
    const MARKER: &str = "...";
    let mut out = String::new();
    let mut boundary = 0;
    let mut buf = [0u8; 4];
    for character in text.chars() {
        let token = crate::mcp::terminal_safe(character.encode_utf8(&mut buf));
        if out.len() + token.len() > max {
            out.truncate(boundary);
            out.push_str(MARKER);
            return out;
        }
        out.push_str(&token);
        if out.len() <= max - MARKER.len() {
            boundary = out.len();
        }
    }
    out
}

/// Whether a terminal shows `name` as it is: no character in it that
/// [`crate::mcp::terminal_safe`] spells -- a control, a default-ignorable
/// code point (the bidirectional and zero-width ones among them), one a
/// terminal was measured drawing wider than xfx counts, a zero-width one
/// that isn't a combining mark. fx's `isTerminalSafe`
/// (`text_utils.zig:478-488@481104fd`) tests a smaller set: the C0 controls,
/// `DEL`, and `isNonPrintingCodepoint`'s (`:703-709`), the C1 controls among
/// them (`docs/parity.md`).
fn printable(name: &str) -> bool {
    let mut buf = [0u8; 4];
    name.chars().all(|character| {
        let one = character.encode_utf8(&mut buf);
        crate::mcp::terminal_safe(one) == *one
    })
}

/// Why an entry was skipped, by the error that refused it (`entryProblem`,
/// `project_config.zig:121-140@481104fd`, in `renderWorkspaceDiagnostic`'s
/// words, `:198-206`), `fx` spelled `xfx`. `None` is a file that could not be
/// read at all (`invalidResult`, `workspace_config.zig:57-62@481104fd`).
fn entry_problem(err: Option<ConfigError>) -> &'static str {
    match err.map(|err| err.0) {
        Some("McpConfigServerMustBeObject") => "it isn't a JSON object",
        Some("McpConfigUrlWithoutType") => {
            "it has a \"url\" but no \"type\"; add \"type\": \"http\""
        }
        Some("McpConfigInvalidCommand") => "it needs a \"command\", with any \"args\" as strings",
        Some("McpConfigInvalidType") => "its \"type\" isn't valid",
        Some("McpConfigMissingUrl" | "McpConfigInvalidUrl") => "its \"url\" isn't valid",
        Some("McpConfigInvalidHeaders") => "its \"headers\" isn't valid",
        Some("McpConfigInvalidOAuth") => "its \"oauth\" isn't valid",
        Some("McpConfigInvalidEnvironment") => "its \"env\" isn't valid",
        Some("McpConfigInvalidBearerEnvironment") => "its \"bearer_token_env\" isn't valid",
        Some("McpConfigInvalidEnabled") => "its \"enabled\" isn't valid",
        Some("McpConfigInvalidRequired") => "its \"required\" isn't valid",
        Some("McpConfigInvalidStartupTimeout") => "its \"startup_timeout_ms\" isn't valid",
        Some("McpConfigInvalidOperationTimeout") => "its \"operation_timeout_ms\" isn't valid",
        Some("McpConfigInvalidRestartLimit") => "its \"restart_limit\" isn't valid",
        // A `${...}` that isn't a variable (`.variable`).
        Some(VARIABLE_PROBLEM) => "a ${...} in it isn't valid",
        // A name a terminal can't show as is (`.name`, `skipUnprintableNames`,
        // `src/builtins/mcp.zig:99-116@481104fd`).
        Some(NAME_PROBLEM) => "its name has a character xfx won't print",
        // What expanding a profile entry's `${...}` would have made past
        // fx's cap: the cause's own name, as fx renders a cause with no
        // problem (`environment_expansion_limit_exceeded`, `project_config.zig:207-210@481104fd`).
        Some(LIMIT_PROBLEM) => LIMIT_PROBLEM,
        _ => "xfx can't read it",
    }
}

/// [`entry_problem`]'s key for a `${...}` that isn't a variable.
const VARIABLE_PROBLEM: &str = "variable";
/// [`entry_problem`]'s key for a name a terminal can't show as is.
const NAME_PROBLEM: &str = "name";
/// [`entry_problem`]'s key, and its words, for an expansion past the cap.
const LIMIT_PROBLEM: &str = "environment_expansion_limit_exceeded";

/// `<file> server '<name>' was skipped: <why>.` (`:211-214@481104fd`).
fn skipped(source: Source, name: &str, err: Option<ConfigError>) -> String {
    format!(
        "{} server '{}' was skipped: {}.",
        source.file(),
        note_name(name),
        entry_problem(err)
    )
}

/// What fx says when a whole `.mcp.json` is refused: its cause's own name
/// where a server's would be (`renderWorkspaceDiagnostic` with no server and
/// no problem, `:207-214@481104fd`), as `.mcp.json server 'unknown' was
/// skipped: invalid_json.`
fn whole_file(cause: &str) -> String {
    format!(
        "{} server 'unknown' was skipped: {cause}.",
        Source::Workspace.file()
    )
}

/// The profile file's bytes, in c1db9191's shape and key order
/// (`renderConfigJson`, `mcp.zig:1075-1180@c1db9191`). Tests only: the
/// `/mcp` verbs write through [`update`], as fx v0.0.16's do.
#[cfg(test)]
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
/// name or after the others, under the profile lock: how a test seeds a
/// profile with a whole [`ServerConfig`], timeouts and all.
#[cfg(test)]
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
#[cfg(test)]
pub fn remove_profile_server(path: &Path, name: &str) -> Result<bool, ConfigError> {
    edit_profile(path, |servers| {
        let before = servers.len();
        servers.retain(|server| server.name != name);
        servers.len() != before
    })
}

/// One read-modify-write of the profile under `mcp.lock`; nothing is written
/// when `edit` says it changed nothing.
#[cfg(test)]
fn edit_profile(
    path: &Path,
    edit: impl FnOnce(&mut Vec<ServerConfig>) -> bool,
) -> Result<bool, ConfigError> {
    let dir = path.parent().ok_or(ConfigError("McpConfigPathInvalid"))?;
    crate::profile_home::ensure(dir, dir).map_err(|err| profile_error(&err))?;
    let _lock = ProfileLock::acquire(&dir.join(PROFILE_LOCK))?;
    let mut servers = match read_bounded(path).map_err(|_| ConfigError("McpConfigUnreadable"))? {
        Some(text) => parse_profile_strictly(&text)?,
        None => Vec::new(),
    };
    if !edit(&mut servers) {
        return Ok(false);
    }
    let bytes = render_profile(&servers);
    crate::provider::profile::replace_private_file(dir, path, bytes.as_bytes())
        .map_err(|_| ConfigError("McpConfigWriteFailed"))?;
    Ok(true)
}

/// What a profile home that could not be used is named, in upstream's error
/// names: a link or something that is not a directory is
/// `DurablePathUnsafe`, another account's or an open one
/// `PrivateStatePermissionsUnsupported` (`openOrCreateVerifiedPrivateChild`,
/// `src/core/shared/io.zig:733-739@481104fd`). Upstream `chmod`s an open one
/// it owns and goes on; xfx refuses it ([`crate::profile_home`]). Test-only
/// with the writer it speaks for ([`edit_profile`]); `/mcp`'s own writer
/// ([`update`]) says the refusal in words ([`EditError::Refused`]).
#[cfg(test)]
fn profile_error(err: &io::Error) -> ConfigError {
    use crate::profile_home::{Problem, Refused};
    match Refused::of(err).map(|refused| refused.problem) {
        Some(Problem::Link | Problem::NotDirectory) => ConfigError("DurablePathUnsafe"),
        Some(Problem::Owner { .. } | Problem::Open { .. }) => {
            ConfigError("PrivateStatePermissionsUnsupported")
        }
        None => ConfigError("McpConfigPathInvalid"),
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
    read_bounded_then(path, |_| {})
}

/// Where [`read_bounded_then`] is between its looks: its tests grow, swap or
/// record the file there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReadStep {
    /// The path has been looked at; the file is about to be opened.
    Opening,
    /// The open file has been looked at; its bytes are about to be read.
    Reading,
}

/// [`read_bounded`], `step` told where it is.
///
/// A path that is not a regular file, through any links, is refused before
/// it is opened: a cloned repository's `.mcp.json -> /dev/<device>` is never
/// opened at all. The file is then opened once, and that descriptor decides:
/// it is what is looked at and what is read, so a path swapped after the
/// first look is not read, a FIFO or a device swapped in is refused without
/// blocking (`O_NONBLOCK`), and no more than [`MAX_CONFIG_BYTES`] are read
/// whatever size the file reported.
fn read_bounded_then(path: &Path, mut step: impl FnMut(ReadStep)) -> io::Result<Option<String>> {
    let refused = || io::Error::new(io::ErrorKind::InvalidData, "not a readable config");
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => {}
        Ok(_) => return Err(refused()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err),
    }
    step(ReadStep::Opening);
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // Never a controlling terminal, if a link names a terminal device.
        options.custom_flags(libc::O_NONBLOCK | libc::O_NOCTTY);
    }
    let file = match options.open(path) {
        Ok(file) => file,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err),
    };
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > MAX_CONFIG_BYTES {
        return Err(refused());
    }
    step(ReadStep::Reading);
    let bytes = read_capped(file)?;
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "not UTF-8"))
}

/// At most [`MAX_CONFIG_BYTES`] of `reader`, or an error when it holds more:
/// the size a file reports bounds nothing it holds (one that grew after it was
/// looked at, a procfs file that says 0).
fn read_capped(reader: impl io::Read) -> io::Result<Vec<u8>> {
    use std::io::Read as _;
    let mut bytes = Vec::new();
    reader.take(MAX_CONFIG_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_CONFIG_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "larger than a config is read",
        ));
    }
    Ok(bytes)
}

// ---------------------------------------------------------------------------
// the files' JSON, as an edit reads and writes it
// ---------------------------------------------------------------------------

/// One JSON value as a file wrote it: an object's keys in the file's order,
/// and a number as its own text (`.parse_numbers = false`,
/// `config_file.zig:176@481104fd`), so an edit to one entry leaves what every
/// other key says as written. Strings are decoded and written back in
/// `std.json`'s spelling, as fx's are.
#[derive(Debug, Clone, PartialEq)]
enum Json {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

/// The most objects and arrays one inside another before a file is not read:
/// `serde_json`'s bound for the reader ([`read_json`]), so the reader and the
/// writer take the same files. fx's `std.json` has none; this one keeps a
/// file a repository wrote from running a recursive reader out of stack.
const MAX_JSON_DEPTH: usize = 127;

/// A strict reader of RFC 8259 JSON, as `std.json`'s scanner is: no comment,
/// trailing comma, leading zero or bare control byte; a `\u` high surrogate
/// only with its low half; no key twice in one object.
struct Reader<'a> {
    text: &'a str,
    at: usize,
    /// Refuse a key twice in one object: always, but in the test that checks
    /// this reader against `serde_json`, which takes the last.
    unique_keys: bool,
}

impl Reader<'_> {
    fn peek(&self) -> Option<u8> {
        self.text.as_bytes().get(self.at).copied()
    }

    fn skip_space(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    fn eat(&mut self, word: &str) -> Option<()> {
        let found = self.text.as_bytes()[self.at..].starts_with(word.as_bytes());
        found.then(|| self.at += word.len())
    }

    fn value(&mut self, depth: usize) -> Option<Json> {
        match self.peek()? {
            b'{' => self.object(depth + 1),
            b'[' => self.array(depth + 1),
            b'"' => self.string().map(Json::String),
            b't' => self.eat("true").map(|()| Json::Bool(true)),
            b'f' => self.eat("false").map(|()| Json::Bool(false)),
            b'n' => self.eat("null").map(|()| Json::Null),
            b'-' | b'0'..=b'9' => self.number().map(Json::Number),
            _ => None,
        }
    }

    fn object(&mut self, depth: usize) -> Option<Json> {
        if depth > MAX_JSON_DEPTH {
            return None;
        }
        self.at += 1;
        let mut fields: Vec<(String, Json)> = Vec::new();
        let mut keys = std::collections::HashSet::new();
        self.skip_space();
        if self.eat("}").is_some() {
            return Some(Json::Object(fields));
        }
        loop {
            self.skip_space();
            if self.peek()? != b'"' {
                return None;
            }
            let key = self.string()?;
            if !keys.insert(key.clone()) && self.unique_keys {
                return None;
            }
            self.skip_space();
            self.eat(":")?;
            self.skip_space();
            let value = self.value(depth)?;
            fields.push((key, value));
            self.skip_space();
            if self.eat("}").is_some() {
                return Some(Json::Object(fields));
            }
            self.eat(",")?;
        }
    }

    fn array(&mut self, depth: usize) -> Option<Json> {
        if depth > MAX_JSON_DEPTH {
            return None;
        }
        self.at += 1;
        let mut items = Vec::new();
        self.skip_space();
        if self.eat("]").is_some() {
            return Some(Json::Array(items));
        }
        loop {
            self.skip_space();
            items.push(self.value(depth)?);
            self.skip_space();
            if self.eat("]").is_some() {
                return Some(Json::Array(items));
            }
            self.eat(",")?;
        }
    }

    /// `-? (0 | [1-9][0-9]*) (. [0-9]+)? ([eE] [+-]? [0-9]+)?`, kept as written.
    fn number(&mut self) -> Option<String> {
        let start = self.at;
        let _ = self.eat("-");
        match self.peek()? {
            b'0' => self.at += 1,
            b'1'..=b'9' => self.digits(),
            _ => return None,
        }
        if self.eat(".").is_some() {
            if !self.peek()?.is_ascii_digit() {
                return None;
            }
            self.digits();
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.at += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.at += 1;
            }
            if !self.peek()?.is_ascii_digit() {
                return None;
            }
            self.digits();
        }
        Some(self.text[start..self.at].to_string())
    }

    fn digits(&mut self) {
        while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
            self.at += 1;
        }
    }

    fn string(&mut self) -> Option<String> {
        self.at += 1;
        let mut out = String::new();
        loop {
            // A run up to the next quote, backslash or control byte: each of
            // them ASCII, so the run ends on a character boundary.
            let start = self.at;
            while self
                .peek()
                .is_some_and(|byte| byte != b'"' && byte != b'\\' && byte >= 0x20)
            {
                self.at += 1;
            }
            out.push_str(&self.text[start..self.at]);
            match self.peek()? {
                b'"' => {
                    self.at += 1;
                    return Some(out);
                }
                b'\\' => {
                    self.at += 1;
                    out.push(self.escape()?);
                }
                _ => return None,
            }
        }
    }

    fn escape(&mut self) -> Option<char> {
        let byte = self.peek()?;
        self.at += 1;
        Some(match byte {
            b'"' => '"',
            b'\\' => '\\',
            b'/' => '/',
            b'b' => '\u{8}',
            b'f' => '\u{c}',
            b'n' => '\n',
            b'r' => '\r',
            b't' => '\t',
            b'u' => match self.hex4()? {
                high @ 0xD800..=0xDBFF => {
                    self.eat("\\u")?;
                    let low = self.hex4()?;
                    if !(0xDC00..=0xDFFF).contains(&low) {
                        return None;
                    }
                    char::from_u32(0x10000 + ((high - 0xD800) << 10) + (low - 0xDC00))?
                }
                0xDC00..=0xDFFF => return None,
                unit => char::from_u32(unit)?,
            },
            _ => return None,
        })
    }

    fn hex4(&mut self) -> Option<u32> {
        let digits = self.text.get(self.at..self.at + 4)?;
        if !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        self.at += 4;
        u32::from_str_radix(digits, 16).ok()
    }
}

/// A file's text as an edit reads it; a blank one is an empty object
/// (`parse`, `config_file.zig:173-180@481104fd`).
fn parse_document(text: &str) -> Result<Json, EditError> {
    parse_with(text, true)
}

fn parse_with(text: &str, unique_keys: bool) -> Result<Json, EditError> {
    if text.trim_matches([' ', '\t', '\r', '\n']).is_empty() {
        return Ok(Json::Object(Vec::new()));
    }
    let mut reader = Reader {
        text,
        at: 0,
        unique_keys,
    };
    reader.skip_space();
    let value = reader.value(0).ok_or(EditError::Unreadable)?;
    reader.skip_space();
    if reader.at != text.len() {
        return Err(EditError::Unreadable);
    }
    Ok(value)
}

/// `value` as `std.json.Stringify` writes it with `.whitespace = .indent_2`,
/// and a line break after it (`render`, `config_file.zig:224-230@481104fd`) --
/// or [`EditError::TooLarge`] when that is over [`MAX_CONFIG_BYTES`], a file
/// the reader would refuse. Writing stops at the cap: a file of 1 MiB nested
/// 127 deep would be some 130 MB written out.
fn render(value: &Json) -> Result<String, EditError> {
    let mut out = String::new();
    write_value(value, 0, &mut out);
    out.push('\n');
    if past_cap(&out) {
        return Err(EditError::TooLarge);
    }
    Ok(out)
}

fn write_value(value: &Json, level: usize, out: &mut String) {
    match value {
        Json::Null => out.push_str("null"),
        Json::Bool(true) => out.push_str("true"),
        Json::Bool(false) => out.push_str("false"),
        Json::Number(text) => out.push_str(text),
        Json::String(text) => write_string(text, out),
        Json::Array(items) if items.is_empty() => out.push_str("[]"),
        Json::Object(fields) if fields.is_empty() => out.push_str("{}"),
        Json::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if past_cap(out) {
                    return;
                }
                if index > 0 {
                    out.push(',');
                }
                new_line(level + 1, out);
                write_value(item, level + 1, out);
            }
            new_line(level, out);
            out.push(']');
        }
        Json::Object(fields) => {
            out.push('{');
            for (index, (key, item)) in fields.iter().enumerate() {
                if past_cap(out) {
                    return;
                }
                if index > 0 {
                    out.push(',');
                }
                new_line(level + 1, out);
                write_string(key, out);
                out.push_str(": ");
                write_value(item, level + 1, out);
            }
            new_line(level, out);
            out.push('}');
        }
    }
}

/// Whether [`render`] has written past what it may return: each item it
/// starts adds a line, so it stops before the next one.
fn past_cap(out: &str) -> bool {
    out.len() as u64 > MAX_CONFIG_BYTES
}

fn new_line(level: usize, out: &mut String) {
    out.push('\n');
    for _ in 0..level {
        out.push_str("  ");
    }
}

/// `encodeJsonString` without `escape_unicode` (`std/json/Stringify.zig`):
/// the quote, the backslash, and every byte below `0x20` -- five by letter,
/// the rest `\u00xx` -- and nothing else.
fn write_string(text: &str, out: &mut String) {
    use std::fmt::Write as _;
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            control if u32::from(control) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(control));
            }
            other => out.push(other),
        }
    }
    out.push('"');
}

// ---------------------------------------------------------------------------
// editing a file in place
// ---------------------------------------------------------------------------

/// Which file an edit is for (`File`, `config_file.zig:12-14@481104fd`): the
/// profile keeps its servers under `mcp` or `mcpServers`, a project's
/// `.mcp.json` only under `mcpServers`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum File {
    Profile,
    Project,
}

impl File {
    /// How a sentence names it (`fileLabel`, `verbs.zig:338-343@481104fd`).
    pub fn label(self) -> &'static str {
        match self {
            Self::Profile => Source::Profile.file(),
            Self::Project => WORKSPACE_FILE,
        }
    }
}

/// The workspace's `.mcp.json`, the file a `--project` edit is for.
pub fn project_path(workspace: &Path) -> PathBuf {
    workspace.join(WORKSPACE_FILE)
}

/// One server as an edit writes it, in Claude's shape (`Entry`,
/// `config_file.zig:18-21@481104fd`, written by `entryValue`, `:197-216`):
/// `{"command", "args", "env"}`, or `{"type": "http", "url", "headers"}`,
/// each list only when it has something in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    Stdio {
        command: String,
        args: Vec<String>,
        env: Vec<(String, String)>,
    },
    Http {
        url: String,
        headers: Vec<(String, String)>,
    },
}

/// One edit (`Change`, `:84-87`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Add { name: String, entry: Entry },
    Remove(String),
}

/// What an edit did (`Updated`, `:89-94`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Updated {
    Added,
    /// The removed entry's `url`, which keys an HTTP server's sign-in.
    Removed {
        url: Option<String>,
    },
    /// The name was not in the file, or there is no file: nothing written.
    Missing,
}

/// A file with one entry fewer (`Removed`, `:46-56`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Removed {
    pub text: String,
    /// The entry's `url`, when it had one.
    pub url: Option<String>,
}

/// Why an edit wrote nothing (`Error`, `:23-32`, and the errors `update`
/// meets on the way).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditError {
    /// Not JSON, a key twice in one object, or nested past [`MAX_JSON_DEPTH`].
    Unreadable,
    /// The file's top level, or its server list, isn't an object.
    NotAnObject,
    /// A profile with both `mcp` and `mcpServers`.
    BothKeys,
    /// The name is already in the file.
    Exists,
    /// The file is a symbolic link -- to this, as the link reads, when it can
    /// be read: replacing it would fork the file it points to, and following
    /// it could write outside the project.
    Symlink(Option<String>),
    /// The file has another hard link: replacing it would fork the file the
    /// other name shares.
    HardLink,
    /// The edit would leave a file over 1 MiB, which the reader refuses whole
    /// at the next launch: a compact file grows when it is written two-space.
    TooLarge,
    /// The file has no write permission.
    ReadOnly,
    /// Another xfx held `mcp.lock` past upstream's two seconds.
    Busy,
    /// The profile home `mcp.lock` is in could not be used -- a link, not a
    /// directory, another account's or open to group or other, or a home
    /// directory that is not there -- in the session store's words
    /// ([`crate::profile_home::refusal`]). Upstream repairs an open one it
    /// owns, and says `something went wrong` for the rest (`describe`,
    /// `verbs.zig:313-327@481104fd`); xfx refuses it and says why, as every
    /// writer in the profile does.
    Refused(String),
    /// Anything else: an I/O error, a file over 1 MiB or not a regular file.
    Failed,
}

impl EditError {
    /// Why, in fx's words (`whyNot` and `describe`,
    /// `verbs.zig:302-327@481104fd`) with `fx` spelled `xfx`: what follows
    /// `Couldn't add <name> to <file>: `. A link's target is from the disk,
    /// so it is spelled by [`crate::mcp::terminal_safe`].
    pub fn why(&self) -> String {
        match self {
            Self::Unreadable => {
                "the file isn't valid JSON, or repeats a key; fix it by hand first".into()
            }
            Self::NotAnObject => {
                "the file's servers aren't a JSON object; fix it by hand first".into()
            }
            Self::BothKeys => {
                "the file has both \"mcp\" and \"mcpServers\"; merge them into one".into()
            }
            Self::Exists => "that name is already in the file; remove it first".into(),
            Self::Symlink(Some(target)) => {
                let target = crate::mcp::terminal_safe(target);
                format!(
                    "it's a link to {target}, and xfx edits only a plain file; edit {target} by hand"
                )
            }
            // fx says the hard-link sentence here (`whyNot`, `:307`): it is a
            // link whose target couldn't be read.
            Self::Symlink(None) => {
                "it's a link, and xfx edits only a plain file; edit the file it points to by hand"
                    .into()
            }
            Self::HardLink => {
                "it has another hard link, and xfx edits only a plain file; edit it by hand".into()
            }
            // xfx's own: fx writes such a file and its next launch can't read
            // it (`readFileToEnd(.., 1024 * 1024)`, `config_file.zig:128@481104fd`).
            Self::TooLarge => {
                "the file would be over 1 MiB written out, more than xfx reads; make it smaller by hand first"
                    .into()
            }
            Self::ReadOnly => "permission denied".into(),
            Self::Busy => "another xfx is changing the MCP config; try again".into(),
            Self::Refused(why) => why.clone(),
            Self::Failed => "something went wrong".into(),
        }
    }
}

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.why())
    }
}

impl std::error::Error for EditError {}

/// The file's text with `name` added (`add`, `config_file.zig:34-44@481104fd`);
/// `text` is `None` when there is no file yet.
pub fn add_entry(
    file: File,
    text: Option<&str>,
    name: &str,
    entry: &Entry,
) -> Result<String, EditError> {
    let mut root = parse_document(text.unwrap_or(""))?;
    let Some(servers) = servers_of(file, &mut root, true)? else {
        return Err(EditError::NotAnObject);
    };
    if servers.iter().any(|(key, _)| key == name) {
        return Err(EditError::Exists);
    }
    servers.push((name.to_string(), entry_value(entry)));
    render(&root)
}

/// The file's text without `name`, or `None` when `name` isn't in it
/// (`remove`, `:58-73`).
pub fn remove_entry(file: File, text: &str, name: &str) -> Result<Option<Removed>, EditError> {
    let mut root = parse_document(text)?;
    let Some(servers) = servers_of(file, &mut root, false)? else {
        return Ok(None);
    };
    let Some(index) = servers.iter().position(|(key, _)| key == name) else {
        return Ok(None);
    };
    let (_, old) = servers.remove(index);
    let url = match old {
        Json::Object(fields) => fields.into_iter().find_map(|(key, value)| match value {
            Json::String(url) if key == "url" => Some(url),
            _ => None,
        }),
        _ => None,
    };
    Ok(Some(Removed {
        text: render(&root)?,
        url,
    }))
}

/// A config file's text for [`defines`], or `None` when there is none
/// (`read`, `config_file.zig:121-129@481104fd`): a regular file of at most
/// 1 MiB, read through a link as fx reads it -- only to answer whether a name
/// is there; [`update`] refuses to write through one.
pub fn read_config(path: &Path) -> Result<Option<String>, EditError> {
    read_bounded(path).map_err(|err| match err.kind() {
        io::ErrorKind::InvalidData => EditError::Unreadable,
        _ => EditError::Failed,
    })
}

/// Whether the file defines `name`; false for one that can't be read
/// (`has`, `:75-82`).
pub fn defines(file: File, text: &str, name: &str) -> bool {
    let Ok(mut root) = parse_document(text) else {
        return false;
    };
    matches!(servers_of(file, &mut root, false), Ok(Some(servers)) if servers.iter().any(|(key, _)| key == name))
}

/// The server list an edit changes (`serversOf`, `:182-195`): `mcpServers`,
/// or in the profile an existing `mcp` -- an old profile keeps the key it
/// has -- and with `create` a new `mcpServers` after the file's other keys.
fn servers_of(
    file: File,
    root: &mut Json,
    create: bool,
) -> Result<Option<&mut Vec<(String, Json)>>, EditError> {
    let Json::Object(top) = root else {
        return Err(EditError::NotAnObject);
    };
    let current = top.iter().position(|(key, _)| key == "mcpServers");
    let old = match file {
        File::Profile => top.iter().position(|(key, _)| key == "mcp"),
        File::Project => None,
    };
    let index = match (current, old) {
        (Some(_), Some(_)) => return Err(EditError::BothKeys),
        (Some(index), None) | (None, Some(index)) => index,
        (None, None) if create => {
            top.push(("mcpServers".to_string(), Json::Object(Vec::new())));
            top.len() - 1
        }
        (None, None) => return Ok(None),
    };
    match &mut top[index].1 {
        Json::Object(servers) => Ok(Some(servers)),
        _ => Err(EditError::NotAnObject),
    }
}

/// `entryValue` (`:197-216`).
fn entry_value(entry: &Entry) -> Json {
    let mut object: Vec<(String, Json)> = Vec::new();
    match entry {
        Entry::Stdio { command, args, env } => {
            object.push(("command".into(), Json::String(command.clone())));
            if !args.is_empty() {
                let args = args.iter().cloned().map(Json::String).collect();
                object.push(("args".into(), Json::Array(args)));
            }
            if !env.is_empty() {
                object.push(("env".into(), pairs(env)));
            }
        }
        Entry::Http { url, headers } => {
            object.push(("type".into(), Json::String("http".into())));
            object.push(("url".into(), Json::String(url.clone())));
            if !headers.is_empty() {
                object.push(("headers".into(), pairs(headers)));
            }
        }
    }
    Json::Object(object)
}

/// Why the reader would skip `entry` as `file`'s entry `name` -- in the
/// words its note gives ([`entry_problem`]) -- or `None` when it would read
/// it: the reader's own rules ([`parse_entry`]) over the JSON the writer would
/// write ([`entry_value`]), so `/mcp add` never says `Added` for an entry the
/// next load skips -- nor writes a padded `Authorization` header into a
/// repository's file that the reader then refuses. An entry that can't even be
/// written out is refused too.
///
/// A `${...}` in a **project** URL is the reader's to expand once the entry is
/// approved ([`load_workspace`]), so what an expansion can't change is checked
/// now -- the template's syntax, a literal `http` or `https` scheme, and no
/// literal `@` or `#` anywhere after `://` ([`template_url_holds`]: where the
/// authority ends is the expansion's to say) -- and the rest once it is
/// expanded. A profile's entries are never expanded, so its URL is checked as
/// written.
pub fn unreadable(file: File, name: &str, entry: &Entry) -> Option<&'static str> {
    let cannot = || Some(entry_problem(None));
    let Ok(text) = render(&Json::Object(vec![(name.to_string(), entry_value(entry))])) else {
        return cannot();
    };
    let Some(Value::Object(mut wrapped)) = read_json(&text) else {
        return cannot();
    };
    let Some(mut value) = wrapped.remove(name) else {
        return cannot();
    };
    if file == File::Project {
        if let Some(Value::String(url)) = value.get_mut("url") {
            if url.contains("${") {
                if expand_text(url, "url", &|_| Some(String::new())).is_err() {
                    return Some(entry_problem(Some(ConfigError(VARIABLE_PROBLEM))));
                }
                if !template_url_holds(url) {
                    return Some(entry_problem(Some(ConfigError("McpConfigInvalidUrl"))));
                }
                *url = "https://expanded.invalid/".to_string();
            }
        }
    }
    let (source, admission) = match file {
        File::Profile => (Source::Profile, None),
        File::Project => (Source::Workspace, Some(Admission::Pending)),
    };
    parse_entry(name, &value, source, admission)
        .err()
        .map(|err| entry_problem(Some(err)))
}

/// What of [`validate_endpoint`]'s rule a URL with a `${...}` in it already
/// keeps, whatever the expansion: its scheme is a literal `http` or `https`,
/// and nothing after `://` has a literal `@` or `#`. **Anywhere**, not in the
/// authority alone: where the authority ends is the expansion's to say -- a
/// `/` or `?` inside a default (`${A:-/}user:pw@host`) is no end of it -- so
/// a template with userinfo text anywhere is refused before it is written
/// into the repository's file. An `@` in a templated path is refused with it,
/// which fx would write. A plain `http` address's loopback host and port are
/// the expansion's to say.
fn template_url_holds(url: &str) -> bool {
    let Some((scheme, rest)) = url.split_once("://") else {
        return false;
    };
    (scheme.eq_ignore_ascii_case("https") || scheme.eq_ignore_ascii_case("http"))
        && !rest.contains('@')
        && !rest.contains('#')
}

/// The workspace's `.mcp.json` entry `name` as the reader reads it -- a
/// regular file of at most 1 MiB, strict JSON, a valid server name -- or
/// `None`: what an approval of that name is for ([`entry_digest`]), whether
/// or not it parses before it is expanded.
pub fn project_entry(workspace: &Path, name: &str) -> Option<Value> {
    if !is_valid_server_name(name) {
        return None;
    }
    let text = read_bounded(&project_path(workspace)).ok()??;
    read_json(&text)?.get("mcpServers")?.get(name).cloned()
}

/// `pairs` (`:218-222`): a key given twice keeps its first place and its
/// last value, as `put` does.
fn pairs(list: &[(String, String)]) -> Json {
    let mut object: Vec<(String, Json)> = Vec::new();
    for (key, value) in list {
        match object.iter_mut().find(|(seen, _)| seen == key) {
            Some(field) => field.1 = Json::String(value.clone()),
            None => object.push((key.clone(), Json::String(value.clone()))),
        }
    }
    Json::Object(object)
}

/// Applies `change` to the file at `path` (`update`,
/// `config_file.zig:96-119@481104fd`), under the profile's `mcp.lock` beside
/// `profile_path`, which every MCP config write holds, so two xfx processes
/// can't lose each other's change. The file is read once, edited, and
/// replaced whole or not at all: on any error nothing is written. A link,
/// or a file with another hard link, is refused before it is read; an edit
/// that would leave a file over 1 MiB is refused too ([`EditError::TooLarge`]).
pub fn update(
    profile_path: &Path,
    file: File,
    path: &Path,
    change: &Change,
) -> Result<Updated, EditError> {
    let dir = profile_path.parent().ok_or(EditError::Failed)?;
    // The profile home the lock is in, made private or refused, never
    // repaired ([`crate::profile_home::ensure`]); upstream opens it verified
    // private for the lock the same way (`profileDir`,
    // `config_file.zig:147-155@481104fd`), whichever file the edit is of.
    crate::profile_home::ensure(dir, dir).map_err(|err| {
        if crate::profile_home::Refused::of(&err).is_some() || err.kind() == io::ErrorKind::NotFound
        {
            EditError::Refused(err.to_string())
        } else {
            EditError::Failed
        }
    })?;
    let _lock = ProfileLock::acquire(&dir.join(PROFILE_LOCK)).map_err(|err| match err.0 {
        "McpConfigLockTimeout" => EditError::Busy,
        _ => EditError::Failed,
    })?;
    let read = read_plain(path)?;
    match change {
        Change::Add { name, entry } => {
            let text = read.as_ref().map(|plain| plain.text.as_str());
            let next = add_entry(file, text, name, entry)?;
            replace(file, path, read.as_ref(), &next)?;
            Ok(Updated::Added)
        }
        Change::Remove(name) => {
            let Some(plain) = &read else {
                return Ok(Updated::Missing);
            };
            let Some(removed) = remove_entry(file, &plain.text, name)? else {
                return Ok(Updated::Missing);
            };
            replace(file, path, Some(plain), &removed.text)?;
            Ok(Updated::Removed { url: removed.url })
        }
    }
}

/// [`EditError::Symlink`] for a symbolic link, [`EditError::HardLink`] for a
/// regular file with another name.
fn refuse_link(path: &Path, metadata: &fs::Metadata) -> Result<(), EditError> {
    if metadata.file_type().is_symlink() {
        let target = fs::read_link(path)
            .ok()
            .map(|target| target.to_string_lossy().into_owned());
        return Err(EditError::Symlink(target));
    }
    // A directory's own links are its entries', not other names for it.
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.is_file() && metadata.nlink() > 1 {
            return Err(EditError::HardLink);
        }
    }
    Ok(())
}

/// A file an edit read: its text, and the permission bits of the descriptor
/// it was read through.
struct Plain {
    text: String,
    mode: u32,
}

/// The file an edit replaces, or `None` when there is none: never a link or
/// a file with another hard link, opened without following one and without
/// blocking, checked to be the regular file looked at, at most 1 MiB
/// (`read`, `config_file.zig:121-129@481104fd`).
fn read_plain(path: &Path) -> Result<Option<Plain>, EditError> {
    let looked = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(EditError::Failed),
    };
    refuse_link(path, &looked)?;
    if !looked.is_file() {
        return Err(EditError::Failed);
    }
    let mut options = fs::OpenOptions::new();
    options.read(true);
    // Non-blocking too: a FIFO swapped in after the look must not hold the
    // edit open (it is then not the file looked at, and refused).
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_NOCTTY);
    }
    let opened = options.open(path).map_err(|_| EditError::Failed)?;
    let metadata = opened.metadata().map_err(|_| EditError::Failed)?;
    if !metadata.is_file() {
        return Err(EditError::Failed);
    }
    refuse_link(path, &metadata)?;
    #[cfg(unix)]
    let mode = {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if (metadata.dev(), metadata.ino()) != (looked.dev(), looked.ino()) {
            return Err(EditError::Failed);
        }
        metadata.permissions().mode() & 0o777
    };
    #[cfg(not(unix))]
    let mode = if metadata.permissions().readonly() {
        0o444
    } else {
        0o644
    };
    let bytes = read_capped(opened).map_err(|_| EditError::Failed)?;
    // Text that isn't UTF-8 isn't JSON (`std.json`'s scanner checks it).
    let text = String::from_utf8(bytes).map_err(|_| EditError::Unreadable)?;
    Ok(Some(Plain { text, mode }))
}

/// The profile's permission bits, whatever they were: owner-only.
const PRIVATE_FILE_MODE: u32 = 0o600;

/// Replaces the file with `text` (`write`, `config_file.zig:157-171@481104fd`):
/// checked once more not to be a link; the profile owner-only, as every
/// profile write is; a project's `.mcp.json`, which the repository shares,
/// keeping the mode `read` had -- one with no write permission refused, as
/// fx refuses it (`io.zig:595-598@481104fd`) -- and a new one born at the
/// default mode less the umask, as fx's is.
fn replace(file: File, path: &Path, read: Option<&Plain>, text: &str) -> Result<(), EditError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => refuse_link(path, &metadata)?,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {}
        Err(_) => return Err(EditError::Failed),
    }
    let mode = match (file, read) {
        (File::Profile, _) => Some(PRIVATE_FILE_MODE),
        (File::Project, Some(plain)) if plain.mode & 0o222 == 0 => return Err(EditError::ReadOnly),
        (File::Project, Some(plain)) => Some(plain.mode),
        (File::Project, None) => None,
    };
    replace_staged(path, text.as_bytes(), mode)
}

/// A staged file that removes itself unless it was renamed into place: an
/// edit that fails leaves no part of a file behind. Made only once the stage
/// exists, so it never removes a name this edit did not create.
struct Stage {
    path: PathBuf,
    committed: bool,
}

impl Drop for Stage {
    fn drop(&mut self) {
        if !self.committed {
            // Only this edit's own, uniquely named stage.
            let _ = fs::remove_file(&self.path);
        }
    }
}

/// A step of [`replace_staged`] after its stage exists, which its test can
/// make fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StageStep {
    Write,
    Sync,
    Rename,
}

#[cfg(test)]
thread_local! {
    /// The step [`replace_staged`] fails at on this thread, in its test.
    static FAIL_AT: std::cell::Cell<Option<StageStep>> = const { std::cell::Cell::new(None) };
}

/// `Err` at the step a test armed; nothing in a build that is not a test.
fn stage_step(step: StageStep) -> Result<(), EditError> {
    #[cfg(test)]
    if FAIL_AT.with(std::cell::Cell::get) == Some(step) {
        return Err(EditError::Failed);
    }
    let _ = step;
    Ok(())
}

/// `writeFileAtomic` (`src/core/shared/io.zig:593-615@481104fd`): a stage
/// beside the file -- `create_new`, never through a name this edit did not
/// create -- born no wider than `mode` (the default mode less the umask for a
/// new file) and given exactly `mode` before any byte is written, synced, and
/// renamed over the file, so a reader sees the old file or the new one and
/// never a part. Once the rename has landed the edit is made: flushing the
/// directory is best effort, as fx does not flush it at all.
fn replace_staged(path: &Path, bytes: &[u8], mode: Option<u32>) -> Result<(), EditError> {
    let dir = path
        .parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .ok_or(EditError::Failed)?;
    let name = path.file_name().ok_or(EditError::Failed)?;
    let nonce: String = crate::session::new_identifier().chars().take(16).collect();
    let stage_path = dir.join(format!(
        "{}.{}.{nonce}{}",
        name.to_string_lossy(),
        std::process::id(),
        crate::session::STAGE_SUFFIX
    ));
    replace_through(path, dir, stage_path, bytes, mode)
}

/// [`replace_staged`] through the stage named `stage_path`.
fn replace_through(
    path: &Path,
    dir: &Path,
    stage_path: PathBuf,
    bytes: &[u8],
    mode: Option<u32>,
) -> Result<(), EditError> {
    use std::io::Write as _;
    let failed = |_| EditError::Failed;
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(mode.unwrap_or(0o666))
            .custom_flags(libc::O_NOFOLLOW);
    }
    let mut staged = options.open(&stage_path).map_err(failed)?;
    let mut stage = Stage {
        path: stage_path,
        committed: false,
    };
    // The umask may have narrowed it; widening a stage that holds nothing yet
    // to the file's own bits shows nothing.
    #[cfg(unix)]
    if let Some(mode) = mode {
        use std::os::unix::fs::PermissionsExt;
        staged
            .set_permissions(fs::Permissions::from_mode(mode))
            .map_err(failed)?;
    }
    #[cfg(not(unix))]
    let _ = mode;
    stage_step(StageStep::Write)?;
    staged.write_all(bytes).map_err(failed)?;
    stage_step(StageStep::Sync)?;
    staged.sync_all().map_err(failed)?;
    drop(staged);
    stage_step(StageStep::Rename)?;
    fs::rename(&stage.path, path).map_err(failed)?;
    stage.committed = true;
    #[cfg(unix)]
    if let Ok(directory) = fs::File::open(dir) {
        let _ = directory.sync_all();
    }
    Ok(())
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
    /// `/mcp approve NAME` of a server the runtime holds. `/mcp`'s approval
    /// screen and `approve NAME` of an entry that parses only once expanded
    /// (`"url": "${URL}"`, which has no server to hold) send
    /// [`Self::ApproveShown`] instead.
    Approve(String),
    /// Approve the entry `/mcp`'s approval screen showed, whose
    /// [`entry_digest`] this is: if the file changed after it was drawn, the
    /// approval is for an entry no longer there, and the new one waits.
    ApproveShown {
        name: String,
        digest: String,
    },
    Reject(String),
    /// Approve every entry the runtime last read as a pending server -- the
    /// servers there now, not ones added later, as fx v0.0.16 does
    /// (`CHANGELOG.md:165@481104fd`, `src/core/mcp_host/verbs.zig:641-655@481104fd`).
    ApproveAll,
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
    // A whole file refused is one note, in fx's words
    // (`parseWorkspaceJsonInternal`, `project_config.zig:543-559@481104fd`;
    // `invalidResult`, `workspace_config.zig:57-62@481104fd`).
    let refused = |issue: String| WorkspaceServers {
        issues: vec![issue],
        ..WorkspaceServers::default()
    };
    let text = match read_bounded(&path) {
        Ok(Some(text)) => text,
        Ok(None) => return WorkspaceServers::default(),
        Err(_) => return refused(skipped(Source::Workspace, "unknown", None)),
    };
    let Some(document) = read_json(&text) else {
        return refused(whole_file("invalid_json"));
    };
    let Value::Object(document) = document else {
        return refused(whole_file("root_must_be_object"));
    };
    let Some(servers) = document.get("mcpServers") else {
        return WorkspaceServers::default();
    };
    let Some(servers) = servers.as_object() else {
        return refused(whole_file("servers_must_be_object"));
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
            // fx skips only a name a terminal can't show (`.name`); xfx's
            // names are the ones `/mcp add` takes, since a tool is named by
            // its server's (`mcp_<server>_<tool>`), and the rest are notes in
            // fx's general words.
            let problem = (!printable(name)).then_some(ConfigError(NAME_PROBLEM));
            result
                .issues
                .push(skipped(Source::Workspace, name, problem));
            continue;
        }
        let digest = entry_digest(value);
        let admission = choices.admission(name, &digest);
        result.digests.push((name.clone(), digest));
        let raw = value;
        let value = if admission == Admission::Approved {
            match expand_entry(value, &lookup) {
                Ok(value) => value,
                Err(unresolved) => {
                    result
                        .issues
                        .push(unresolved.issue(Source::Workspace, name));
                    continue;
                }
            }
        } else {
            value.clone()
        };
        match parse_entry(name, &value, Source::Workspace, Some(admission)) {
            // What a screen shows of an approved one is its file's text,
            // `${VAR}` and all: the values are for the server (EPIC2-INT1).
            Ok(mut config) if admission == Admission::Approved => {
                config.written = Some(Written::of(raw));
                match withheld_source(&config) {
                    Some((field, variable)) => result.issues.push(withheld(name, field, variable)),
                    None => result.servers.push(config),
                }
            }
            Ok(config) => match withheld_source(&config) {
                Some((field, variable)) => result.issues.push(withheld(name, field, variable)),
                None => result.servers.push(config),
            },
            Err(err) => result
                .issues
                .push(skipped(Source::Workspace, name, Some(err))),
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
        ".mcp.json server '{}' field {field} uses '{}', which xfx withholds \
         from project servers; a server in ~/.xfx/mcp.json may use it.",
        note_name(server),
        spelled(variable, MAX_RENDERED_VARIABLE)
    )
}

/// The most of a variable's name a sentence carries
/// (`max_rendered_environment_name_bytes`, `project_config.zig:662@481104fd`).
const MAX_RENDERED_VARIABLE: usize = 128;

/// Why a server's `${...}` could not be expanded: an approved workspace
/// server's, or a profile server's. The server is dropped, as upstream drops
/// it (`expandConfigs`, `project_config.zig:607-660@481104fd`).
enum Unresolved {
    /// A plain name with no value and no default, or one of xfx's
    /// credentials, which it never gets.
    Missing {
        /// Upstream's name for the field (`WorkspaceEnvironmentField`, `:126-131`).
        field: &'static str,
        variable: String,
    },
    /// Not a `${NAME}` or `${NAME:-default}` with a plain name, or a `${`
    /// never closed (`expandWorkspaceTemplate`'s `.invalid`, `:603-612`).
    /// Nothing is looked up for it: the lookup is `getenv`, and macOS's
    /// answers `NAME=…` with `NAME`'s value, past the exact-name credential
    /// guard.
    Invalid,
    /// A profile server's expansion past fx's cap: one value over 1 MiB, or
    /// every profile server's together (`.limit_exceeded`, `:735-749`). Only
    /// the profile's expansion is capped ([`expand_profile_server`]).
    Limit,
}

impl Unresolved {
    /// What `/mcp` says about the server called `server` in `source`'s file
    /// (`renderWorkspaceDiagnostic`, `project_config.zig:161-215@481104fd`),
    /// the server's name and the variable's spelled and cut as fx cuts them,
    /// and never a value.
    fn issue(&self, source: Source, server: &str) -> String {
        let (field, variable) = match self {
            Self::Missing { field, variable } => (field, variable),
            Self::Invalid => return skipped(source, server, Some(ConfigError(VARIABLE_PROBLEM))),
            Self::Limit => return skipped(source, server, Some(ConfigError(LIMIT_PROBLEM))),
        };
        // A profile server may name xfx's credentials: the person wrote it.
        if source == Source::Workspace && is_credential_variable(variable) {
            return withheld(server, field, variable);
        }
        let variable = spelled(variable, MAX_RENDERED_VARIABLE);
        format!(
            "{} server '{}' field {field} requires environment variable '{variable}'; \
             set it or use ${{{variable}:-default}}.",
            source.file(),
            note_name(server)
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
        let end = after.find('}').ok_or(Unresolved::Invalid)?;
        let inner = &after[..end];
        let (name, default) = match inner.split_once(":-") {
            Some((name, default)) => (name, Some(default)),
            None => (inner, None),
        };
        if !is_env_name(name) {
            return Err(Unresolved::Invalid);
        }
        match (
            environment(name).filter(|value| !value.is_empty() || default.is_none()),
            default,
        ) {
            (Some(value), _) => out.push_str(&value),
            (None, Some(default)) => out.push_str(default),
            (None, None) => {
                return Err(Unresolved::Missing {
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

// ---------------------------------------------------------------------------
// before the servers start
// ---------------------------------------------------------------------------

/// What fx v0.0.16 does to the servers both files gave, the workspace's
/// already behind the profile's, before it starts any (`loadHost`,
/// `src/builtins/mcp.zig:121-134@481104fd`), each server it drops a note
/// after `notes`' own:
///
/// 1. **A profile server's `${VAR}` and `${VAR:-default}` are expanded**
///    from `environment` (`expandProfileConfigs`,
///    `src/core/mcp/project_config.zig:598-605@481104fd`), in its command,
///    each argument, each `env` value and each header value, and nowhere
///    else ([`expand_profile_server`]). One left without a value, a `${...}`
///    that isn't a variable, a header the expansion made invalid, or
///    expansion past fx's cap drops the server. Only a profile server: a
///    workspace server here is one [`load_workspace`] already expanded, once
///    approved and never with xfx's credentials, and expanding what that
///    produced would read a variable the repository named in a value.
/// 2. **A server whose name a terminal can't show as is is dropped**
///    (`skipUnprintableNames`, `src/builtins/mcp.zig:99-116@481104fd`), so it
///    never reaches a row or the model; its note spells the name.
///
/// The profile on disk is never what changes: a profile server here carries
/// the values its variables had, which go to the server it starts or
/// reaches -- no note, row or error of xfx's names one; what the server
/// prints to its stderr, quoted in its failure as it wrote it, is its own.
pub fn prepare_to_start(
    servers: &mut Vec<ServerConfig>,
    notes: &mut Vec<String>,
    environment: &dyn Fn(&str) -> Option<String>,
) {
    let mut budget = ExpansionBudget {
        remaining: MAX_EXPANDED_BYTES,
    };
    servers.retain_mut(|server| {
        if server.source != Source::Profile {
            return true;
        }
        match expand_profile_server(server, environment, &mut budget) {
            Ok(()) => true,
            Err(unresolved) => {
                notes.push(unresolved.issue(Source::Profile, &server.name));
                false
            }
        }
    });
    servers.retain(|server| {
        if printable(&server.name) {
            return true;
        }
        notes.push(skipped(
            server.source,
            &server.name,
            Some(ConfigError(NAME_PROBLEM)),
        ));
        false
    });
}

/// The most bytes one expanded value may come to, and every profile server's
/// together (`max_expanded_workspace_value_bytes`,
/// `max_expanded_workspace_total_bytes`, `project_config.zig:663-664@481104fd`).
const MAX_EXPANDED_BYTES: usize = 1024 * 1024;

/// What is left of [`MAX_EXPANDED_BYTES`] for one [`prepare_to_start`]
/// (`WorkspaceExpansionBudget`, `project_config.zig:666-681@481104fd`): every
/// byte an expansion writes counts, the text around a `${...}` and a value
/// with no `${` in it included, and a server dropped part way through keeps
/// what it used.
struct ExpansionBudget {
    remaining: usize,
}

impl ExpansionBudget {
    /// `appendExpandedBytes` (`:735-749@481104fd`): `bytes` onto `out`, or
    /// [`Unresolved::Limit`] when `out` would pass the cap for one value, or
    /// `bytes` what is left -- which then leaves nothing for any server after.
    fn append(&mut self, out: &mut String, bytes: &str) -> Result<(), Unresolved> {
        if out.len() > MAX_EXPANDED_BYTES || bytes.len() > MAX_EXPANDED_BYTES - out.len() {
            return Err(Unresolved::Limit);
        }
        if bytes.len() > self.remaining {
            self.remaining = 0;
            return Err(Unresolved::Limit);
        }
        self.remaining -= bytes.len();
        out.push_str(bytes);
        Ok(())
    }
}

/// One profile server's expansion, fx's fields in fx's order, the first that
/// fails dropping it (`expandWorkspaceConfig`, `project_config.zig:760-788@481104fd`):
/// a stdio server's command (`command`), each argument (`argument`), each
/// `env` value whatever the transport (`environment`), each header value
/// (`http_header`) -- and then the headers held to the rules a written one is
/// ([`valid_static_headers`]), so a value that expands to a line break is
/// `a ${...} in it isn't valid` rather than a header of its own. The URL,
/// `header_env`, `bearer_token_env`, `oauth` and every name are as written.
fn expand_profile_server(
    server: &mut ServerConfig,
    environment: &dyn Fn(&str) -> Option<String>,
    budget: &mut ExpansionBudget,
) -> Result<(), Unresolved> {
    server.written = Some(Written {
        command: server.command.clone(),
        args: server.args.clone(),
        url: server.url.clone(),
    });
    let mut expand = |value: &mut String, field: &'static str| {
        *value = expand_template(value, field, environment, budget)?;
        Ok(())
    };
    if server.transport == Transport::Stdio {
        expand(&mut server.command, "command")?;
    }
    for argument in &mut server.args {
        expand(argument, "argument")?;
    }
    for (_, value) in &mut server.env {
        expand(value, "environment")?;
    }
    for (_, value) in &mut server.headers {
        expand(value, "http_header")?;
    }
    if valid_static_headers(&server.headers) {
        Ok(())
    } else {
        Err(Unresolved::Invalid)
    }
}

/// `${VAR}` and `${VAR:-default}` in `text` (`expandWorkspaceTemplate`,
/// `project_config.zig:698-733@481104fd`): the first `}` ends the expression,
/// the name is what comes before its first `:-`, and a name that isn't a
/// plain variable's is [`Unresolved::Invalid`] without a lookup. A variable
/// that is set is its value, an empty one too; one that isn't is the default
/// when there is one. Once through: a value or a default is written in as it
/// is, never read for a `${` of its own. Not [`expand_text`], the
/// workspace's, where an empty variable takes the default and nothing is
/// capped (`docs/parity.md`).
fn expand_template(
    text: &str,
    field: &'static str,
    environment: &dyn Fn(&str) -> Option<String>,
    budget: &mut ExpansionBudget,
) -> Result<String, Unresolved> {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("${") {
        budget.append(&mut out, &rest[..start])?;
        let after = &rest[start + 2..];
        let end = after.find('}').ok_or(Unresolved::Invalid)?;
        let expression = &after[..end];
        let (name, default) = match expression.split_once(":-") {
            Some((name, default)) => (name, Some(default)),
            None => (expression, None),
        };
        if !is_env_name(name) {
            return Err(Unresolved::Invalid);
        }
        let value = match (environment(name), default) {
            (Some(value), _) => value,
            (None, Some(default)) => default.to_string(),
            (None, None) => {
                return Err(Unresolved::Missing {
                    field,
                    variable: name.to_string(),
                })
            }
        };
        budget.append(&mut out, &value)?;
        rest = &after[end + 1..];
    }
    budget.append(&mut out, rest)?;
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
        if next == current {
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
                r#"{"mcp":{"a":{"command":"x","url":"https://example.test/mcp"}}}"#,
                "McpConfigUrlWithoutType",
            ),
            (
                r#"{"mcp":{"a":{"command":"x"},"a":{"command":"y"}}}"#,
                "McpConfigInvalidJson",
            ),
            (
                r#"{"mcp":{"a":{"type":"http","url":"http://example.test/mcp"}}}"#,
                "McpConfigInvalidUrl",
            ),
        ] {
            assert_eq!(
                parse_profile_strictly(text).err(),
                Some(ConfigError(expected)),
                "{text}"
            );
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
                parse_profile_strictly(&remote(fields)).err(),
                Some(ConfigError("McpConfigInvalidHeaders")),
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
        // An approval of what a screen showed is for that, whatever was read since.
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
        let dir = crate::profile_home::temp_home();
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

    // -----------------------------------------------------------------------
    // fx v0.0.16's writer (`src/core/mcp_host/config_file.zig@481104fd`)
    // -----------------------------------------------------------------------

    /// What fx v0.0.16 left on disk in a capture, as the receipt keeps it
    /// (`.prd/tui-phase5/receipts/mcp-config/oracle/`): the bytes its `home`
    /// or `workls` printed, the machine's `bun` and scenario directory written
    /// as `<bun>` and `<scenarios>`.
    macro_rules! oracle {
        ($name:literal) => {
            include_str!(concat!(
                "../../.prd/tui-phase5/receipts/mcp-config/oracle/",
                $name
            ))
        };
    }

    /// A scenario file with its placeholders written as the oracle writes them.
    fn placeholders(text: &str) -> String {
        text.replace("@BUN@", "<bun>")
            .replace("@SCENARIOS@", "<scenarios>")
    }

    const SCRIPT: &str = "<scenarios>/p5-mcp-stdio.ts";

    fn stdio(args: &[&str], env: &[(&str, &str)]) -> Entry {
        Entry::Stdio {
            command: "<bun>".into(),
            args: args.iter().map(|arg| arg.to_string()).collect(),
            env: env
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect(),
        }
    }

    fn http(url: &str, headers: &[(&str, &str)]) -> Entry {
        Entry::Http {
            url: url.into(),
            headers: headers
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect(),
        }
    }

    #[test]
    fn the_verbs_scenario_leaves_fx_v0_0_16s_bytes() {
        // `p5-mcp-verbs`: the seeded `mcpServers` profile, `add two -- …`,
        // `add web2 URL`, then `remove two`, `remove three`, `remove web2`.
        let before = placeholders(include_str!(
            "../../scripts/fxcap/scenarios/p5-mcp-profile.json"
        ));
        let two = add_entry(File::Profile, Some(&before), "two", &stdio(&[SCRIPT], &[])).unwrap();
        let after = add_entry(
            File::Profile,
            Some(&two),
            "web2",
            &http("http://127.0.0.1:9/web2", &[]),
        )
        .unwrap();
        assert_eq!(after, oracle!("verbs-after-add.mcp.json"));
        let removed = remove_entry(File::Profile, &after, "two").unwrap().unwrap();
        assert_eq!(removed.url, None);
        assert_eq!(
            remove_entry(File::Profile, &removed.text, "three"),
            Ok(None)
        );
        let end = remove_entry(File::Profile, &removed.text, "web2")
            .unwrap()
            .unwrap();
        assert_eq!(end.url.as_deref(), Some("http://127.0.0.1:9/web2"));
        assert_eq!(end.text, oracle!("verbs-end.mcp.json"));
    }

    #[test]
    fn an_old_profile_keeps_its_mcp_key_and_every_other_byte_fx_keeps() {
        // `p5-mcp-config`: an old `{"mcp":…}` profile with keys, numbers and
        // escapes fx doesn't know -- the migration case.
        let before = oracle!("config-before.mcp.json");
        assert_eq!(
            before,
            placeholders(include_str!(
                "../../scripts/fxcap/scenarios/p5-mcp-config-profile.json"
            ))
        );
        let mut text = before.to_string();
        for (name, entry) in [
            ("two", stdio(&[SCRIPT], &[])),
            (
                "three",
                stdio(&[SCRIPT, "--flag"], &[("K", "v"), ("Z", "1")]),
            ),
            ("web", http("http://127.0.0.1:9/web", &[("X-Team", "core")])),
        ] {
            text = add_entry(File::Profile, Some(&text), name, &entry).unwrap();
        }
        assert_eq!(text, oracle!("config-after-add.mcp.json"));
        // The reader takes a Claude-shape entry under `mcp`, as fx v0.0.16
        // does (its `/mcp list` showed `two` ready, `p5-mcp-config-…-list`).
        let profile = parse_profile(&text).expect("reads");
        assert!(profile.notes.is_empty(), "{:?}", profile.notes);
        let names: Vec<&str> = profile.servers.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["old", "two", "three", "web"]);
        assert_eq!(
            (
                profile.servers[1].command.as_str(),
                &profile.servers[1].args
            ),
            ("<bun>", &vec![SCRIPT.to_string()])
        );
        assert_eq!(
            profile.servers[2].env,
            [("K".into(), "v".into()), ("Z".into(), "1".into())]
        );
        assert_eq!(
            profile.servers[3].headers,
            [("X-Team".into(), "core".into())]
        );
        for name in ["old", "three", "web"] {
            text = remove_entry(File::Profile, &text, name)
                .unwrap()
                .unwrap()
                .text;
        }
        assert_eq!(text, oracle!("config-end.mcp.json"));
    }

    #[test]
    fn a_project_file_is_edited_as_fx_edits_it() {
        let before = oracle!("config-work-before.mcp.json");
        let after = add_entry(File::Project, Some(before), "p2", &stdio(&[SCRIPT], &[])).unwrap();
        assert_eq!(after, oracle!("config-work-after-add.mcp.json"));
        let end = remove_entry(File::Project, &after, "p2").unwrap().unwrap();
        assert_eq!(end.text, oracle!("config-work-end.mcp.json"));
    }

    #[test]
    fn add_keeps_every_key_it_doesnt_know_in_order_with_numbers_as_written() {
        // fx's own case (`config_file.zig:234-267@481104fd`).
        let before = r#"{"theme":"dark","mcpServers":{"a":{"command":"x","environment":{"K":"v"},"startup_timeout_ms":1.50,"extra":[1,2]}},"zz":true}"#;
        let after = add_entry(
            File::Profile,
            Some(before),
            "web",
            &http("https://mcp.example/mcp", &[("X-Key", "${KEY}")]),
        )
        .unwrap();
        assert_eq!(
            after,
            "{\n  \"theme\": \"dark\",\n  \"mcpServers\": {\n    \"a\": {\n      \"command\": \"x\",\n      \"environment\": {\n        \"K\": \"v\"\n      },\n      \"startup_timeout_ms\": 1.50,\n      \"extra\": [\n        1,\n        2\n      ]\n    },\n    \"web\": {\n      \"type\": \"http\",\n      \"url\": \"https://mcp.example/mcp\",\n      \"headers\": {\n        \"X-Key\": \"${KEY}\"\n      }\n    }\n  },\n  \"zz\": true\n}\n"
        );
    }

    #[test]
    fn add_writes_a_stdio_server_into_a_new_file_under_mcp_servers() {
        // `config_file.zig:269-289@481104fd`.
        let after = add_entry(
            File::Project,
            None,
            "local",
            &Entry::Stdio {
                command: "node".into(),
                args: vec!["server.js".into(), "--x".into()],
                env: vec![("A".into(), "1".into())],
            },
        )
        .unwrap();
        assert_eq!(
            after,
            "{\n  \"mcpServers\": {\n    \"local\": {\n      \"command\": \"node\",\n      \"args\": [\n        \"server.js\",\n        \"--x\"\n      ],\n      \"env\": {\n        \"A\": \"1\"\n      }\n    }\n  }\n}\n"
        );
    }

    #[test]
    fn an_older_profile_keeps_its_mcp_key_and_a_project_file_never_uses_it() {
        // `config_file.zig:291-303@481104fd`.
        let old = r#"{"mcp":{"a":{"command":"x"}}}"#;
        let y = Entry::Stdio {
            command: "y".into(),
            args: vec![],
            env: vec![],
        };
        let profile = add_entry(File::Profile, Some(old), "b", &y).unwrap();
        assert!(!profile.contains("\"mcpServers\""), "{profile}");
        assert!(defines(File::Profile, &profile, "b"));
        let project = add_entry(File::Project, Some(old), "b", &y).unwrap();
        assert!(defines(File::Project, &project, "b"));
        assert!(!defines(File::Project, &project, "a"));
        assert!(project.contains("\"mcp\": {"), "{project}");
    }

    #[test]
    fn add_refuses_what_it_cant_edit_safely() {
        // `config_file.zig:305-317@481104fd`, and what this reader adds.
        let y = Entry::Stdio {
            command: "y".into(),
            args: vec![],
            env: vec![],
        };
        for (text, expected) in [
            (r#"{"mcpServers":{"b":{}}}"#, EditError::Exists),
            (r#"{"mcp":{},"mcpServers":{}}"#, EditError::BothKeys),
            (
                r#"{"mcpServers":{},"mcpServers":{}}"#,
                EditError::Unreadable,
            ),
            (
                r#"{"mcpServers":{"a":{"k":1,"k":2}}}"#,
                EditError::Unreadable,
            ),
            ("{nope", EditError::Unreadable),
            ("[]", EditError::NotAnObject),
            (r#"{"mcpServers":[]}"#, EditError::NotAnObject),
            (r#"{"a":01}"#, EditError::Unreadable),
            (r#"{"a":1,}"#, EditError::Unreadable),
            (r#"{"a":1.}"#, EditError::Unreadable),
            (r#"{"a":"\ud800"}"#, EditError::Unreadable),
            (r#"{"a":"\udc00"}"#, EditError::Unreadable),
            ("{\"a\":\"\u{1}\"}", EditError::Unreadable),
            ("\u{feff}{}", EditError::Unreadable),
            ("{} // note", EditError::Unreadable),
            ("{} {}", EditError::Unreadable),
        ] {
            assert_eq!(
                add_entry(File::Profile, Some(text), "b", &y),
                Err(expected),
                "{text}"
            );
        }
        let blank = add_entry(File::Profile, Some(" \n"), "b", &y).unwrap();
        assert!(defines(File::Profile, &blank, "b"));
    }

    #[test]
    fn remove_drops_one_entry_keeps_the_rest_in_order_and_returns_its_url() {
        // `config_file.zig:319-336@481104fd`.
        let before = r#"{"x":1,"mcpServers":{"a":{"command":"x"},"web":{"type":"http","url":"https://w/mcp"},"c":{"command":"z"}}}"#;
        let removed = remove_entry(File::Profile, before, "web").unwrap().unwrap();
        assert_eq!(removed.url.as_deref(), Some("https://w/mcp"));
        assert!(!defines(File::Profile, &removed.text, "web"));
        let a = removed.text.find("\"a\"").unwrap();
        let c = removed.text.find("\"c\"").unwrap();
        assert!(a < c);
        assert!(removed.text.contains("\"x\": 1"));
        assert_eq!(
            remove_entry(File::Profile, before, "a")
                .unwrap()
                .unwrap()
                .url,
            None
        );
        assert_eq!(remove_entry(File::Profile, before, "missing"), Ok(None));
        assert_eq!(
            remove_entry(File::Project, r#"{"mcp":{"a":{}}}"#, "a"),
            Ok(None)
        );
        assert!(!defines(File::Profile, "{nope", "a"));
    }

    #[test]
    fn strings_are_written_back_in_std_jsons_spelling() {
        // `encodeJsonStringChars` without `escape_unicode`: five controls by
        // letter, the rest `\u00xx`, `/` and anything past ASCII as they are
        // -- the capture's `"café \/ \u001f 😀 tab\t"` came
        // back `"café / \u001f 😀 tab\t"`.
        let text = r#"{"s":"\u0000\u001f\u007f\b\f\n\r\t\"\\\/é😀","kA":[{},[]]}"#;
        let root = parse_document(text).unwrap();
        assert_eq!(
            render(&root).unwrap(),
            "{\n  \"s\": \"\\u0000\\u001f\u{7f}\\b\\f\\n\\r\\t\\\"\\\\/é😀\",\n  \"kA\": [\n    {},\n    []\n  ]\n}\n"
        );
    }

    #[test]
    fn the_writer_reads_json_as_serde_json_does_and_writes_back_the_same_values() {
        // A differential check over many small documents, some well formed and
        // most not: the writer's reader takes exactly what `serde_json` takes
        // (but a key twice, which it refuses, and a blank file, which is an
        // empty object to fx), and what it writes reads back as the same value.
        const PIECES: [&str; 32] = [
            "{",
            "}",
            "[",
            "]",
            ",",
            ":",
            " ",
            "\n",
            "\"a\"",
            "\"b\"",
            "\"\\u00e9\"",
            "\"\\ud83d\\ude00\"",
            "\"\\ud800\"",
            "\"\\/\\t\"",
            "\"\u{1}\"",
            "0",
            "-0",
            "01",
            "1.5",
            "1.",
            "-",
            "1e5",
            "1E+2",
            "2e",
            "true",
            "false",
            "null",
            "nul",
            "\"é😀\"",
            "\"\\x\"",
            "\t",
            "\u{feff}",
        ];
        let mut seed: u64 = 0x2026_1106;
        let mut next = || {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (seed >> 33) as usize
        };
        let mut accepted = 0;
        for _ in 0..40_000 {
            let mut text = String::new();
            for _ in 0..(next() % 12) + 1 {
                text.push_str(PIECES[next() % PIECES.len()]);
            }
            let theirs = serde_json::from_str::<Value>(&text);
            // `serde_json` reads a number into an `f64`, so one past its range
            // is no JSON to it; `std.json`'s `.parse_numbers = false`, and
            // this reader, keep its text.
            if matches!(&theirs, Err(err) if err.to_string().contains("number out of range")) {
                continue;
            }
            match parse_document(&text) {
                Ok(ours) => {
                    if text.trim_matches([' ', '\t', '\r', '\n']).is_empty() {
                        continue;
                    }
                    let theirs = theirs.unwrap_or_else(|err| panic!("{text:?}: {err}"));
                    let rendered = render(&ours).unwrap();
                    let written: Value = serde_json::from_str(&rendered)
                        .unwrap_or_else(|err| panic!("{text:?} wrote {rendered:?}: {err}"));
                    assert_eq!(written, theirs, "{text:?}");
                    accepted += 1;
                }
                // Refused: `serde_json` refuses it too, or a key is there twice.
                Err(EditError::Unreadable) => assert!(
                    theirs.is_err() || parse_with(&text, false).is_ok(),
                    "{text:?} refused, and serde_json took it"
                ),
                Err(other) => panic!("{text:?}: {other:?}"),
            }
        }
        assert!(
            accepted > 1_000,
            "only {accepted} documents were well formed"
        );
    }

    #[test]
    fn the_reader_and_the_writer_take_the_same_depth() {
        // Arrays and objects each: the launch reader runs this one before
        // `serde_json` (`read_json`), so its cap is the only stack guard.
        let arrays = |depth: usize| format!("{}{}", "[".repeat(depth), "]".repeat(depth));
        let objects = |depth: usize| format!("{}0{}", "{\"a\":".repeat(depth), "}".repeat(depth));
        for nested in [&arrays as &dyn Fn(usize) -> String, &objects] {
            let deepest = nested(MAX_JSON_DEPTH);
            assert!(parse_document(&deepest).is_ok(), "{}", &deepest[..8]);
            assert!(serde_json::from_str::<Value>(&deepest).is_ok());
            let deeper = nested(MAX_JSON_DEPTH + 1);
            assert_eq!(parse_document(&deeper), Err(EditError::Unreadable));
            assert!(serde_json::from_str::<Value>(&deeper).is_err());
            assert!(read_json(&deeper).is_none());
            // A file a repository could write, past any stack: refused, not a
            // crash.
            let hostile = nested(200_000);
            assert_eq!(parse_document(&hostile), Err(EditError::Unreadable));
            assert!(read_json(&hostile).is_none());
        }
    }

    /// A throwaway repository under the run's TMPDIR -- never the checkout
    /// these tests are built from -- and a profile path beside it.
    fn scratch_repo() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().expect("dir");
        let checkout = fs::canonicalize(env!("CARGO_MANIFEST_DIR")).unwrap();
        let scratch = fs::canonicalize(dir.path()).unwrap();
        assert!(!scratch.starts_with(&checkout), "{scratch:?}");
        assert!(
            scratch.starts_with(fs::canonicalize(std::env::temp_dir()).unwrap()),
            "{scratch:?}"
        );
        let repo = scratch.join("repo");
        fs::create_dir_all(repo.join(".git")).unwrap();
        fs::write(repo.join(".git").join("HEAD"), "ref: refs/heads/main\n").unwrap();
        // A home that is there, as a person's is: xfx makes `~/.xfx` in it,
        // never the home itself ([`crate::profile_home`]).
        fs::create_dir(scratch.join("home")).unwrap();
        let profile = scratch.join("home").join(".xfx").join(PROFILE_FILE);
        (dir, repo, profile)
    }

    fn names_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[cfg(unix)]
    fn mode_of(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        fs::symlink_metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[cfg(unix)]
    #[test]
    fn a_project_edit_replaces_the_file_whole_and_keeps_its_mode() {
        use std::os::unix::fs::PermissionsExt;
        let (_dir, repo, profile) = scratch_repo();
        let path = project_path(&repo);
        fs::write(&path, oracle!("config-work-before.mcp.json")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o664)).unwrap();
        let add = Change::Add {
            name: "p2".into(),
            entry: stdio(&[SCRIPT], &[]),
        };
        assert_eq!(
            update(&profile, File::Project, &path, &add),
            Ok(Updated::Added)
        );
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            oracle!("config-work-after-add.mcp.json")
        );
        assert_eq!(mode_of(&path), 0o664);
        // No stage left beside it, and the profile's lock beside the profile.
        assert_eq!(names_in(&repo), [".git", ".mcp.json"]);
        assert_eq!(mode_of(profile.parent().unwrap()), 0o700);
        assert!(profile.parent().unwrap().join(PROFILE_LOCK).exists());
        assert!(!profile.exists());
        let remove = Change::Remove("p2".into());
        assert_eq!(
            update(&profile, File::Project, &path, &remove),
            Ok(Updated::Removed { url: None })
        );
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            oracle!("config-work-end.mcp.json")
        );
        assert_eq!(mode_of(&path), 0o664);
        assert_eq!(
            update(&profile, File::Project, &path, &remove),
            Ok(Updated::Missing)
        );
        // A new file: `mcpServers` alone, at the default mode less the umask.
        fs::remove_file(&path).unwrap();
        assert_eq!(
            update(&profile, File::Project, &path, &add),
            Ok(Updated::Added)
        );
        assert!(defines(
            File::Project,
            &fs::read_to_string(&path).unwrap(),
            "p2"
        ));
        assert_eq!(mode_of(&path) & 0o600, 0o600);
        assert_eq!(mode_of(&path) & 0o111, 0, "no execute bit");
        assert_eq!(names_in(&repo), [".git", ".mcp.json"]);
    }

    #[cfg(unix)]
    #[test]
    fn a_linked_config_file_is_never_edited_whether_a_symlink_or_a_hard_link() {
        // `config_file.zig:338-375@481104fd`, through `update`: refused
        // before it is read, the file it shares as it was, the link a link.
        // `outside.json` isn't JSON: an edit that read through the link would
        // say `Unreadable`, so the refusal before the read is what answers.
        let (dir, repo, profile) = scratch_repo();
        let shared = repo.join("shared.json");
        let outside = dir.path().join("outside.json");
        let original = r#"{"mcpServers":{}}"#;
        let broken = "{nope";
        fs::write(&shared, original).unwrap();
        fs::write(&outside, broken).unwrap();
        let path = project_path(&repo);
        let add = Change::Add {
            name: "p2".into(),
            entry: stdio(&[SCRIPT], &[]),
        };
        let remove = Change::Remove("p2".into());
        for (target, relative, text) in [
            (&shared, "shared.json", original),
            (&outside, "../outside.json", broken),
        ] {
            std::os::unix::fs::symlink(relative, &path).unwrap();
            for change in [&add, &remove] {
                assert_eq!(
                    update(&profile, File::Project, &path, change),
                    Err(EditError::Symlink(Some(relative.to_string())))
                );
            }
            assert!(fs::symlink_metadata(&path)
                .unwrap()
                .file_type()
                .is_symlink());
            assert_eq!(fs::read_to_string(target).unwrap(), text);
            fs::remove_file(&path).unwrap();
        }
        assert_eq!(
            EditError::Symlink(Some("shared.json".into())).why(),
            "it's a link to shared.json, and xfx edits only a plain file; edit shared.json by hand"
        );
        assert_eq!(
            EditError::Symlink(None).why(),
            "it's a link, and xfx edits only a plain file; edit the file it points to by hand"
        );
        // A link that points nowhere is a link too.
        std::os::unix::fs::symlink("missing.json", &path).unwrap();
        assert_eq!(
            update(&profile, File::Project, &path, &add),
            Err(EditError::Symlink(Some("missing.json".into())))
        );
        assert!(!repo.join("missing.json").exists());
        fs::remove_file(&path).unwrap();
        // A second name for a file that isn't JSON: refused before the read.
        fs::hard_link(&outside, &path).unwrap();
        for change in [&add, &remove] {
            assert_eq!(
                update(&profile, File::Project, &path, change),
                Err(EditError::HardLink)
            );
        }
        assert_eq!(fs::read_to_string(&outside).unwrap(), broken);
        assert_eq!(fs::read_to_string(&path).unwrap(), broken);
        fs::remove_file(&path).unwrap();
        fs::hard_link(&shared, &path).unwrap();
        assert_eq!(
            update(&profile, File::Project, &path, &add),
            Err(EditError::HardLink)
        );
        assert_eq!(fs::read_to_string(&shared).unwrap(), original);
        assert_eq!(
            EditError::HardLink.why(),
            "it has another hard link, and xfx edits only a plain file; edit it by hand"
        );
        // A target with a control character is spelled, not sent.
        assert_eq!(
            EditError::Symlink(Some("x\u{1b}]0;t\u{7}".into())).why(),
            "it's a link to x\\x1b]0;t\\x07, and xfx edits only a plain file; edit x\\x1b]0;t\\x07 by hand"
        );
        // The profile is held to the same rule.
        crate::profile_home::made(profile.parent().unwrap());
        std::os::unix::fs::symlink(&outside, &profile).unwrap();
        assert!(matches!(
            update(&profile, File::Profile, &profile, &add),
            Err(EditError::Symlink(Some(_)))
        ));
        assert_eq!(fs::read_to_string(&outside).unwrap(), broken);
        assert_eq!(names_in(&repo), [".git", ".mcp.json", "shared.json"]);
    }

    #[cfg(unix)]
    #[test]
    fn an_edit_that_fails_writes_nothing() {
        use std::os::unix::fs::PermissionsExt;
        let (_dir, repo, profile) = scratch_repo();
        let path = project_path(&repo);
        let add = Change::Add {
            name: "b".into(),
            entry: stdio(&[SCRIPT], &[]),
        };
        let deep = format!("{}{}", "[".repeat(10_000), "]".repeat(10_000));
        for (text, expected) in [
            (r#"{"mcpServers":{"b":{"command":"x"}}}"#, EditError::Exists),
            (
                r#"{"mcpServers":{},"mcpServers":{}}"#,
                EditError::Unreadable,
            ),
            ("{nope", EditError::Unreadable),
            (deep.as_str(), EditError::Unreadable),
            (r#"{"mcpServers":[]}"#, EditError::NotAnObject),
        ] {
            fs::write(&path, text).unwrap();
            assert_eq!(
                update(&profile, File::Project, &path, &add),
                Err(expected),
                "{text}"
            );
            assert_eq!(fs::read_to_string(&path).unwrap(), text);
            assert_eq!(names_in(&repo), [".git", ".mcp.json"]);
        }
        // Not UTF-8 is not JSON.
        fs::write(&path, b"{\"a\":\"\xff\"}").unwrap();
        assert_eq!(
            update(&profile, File::Project, &path, &add),
            Err(EditError::Unreadable)
        );
        // A file with no write permission is refused, as fx refuses it.
        fs::write(&path, "{}").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).unwrap();
        assert_eq!(
            update(&profile, File::Project, &path, &add),
            Err(EditError::ReadOnly)
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "{}");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        // Past 1 MiB is not read.
        let big = format!("{{\"pad\":\"{}\"}}", "x".repeat(MAX_CONFIG_BYTES as usize));
        fs::write(&path, &big).unwrap();
        assert_eq!(
            update(&profile, File::Project, &path, &add),
            Err(EditError::Failed)
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), big);
        // A directory where the file should be.
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert_eq!(
            update(&profile, File::Project, &path, &add),
            Err(EditError::Failed)
        );
        fs::remove_dir(&path).unwrap();
        // A workspace that isn't there.
        let gone = project_path(&repo.join("no-dir"));
        assert_eq!(
            update(&profile, File::Project, &gone, &add),
            Err(EditError::Failed)
        );
        assert_eq!(names_in(&repo), [".git"]);
    }

    #[cfg(unix)]
    #[test]
    fn the_profiles_first_write_creates_its_directory_readable_only_by_its_owner() {
        // `config_file.zig:377-391@481104fd`.
        let (_dir, _repo, profile) = scratch_repo();
        let add = Change::Add {
            name: "two".into(),
            entry: stdio(&[SCRIPT], &[]),
        };
        assert_eq!(
            update(&profile, File::Profile, &profile, &add),
            Ok(Updated::Added)
        );
        assert_eq!(mode_of(profile.parent().unwrap()), 0o700);
        assert_eq!(mode_of(&profile), 0o600);
        assert_eq!(
            fs::read_to_string(&profile).unwrap(),
            "{\n  \"mcpServers\": {\n    \"two\": {\n      \"command\": \"<bun>\",\n      \"args\": [\n        \"<scenarios>/p5-mcp-stdio.ts\"\n      ]\n    }\n  }\n}\n"
        );
        assert_eq!(
            names_in(profile.parent().unwrap()),
            [PROFILE_FILE, PROFILE_LOCK]
        );
        assert_eq!(
            update(&profile, File::Profile, &profile, &add),
            Err(EditError::Exists)
        );
        assert_eq!(
            update(
                &profile,
                File::Profile,
                &profile,
                &Change::Remove("two".into())
            ),
            Ok(Updated::Removed { url: None })
        );
        assert_eq!(mode_of(&profile), 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn a_profile_is_owner_only_after_an_edit_whatever_mode_it_had() {
        // The profile is the person's own and may hold credentials
        // (`headers`, `env`): an edit leaves it 0600 even when it was wider,
        // where a project's `.mcp.json` keeps its mode.
        use std::os::unix::fs::PermissionsExt;
        let (_dir, _repo, profile) = scratch_repo();
        crate::profile_home::made(profile.parent().unwrap());
        for (mode, change) in [
            (
                0o644,
                Change::Add {
                    name: "two".into(),
                    entry: stdio(&[SCRIPT], &[]),
                },
            ),
            (0o666, Change::Remove("two".into())),
        ] {
            if !profile.exists() {
                fs::write(&profile, r#"{"mcpServers":{}}"#).unwrap();
            }
            fs::set_permissions(&profile, fs::Permissions::from_mode(mode)).unwrap();
            assert!(matches!(
                update(&profile, File::Profile, &profile, &change),
                Ok(Updated::Added | Updated::Removed { .. })
            ));
            assert_eq!(mode_of(&profile), 0o600, "from {mode:o}");
        }
    }

    #[test]
    fn every_edit_error_says_why_in_fxs_words() {
        assert_eq!(
            EditError::Unreadable.why(),
            "the file isn't valid JSON, or repeats a key; fix it by hand first"
        );
        assert_eq!(
            EditError::NotAnObject.why(),
            "the file's servers aren't a JSON object; fix it by hand first"
        );
        assert_eq!(
            EditError::BothKeys.why(),
            "the file has both \"mcp\" and \"mcpServers\"; merge them into one"
        );
        assert_eq!(
            EditError::Exists.why(),
            "that name is already in the file; remove it first"
        );
        assert_eq!(EditError::ReadOnly.why(), "permission denied");
        assert_eq!(
            EditError::Busy.why(),
            "another xfx is changing the MCP config; try again"
        );
        assert_eq!(EditError::Failed.why(), "something went wrong");
        assert_eq!(File::Profile.label(), "~/.xfx/mcp.json");
        assert_eq!(File::Project.label(), ".mcp.json");
    }

    // -----------------------------------------------------------------------
    // notes (`renderWorkspaceDiagnostic`, `project_config.zig:161-215@481104fd`)
    // -----------------------------------------------------------------------

    #[test]
    fn a_profile_entry_xfx_cant_read_is_skipped_with_fxs_note() {
        // `p5-mcp-notes`: fx v0.0.16 starts with `fixture` and says, under
        // `/mcp list`, `note: ~/.fx/mcp.json server 'broken' was skipped: it
        // needs a "command", with any "args" as strings.`
        let profile = parse_profile(&placeholders(include_str!(
            "../../scripts/fxcap/scenarios/p5-mcp-notes-profile.json"
        )))
        .expect("the file reads");
        let names: Vec<&str> = profile.servers.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["fixture"]);
        assert_eq!(
            profile.notes,
            ["~/.xfx/mcp.json server 'broken' was skipped: it needs a \"command\", with any \"args\" as strings."]
        );
        // The sentence is the one fx v0.0.16 printed, read off its capture
        // with `/.fx/` read as `/.xfx/`, as `scripts/fxcap/cells.py` reads it.
        let captured = include_str!(
            "../../.prd/tui-phase5/receipts/mcp-config/cap/fx/fx-p5-mcp-notes-40x120-list.txt"
        )
        .lines()
        .find_map(|row| row.split_once("! mcp: note: "))
        .map(|(_, note)| note.trim_end().replace("/.fx/", "/.xfx/"))
        .expect("fx's note row");
        assert_eq!(profile.notes, [captured]);
        // A file that isn't a JSON object of servers is still refused whole.
        for (text, expected) in [
            ("{", "McpConfigInvalidJson"),
            (r#"{"mcp":{},"mcp":{}}"#, "McpConfigInvalidJson"),
            ("[]", "McpConfigRootMustBeObject"),
            (r#"{"mcp":[]}"#, "McpConfigServersMustBeObject"),
        ] {
            assert_eq!(parse_profile(text), Err(ConfigError(expected)), "{text}");
        }
    }

    #[test]
    fn each_reason_an_entry_is_skipped_is_fxs_sentence() {
        let note = |entry: &str| {
            let profile = parse_profile(&format!(r#"{{"mcp":{{"s":{entry}}}}}"#)).unwrap();
            assert!(profile.servers.is_empty(), "{entry}");
            let prefix = "~/.xfx/mcp.json server 's' was skipped: ";
            let [note] = profile.notes.as_slice() else {
                panic!("{entry}: {:?}", profile.notes)
            };
            note.strip_prefix(prefix).unwrap().to_string()
        };
        for (entry, why) in [
            ("1", "it isn't a JSON object."),
            (
                r#"{"command":"x","url":"https://a.test/mcp"}"#,
                "it has a \"url\" but no \"type\"; add \"type\": \"http\".",
            ),
            (
                r#"{"command":3}"#,
                "it needs a \"command\", with any \"args\" as strings.",
            ),
            (r#"{"type":"ws"}"#, "its \"type\" isn't valid."),
            (r#"{"type":"http"}"#, "its \"url\" isn't valid."),
            (
                r#"{"type":"http","url":"http://a.test/mcp"}"#,
                "its \"url\" isn't valid.",
            ),
            (
                r#"{"type":"http","url":"https://a.test/mcp","headers":{"Host":"h"}}"#,
                "its \"headers\" isn't valid.",
            ),
            (
                r#"{"type":"http","url":"https://a.test/mcp","oauth":1}"#,
                "its \"oauth\" isn't valid.",
            ),
            (
                r#"{"command":"x","env":{"1":"v"}}"#,
                "its \"env\" isn't valid.",
            ),
            (
                r#"{"type":"http","url":"https://a.test/mcp","bearer_token_env":"no way"}"#,
                "its \"bearer_token_env\" isn't valid.",
            ),
            (
                r#"{"command":"x","enabled":1}"#,
                "its \"enabled\" isn't valid.",
            ),
            (
                r#"{"command":"x","required":1}"#,
                "its \"required\" isn't valid.",
            ),
            (
                r#"{"command":"x","startup_timeout_ms":0}"#,
                "its \"startup_timeout_ms\" isn't valid.",
            ),
            (
                r#"{"command":"x","operation_timeout_ms":-1}"#,
                "its \"operation_timeout_ms\" isn't valid.",
            ),
            (
                r#"{"command":"x","restart_limit":256}"#,
                "its \"restart_limit\" isn't valid.",
            ),
        ] {
            assert_eq!(note(entry), why, "{entry}");
        }
        // Other clients' spelling of Streamable HTTP is HTTP.
        let profile =
            parse_profile(r#"{"mcp":{"s":{"type":"streamable-http","url":"https://a.test/mcp"}}}"#)
                .unwrap();
        assert_eq!(profile.servers[0].transport, Transport::Http);
    }

    #[test]
    fn a_name_in_a_note_is_spelled_for_a_terminal_and_cut_at_160_bytes() {
        let hostile = "\u{1b}]0;owned\u{7}\u{202e}live";
        let profile =
            parse_profile(&serde_json::json!({"mcp": {hostile: {"command": 3}}}).to_string())
                .unwrap();
        assert_eq!(
            profile.notes,
            [
                r#"~/.xfx/mcp.json server '\x1b]0;owned\x07\u{202e}live' was skipped: it needs a "command", with any "args" as strings."#
            ]
        );
        let long = "n".repeat(400);
        let profile =
            parse_profile(&serde_json::json!({"mcp": {&long: {"command": 3}}}).to_string())
                .unwrap();
        let name = &profile.notes[0]["~/.xfx/mcp.json server '".len()..];
        let name = &name[..name.find('\'').unwrap()];
        assert_eq!(name, format!("{}...", "n".repeat(157)));
    }

    #[test]
    fn a_name_under_mcp_servers_beside_mcp_is_a_note() {
        // `noteIgnoredAlias` (`project_config.zig:403-418@481104fd`): a name
        // under both loads from `mcp`, and gets none.
        let profile = parse_profile(
            r#"{"mcp":{"a":{"command":"x"}},"mcpServers":{"a":{"command":"y"},"b":{"command":"z"}}}"#,
        )
        .unwrap();
        assert_eq!(profile.servers.len(), 1);
        assert_eq!(profile.servers[0].command, "x");
        assert_eq!(
            profile.notes,
            ["~/.xfx/mcp.json server 'b' was skipped: the file has both \"mcp\" and \"mcpServers\", and xfx reads only \"mcp\"; merge them into one."]
        );
        let empty = parse_profile(r#"{"mcp":{"a":{"command":"x"}},"mcpServers":{}}"#).unwrap();
        assert!(empty.notes.is_empty());
    }

    #[test]
    fn the_typed_add_never_drops_an_entry_it_cannot_read() {
        // `add_profile_server` writes the whole profile from what it read:
        // an entry the reader would skip must stop it instead.
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join("p").join(PROFILE_FILE);
        crate::profile_home::made(path.parent().unwrap());
        let text = r#"{"mcp":{"broken":{"command":123}}}"#;
        fs::write(&path, text).unwrap();
        assert_eq!(
            add_profile_server(&path, ServerConfig::local("a", "x", &[])),
            Err(ConfigError("McpConfigInvalidCommand"))
        );
        assert_eq!(
            remove_profile_server(&path, "broken"),
            Err(ConfigError("McpConfigInvalidCommand"))
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), text);
    }

    #[test]
    fn a_workspace_file_xfx_cant_read_is_one_note_in_fxs_words() {
        let dir = tempfile::tempdir().expect("dir");
        let issues = |text: &str| {
            fs::write(dir.path().join(WORKSPACE_FILE), text).unwrap();
            load_workspace(dir.path(), &Choices::default(), &|_| None).issues
        };
        assert_eq!(
            issues("{"),
            [".mcp.json server 'unknown' was skipped: invalid_json."]
        );
        assert_eq!(
            issues(r#"{"mcpServers":{"a":{},"a":{}}}"#),
            [".mcp.json server 'unknown' was skipped: invalid_json."]
        );
        assert_eq!(
            issues("[]"),
            [".mcp.json server 'unknown' was skipped: root_must_be_object."]
        );
        assert_eq!(
            issues(r#"{"mcpServers":[]}"#),
            [".mcp.json server 'unknown' was skipped: servers_must_be_object."]
        );
        assert!(issues(r#"{"other":1}"#).is_empty());
        assert_eq!(
            issues(
                r#"{"mcpServers":{"bad.name":{"command":"x"},"t":{"type":7},"ok":{"command":"x"}}}"#
            ),
            [
                ".mcp.json server 'bad.name' was skipped: xfx can't read it.",
                ".mcp.json server 't' was skipped: its \"type\" isn't valid.",
            ]
        );
        fs::write(
            dir.path().join(WORKSPACE_FILE),
            "x".repeat(MAX_CONFIG_BYTES as usize + 1),
        )
        .unwrap();
        assert_eq!(
            load_workspace(dir.path(), &Choices::default(), &|_| None).issues,
            [".mcp.json server 'unknown' was skipped: xfx can't read it."]
        );
    }

    // -----------------------------------------------------------------------
    // the launch reader's bound (`read_bounded`)
    // -----------------------------------------------------------------------

    #[test]
    fn a_config_that_grows_after_it_is_looked_at_is_not_read_past_the_cap() {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join(WORKSPACE_FILE);
        fs::write(&path, "{}").unwrap();
        let read = read_bounded_then(&path, |step| {
            if step == ReadStep::Reading {
                use std::io::Write as _;
                let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
                file.write_all(&vec![b' '; 2 * MAX_CONFIG_BYTES as usize])
                    .unwrap();
            }
        });
        assert!(
            read.is_err(),
            "read {} bytes",
            read.map(|t| t.map_or(0, |t| t.len())).unwrap_or(0)
        );
    }

    #[test]
    fn a_config_swapped_after_it_is_looked_at_is_still_the_file_looked_at() {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join(WORKSPACE_FILE);
        fs::write(&path, r#"{"a":1}"#).unwrap();
        let big = dir.path().join("big.json");
        fs::write(&big, vec![b' '; 2 * MAX_CONFIG_BYTES as usize]).unwrap();
        let read = read_bounded_then(&path, |step| {
            if step == ReadStep::Reading {
                fs::rename(&big, &path).unwrap();
            }
        });
        assert_eq!(read.unwrap().as_deref(), Some(r#"{"a":1}"#));
    }

    #[cfg(unix)]
    #[test]
    fn a_config_swapped_for_a_fifo_after_it_is_looked_at_does_not_block_the_read() {
        // Swapped after the open: the descriptor is the file looked at.
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join(WORKSPACE_FILE);
        fs::write(&path, r#"{"a":1}"#).unwrap();
        let read = within_seconds(&path, 10, {
            let path = path.clone();
            move || {
                read_bounded_then(&path, |step| {
                    if step == ReadStep::Reading {
                        fs::remove_file(&path).unwrap();
                        make_fifo(&path);
                    }
                })
                .map_err(|err| err.kind())
            }
        });
        assert_eq!(read.unwrap().as_deref(), Some(r#"{"a":1}"#));
        // Swapped after the path was looked at and before the open: the FIFO
        // is what is opened, and `O_NONBLOCK` is what keeps that open from
        // waiting for a writer that never comes. Refused, not blocked.
        fs::remove_file(&path).unwrap();
        fs::write(&path, r#"{"a":1}"#).unwrap();
        let read = within_seconds(&path, 10, {
            let path = path.clone();
            move || {
                read_bounded_then(&path, |step| {
                    if step == ReadStep::Opening {
                        fs::remove_file(&path).unwrap();
                        make_fifo(&path);
                    }
                })
                .map_err(|err| err.kind())
            }
        });
        assert_eq!(read, Err(io::ErrorKind::InvalidData));
    }

    /// `read` on a thread of its own, its answer within `seconds`, or the
    /// test fails -- rather than hangs, when a read blocks on the FIFO at
    /// `fifo`. A read that panics fails the test with its own panic, at once.
    /// One past its time is let go by opening the FIFO for writing without
    /// blocking (no reader there is `ENXIO`, not a wait), given `seconds`
    /// more, and joined; one still running then is named in the failure, the
    /// only way a thread that never returns can be accounted for.
    #[cfg(unix)]
    fn within_seconds<T: Send + 'static>(
        fifo: &Path,
        seconds: u64,
        read: impl FnOnce() -> T + Send + 'static,
    ) -> T {
        use std::os::unix::fs::OpenOptionsExt;
        use std::sync::mpsc::RecvTimeoutError;
        let (sent, answer) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let _ = sent.send(read());
        });
        let wait = Duration::from_secs(seconds);
        match answer.recv_timeout(wait) {
            Ok(answer) => {
                let _ = worker.join();
                answer
            }
            // The answer is gone because the thread is: it panicked.
            Err(RecvTimeoutError::Disconnected) => match worker.join() {
                Err(panic) => std::panic::resume_unwind(panic),
                Ok(()) => panic!("the read ended without an answer"),
            },
            Err(RecvTimeoutError::Timeout) => {
                // Held until the read has answered: a reader woken by a
                // writer that is already gone may go back to waiting.
                let release = fs::OpenOptions::new()
                    .write(true)
                    .custom_flags(libc::O_NONBLOCK)
                    .open(fifo);
                let let_go = answer.recv_timeout(wait).is_ok();
                drop(release);
                if let_go {
                    let _ = worker.join();
                    panic!("the read blocked on {fifo:?} for {seconds} s");
                }
                panic!(
                    "the read blocked on {fifo:?} for {seconds} s, and was still running \
                     {seconds} s after it was let go: its thread is left behind"
                );
            }
        }
    }

    /// A payload's text, when a panic carried one.
    fn panic_text(payload: &(dyn std::any::Any + Send)) -> Option<&str> {
        payload
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
    }

    #[cfg(unix)]
    #[test]
    fn a_read_that_panics_fails_its_test_at_once_with_its_own_panic() {
        // The read made the FIFO and then panicked: the answer is gone, and
        // no reader will ever open it. The test fails now, with that panic.
        let dir = tempfile::tempdir().expect("dir");
        let fifo = dir.path().join("fifo");
        let started = std::time::Instant::now();
        let caught = std::panic::catch_unwind(|| {
            within_seconds(&fifo, 10, {
                let fifo = fifo.clone();
                move || -> () {
                    make_fifo(&fifo);
                    panic!("the read's own panic");
                }
            })
        });
        let payload = caught.expect_err("the read's panic reaches the test");
        assert_eq!(panic_text(&*payload), Some("the read's own panic"));
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_read_blocked_on_the_fifo_is_let_go_joined_and_reported() {
        let dir = tempfile::tempdir().expect("dir");
        let fifo = dir.path().join("fifo");
        make_fifo(&fifo);
        let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let caught = std::panic::catch_unwind(|| {
            within_seconds(&fifo, 1, {
                let (fifo, done) = (fifo.clone(), std::sync::Arc::clone(&done));
                move || {
                    // A blocking open: it waits for a writer.
                    let opened = fs::File::open(&fifo).is_ok();
                    done.store(true, std::sync::atomic::Ordering::SeqCst);
                    opened
                }
            })
        });
        let payload = caught.expect_err("a blocked read fails its test");
        assert!(
            panic_text(&*payload).is_some_and(|text| text.contains("blocked")),
            "{:?}",
            panic_text(&*payload)
        );
        // Let go, and joined before the helper returned.
        assert!(done.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[cfg(unix)]
    #[test]
    fn a_slow_read_with_no_reader_on_the_fifo_does_not_block_the_release() {
        // Nothing has the FIFO open: a release that waited for a reader would
        // wait for ever. It doesn't, and the read is still accounted for.
        let dir = tempfile::tempdir().expect("dir");
        let fifo = dir.path().join("fifo");
        make_fifo(&fifo);
        let started = std::time::Instant::now();
        let caught = std::panic::catch_unwind(|| {
            within_seconds(&fifo, 1, || {
                std::thread::sleep(Duration::from_millis(1_500))
            })
        });
        let payload = caught.expect_err("a read past its time fails its test");
        assert!(
            panic_text(&*payload).is_some_and(|text| text.contains("blocked")),
            "{:?}",
            panic_text(&*payload)
        );
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
    }

    /// The paths [`read_bounded_then`] reached its open with.
    fn opened_by_the_reader(path: &Path) -> (io::Result<Option<String>>, bool) {
        let mut opened = false;
        let read = read_bounded_then(path, |step| opened |= step == ReadStep::Opening);
        (read, opened)
    }

    #[cfg(unix)]
    #[test]
    fn a_config_linked_to_a_device_is_refused_without_being_opened() {
        // A cloned repository can carry `.mcp.json -> /dev/<device>`, and the
        // launch reads it: a device is never opened, whatever opening it does.
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join(WORKSPACE_FILE);
        for device in ["/dev/null", "/dev/zero", "/dev/tty"] {
            if !Path::new(device).exists() {
                continue;
            }
            std::os::unix::fs::symlink(device, &path).unwrap();
            let (read, opened) = opened_by_the_reader(&path);
            assert!(read.is_err(), "{device}");
            assert!(!opened, "{device} was opened");
            assert_eq!(read_config(&path), Err(EditError::Unreadable), "{device}");
            assert_eq!(
                load_workspace(dir.path(), &Choices::default(), &|_| None).issues,
                [".mcp.json server 'unknown' was skipped: xfx can't read it."],
                "{device}"
            );
            fs::remove_file(&path).unwrap();
        }
        // A FIFO, a directory, and a link to either: not opened either.
        let fifo = dir.path().join("fifo");
        make_fifo(&fifo);
        let sub = dir.path().join("sub");
        fs::create_dir(&sub).unwrap();
        for target in [&fifo, &sub] {
            std::os::unix::fs::symlink(target, &path).unwrap();
            let (read, opened) = opened_by_the_reader(&path);
            assert!(read.is_err() && !opened, "{target:?}");
            fs::remove_file(&path).unwrap();
            let (read, opened) = opened_by_the_reader(target);
            assert!(read.is_err() && !opened, "{target:?}");
        }
        // A plain file, and a link to one, are opened and read.
        fs::write(dir.path().join("plain.json"), "{}").unwrap();
        std::os::unix::fs::symlink("plain.json", &path).unwrap();
        let (read, opened) = opened_by_the_reader(&path);
        assert_eq!((read.unwrap().as_deref(), opened), (Some("{}"), true));
        let (read, opened) = opened_by_the_reader(&dir.path().join("none.json"));
        assert_eq!((read.unwrap(), opened), (None, false));
    }

    #[cfg(unix)]
    fn make_fifo(path: &Path) {
        use std::os::unix::ffi::OsStrExt;
        let name = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
        // SAFETY: `name` is a NUL-terminated path that outlives the call.
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    }

    #[test]
    fn a_reader_reads_no_more_than_the_cap_whatever_the_file_reports() {
        // `std::io::repeat` never ends: a procfs file that says 0 bytes.
        assert!(read_capped(std::io::repeat(b' ')).is_err());
        let exact = vec![b' '; MAX_CONFIG_BYTES as usize];
        assert_eq!(read_capped(exact.as_slice()).unwrap().len(), exact.len());
        let over = vec![b' '; MAX_CONFIG_BYTES as usize + 1];
        assert!(read_capped(over.as_slice()).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_fifo_where_a_config_should_be_is_refused_without_blocking() {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join(WORKSPACE_FILE);
        make_fifo(&path);
        let (read, config, issues) = within_seconds(&path, 10, {
            let root = dir.path().to_path_buf();
            move || {
                let path = root.join(WORKSPACE_FILE);
                (
                    read_bounded(&path).is_err(),
                    read_config(&path),
                    load_workspace(&root, &Choices::default(), &|_| None).issues,
                )
            }
        });
        assert!(read);
        assert_eq!(config, Err(EditError::Unreadable));
        assert_eq!(
            issues,
            [".mcp.json server 'unknown' was skipped: xfx can't read it."]
        );
    }

    /// A compact file under 1 MiB that is over it written two-space: an
    /// array of `n` zeroes, each a line of its own when written.
    fn compact_but_large(servers: &str) -> String {
        let zeroes = vec!["0"; 400_000].join(",");
        let text = format!(r#"{{"pad":[{zeroes}],"mcpServers":{servers}}}"#);
        assert!((text.len() as u64) < MAX_CONFIG_BYTES, "{}", text.len());
        text
    }

    #[cfg(unix)]
    #[test]
    fn an_edit_that_would_leave_a_file_over_the_cap_is_refused_and_writes_nothing() {
        let (_dir, repo, profile) = scratch_repo();
        let path = project_path(&repo);
        for (text, change) in [
            (
                compact_but_large("{}"),
                Change::Add {
                    name: "p2".into(),
                    entry: stdio(&[SCRIPT], &[]),
                },
            ),
            (
                compact_but_large(r#"{"p2":{"command":"x"}}"#),
                Change::Remove("p2".into()),
            ),
        ] {
            fs::write(&path, &text).unwrap();
            assert_eq!(
                update(&profile, File::Project, &path, &change),
                Err(EditError::TooLarge),
                "{change:?}"
            );
            assert_eq!(fs::read_to_string(&path).unwrap(), text);
            assert_eq!(names_in(&repo), [".git", ".mcp.json"]);
        }
        // The pure edits say the same, and a hostile file nested to the cap
        // is not written out in full first (some 130 MB): writing stops there.
        let large = compact_but_large("{}");
        let entry = stdio(&[SCRIPT], &[]);
        assert_eq!(
            add_entry(File::Project, Some(&large), "p2", &entry),
            Err(EditError::TooLarge)
        );
        let deep = format!(
            "{{\"mcpServers\":{{}},\"pad\":{}{}{}}}",
            "[".repeat(MAX_JSON_DEPTH - 1),
            vec!["0"; 400_000].join(","),
            "]".repeat(MAX_JSON_DEPTH - 1)
        );
        assert!((deep.len() as u64) < MAX_CONFIG_BYTES);
        let root = parse_document(&deep).unwrap();
        let mut out = String::new();
        write_value(&root, 0, &mut out);
        assert!(out.len() < 2 * MAX_CONFIG_BYTES as usize, "{}", out.len());
        assert_eq!(
            add_entry(File::Project, Some(&deep), "p2", &entry),
            Err(EditError::TooLarge)
        );
        assert_eq!(
            EditError::TooLarge.why(),
            "the file would be over 1 MiB written out, more than xfx reads; make it smaller by hand first"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_write_that_fails_after_its_stage_exists_leaves_no_stage_and_the_file_as_it_was() {
        let (_dir, repo, profile) = scratch_repo();
        let path = project_path(&repo);
        let before = oracle!("config-work-before.mcp.json");
        fs::write(&path, before).unwrap();
        let add = Change::Add {
            name: "p2".into(),
            entry: stdio(&[SCRIPT], &[]),
        };
        for step in [StageStep::Write, StageStep::Sync, StageStep::Rename] {
            FAIL_AT.with(|cell| cell.set(Some(step)));
            let result = update(&profile, File::Project, &path, &add);
            FAIL_AT.with(|cell| cell.set(None));
            assert_eq!(result, Err(EditError::Failed), "{step:?}");
            assert_eq!(fs::read_to_string(&path).unwrap(), before, "{step:?}");
            assert_eq!(names_in(&repo), [".git", ".mcp.json"], "{step:?}");
        }
        assert_eq!(
            update(&profile, File::Project, &path, &add),
            Ok(Updated::Added)
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_stage_name_already_there_is_never_removed() {
        // The guard is made only once `create_new` has made the stage, so a
        // name this edit did not create -- here, one already at the stage's
        // name -- stays as it was, and so does the file.
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join(WORKSPACE_FILE);
        fs::write(&path, "{}").unwrap();
        let taken = dir.path().join(".mcp.json.taken.staged");
        fs::write(&taken, "someone else's").unwrap();
        assert_eq!(
            replace_through(&path, dir.path(), taken.clone(), b"{\"a\":1}", None),
            Err(EditError::Failed)
        );
        assert_eq!(fs::read_to_string(&taken).unwrap(), "someone else's");
        assert_eq!(fs::read_to_string(&path).unwrap(), "{}");
    }

    // -----------------------------------------------------------------------
    // before the servers start: the profile's `${VAR}`, and names a terminal
    // can't show (MCP-C2)
    // -----------------------------------------------------------------------

    /// A lookup that answers `pairs` and nothing else: every value a test
    /// expands is a fake one of its own.
    fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let pairs: Vec<(String, String)> = pairs
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect();
        move |name: &str| {
            pairs
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.clone())
        }
    }

    /// `text`'s profile, then [`prepare_to_start`] over its servers under
    /// `pairs`: what would start, and every note.
    fn prepared(text: &str, pairs: &[(&str, &str)]) -> (Vec<ServerConfig>, Vec<String>) {
        let Profile {
            mut servers,
            mut notes,
        } = parse_profile(text).expect("the profile reads");
        prepare_to_start(&mut servers, &mut notes, &env_of(pairs));
        (servers, notes)
    }

    /// The `note:` rows of an fx `/mcp list` capture, each whole: a note too
    /// long for its row goes on in the rows under it, indented two columns.
    /// `/.fx/` is read as `/.xfx/` and fx's name as xfx's, as the rest of
    /// this file reads fx's sentences.
    fn fx_notes(capture: &str) -> Vec<String> {
        let mut notes: Vec<String> = Vec::new();
        let mut open = false;
        for row in capture.lines().map(str::trim_end) {
            if let Some((_, note)) = row.split_once("! mcp: note: ") {
                notes.push(note.to_string());
                open = true;
            } else if !open {
                continue;
            } else if let Some(note) = row.strip_prefix("  note: ") {
                notes.push(note.to_string());
            } else if let Some(more) = row.strip_prefix("  ") {
                let last = notes.last_mut().expect("a note it goes on from");
                last.push(' ');
                last.push_str(more.trim_start());
            } else {
                break;
            }
        }
        notes
            .into_iter()
            .map(|note| {
                note.replace("/.fx/", "/.xfx/")
                    .replace("fx won't print", "xfx won't print")
            })
            .collect()
    }

    /// The fake values the `p5-mcp-config-vars` scenario's run is given, as
    /// this file names its placeholders.
    const VARS_ENV: [(&str, &str); 3] = [
        ("P5_DIR", "<scenarios>"),
        ("P5_HOME", "<home>"),
        ("P5_TAG", "fake-p5-tag"),
    ];

    #[test]
    fn the_vars_scenario_starts_what_fx_v0_0_16_starts_and_says_what_it_says() {
        // `cap/fx/fx-p5-mcp-config-vars-40x120-list.txt`: fx starts `fixture`
        // and `fallback`, each `ready, 1 tool` only because its `${...}`
        // reached the script, and gives five notes; `fixture`'s wire log is
        // where its expanded `env` put it (`...-end.files`,
        // `~/p5-wire-fake-p5-tag.log`).
        let (servers, notes) = prepared(
            &placeholders(include_str!(
                "../../scripts/fxcap/scenarios/p5-mcp-config-vars-profile.json"
            )),
            &VARS_ENV,
        );
        let names: Vec<&str> = servers.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["fixture", "fallback"]);
        assert_eq!(servers[0].command, "<bun>");
        assert_eq!(servers[0].args, [SCRIPT]);
        assert_eq!(
            servers[0].env,
            [(
                "MCPSTDIO_WIRE_LOG".to_string(),
                "<home>/p5-wire-fake-p5-tag.log".to_string()
            )]
        );
        assert_eq!(servers[1].args, [SCRIPT]);
        let captured = fx_notes(include_str!(
            "../../.prd/tui-phase5/receipts/mcp-config-deltas/cap/fx/fx-p5-mcp-config-vars-40x120-list.txt"
        ));
        assert_eq!(captured.len(), 5, "{captured:?}");
        assert_eq!(notes, captured);
        assert_eq!(
            notes,
            [
                "~/.xfx/mcp.json server 'unset' field argument requires environment variable \
                 'P5_UNSET_TOKEN'; set it or use ${P5_UNSET_TOKEN:-default}.",
                "~/.xfx/mcp.json server 'remote' field http_header requires environment \
                 variable 'P5_UNSET_HEADER'; set it or use ${P5_UNSET_HEADER:-default}.",
                "~/.xfx/mcp.json server 'badref' was skipped: a ${...} in it isn't valid.",
                r"~/.xfx/mcp.json server 'evil\x1b]0;x\x07' was skipped: its name has a character xfx won't print.",
                r"~/.xfx/mcp.json server 'rtl\u{202e}txt' was skipped: its name has a character xfx won't print.",
            ]
        );
        // Not one value the variables had is in a note.
        for note in &notes {
            for (_, value) in VARS_ENV {
                assert!(!note.contains(value), "{note}");
            }
        }
    }

    #[test]
    fn a_profile_servers_variables_expand_where_fxs_do_and_nowhere_else() {
        let (servers, notes) = prepared(
            r#"{"mcp":{
                "local":{"command":"${A}/bin","args":["--a=${A}","${B:-b}","plain"],
                         "env":{"K":"${A}-v","L":"x"}},
                "web":{"type":"http","url":"https://${A}.example.test/mcp",
                       "headers":{"X-One":"${B}","X-Two":"t"},
                       "header_env":{"X-E":"A"},"bearer_token_env":"B",
                       "env":{"E":"${A}"},"oauth":{"client_id":"${A}"}}}}"#,
            &[("A", "fake-a"), ("B", "fake-b")],
        );
        assert_eq!(notes, Vec::<String>::new());
        let [local, web] = servers.as_slice() else {
            panic!("{servers:?}")
        };
        assert_eq!(local.command, "fake-a/bin");
        assert_eq!(local.args, ["--a=fake-a", "fake-b", "plain"]);
        // A value is expanded; a name has no `${` to expand, since a
        // variable's or a header's name that did would not parse.
        assert_eq!(
            local.env,
            [
                ("K".to_string(), "fake-a-v".to_string()),
                ("L".to_string(), "x".to_string())
            ]
        );
        assert_eq!(web.command, "");
        assert_eq!(web.url, "https://${A}.example.test/mcp");
        assert_eq!(
            web.headers,
            [
                ("X-One".to_string(), "fake-b".to_string()),
                ("X-Two".to_string(), "t".to_string())
            ]
        );
        assert_eq!(web.header_env, [("X-E".to_string(), "A".to_string())]);
        assert_eq!(web.bearer_token_env.as_deref(), Some("B"));
        // fx expands an HTTP server's `env` too, which it then never uses.
        assert_eq!(web.env, [("E".to_string(), "fake-a".to_string())]);
        assert_eq!(web.oauth, Some(json!({"client_id": "${A}"})));
    }

    #[test]
    fn a_variable_is_its_value_even_empty_and_its_default_only_when_unset() {
        // `expandWorkspaceTemplate` (`project_config.zig:698-733@481104fd`),
        // one argument at a time.
        let expanded = |template: &str| {
            let (servers, notes) = prepared(
                &json!({"mcp": {"s": {"command": "x", "args": [template]}}}).to_string(),
                &[("EMPTY", ""), ("SET", "v"), ("NESTED", "${SET}")],
            );
            match (servers.as_slice(), notes.as_slice()) {
                ([server], []) => Ok(server.args[0].clone()),
                ([], [note]) => Err(note
                    .strip_prefix("~/.xfx/mcp.json server 's' ")
                    .expect("the note's start")
                    .to_string()),
                _ => panic!("{template}: {servers:?} {notes:?}"),
            }
        };
        let invalid = Err("was skipped: a ${...} in it isn't valid.".to_string());
        let missing = |name: &str| {
            Err(format!(
                "field argument requires environment variable '{name}'; set it or use \
                 ${{{name}:-default}}."
            ))
        };
        for (template, expected) in [
            ("${SET}", Ok("v".to_string())),
            ("a${SET}b${SET}c", Ok("avbvc".to_string())),
            // Set to nothing is a value, not a reason for the default.
            ("<${EMPTY:-d}>", Ok("<>".to_string())),
            ("${UNSET:-d}", Ok("d".to_string())),
            ("${UNSET:-}", Ok(String::new())),
            // The first `}` ends it, and a default is written in as it is.
            ("${UNSET:-b}c}", Ok("bc}".to_string())),
            ("${UNSET:-${SET}}", Ok("${SET}".to_string())),
            ("${UNSET:-a:-b}", Ok("a:-b".to_string())),
            // A value is never read for a `${` of its own.
            ("${NESTED}", Ok("${SET}".to_string())),
            ("$SET {SET} $", Ok("$SET {SET} $".to_string())),
            ("${UNSET}", missing("UNSET")),
            ("ok ${SET} ${UNSET}", missing("UNSET")),
            ("${}", invalid.clone()),
            ("${:-x}", invalid.clone()),
            ("${1A}", invalid.clone()),
            ("${A B}", invalid.clone()),
            ("${KEY=}", invalid.clone()),
            ("${SET", invalid.clone()),
            ("${SET}${", invalid.clone()),
        ] {
            assert_eq!(expanded(template), expected, "{template}");
        }
    }

    #[test]
    fn a_server_left_without_a_value_is_dropped_with_fxs_sentence_for_its_field() {
        // `renderWorkspaceDiagnostic`'s `.missing_environment_variable`
        // (`project_config.zig:171-192@481104fd`): the first field that
        // fails, in fx's order, names it.
        let (servers, notes) = prepared(
            r#"{"mcp":{
                "c":{"command":"${U1}","args":["${U2}"]},
                "a":{"command":"x","args":["ok","${U2}"],"env":{"K":"${U3}"}},
                "e":{"command":"x","env":{"K":"${U3}"}},
                "h":{"type":"http","url":"https://example.test/mcp","env":{"K":"${U3}"},"headers":{"X":"${U4}"}},
                "hh":{"type":"http","url":"https://example.test/mcp","headers":{"X":"${U4}"}},
                "kept":{"command":"x","args":["${U5:-d}"]}}}"#,
            &[],
        );
        let names: Vec<&str> = servers.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["kept"]);
        let sentence = |server: &str, field: &str, variable: &str| {
            format!(
                "~/.xfx/mcp.json server '{server}' field {field} requires environment variable \
                 '{variable}'; set it or use ${{{variable}:-default}}."
            )
        };
        assert_eq!(
            notes,
            [
                sentence("c", "command", "U1"),
                sentence("a", "argument", "U2"),
                sentence("e", "environment", "U3"),
                sentence("h", "environment", "U3"),
                sentence("hh", "http_header", "U4"),
            ]
        );
        // A name spelled and cut at 160 bytes, a variable's at 128, each to
        // end `...` (`encodeTerminalSafe`, `text_utils.zig:353-390@481104fd`).
        let long_name = format!("\u{1b}{}", "n".repeat(300));
        let long_variable = "V".repeat(200);
        let (_, notes) = prepared(
            &json!({"mcp": {long_name.clone(): {"command": format!("${{{long_variable}}}")}}})
                .to_string(),
            &[],
        );
        let name = format!("\\x1b{}...", "n".repeat(160 - 4 - 3));
        let variable = format!("{}...", "V".repeat(125));
        assert_eq!(notes, [sentence(&name, "command", &variable)]);
        // The workspace's sentence cuts them the same way.
        let workspace = Unresolved::Missing {
            field: "argument",
            variable: long_variable,
        }
        .issue(Source::Workspace, &"w".repeat(300));
        assert_eq!(
            workspace,
            sentence(&format!("{}...", "w".repeat(157)), "argument", &variable)
                .replace("~/.xfx/mcp.json", ".mcp.json")
        );
    }

    #[test]
    fn an_expanded_header_is_held_to_a_written_headers_rules() {
        // `http_config.validateStaticHeaders` after the expansion
        // (`project_config.zig:786@481104fd`): a value that expands to a line
        // break is no header of its own, and the server does not connect.
        let entry = |value: &str| {
            json!({"mcp": {"web": {"type": "http", "url": "https://example.test/mcp",
                "headers": {"X-A": value}}}})
            .to_string()
        };
        for value in ["x\r\nInjected: y", "x\ny", "\u{7f}", "a\u{0}b"] {
            let (servers, notes) = prepared(&entry("${V}"), &[("V", value)]);
            assert!(servers.is_empty(), "{value:?}");
            assert_eq!(
                notes,
                ["~/.xfx/mcp.json server 'web' was skipped: a ${...} in it isn't valid."],
                "{value:?}"
            );
            assert!(!notes[0].contains(value), "{value:?}");
        }
        let (servers, notes) = prepared(&entry("${V}"), &[("V", "tab\there")]);
        assert!(notes.is_empty(), "{notes:?}");
        assert_eq!(servers[0].headers[0].1, "tab\there");
    }

    #[test]
    fn the_expansion_is_capped_as_fxs_is() {
        // 1 MiB for one value, and 1 MiB for every profile server's together,
        // each byte written counting (`appendExpandedBytes`,
        // `project_config.zig:735-749@481104fd`).
        let big = "v".repeat(600 * 1024);
        let limit = |name: &str| {
            format!(
                "~/.xfx/mcp.json server '{name}' was skipped: environment_expansion_limit_exceeded."
            )
        };
        // One value over the cap: the server goes, and what it wrote counts.
        let (servers, notes) = prepared(
            r#"{"mcp":{"one":{"command":"x","args":["${BIG}${BIG}"]},"next":{"command":"x","args":["${BIG}"]}}}"#,
            &[("BIG", &big)],
        );
        assert!(servers.is_empty(), "{:?}", servers.len());
        assert_eq!(notes, [limit("one"), limit("next")]);
        // The two checks are fx's two: the value's own cap refuses the second
        // `${BIG}` before the shared budget is asked, so the 400-odd KiB the
        // budget still has is left -- and `small`, one byte, starts. Were the
        // budget the only check, the second `${BIG}` would have emptied it and
        // `small` would go too.
        let (servers, notes) = prepared(
            r#"{"mcp":{"one":{"command":"x","args":["${BIG}${BIG}"]},"small":{"command":"x"}}}"#,
            &[("BIG", &big)],
        );
        let names: Vec<&str> = servers.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["small"]);
        assert_eq!(notes, [limit("one")]);
        // Two servers under the cap each, over it together: the second goes,
        // and then nothing is left for one with a byte to write -- but a
        // server with none still starts.
        let (servers, notes) = prepared(
            r#"{"mcp":{"a":{"command":"x","args":["${BIG}"]},"b":{"command":"x","args":["${BIG}"]},"c":{"command":"","args":[""]},"d":{"command":"x"}}}"#,
            &[("BIG", &big)],
        );
        let names: Vec<&str> = servers.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["a", "c"]);
        assert_eq!(servers[0].args[0].len(), big.len());
        assert_eq!(notes, [limit("b"), limit("d")]);
        // A workspace server is not counted, nor capped here.
        let mut servers = vec![ServerConfig {
            source: Source::Workspace,
            ..ServerConfig::local("w", &big, &[big.clone(), big.clone()])
        }];
        let mut notes = Vec::new();
        prepare_to_start(&mut servers, &mut notes, &|_| None);
        assert_eq!((servers.len(), notes.len()), (1, 0));
    }

    #[test]
    fn a_servers_debug_names_it_and_never_shows_a_value() {
        // `{:?}` -- an assertion's, a panic's, a trace's -- of a server, of the
        // profile and the workspace lists that hold them, before and after the
        // expansion: neither `${TOKEN}` as written nor the (fake) value it
        // expands to, from any field an expansion can fill.
        const SECRET: &str = "fake-token-6d1f";
        let text = r#"{"mcp":{
            "local":{"command":"/opt/${TOKEN}/bin","args":["--t=${TOKEN}"],"env":{"API_TOKEN":"${TOKEN}"}},
            "web":{"type":"http","url":"https://example.test/mcp","headers":{"X-Token":"Bearer ${TOKEN}"},
                   "header_env":{"X-Other":"TOKEN"},"bearer_token_env":"TOKEN","oauth":{"client_id":"${TOKEN}"}}}}"#;
        let written = parse_profile(text).unwrap();
        let (expanded, notes) = prepared(text, &[("TOKEN", SECRET)]);
        assert_eq!(notes, Vec::<String>::new());
        assert_eq!(expanded[0].env[0].1, SECRET);
        assert_eq!(expanded[1].headers[0].1, format!("Bearer {SECRET}"));
        // An approved workspace server, whose every string was expanded, the
        // URL among them.
        let dir = tempfile::tempdir().expect("dir");
        fs::write(
            dir.path().join(WORKSPACE_FILE),
            r#"{"mcpServers":{"proj":{"type":"http","url":"https://${TOKEN}.example.test/mcp","headers":{"X-T":"${TOKEN}"}}}}"#,
        )
        .unwrap();
        let env = env_of(&[("TOKEN", SECRET)]);
        let digest = load_workspace(dir.path(), &Choices::default(), &env).digests[0]
            .1
            .clone();
        let approved = Choices {
            approved: vec![("proj".into(), digest)],
            ..Choices::default()
        };
        let workspace = load_workspace(dir.path(), &approved, &env);
        assert_eq!(
            workspace.servers[0].url,
            format!("https://{SECRET}.example.test/mcp")
        );
        for shown in [
            format!("{written:?}"),
            format!("{expanded:?}"),
            format!("{:#?}", expanded[1]),
            format!("{workspace:?}"),
        ] {
            assert!(!shown.contains(SECRET), "{shown}");
            assert!(!shown.contains("${TOKEN}"), "{shown}");
            assert!(!shown.contains("TOKEN"), "{shown}");
            assert!(!shown.contains("example.test"), "{shown}");
        }
        assert_eq!(
            format!("{:?}", expanded[1]),
            "ServerConfig { name: \"web\", source: Profile, transport: Http, \
             command: <none>, args: <0 redacted>, env: <0 redacted>, url: <redacted>, \
             headers: <1 redacted>, header_env: <1 redacted>, bearer_token_env: <redacted>, \
             oauth: <redacted>, enabled: true, required: false, startup_timeout_ms: 30000, \
             operation_timeout_ms: 60000, restart_limit: 1, admission: None }"
        );
    }

    #[test]
    fn only_a_profile_server_is_expanded_so_a_repository_never_reads_a_credential() {
        // A repository's approved entry asks for `${FOO}`, and the environment's
        // `FOO` holds `${AI_GATEWAY_API_KEY}`: the workspace's expansion writes
        // that in as it is, and the profile's pass must leave it so. Read
        // again, it would hand the repository's server xfx's credential.
        let dir = tempfile::tempdir().expect("dir");
        fs::write(
            dir.path().join(WORKSPACE_FILE),
            r#"{"mcpServers":{"proj":{"command":"node","args":["${FOO}","${P5_TAG}"]},"pending":{"command":"node","args":["${P5_TAG}"]}}}"#,
        )
        .unwrap();
        let env = env_of(&[
            ("FOO", "${AI_GATEWAY_API_KEY}"),
            ("AI_GATEWAY_API_KEY", "fake-gateway-key"),
            ("P5_TAG", "fake-p5-tag"),
        ]);
        let read = load_workspace(dir.path(), &Choices::default(), &env);
        let approved = Choices {
            approved: vec![("proj".into(), read.digests[0].1.clone())],
            ..Choices::default()
        };
        let workspace = load_workspace(dir.path(), &approved, &env);
        let Profile {
            mut servers,
            mut notes,
        } = parse_profile(
            r#"{"mcp":{"mine":{"command":"x","args":["${FOO}","${AI_GATEWAY_API_KEY}"]}}}"#,
        )
        .unwrap();
        servers.extend(workspace.servers);
        prepare_to_start(&mut servers, &mut notes, &env);
        assert_eq!(notes, Vec::<String>::new());
        let args: Vec<(&str, &[String])> = servers
            .iter()
            .map(|s| (s.name.as_str(), s.args.as_slice()))
            .collect();
        assert_eq!(
            args,
            [
                // The person's own file may name the credential, as fx's may.
                (
                    "mine",
                    &[
                        "${AI_GATEWAY_API_KEY}".to_string(),
                        "fake-gateway-key".to_string()
                    ][..]
                ),
                (
                    "proj",
                    &[
                        "${AI_GATEWAY_API_KEY}".to_string(),
                        "fake-p5-tag".to_string()
                    ][..]
                ),
                // Pending: never expanded at all.
                ("pending", &["${P5_TAG}".to_string()][..]),
            ]
        );
        for server in &servers[1..] {
            assert!(
                !format!("{server:?}").contains("fake-gateway-key"),
                "{server:?}"
            );
        }
    }

    #[test]
    fn a_name_a_terminal_cant_show_is_skipped_last_with_a_note_that_spells_it() {
        // fx's own case (`src/builtins/mcp.zig:593-619@481104fd`), but for
        // the name that isn't UTF-8, which a Rust string can't be: an escape
        // sequence and a right-to-left override go, a printable `caf\u{e9} two`
        // stays.
        let (servers, notes) = prepared(
            r#"{"mcpServers":{"ok":{"command":"true"},"evil\u001b]0;x\u0007":{"command":"true"},"rtl\u202etxt":{"command":"true"},"caf\u00e9 two":{"command":"true"}}}"#,
            &[],
        );
        let names: Vec<&str> = servers.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["ok", "caf\u{e9} two"]);
        assert_eq!(
            notes,
            [
                r"~/.xfx/mcp.json server 'evil\x1b]0;x\x07' was skipped: its name has a character xfx won't print.",
                r"~/.xfx/mcp.json server 'rtl\u{202e}txt' was skipped: its name has a character xfx won't print.",
            ]
        );
        // After the expansion: a name that can't be shown and is left without
        // a value has the expansion's note, once; and the name notes come
        // after every expansion note, whatever the file's order.
        let (servers, notes) = prepared(
            r#"{"mcp":{"tab\tname":{"command":"true"},"bel\u0007":{"command":"${UNSET}"},"later":{"command":"${UNSET}"}}}"#,
            &[],
        );
        assert!(servers.is_empty(), "{servers:?}");
        assert_eq!(
            notes,
            [
                r"~/.xfx/mcp.json server 'bel\x07' field command requires environment variable 'UNSET'; set it or use ${UNSET:-default}.",
                "~/.xfx/mcp.json server 'later' field command requires environment variable 'UNSET'; set it or use ${UNSET:-default}.",
                r"~/.xfx/mcp.json server 'tab\x09name' was skipped: its name has a character xfx won't print.",
            ]
        );
        // xfx's set is its `terminal_safe`'s, wider than fx's
        // `isNonPrintingCodepoint`: a variation selector and a soft hyphen go
        // too (`docs/parity.md`); a combining mark is printable.
        for name in ["vs\u{fe0f}", "soft\u{ad}hyphen"] {
            let (servers, notes) = prepared(
                &json!({"mcp": {name: {"command": "true"}}}).to_string(),
                &[],
            );
            assert!(servers.is_empty(), "{name:?}");
            assert_eq!(notes.len(), 1, "{name:?}");
            assert!(!notes[0].contains(name), "{}", notes[0]);
        }
        let (servers, notes) = prepared(r#"{"mcp":{"e\u0301":{"command":"true"}}}"#, &[]);
        assert_eq!((servers.len(), notes.len()), (1, 0));
    }

    #[test]
    fn the_profile_on_disk_keeps_its_variables_as_written() {
        // The expansion is of what is about to start; the file, and the
        // writer that edits it, keep `${...}`.
        let text = r#"{"mcp":{"s":{"command":"${A}","args":["${B:-b}"],"env":{"K":"${A}"}}}}"#;
        let profile = parse_profile(text).unwrap();
        assert_eq!(profile.servers[0].command, "${A}");
        assert_eq!(profile.servers[0].args, ["${B:-b}"]);
        assert_eq!(profile.servers[0].env[0].1, "${A}");
        let edited = add_entry(
            File::Profile,
            Some(text),
            "t",
            &Entry::Stdio {
                command: "y".into(),
                args: Vec::new(),
                env: Vec::new(),
            },
        )
        .unwrap();
        assert!(edited.contains(r#""command": "${A}""#), "{edited}");
        assert!(edited.contains(r#""${B:-b}""#), "{edited}");
    }

    #[test]
    fn the_runtime_starts_only_what_the_expansion_and_the_name_check_leave() {
        // The one call site (`McpRuntime::read_configuration`): what a
        // session reads goes through `prepare_to_start`. Variables no test
        // environment sets, so nothing here reads a real one, and no server
        // in it starts.
        let dir = tempfile::tempdir().expect("dir");
        let home = dir.path().join(".xfx");
        fs::create_dir(&home).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&home, fs::Permissions::from_mode(0o700)).unwrap();
        }
        fs::create_dir(dir.path().join("work")).unwrap();
        let profile = home.join(PROFILE_FILE);
        let unset = "XFX_P5_MCP_C2_NEVER_SET_7F3A";
        let text = format!(
            r#"{{"mcp":{{"needs":{{"command":"${{{unset}}}"}},"esc\u001b[2J":{{"command":"${{{unset}:-}}"}}}}}}"#
        );
        fs::write(&profile, &text).unwrap();
        let runtime = crate::mcp::McpRuntime::new(crate::mcp::Paths {
            profile: Some(profile.clone()),
            settings: Some(home.join("settings.json")),
            workspace: dir.path().join("work"),
        });
        runtime.start_and_wait(Duration::from_secs(10));
        assert!(runtime.views().is_empty());
        assert_eq!(
            runtime.issues(),
            [
                format!(
                    "~/.xfx/mcp.json server 'needs' field command requires environment variable \
                     '{unset}'; set it or use ${{{unset}:-default}}."
                ),
                r"~/.xfx/mcp.json server 'esc\x1b[2J' was skipped: its name has a character xfx won't print."
                    .to_string(),
            ]
        );
        assert_eq!(fs::read_to_string(&profile).unwrap(), text);
    }

    #[test]
    fn fxs_ignored_key_notes_are_not_given_since_xfx_acts_on_both_keys() {
        // `cap/fx/fx-p5-mcp-config-ignored-40x120-list.txt`: fx v0.0.16 reads
        // `required` and `restart_limit` and no longer acts on them, and says
        // so (`noteIgnoredKeys`, `project_config.zig:217-235@481104fd`). xfx
        // restarts a stopped stdio server up to its `restart_limit` and shows
        // `required` as the server's `Policy` in `/mcp`'s details, so it
        // gives neither note (`docs/parity.md`).
        let captured = fx_notes(include_str!(
            "../../.prd/tui-phase5/receipts/mcp-config-deltas/cap/fx/fx-p5-mcp-config-ignored-40x120-list.txt"
        ));
        assert_eq!(
            captured,
            [
                "~/.xfx/mcp.json server 'fixture' sets \"required\", which fx ignores; remove it.",
                "~/.xfx/mcp.json server 'fixture' sets \"restart_limit\", which fx ignores; remove it.",
            ]
        );
        let (servers, notes) = prepared(
            &placeholders(include_str!(
                "../../scripts/fxcap/scenarios/p5-mcp-config-ignored-profile.json"
            )),
            &[],
        );
        assert_eq!(notes, Vec::<String>::new());
        assert!(servers[0].required);
        assert_eq!(servers[0].restart_limit, 3);
    }
}
