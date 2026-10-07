//! The live MCP servers of one session, and everything that asks them.
//!
//! One [`McpRuntime`] per TUI session, shared by the UI thread (the `/mcp`
//! menu reads its snapshots) and the runtime thread (a turn's tools call it).
//! It owns the servers' connections and the state machine upstream's
//! `health.zig` classifies (`vercel-labs/fx@c1db9191
//! src/core/mcp/health.zig:8-69`): a server is `Disabled` (switched off, or a
//! workspace server nobody approved), `Connecting`, `Ready`, or `Failed`, and
//! a remote one that answered 401 also needs authentication.
//!
//! # Connecting
//!
//! Each enabled, admitted server is connected on a thread of its own, so a
//! slow one never holds up the others or the session: the child is started
//! (or the endpoint opened), `initialize` is asked with upstream's legacy
//! default -- `2025-11-25`, any of the four legacy versions accepted back
//! (`protocol_negotiation.zig:7-19`, `:66-117`) -- `notifications/initialized`
//! is sent, and `tools/list` is read page by page, all inside the server's
//! `startup_timeout_ms`. A stdio server that fails to start is started again,
//! up to its `restart_limit` (`[mcp] restarting stdio server after startup
//! failure ... attempt=1`, `cap/mcp-diag-trace.log`).
//!
//! # Reloading
//!
//! A reload reads both files again and connects what changed while the
//! servers already up stay up: a server whose configuration is the same and
//! which is ready (or still connecting) is carried over as it is. Only once
//! every new connection has settled is the new set put in force and the old
//! ones stopped, and the person is told how it went
//! (`formatMcpPublishedReload`, `app_commands.zig:91-129`).
//!
//! # Commands are answered in order
//!
//! Every `/mcp` command, every menu catalog read and every trust choice goes
//! to one worker thread in the order it was given, and what it says comes
//! back as [`Event`]s the UI drains: a resource read that waits on a slow
//! server cannot be overtaken by the `/mcp path` typed after it, and the UI
//! thread never waits on a server.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use super::config::{
    self, Admission, Choices, ServerConfig, Source, Transport as Kind, TrustAction,
};
use super::transport::{HttpTransport, StdioTransport, Transport};
use super::{names::ToolNames, McpError};
use crate::gateway::CancelToken;

/// The version `initialize` asks for (`connectServerLegacy(..., .v2025_11_25)`,
/// `server_transport.zig:126-128`).
pub const REQUESTED_VERSION: &str = "2025-11-25";

/// The legacy versions a stdio server may answer with (`LegacyStdioVersion`).
const STDIO_VERSIONS: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

/// The legacy versions a streamable HTTP server may answer with
/// (`legacy_streamable_http.zig:14-25`).
const HTTP_VERSIONS: [&str; 3] = ["2025-11-25", "2025-06-18", "2025-03-26"];

/// How many pages of a list are read before the list is called complete.
const MAX_PAGES: usize = 64;

/// `ConnectionState` (`health.zig:8-14`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Connection {
    Disconnected,
    Disabled,
    Connecting,
    Ready,
    Failed,
}

/// `health.Status` (`health.zig:41-69`): one classification every surface
/// derives from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Disabled,
    Connecting,
    Ready,
    NeedsAuth,
    Failed,
    Unavailable,
}

/// What a server said it can do (`initialize`'s `capabilities`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Capabilities {
    pub tools: bool,
    pub resources: bool,
    pub prompts: bool,
    pub completions: bool,
}

/// One tool a server listed.
#[derive(Debug, Clone, PartialEq)]
pub struct McpTool {
    /// The server's own name for it.
    pub name: String,
    /// The name the model calls it by, `mcp_<server>_<tool>` ([`ToolNames`]).
    pub alias: String,
    pub title: Option<String>,
    pub description: String,
    pub input_schema: Value,
    /// `tagsFor` (`tool_names.zig:146-154`): `mcp`, then the identifier
    /// tokens of the server's name and the tool's, lowercased and unique.
    pub tags: Vec<String>,
}

/// One configured server, as every surface sees it at one moment.
#[derive(Debug, Clone, PartialEq)]
pub struct ServerView {
    pub name: String,
    pub source: Source,
    pub transport: Kind,
    pub admission: Option<Admission>,
    pub connection: Connection,
    pub needs_auth: bool,
    pub required: bool,
    pub reloading: bool,
    pub protocol_version: Option<String>,
    pub capabilities: Capabilities,
    /// Known only once the server is ready.
    pub tool_count: Option<usize>,
    pub failure: Option<String>,
}

impl ServerView {
    /// `classify` (`health.zig:55-69`).
    pub fn status(&self) -> Status {
        if self.connection == Connection::Disabled {
            return Status::Disabled;
        }
        if self.needs_auth {
            return Status::NeedsAuth;
        }
        match self.connection {
            Connection::Disabled => Status::Disabled,
            Connection::Connecting => Status::Connecting,
            Connection::Ready => Status::Ready,
            Connection::Failed => Status::Failed,
            Connection::Disconnected => Status::Unavailable,
        }
    }

    /// The menu's state column (`serverStateLabel`,
    /// `mcp_menu_presentation.zig:404-416`).
    pub fn state_label(&self) -> &'static str {
        if self.reloading {
            return "Reloading";
        }
        if self.admission == Some(Admission::Pending) {
            return "Pending trust";
        }
        match self.status() {
            Status::NeedsAuth => "Needs authentication",
            Status::Unavailable => "Disconnected",
            Status::Disabled => "Disabled",
            Status::Connecting => "Connecting",
            Status::Ready => "Ready",
            Status::Failed => "Failed",
        }
    }
}

/// One resource or resource template a server listed.
#[derive(Debug, Clone, PartialEq)]
pub struct Resource {
    pub uri: String,
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub mime_type: Option<String>,
    pub template: bool,
}

/// One argument of a prompt.
#[derive(Debug, Clone, PartialEq)]
pub struct PromptArgument {
    pub name: String,
    pub required: bool,
    pub description: Option<String>,
}

/// One prompt a server listed.
#[derive(Debug, Clone, PartialEq)]
pub struct Prompt {
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub arguments: Vec<PromptArgument>,
}

/// Where a reload's outcome is told.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// The session's first load: nothing is said unless a server failed.
    Startup,
    /// A notice in the transcript.
    Transcript,
    /// The menu's feedback row.
    Menu,
}

/// Something the UI has to show.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// A notice: neutral or a warning, under `mcp` or, for a usage line, no
    /// topic at all (`noticeTopicForBody`, `app_commands.zig:1851-1853`).
    Notice { warning: bool, body: String },
    /// Rows written into the transcript as they are, with no glyph: the
    /// project trust prompt (`presentProjectMcpPrompt`, `main.zig:1818-1832`).
    Raw(String),
    /// The trust prompt is up for this server.
    TrustPrompt { name: String },
    /// The menu's feedback row.
    MenuFeedback(String),
    /// What a menu catalog read found, for the read that asked.
    Catalog {
        request: u64,
        result: Result<Catalog, String>,
    },
    /// The servers changed: the menu repaints.
    Changed,
}

/// One section's items, for the menu.
#[derive(Debug, Clone, PartialEq)]
pub enum Catalog {
    Tools(Vec<String>),
    Resources(Vec<Resource>),
    Prompts(Vec<Prompt>),
}

/// The files a runtime reads its servers from.
#[derive(Debug, Clone, Default)]
pub struct Paths {
    /// `~/.xfx/mcp.json`, when there is a profile.
    pub profile: Option<PathBuf>,
    /// `~/.xfx/settings.json`, where trust choices are kept.
    pub settings: Option<PathBuf>,
    /// The workspace whose `.mcp.json` is read.
    pub workspace: PathBuf,
}

/// One unit of work for the runtime's worker thread.
pub enum Job {
    /// A `/mcp` command line, without the `/mcp`.
    Command(String),
    /// A trust choice, and the sentence that announces it.
    Trust { action: TrustAction, body: String },
    /// A menu catalog read.
    Catalog {
        request: u64,
        section: Section,
        server: Option<String>,
    },
    /// A reload the menu's `r` asked for.
    MenuReload,
}

/// The menu's catalog sections that read something.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Tools,
    Resources,
    Prompts,
}

/// A server's live half.
struct Live {
    connection: Connection,
    needs_auth: bool,
    failure: Option<String>,
    transport: Option<Arc<dyn Transport>>,
    protocol_version: Option<String>,
    instructions: Option<String>,
    capabilities: Capabilities,
    tools: Vec<McpTool>,
    /// Restarts spent since the last good start.
    restarts: u8,
}

/// One configured server and its connection.
pub struct Server {
    pub config: ServerConfig,
    live: Mutex<Live>,
    /// Set when the server is being stopped, so a connection in progress
    /// does not publish itself afterwards.
    stopped: AtomicBool,
    /// Set by the one reload that starts its connection, so a server carried
    /// over while it is still coming up is connected once and only once.
    claimed: AtomicBool,
}

impl Server {
    fn new(config: ServerConfig) -> Arc<Self> {
        let disabled = !config.enabled
            || matches!(
                config.admission,
                Some(Admission::Pending | Admission::Rejected)
            );
        Arc::new(Self {
            config,
            live: Mutex::new(Live {
                connection: if disabled {
                    Connection::Disabled
                } else {
                    Connection::Connecting
                },
                needs_auth: false,
                failure: None,
                transport: None,
                protocol_version: None,
                instructions: None,
                capabilities: Capabilities::default(),
                tools: Vec::new(),
                restarts: 0,
            }),
            stopped: AtomicBool::new(false),
            claimed: AtomicBool::new(false),
        })
    }

    fn live(&self) -> std::sync::MutexGuard<'_, Live> {
        self.live.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn view(&self, reloading: bool) -> ServerView {
        let live = self.live();
        // A ready server whose connection has gone is a failed one
        // (`observedConnection`, `health.zig:16-22`).
        let connection = match (&live.transport, live.connection) {
            (Some(transport), Connection::Ready) if !transport.alive() => Connection::Failed,
            (_, connection) => connection,
        };
        ServerView {
            name: self.config.name.clone(),
            source: self.config.source,
            transport: self.config.transport,
            admission: self.config.admission,
            connection,
            needs_auth: live.needs_auth,
            required: self.config.required,
            reloading,
            protocol_version: live.protocol_version.clone(),
            capabilities: live.capabilities,
            tool_count: (connection == Connection::Ready).then_some(live.tools.len()),
            failure: live.failure.clone(),
        }
    }

    /// Whether a connection should be made for it.
    fn should_connect(&self) -> bool {
        self.live().connection == Connection::Connecting
    }

    /// The tools, while it is ready.
    pub fn tools(&self) -> Vec<McpTool> {
        let live = self.live();
        if live.connection == Connection::Ready {
            live.tools.clone()
        } else {
            Vec::new()
        }
    }

    pub fn instructions(&self) -> Option<String> {
        self.live().instructions.clone()
    }

    fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
        let transport = self.live().transport.take();
        if let Some(transport) = transport {
            transport.close();
        }
    }
}

/// Everything the lock covers.
struct State {
    servers: Vec<Arc<Server>>,
    issues: Vec<String>,
    reloading: bool,
    /// A reload asked for while one was running, and where it is told.
    again: Option<Origin>,
    prompts_suppressed: bool,
    /// The server the trust prompt is up for.
    prompting: Option<String>,
    /// The configuration the last reload read, for one that only changes
    /// trust choices to compare against.
    choices: Choices,
}

/// How a workspace server's `${VAR}` is looked up.
type Lookup = dyn Fn(&str) -> Option<String> + Send + Sync;

/// What an HTTP server is sent besides the protocol's own headers
/// (`buildResolvedHeaders`, `vercel-labs/fx@c1db9191 src/core/mcp/
/// server_auth.zig:64-110`): its `headers` as written, then each `header_env`
/// header with the value of the variable it names, then `Authorization:
/// Bearer` with the value of `bearer_token_env`. A variable that is not set
/// fails the connection by upstream's name rather than sending the header
/// without it. The values are read when the server connects and go nowhere
/// but its URL.
pub(super) fn http_headers(
    config: &ServerConfig,
    environment: &Lookup,
) -> Result<Vec<(String, String)>, McpError> {
    let mut headers = config.headers.clone();
    for (name, variable) in &config.header_env {
        let value = environment(variable).ok_or(McpError::Named("McpHeaderEnvironmentMissing"))?;
        headers.push((name.clone(), value));
    }
    if let Some(variable) = &config.bearer_token_env {
        let token = environment(variable).ok_or(McpError::Named("McpBearerEnvironmentMissing"))?;
        headers.push(("Authorization".to_string(), format!("Bearer {token}")));
    }
    Ok(headers)
}

/// The session's MCP servers.
pub struct McpRuntime {
    paths: Paths,
    state: Mutex<State>,
    names: Mutex<ToolNames>,
    events: Mutex<VecDeque<Event>>,
    jobs: Mutex<Option<mpsc::Sender<Job>>>,
    /// Bumped by every reload that put a new set in force.
    generation: AtomicU64,
    /// What the environment says, for a workspace server's `${VAR}` and an
    /// HTTP server's `header_env` and `bearer_token_env`.
    environment: Box<Lookup>,
}

impl std::fmt::Debug for McpRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpRuntime")
            .field("paths", &self.paths)
            .finish_non_exhaustive()
    }
}

impl McpRuntime {
    /// A runtime over `paths`, with nothing loaded and nothing started.
    pub fn new(paths: Paths) -> Arc<Self> {
        Arc::new(Self {
            paths,
            state: Mutex::new(State {
                servers: Vec::new(),
                issues: Vec::new(),
                reloading: false,
                again: None,
                prompts_suppressed: false,
                prompting: None,
                choices: Choices::default(),
            }),
            names: Mutex::new(ToolNames::new(crate::tools::ADVERTISED_TOOLS)),
            events: Mutex::new(VecDeque::new()),
            jobs: Mutex::new(None),
            generation: AtomicU64::new(0),
            environment: Box::new(|name| std::env::var(name).ok()),
        })
    }

    /// A runtime that reads no file: a test's, or a session with no profile.
    pub fn inert() -> Arc<Self> {
        Self::new(Paths::default())
    }

    pub fn paths(&self) -> &Paths {
        &self.paths
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Something for the UI.
    pub fn say(&self, event: Event) {
        self.events
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push_back(event);
    }

    /// A notice under `mcp`, or under no topic when it is a usage line.
    pub fn notice(&self, warning: bool, body: impl Into<String>) {
        self.say(Event::Notice {
            warning,
            body: body.into(),
        });
    }

    /// Everything said since the last call, oldest first.
    pub fn take_events(&self) -> Vec<Event> {
        self.events
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .drain(..)
            .collect()
    }

    /// Every configured server, in the order the files list them: the
    /// profile's, then the workspace's.
    pub fn views(&self) -> Vec<ServerView> {
        let state = self.state();
        state
            .servers
            .iter()
            .map(|server| server.view(state.reloading && server.should_connect()))
            .collect()
    }

    /// The configuration problems the last reload found.
    pub fn issues(&self) -> Vec<String> {
        self.state().issues.clone()
    }

    /// The server called `name`.
    pub fn server(&self, name: &str) -> Option<Arc<Server>> {
        self.state()
            .servers
            .iter()
            .find(|server| server.config.name == name)
            .cloned()
    }

    fn servers(&self) -> Vec<Arc<Server>> {
        self.state().servers.clone()
    }

    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    // -----------------------------------------------------------------------
    // the worker
    // -----------------------------------------------------------------------

    /// Hands `job` to the worker thread, starting it the first time.
    pub fn submit(self: &Arc<Self>, job: Job) {
        let mut jobs = self.jobs.lock().unwrap_or_else(PoisonError::into_inner);
        if jobs.is_none() {
            let (sender, receiver) = mpsc::channel::<Job>();
            let runtime = Arc::downgrade(self);
            let spawned = std::thread::Builder::new()
                .name("xfx-mcp".into())
                .spawn(move || {
                    while let Ok(job) = receiver.recv() {
                        let Some(runtime) = runtime.upgrade() else {
                            return;
                        };
                        runtime.run(job);
                    }
                });
            if spawned.is_err() {
                drop(jobs);
                self.notice(
                    true,
                    "MCP is unavailable: its worker thread could not start.",
                );
                return;
            }
            *jobs = Some(sender);
        }
        if let Some(sender) = jobs.as_ref() {
            let _ = sender.send(job);
        }
    }

    /// Runs one job on the calling thread: the worker's, or a test's.
    pub fn run(self: &Arc<Self>, job: Job) {
        match job {
            Job::Command(line) => super::command::handle(self, &line),
            Job::Trust { action, body } => self.trust(action, &body),
            Job::Catalog {
                request,
                section,
                server,
            } => {
                let result = self.catalog(section, server.as_deref());
                self.say(Event::Catalog { request, result });
            }
            Job::MenuReload => self.reload(Origin::Menu),
        }
    }

    fn catalog(&self, section: Section, server: Option<&str>) -> Result<Catalog, String> {
        let failed = |err: McpError| format!("MCP catalog failed: {err}");
        match section {
            Section::Tools => {
                let mut names: Vec<String> = self
                    .tools()
                    .into_iter()
                    .map(|(_, tool)| tool.alias)
                    .collect();
                names.sort();
                Ok(Catalog::Tools(names))
            }
            Section::Resources => {
                let name = server.ok_or_else(|| failed(McpError::Named("McpServerNotFound")))?;
                match self.list_resources(name, false, None) {
                    Ok(mut resources) => {
                        let templates = self.list_resources(name, true, None).map_err(failed)?;
                        resources.extend(templates);
                        Ok(Catalog::Resources(resources))
                    }
                    // An empty catalog, not a failure (`completeEmptyMenuCatalog`,
                    // `app_mcp_runtime.zig:1481-1484`).
                    Err(McpError::Named("McpResourcesUnsupported")) => {
                        Ok(Catalog::Resources(Vec::new()))
                    }
                    Err(err) => Err(failed(err)),
                }
            }
            Section::Prompts => {
                let name = server.ok_or_else(|| failed(McpError::Named("McpServerNotFound")))?;
                match self.list_prompts(name, None) {
                    Ok(prompts) => Ok(Catalog::Prompts(prompts)),
                    Err(McpError::Named("McpPromptsUnsupported")) => {
                        Ok(Catalog::Prompts(Vec::new()))
                    }
                    Err(err) => Err(failed(err)),
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // loading, connecting, reloading
    // -----------------------------------------------------------------------

    /// The session's first load, on the worker thread.
    ///
    /// The configured servers are put in force before this returns -- each
    /// `Connecting` or `Disabled`, and the trust prompt up for a pending one
    /// -- so the session's first frame and its record already name them;
    /// connecting them is the thread's.
    pub fn start(self: &Arc<Self>) {
        if self.paths.profile.is_none() && self.paths.settings.is_none() {
            return;
        }
        if let Ok((configs, issues, choices)) = self.read_configuration() {
            let mut state = self.state();
            state.servers = configs.into_iter().map(Server::new).collect();
            state.issues = issues;
            state.choices = choices;
            drop(state);
            self.present_prompt();
        }
        let runtime = Arc::clone(self);
        let _ = std::thread::Builder::new()
            .name("xfx-mcp-start".into())
            .spawn(move || runtime.reload(Origin::Startup));
    }

    /// What both files say now.
    fn read_configuration(&self) -> Result<(Vec<ServerConfig>, Vec<String>, Choices), McpError> {
        let mut servers = match &self.paths.profile {
            Some(path) => {
                config::load_profile(path)
                    .map_err(|err| McpError::Named(err.0))?
                    .servers
            }
            None => Vec::new(),
        };
        let choices = config::load_choices(self.paths.settings.as_deref(), &self.paths.workspace);
        let workspace = if self.paths.profile.is_some() || self.paths.settings.is_some() {
            config::load_workspace(&self.paths.workspace, &choices, &*self.environment)
        } else {
            config::WorkspaceServers::default()
        };
        // A workspace server named like a profile one is the profile's.
        for server in workspace.servers {
            if !servers.iter().any(|existing| existing.name == server.name) {
                servers.push(server);
            }
        }
        Ok((servers, workspace.issues, choices))
    }

    /// Reads both files again and connects what changed, telling `origin`
    /// how it went. A reload asked for while one runs runs after it.
    pub fn reload(self: &Arc<Self>, origin: Origin) {
        {
            let mut state = self.state();
            if state.reloading {
                state.again = Some(match state.again {
                    Some(Origin::Transcript) => Origin::Transcript,
                    _ => origin,
                });
                return;
            }
            state.reloading = true;
        }
        self.say(Event::Changed);
        let mut origin = origin;
        loop {
            self.reload_once(origin);
            let mut state = self.state();
            match state.again.take() {
                Some(next) => origin = next,
                None => {
                    state.reloading = false;
                    break;
                }
            }
        }
        self.say(Event::Changed);
        self.present_prompt();
    }

    fn reload_once(self: &Arc<Self>, origin: Origin) {
        let (configs, issues, choices) = match self.read_configuration() {
            Ok(read) => read,
            Err(err) => {
                let body = match origin {
                    Origin::Menu => format!("MCP reload failed: {err}"),
                    _ => "MCP configuration could not be reloaded. Your existing MCP servers are still active. Check the configuration and run /mcp list for details before trying again.".to_string(),
                };
                match origin {
                    Origin::Menu => self.say(Event::MenuFeedback(body)),
                    _ => self.notice(true, body),
                }
                return;
            }
        };
        // The servers carried over: same configuration, and up or coming up.
        let current = self.servers();
        let mut next: Vec<Arc<Server>> = Vec::with_capacity(configs.len());
        let mut connecting: Vec<Arc<Server>> = Vec::new();
        for config in configs {
            let kept = current.iter().find(|server| {
                server.config == config
                    && matches!(
                        server.live().connection,
                        Connection::Ready | Connection::Connecting
                    )
                    && !server.stopped.load(Ordering::Acquire)
            });
            let server = match kept {
                Some(server) => Arc::clone(server),
                None => Server::new(config),
            };
            if server.should_connect() && !server.claimed.swap(true, Ordering::AcqRel) {
                connecting.push(Arc::clone(&server));
            }
            next.push(server);
        }
        // The first load publishes at once, so the menu and the model see
        // `Connecting` rather than nothing while servers come up.
        if origin == Origin::Startup {
            let mut state = self.state();
            state.servers = next.clone();
            state.issues = issues.clone();
            state.choices = choices.clone();
            drop(state);
            self.present_prompt();
            self.say(Event::Changed);
        }
        let threads: Vec<_> = connecting
            .iter()
            .map(|server| {
                let server = Arc::clone(server);
                let runtime = Arc::clone(self);
                std::thread::Builder::new()
                    .name("xfx-mcp-connect".into())
                    .spawn(move || runtime.connect(&server))
            })
            .collect();
        for thread in threads.into_iter().flatten() {
            let _ = thread.join();
        }
        let retired: Vec<Arc<Server>> = {
            let mut state = self.state();
            let old = std::mem::replace(&mut state.servers, next.clone());
            state.issues = issues;
            state.choices = choices;
            old.into_iter()
                .filter(|server| !next.iter().any(|kept| Arc::ptr_eq(kept, server)))
                .collect()
        };
        for server in retired {
            server.stop();
        }
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.report(origin, &next);
    }

    /// Says how a reload went (`formatMcpPublishedReload`,
    /// `app_commands.zig:91-129`; the menu's `applyMenuReloadCompletion`,
    /// `app_mcp_runtime.zig:1530-1560`; the startup notice,
    /// `health.zig:438-480`).
    fn report(&self, origin: Origin, servers: &[Arc<Server>]) {
        let views: Vec<ServerView> = servers.iter().map(|server| server.view(false)).collect();
        let unavailable: Vec<&str> = views
            .iter()
            .filter(|view| {
                matches!(
                    view.status(),
                    Status::Failed | Status::NeedsAuth | Status::Unavailable
                )
            })
            .map(|view| view.name.as_str())
            .collect();
        match origin {
            Origin::Startup => {
                let needs_auth = views
                    .iter()
                    .filter(|view| view.status() == Status::NeedsAuth)
                    .count();
                let failed = views
                    .iter()
                    .filter(|view| view.status() == Status::Failed)
                    .count();
                let total = needs_auth + failed;
                if total == 0 {
                    return;
                }
                let mut body = format!(
                    "MCP startup: {total} server{} need{} attention",
                    if total == 1 { "" } else { "s" },
                    if total == 1 { "s" } else { "" }
                );
                if needs_auth > 0 {
                    body.push_str(&format!(
                        ", {needs_auth} need{} authentication",
                        if needs_auth == 1 { "s" } else { "" }
                    ));
                }
                if failed > 0 {
                    body.push_str(&format!(", {failed} failed"));
                }
                body.push('.');
                if let Some(first) = views.iter().find(|view| view.status() == Status::NeedsAuth) {
                    body.push_str(&format!(" Run /mcp auth {} --open.", first.name));
                }
                body.push_str(" Use /mcp list for details.");
                self.notice(false, body);
            }
            Origin::Menu => {
                let body = if unavailable.is_empty() {
                    "MCP configuration reloaded.".to_string()
                } else {
                    format!(
                        "MCP reloaded with {} unavailable server{}.",
                        unavailable.len(),
                        if unavailable.len() == 1 { "" } else { "s" }
                    )
                };
                self.say(Event::MenuFeedback(body));
            }
            Origin::Transcript => {
                let body = match unavailable.as_slice() {
                    [] if views.is_empty() => {
                        "MCP configuration reloaded. No servers are configured.".to_string()
                    }
                    [] => "MCP configuration reloaded successfully.".to_string(),
                    [one] => format!(
                        "MCP configuration reloaded, but server '{one}' is unavailable. Run /mcp list for details."
                    ),
                    many => format!(
                        "MCP configuration reloaded, but {} servers are unavailable: {}. Run /mcp list for details.",
                        many.len(),
                        many.iter()
                            .map(|name| format!("'{name}'"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                };
                self.notice(!unavailable.is_empty(), body);
            }
        }
    }

    /// Connects `server`, starting it again up to its restart limit.
    fn connect(&self, server: &Arc<Server>) {
        let limit = if server.config.transport == Kind::Stdio {
            server.config.restart_limit
        } else {
            0
        };
        let mut attempt = 0u8;
        loop {
            if server.stopped.load(Ordering::Acquire) {
                return;
            }
            match self.open(server) {
                Ok(()) => return,
                Err(err) => {
                    let auth = err == McpError::Named("McpAuthenticationRequired");
                    if !auth && attempt < limit && !server.stopped.load(Ordering::Acquire) {
                        attempt += 1;
                        continue;
                    }
                    let mut live = server.live();
                    live.connection = Connection::Failed;
                    live.needs_auth = auth;
                    live.failure = Some(err.to_string());
                    return;
                }
            }
        }
    }

    /// One attempt: start, initialize, list the tools, publish.
    fn open(&self, server: &Arc<Server>) -> Result<(), McpError> {
        let config = &server.config;
        let deadline = Instant::now() + config.startup_timeout();
        let left = || deadline.saturating_duration_since(Instant::now());
        let transport: Arc<dyn Transport> = match config.transport {
            Kind::Stdio => Arc::new(StdioTransport::spawn(
                &config.command,
                &config.args,
                &config.env,
                None,
            )?),
            Kind::Http => Arc::new(HttpTransport::open(
                &config.url,
                &http_headers(config, &*self.environment)?,
            )?),
            Kind::Sse => return Err(McpError::Named("McpTransportUnsupported")),
        };
        let result = self.handshake(server, &transport, &left);
        let (version, capabilities, instructions, tools) = match result {
            Ok(found) => found,
            Err(err) => {
                let diagnostic = transport.diagnostic();
                transport.close();
                return Err(match (err, diagnostic) {
                    (McpError::Named("McpConnectionClosed"), Some(_)) => {
                        McpError::Named("McpInitFailed")
                    }
                    (err, _) => err,
                });
            }
        };
        if server.stopped.load(Ordering::Acquire) {
            transport.close();
            return Ok(());
        }
        let mut live = server.live();
        live.connection = Connection::Ready;
        live.needs_auth = false;
        live.failure = None;
        live.transport = Some(transport);
        live.protocol_version = Some(version);
        live.capabilities = capabilities;
        live.instructions = instructions;
        live.tools = tools;
        Ok(())
    }

    #[allow(clippy::type_complexity)]
    fn handshake(
        &self,
        server: &Server,
        transport: &Arc<dyn Transport>,
        left: &dyn Fn() -> Duration,
    ) -> Result<(String, Capabilities, Option<String>, Vec<McpTool>), McpError> {
        let initialized = transport.request(
            "initialize",
            json!({
                "protocolVersion": REQUESTED_VERSION,
                // No elicitation, sampling or roots: xfx answers none of
                // them, so it advertises none of them.
                "capabilities": {},
                "clientInfo": {"name": "xfx", "version": env!("CARGO_PKG_VERSION")},
            }),
            left(),
            None,
        )?;
        let version = initialized
            .get("protocolVersion")
            .and_then(Value::as_str)
            .ok_or(McpError::Named("McpUnsupportedProtocolVersion"))?;
        let accepted: &[&str] = if server.config.transport == Kind::Stdio {
            &STDIO_VERSIONS
        } else {
            &HTTP_VERSIONS
        };
        if !accepted.contains(&version) {
            return Err(McpError::Named("McpUnsupportedProtocolVersion"));
        }
        let advertised = initialized.get("capabilities").and_then(Value::as_object);
        let has = |key: &str| {
            advertised.is_some_and(|map| map.get(key).is_some_and(|value| !value.is_null()))
        };
        let capabilities = Capabilities {
            tools: has("tools"),
            resources: has("resources"),
            prompts: has("prompts"),
            completions: has("completions"),
        };
        let instructions = initialized
            .get("instructions")
            .and_then(Value::as_str)
            .map(str::to_string);
        transport.notify(
            "notifications/initialized",
            (server.config.transport != Kind::Stdio).then(|| json!({})),
        )?;
        let mut tools = Vec::new();
        if capabilities.tools {
            let mut cursor: Option<String> = None;
            for _ in 0..MAX_PAGES {
                let params = match &cursor {
                    Some(cursor) => json!({"cursor": cursor}),
                    None => json!({}),
                };
                let page = transport.request("tools/list", params, left(), None)?;
                for tool in page
                    .get("tools")
                    .and_then(Value::as_array)
                    .ok_or(McpError::Named("McpInvalidResult"))?
                {
                    if let Some(tool) = self.tool(&server.config.name, tool) {
                        tools.push(tool);
                    }
                }
                cursor = page
                    .get("nextCursor")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                if cursor.is_none() {
                    break;
                }
            }
        }
        Ok((version.to_string(), capabilities, instructions, tools))
    }

    /// One `tools/list` entry, named for the model.
    fn tool(&self, server: &str, entry: &Value) -> Option<McpTool> {
        let name = entry.get("name")?.as_str()?.to_string();
        let mut schema = entry
            .get("inputSchema")
            .cloned()
            .unwrap_or_else(|| json!({"type": "object"}));
        if !schema.is_object() {
            return None;
        }
        // A model schema is an object schema, as upstream's dynamic tool
        // schema requires one (`dynamicFunctionSchemaJsonAlloc`).
        if schema.get("type").is_none() {
            schema
                .as_object_mut()
                .expect("an object")
                .insert("type".into(), Value::from("object"));
        }
        let alias = self
            .names
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .name(server, &name)?;
        Some(McpTool {
            tags: super::names::tags(server, &name),
            alias,
            title: entry
                .get("title")
                .and_then(Value::as_str)
                .map(str::to_string),
            description: entry
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            input_schema: schema,
            name,
        })
    }

    // -----------------------------------------------------------------------
    // trust
    // -----------------------------------------------------------------------

    /// The workspace server the trust prompt should ask about next: the
    /// first one still pending, unless the person dismissed the prompts.
    pub fn pending_prompt(&self) -> Option<String> {
        let state = self.state();
        if state.prompts_suppressed {
            return None;
        }
        state
            .servers
            .iter()
            .find(|server| server.config.admission == Some(Admission::Pending))
            .map(|server| server.config.name.clone())
    }

    /// Puts the trust prompt up for the next pending server, when one is
    /// pending and none is up.
    fn present_prompt(&self) {
        let Some(name) = self.pending_prompt() else {
            self.state().prompting = None;
            return;
        };
        {
            let mut state = self.state();
            if state.prompting.as_deref() == Some(name.as_str()) {
                return;
            }
            state.prompting = Some(name.clone());
        }
        self.say(Event::Raw(format!(
            "Project MCP server '{}' is defined in .mcp.json.\n  [1] approve  [2] approve all  [3] reject  [esc] dismiss remaining prompts\n",
            super::terminal_safe(&name)
        )));
        self.say(Event::TrustPrompt { name });
    }

    /// The prompt has been answered or overtaken: the next reload may put
    /// one up again.
    pub fn prompt_answered(&self) {
        self.state().prompting = None;
    }

    /// Esc at the prompt: no more prompts for this process.
    pub fn suppress_prompts(&self) {
        let mut state = self.state();
        state.prompts_suppressed = true;
        state.prompting = None;
    }

    /// A trust choice (`applyProjectMcpAction`, `app_commands.zig:1407-1466`):
    /// saved, announced with `body`, and put in force by a reload.
    fn trust(self: &Arc<Self>, action: TrustAction, body: &str) {
        self.prompt_answered();
        let Some(settings) = self.paths.settings.clone() else {
            self.notice(
                true,
                "Project MCP choices were not applied: SettingsUnavailable.",
            );
            self.present_prompt();
            return;
        };
        match config::save_action(&settings, &self.paths.workspace, &action) {
            Ok(_) => {
                self.notice(false, body);
                self.reload(Origin::Transcript);
            }
            Err(err) => {
                self.notice(
                    true,
                    format!(
                        "Project MCP choices were not applied: {}.",
                        crate::output::safe_one_line(&err.to_string(), 200)
                    ),
                );
                self.present_prompt();
            }
        }
    }

    // -----------------------------------------------------------------------
    // what the model sees
    // -----------------------------------------------------------------------

    /// Every ready server's tools.
    pub fn tools(&self) -> Vec<(Arc<Server>, McpTool)> {
        self.servers()
            .into_iter()
            .flat_map(|server| {
                server
                    .tools()
                    .into_iter()
                    .map(move |tool| (Arc::clone(&server), tool))
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// The tool the model calls `alias`, with the server it is on.
    pub fn tool_by_alias(&self, alias: &str) -> Option<(Arc<Server>, McpTool)> {
        self.tools()
            .into_iter()
            .find(|(_, tool)| tool.alias == alias)
    }

    /// Calls `alias` with `arguments`, under the server's operation timeout.
    /// A stdio server that has stopped is started again once, inside its
    /// restart limit (`restart_failed_result`, `tool_result.zig:196-215`).
    pub fn call_tool(
        &self,
        alias: &str,
        arguments: Value,
        cancel: Option<&CancelToken>,
    ) -> Result<(Arc<Server>, McpTool, Value), McpError> {
        let (server, tool) = self
            .tool_by_alias(alias)
            .ok_or(McpError::Named("McpToolNotFound"))?;
        let transport = self.transport(&server)?;
        let result = transport.request(
            "tools/call",
            json!({"name": tool.name, "arguments": arguments}),
            server.config.operation_timeout(),
            cancel,
        )?;
        Ok((server, tool, result))
    }

    /// A ready server's connection, started again when it has gone and a
    /// restart is left.
    fn transport(&self, server: &Arc<Server>) -> Result<Arc<dyn Transport>, McpError> {
        let (transport, restarts) = {
            let live = server.live();
            (live.transport.clone(), live.restarts)
        };
        match transport {
            Some(transport) if transport.alive() => Ok(transport),
            Some(_)
                if server.config.transport == Kind::Stdio
                    && restarts < server.config.restart_limit =>
            {
                server.live().restarts = restarts + 1;
                match self.open(server) {
                    Ok(()) => server
                        .live()
                        .transport
                        .clone()
                        .ok_or(McpError::Named("McpConnectionClosed")),
                    Err(err) => {
                        let mut live = server.live();
                        live.connection = Connection::Failed;
                        live.failure = Some(err.to_string());
                        Err(McpError::Restart(err.to_string()))
                    }
                }
            }
            Some(_) => Err(McpError::Restart(
                server
                    .live()
                    .failure
                    .clone()
                    .unwrap_or_else(|| "McpConnectionClosed".to_string()),
            )),
            None => Err(McpError::Named("McpServerUnavailable")),
        }
    }

    // -----------------------------------------------------------------------
    // resources, prompts, completion
    // -----------------------------------------------------------------------

    /// The server a feature operation is about (`awaitFeatureServer`,
    /// `mcp_runtime.zig:131-147`): known, admitted, and done connecting.
    fn feature_server(
        &self,
        name: &str,
        cancel: Option<&CancelToken>,
    ) -> Result<Arc<Server>, McpError> {
        let server = self
            .server(name)
            .ok_or(McpError::Named("McpServerNotFound"))?;
        if server.config.source == Source::Workspace
            && server.config.admission != Some(Admission::Approved)
        {
            return Err(McpError::Named("McpWorkspaceApprovalRequired"));
        }
        let deadline = Instant::now() + server.config.operation_timeout();
        while server.live().connection == Connection::Connecting {
            if cancel.is_some_and(CancelToken::is_cancelled) {
                return Err(McpError::Named("Cancelled"));
            }
            if Instant::now() >= deadline {
                return Err(McpError::Named("McpRequestTimedOut"));
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(server)
    }

    fn feature_request(
        &self,
        server: &Arc<Server>,
        method: &str,
        params: Value,
        cancel: Option<&CancelToken>,
    ) -> Result<Value, McpError> {
        let transport = self.transport(server)?;
        transport.request(method, params, server.config.operation_timeout(), cancel)
    }

    /// Every page of one list method's `key`.
    fn paged(
        &self,
        server: &Arc<Server>,
        method: &str,
        key: &str,
        cancel: Option<&CancelToken>,
    ) -> Result<Vec<Value>, McpError> {
        let mut items = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let params = match &cursor {
                Some(cursor) => json!({"cursor": cursor}),
                None => json!({}),
            };
            let page = self.feature_request(server, method, params, cancel)?;
            items.extend(
                page.get(key)
                    .and_then(Value::as_array)
                    .ok_or(McpError::Named("McpInvalidResult"))?
                    .iter()
                    .cloned(),
            );
            cursor = page
                .get("nextCursor")
                .and_then(Value::as_str)
                .map(str::to_string);
            if cursor.is_none() {
                break;
            }
        }
        Ok(items)
    }

    /// `listResources` (`mcp_runtime.zig:2430-2453`).
    pub fn list_resources(
        &self,
        name: &str,
        templates: bool,
        cancel: Option<&CancelToken>,
    ) -> Result<Vec<Resource>, McpError> {
        let server = self.feature_server(name, cancel)?;
        if !server.live().capabilities.resources {
            return Err(McpError::Named("McpResourcesUnsupported"));
        }
        let (method, key, uri) = if templates {
            (
                "resources/templates/list",
                "resourceTemplates",
                "uriTemplate",
            )
        } else {
            ("resources/list", "resources", "uri")
        };
        let items = self.paged(&server, method, key, cancel)?;
        Ok(items
            .iter()
            .filter_map(|item| {
                Some(Resource {
                    uri: item.get(uri)?.as_str()?.to_string(),
                    name: item
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    title: item
                        .get("title")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    description: item
                        .get("description")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    mime_type: item
                        .get("mimeType")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    template: templates,
                })
            })
            .collect())
    }

    /// `readResource`: the `contents` the server returned for `uri`.
    pub fn read_resource(
        &self,
        name: &str,
        uri: &str,
        cancel: Option<&CancelToken>,
    ) -> Result<Vec<Value>, McpError> {
        let server = self.feature_server(name, cancel)?;
        if !server.live().capabilities.resources {
            return Err(McpError::Named("McpResourcesUnsupported"));
        }
        let result =
            self.feature_request(&server, "resources/read", json!({"uri": uri}), cancel)?;
        result
            .get("contents")
            .and_then(Value::as_array)
            .cloned()
            .ok_or(McpError::Named("McpInvalidResult"))
    }

    /// `listPrompts` (`mcp_runtime.zig:2455-2476`).
    pub fn list_prompts(
        &self,
        name: &str,
        cancel: Option<&CancelToken>,
    ) -> Result<Vec<Prompt>, McpError> {
        let server = self.feature_server(name, cancel)?;
        if !server.live().capabilities.prompts {
            return Err(McpError::Named("McpPromptsUnsupported"));
        }
        let items = self.paged(&server, "prompts/list", "prompts", cancel)?;
        Ok(items
            .iter()
            .filter_map(|item| {
                Some(Prompt {
                    name: item.get("name")?.as_str()?.to_string(),
                    title: item
                        .get("title")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    description: item
                        .get("description")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    arguments: item
                        .get("arguments")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(|argument| {
                            Some(PromptArgument {
                                name: argument.get("name")?.as_str()?.to_string(),
                                required: argument.get("required").and_then(Value::as_bool)
                                    == Some(true),
                                description: argument
                                    .get("description")
                                    .and_then(Value::as_str)
                                    .map(str::to_string),
                            })
                        })
                        .collect(),
                })
            })
            .collect())
    }

    /// `getPrompt`: the prompt's description and messages.
    pub fn get_prompt(
        &self,
        name: &str,
        prompt: &str,
        arguments: Map<String, Value>,
        cancel: Option<&CancelToken>,
    ) -> Result<Value, McpError> {
        let server = self.feature_server(name, cancel)?;
        if !server.live().capabilities.prompts {
            return Err(McpError::Named("McpPromptsUnsupported"));
        }
        self.feature_request(
            &server,
            "prompts/get",
            json!({"name": prompt, "arguments": arguments}),
            cancel,
        )
    }

    /// `completion/complete` for a prompt's argument or a template's variable.
    pub fn complete(
        &self,
        name: &str,
        reference: Value,
        argument: &str,
        value: &str,
        context: Map<String, Value>,
        cancel: Option<&CancelToken>,
    ) -> Result<Value, McpError> {
        let server = self.feature_server(name, cancel)?;
        let capabilities = server.live().capabilities;
        let wants_prompts = reference.get("type").and_then(Value::as_str) == Some("ref/prompt");
        if wants_prompts && !capabilities.prompts {
            return Err(McpError::Named("McpPromptsUnsupported"));
        }
        if !wants_prompts && !capabilities.resources {
            return Err(McpError::Named("McpResourcesUnsupported"));
        }
        if !capabilities.completions {
            return Err(McpError::Named("McpCompletionsUnsupported"));
        }
        let mut params = json!({"ref": reference, "argument": {"name": argument, "value": value}});
        if !context.is_empty() {
            params["context"] = json!({"arguments": context});
        }
        let result = self.feature_request(&server, "completion/complete", params, cancel)?;
        result
            .get("completion")
            .cloned()
            .ok_or(McpError::Named("McpInvalidResult"))
    }

    // -----------------------------------------------------------------------
    // the end
    // -----------------------------------------------------------------------

    /// Stops every server. What the session does on its way out.
    pub fn shutdown(&self) {
        self.jobs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        let servers = std::mem::take(&mut self.state().servers);
        for server in servers {
            server.stop();
        }
    }

    /// The profile file's path, when there is a profile.
    pub fn profile_path(&self) -> Option<&Path> {
        self.paths.profile.as_deref()
    }
}

impl Drop for McpRuntime {
    fn drop(&mut self) {
        self.shutdown();
    }
}
