//! `/mcp`'s menu: fx v0.0.16's picker of the servers, under the composer.
//!
//! A port of `vercel-labs/fx@481104fd src/core/mcp_host/menu.zig` (the state
//! and its keys) and `src/ui/footer/mcp_host_menu_presentation.zig` (its
//! rows), as `p5-mcp-menu-new` shows them: a titled list -- the servers that
//! work, then the ones that need attention -- a server's screen with its
//! facts, its choices and its tools, the question Remove and Reject ask on
//! their own row, and the screen an approval keeps. The composer holds the
//! filter, as fx's does: the list's on the list, the open server's tools' on
//! its screen (`swapHostMenuFilter`, `app_mcp_runtime.zig:643-653`).
//!
//! **A choice runs a verb.** Remove, Reject and Approve are `/mcp`'s own
//! verbs, run warnings-only (`.warnings_only`, `app_mcp_runtime.zig:545-560`):
//! the row's status follows, and only a warning reaches the transcript. What
//! the menu adds is what it binds: Approve approves the entry the approval's
//! screen drew, by that entry's digest, and only once every row of it was on
//! the screen ([`Drawn`]) -- the launch trust prompt is gone (`ba0ea4a9`), so
//! this screen and `/mcp approve` are where a project server is approved.
//!
//! **Gated, and not offered.** fx's `Log in` and `Log out` choices sign in to
//! a remote server; that is not built (`docs/parity.md`), so neither is a
//! choice here, and a server that needs a login says xfx's sentence for it
//! where fx says its tools show once you log in.
//!
//! **What reaches the screen.** Every name, address, command, error and tool
//! is the configuration's or a server's, so every one is spelled
//! ([`crate::mcp::command::safe_within`], [`crate::mcp::terminal_safe`]) and
//! a name is said only as far as its first `=`, as the verbs say it
//! ([`crate::mcp::command::typed`]). A variable's or a header's value is never
//! shown: the approval's screen names the variables and headers an entry
//! sets, and nothing they hold.

use std::cell::RefCell;
use std::time::{Duration, Instant};

use crate::mcp::command;
use crate::mcp::config::{Admission, File, Source, Transport};
use crate::mcp::runtime::{Connection, McpRuntime, ServerView};

use super::picker::{cells, ellipsized, pad_to};
use super::theme::Palette;

/// The lines a server's screen has below its title (`server_window`,
/// `menu.zig:267-271`).
const SERVER_WINDOW: usize = 12;

/// The most rows the menu takes (`max_inline_rows`, presentation `:16`):
/// the server window and its title. The approval's screen alone may take
/// more, to show all of what it trusts ([`McpMenu::height`]).
pub(crate) const MAX_ROWS: u16 = SERVER_WINDOW as u16 + 1;

/// Where a server's facts start (`fact_col`, presentation `:17-19`).
const FACT_COL: usize = 12;

/// How often the open menu reads the servers again (`refreshHostMenu`,
/// `app_mcp_runtime.zig:719-728`).
const REFRESH: Duration = Duration::from_millis(250);

/// The most bytes of the list's filter kept while a server's screen filters
/// its tools (`name_capacity`, `menu.zig:193`).
const NAME_CAPACITY: usize = 256;

/// The screens (`Screen`, `menu.zig:12`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Screen {
    List,
    Server,
    Confirm(Choice),
}

/// A server's choices (`Choice`, `menu.zig:14-49`), without fx's `Log in`
/// and `Log out`: signing in is gated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Choice {
    Approve,
    Reject,
    Remove,
}

impl Choice {
    fn label(self) -> &'static str {
        match self {
            Self::Approve => "Approve",
            Self::Reject => "Reject",
            Self::Remove => "Remove",
        }
    }

    /// The `/mcp` verb it runs.
    fn verb(self) -> &'static str {
        match self {
            Self::Approve => "approve",
            Self::Reject => "reject",
            Self::Remove => "remove",
        }
    }

    /// Remove and Reject ask on the choice's own row, with yes and no; an
    /// approval keeps its own screen, which shows everything it trusts
    /// (`asksOnRow`, `menu.zig:44-48`). All three ask first.
    fn asks_on_row(self) -> bool {
        matches!(self, Self::Remove | Self::Reject)
    }
}

/// A key the menu takes (`Key`, `menu.zig:242-243`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Key {
    Up,
    Down,
    Left,
    Right,
    Enter,
    Escape,
}

/// A choice made, for the shell to run through the verbs' own paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Run {
    /// Approve the entry the approval's screen drew: `name`, as read with
    /// this digest ([`crate::mcp::config::TrustAction::ApproveShown`]).
    Approve { name: String, digest: String },
    /// `reject NAME`.
    Reject { name: String },
    /// `remove --profile NAME` or `remove --project NAME`: the file the
    /// question named.
    Remove { name: String, file: File },
}

/// What the shell does about a key the menu took (`Effect`, `menu.zig:245-253`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Effect {
    /// A repaint, if anything.
    None,
    /// The menu is closed.
    Close,
    /// Run this choice.
    Run(Run),
    /// Nothing was run, and this warning says why.
    Refused(String),
}

/// What the menu knows of the open server's tools (`Tools`, `menu.zig:90-97`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Tools {
    Loading,
    NeedsLogin,
    NeedsApproval,
    Failed(String),
    Ready(Vec<Tool>),
}

/// One tool, as its row shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Tool {
    pub(crate) name: String,
    /// Its description's first line (`toolDescription`, `verbs.zig:568-575`).
    pub(crate) description: String,
}

impl Tool {
    /// By its name or by what it says it does, ignoring case (`toolMatches`,
    /// `menu.zig:101-105`).
    fn matches(&self, filter: &str) -> bool {
        matches(&self.name, filter) || matches(&self.description, filter)
    }
}

/// What a tool list for a server would find now: fx's `runtime.tools`
/// (`runtime.zig:604-614`, `:886-895`), which ends at once for a server held
/// back, waits through a start, and ends with a failed start's reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Found {
    Tools(Vec<Tool>),
    /// Held back: its reason (`heldReason`, `runtime.zig:2038-2053`).
    Held(String),
    NeedsLogin,
    Failed(String),
    /// Still coming up.
    Waiting,
}

/// What a server's state was when its tools were last asked for
/// (`MenuLoadedAs`, `app_mcp_runtime.zig:779-787`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LoadedAs {
    connection: Connection,
    admission: Option<Admission>,
    needs_login: bool,
}

/// One server, as the menu shows it (`Row`, `menu.zig:51-56`): read from the
/// runtime at most every [`REFRESH`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Row {
    /// Its name, as the configuration has it: what a choice runs on.
    pub(crate) name: String,
    pub(crate) source: Source,
    pub(crate) transport: Transport,
    /// `statusText` (`verbs.zig:369-388`, [`command::status_text`]).
    pub(crate) status: String,
    /// Ready, with no login needed: the screen's status says just `ready`
    /// (`statusFact`, presentation `:270-275`).
    pub(crate) ready: bool,
    /// In the "Needs attention" group (`needsAttention`, `menu.zig:540-545`).
    pub(crate) attention: bool,
    /// A status that needs the person, drawn bright (presentation `:244-249`).
    pub(crate) bright: bool,
    pub(crate) needs_login: bool,
    pub(crate) admission: Option<Admission>,
    pub(crate) connection: Connection,
    pub(crate) version: Option<String>,
    /// Its last error (`showsError`, `menu.zig:163-167`).
    pub(crate) error: Option<String>,
    /// What it runs or connects to: the command line, or the address.
    pub(crate) target: String,
    /// The names of the variables it sets or the headers it sends -- never
    /// what they hold.
    pub(crate) sets: Vec<String>,
    /// The digest of the entry as read, for a project server not approved
    /// as it stands: what an approval of what was drawn is for.
    pub(crate) digest: Option<String>,
    /// What a tool list would find now.
    pub(crate) found: Found,
}

impl Row {
    fn loaded_as(&self) -> LoadedAs {
        LoadedAs {
            connection: self.connection,
            admission: self.admission,
            needs_login: self.needs_login,
        }
    }

    fn waiting(&self) -> bool {
        self.admission == Some(Admission::Pending)
    }

    fn rejected(&self) -> bool {
        self.admission == Some(Admission::Rejected)
    }

    /// The choices it offers, its next step first and Remove last
    /// (`choices`, `menu.zig:60-83`), less the gated sign-in ones.
    fn choices(&self) -> Vec<Choice> {
        let mut out = Vec::with_capacity(3);
        if self.waiting() || self.rejected() {
            out.push(Choice::Approve);
        }
        if self.source == Source::Workspace && !self.rejected() {
            out.push(Choice::Reject);
        }
        out.push(Choice::Remove);
        out
    }

    fn file(&self) -> File {
        match self.source {
            Source::Profile => File::Profile,
            Source::Workspace => File::Project,
        }
    }

    /// Its last error, when its screen shows one: a login it needs is
    /// already its status.
    fn shown_error(&self) -> Option<&str> {
        self.error.as_deref().filter(|_| !self.needs_login)
    }

    /// The status fact on its screen (`statusFact`, presentation `:270-275`).
    fn status_fact(&self) -> &str {
        if self.ready {
            "ready"
        } else {
            &self.status
        }
    }
}

/// Every server the runtime holds, in configuration order, and how many
/// notes loading the configuration had (`buildMenuRows`,
/// `app_mcp_runtime.zig:759-775`).
pub(crate) fn rows_of(runtime: &McpRuntime) -> (Vec<Row>, usize) {
    let rows = runtime
        .views()
        .into_iter()
        .map(|view| {
            let server = runtime.server(&view.name);
            let enabled = server.as_ref().is_none_or(|server| server.config.enabled);
            let (target, sets) = match &server {
                Some(server) => {
                    let config = &server.config;
                    match config.transport {
                        Transport::Stdio => (
                            command::command_line(&config.command, &config.args),
                            config.env.iter().map(|(name, _)| name.clone()).collect(),
                        ),
                        Transport::Http | Transport::Sse => (
                            config.url.clone(),
                            config
                                .headers
                                .iter()
                                .chain(&config.header_env)
                                .map(|(name, _)| name.clone())
                                .chain(
                                    config
                                        .bearer_token_env
                                        .as_ref()
                                        .map(|_| "Authorization".to_string()),
                                )
                                .collect(),
                        ),
                    }
                }
                None => (String::new(), Vec::new()),
            };
            let found = found(&view, enabled, server.as_deref());
            row(&view, enabled, target, sets, found)
        })
        .collect();
    (rows, runtime.issues().len())
}

/// A row from a view and what its server's configuration says.
fn row(view: &ServerView, enabled: bool, target: String, sets: Vec<String>, found: Found) -> Row {
    let status = command::status_text(view, enabled);
    let needs_login = view.needs_auth;
    let bright = needs_login
        || view.admission == Some(Admission::Pending)
        || matches!(view.connection, Connection::Failed | Connection::Retrying);
    Row {
        name: view.name.clone(),
        source: view.source,
        transport: view.transport,
        ready: view.connection == Connection::Ready && !needs_login && status.starts_with("ready"),
        status,
        attention: super::statusline::needs_attention(view),
        bright,
        needs_login,
        admission: view.admission,
        connection: view.connection,
        version: view.protocol_version.clone(),
        error: view.failure.clone(),
        target,
        sets,
        digest: view.entry.as_ref().map(|entry| entry.digest.clone()),
        found,
    }
}

/// What a tool list for `view` would find now (`handle`'s `.tools`,
/// `runtime.zig:886-895`): a held-back server's reason at once, a login it
/// needs, its tools once it is ready, a failed start's reason -- and, while
/// it is still coming up, nothing yet. A stdio server down by a failed start
/// and waiting to be tried again has its reason too, as fx's list ends with
/// a failed start (`p5-mcp-fail`'s `server-1`); a remote one fx could not
/// reach keeps it waiting (`p5-mcp-menu-new`'s `server-remote`).
fn found(view: &ServerView, enabled: bool, server: Option<&crate::mcp::runtime::Server>) -> Found {
    if !enabled {
        return Found::Held("the server is disabled".into());
    }
    match view.admission {
        Some(Admission::Pending) => {
            return Found::Held("the project server is waiting for approval".into())
        }
        Some(Admission::Rejected) => return Found::Held("the project server was rejected".into()),
        _ => {}
    }
    if view.transport == Transport::Sse {
        return Found::Held("HTTP+SSE servers are not supported".into());
    }
    if view.needs_auth {
        return Found::NeedsLogin;
    }
    match view.connection {
        Connection::Ready => Found::Tools(
            server
                .map(|server| server.tools())
                .unwrap_or_default()
                .into_iter()
                .map(|tool| Tool {
                    description: command::first_line(&tool.description).to_string(),
                    name: tool.name,
                })
                .collect(),
        ),
        Connection::Failed => Found::Failed(
            view.failure
                .clone()
                .unwrap_or_else(|| "it failed to start".into()),
        ),
        Connection::Retrying if view.transport == Transport::Stdio => match &view.failure {
            Some(failure) => Found::Failed(failure.clone()),
            None => Found::Waiting,
        },
        Connection::Disabled => Found::Held("the server is disabled".into()),
        Connection::Connecting | Connection::Retrying | Connection::Disconnected => Found::Waiting,
    }
}

/// Case-insensitive, like fx's other pickers; an empty filter keeps
/// everything (`matches`, `menu.zig:85-88`).
fn matches(text: &str, filter: &str) -> bool {
    filter.is_empty()
        || text
            .to_ascii_lowercase()
            .contains(&filter.to_ascii_lowercase())
}

/// What a server's screen holds below its title (`Layout`, `menu.zig:129-161`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Layout {
    lead: usize,
    choices: usize,
    gap: usize,
    tools: usize,
    window: usize,
}

impl Layout {
    fn lines(self) -> usize {
        self.lead + self.choices + self.gap + self.tools
    }

    fn selectable(self) -> usize {
        self.choices + self.tools
    }

    /// The line the `cursor`th selectable item is on.
    fn line(self, cursor: usize) -> usize {
        if cursor < self.choices {
            self.lead + cursor
        } else {
            self.lead + self.choices + self.gap + (cursor - self.choices)
        }
    }

    /// Content lines shown; when everything doesn't fit, a row goes to the
    /// count of what's below.
    fn shown(self) -> usize {
        if self.lines() <= self.window {
            self.lines()
        } else {
            self.window.saturating_sub(1).max(1)
        }
    }
}

/// The approval's screen the last frame drew: what its Enter approves.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Drawn {
    name: String,
    digest: String,
    /// Every line of it was in that frame, and nothing has written over the
    /// screen since.
    whole: bool,
}

/// The menu's state (`State`, `menu.zig:195-240`), and the servers and tools
/// it shows.
#[derive(Debug, Clone)]
pub(crate) struct McpMenu {
    screen: Screen,
    /// The selected server, kept by name so a reload doesn't move it.
    selected: String,
    /// On a server's screen: its choices, then its tools.
    cursor: usize,
    top: usize,
    /// On a question asked on its row: "no" is chosen instead of "yes".
    answer_no: bool,
    /// The list's filter, kept while a server's screen filters its tools.
    list_filter: String,
    /// What the composer holds: the list's filter, or the open server's tools'.
    filter: String,
    /// Every server, in configuration order, as last read.
    rows: Vec<Row>,
    notes: usize,
    read_at: Option<Instant>,
    tools: Tools,
    loaded_as: Option<LoadedAs>,
    /// Set while drawing, which takes `&self`; Enter reads it.
    drawn: RefCell<Option<Drawn>>,
}

impl McpMenu {
    /// The menu open on the list, the first server selected (`open`,
    /// `menu.zig:314-316`; `openHostMenu`, `app_mcp_runtime.zig:637-641`).
    pub(crate) fn open(rows: Vec<Row>, notes: usize, now: Instant) -> Self {
        Self {
            screen: Screen::List,
            selected: String::new(),
            cursor: 0,
            top: 0,
            answer_no: false,
            list_filter: String::new(),
            filter: String::new(),
            rows,
            notes,
            read_at: Some(now),
            tools: Tools::Loading,
            loaded_as: None,
            drawn: RefCell::new(None),
        }
    }

    pub(crate) fn screen(&self) -> Screen {
        self.screen
    }

    /// Whether Remove or Reject is asking on its row, where ← and → choose
    /// between yes and no (`mcpHostMenuAsksOnRow`, `app_input_runtime.zig:1764-1771`).
    pub(crate) fn asks_on_row(&self) -> bool {
        matches!(self.screen, Screen::Confirm(choice) if choice.asks_on_row())
    }

    /// The list's filter, kept while a server's screen was open.
    pub(crate) fn list_filter(&self) -> &str {
        &self.list_filter
    }

    /// Keeps the list's filter, up to [`NAME_CAPACITY`] bytes and never
    /// inside a character (`keepListFilter`, `menu.zig:220-227`).
    pub(crate) fn keep_list_filter(&mut self, filter: &str) {
        let mut end = filter.len().min(NAME_CAPACITY);
        while !filter.is_char_boundary(end) {
            end -= 1;
        }
        self.list_filter = filter[..end].to_string();
    }

    /// The composer changed (`hostMenuFilterChanged`,
    /// `app_mcp_runtime.zig:655-660`): on a server's screen, a new filter
    /// selects the first tool it keeps, so Enter can't run a choice by
    /// surprise (`filterChanged`, `menu.zig:303-312`).
    pub(crate) fn set_filter(&mut self, filter: &str) {
        if filter == self.filter {
            return;
        }
        self.filter = filter.to_string();
        if self.screen != Screen::Server {
            return;
        }
        let Some(layout) = self.layout() else {
            return;
        };
        self.cursor = if !filter.is_empty() && layout.tools > 0 {
            layout.choices
        } else {
            0
        };
        self.top = 0;
        self.keep_visible(layout);
    }

    /// Something wrote over the screen, or a frame did not land: the
    /// approval's screen is no longer known to be on it.
    pub(crate) fn forget_drawn(&self) {
        if let Some(drawn) = self.drawn.borrow_mut().as_mut() {
            drawn.whole = false;
        }
    }

    /// Reads the servers again when [`REFRESH`] has passed since the last
    /// read, or at once when `force`; then keeps the screen valid, and
    /// loads the open server's tools when they can be had
    /// (`refreshHostMenu`, `app_mcp_runtime.zig:719-757`). Whether anything
    /// it shows may have changed.
    pub(crate) fn refresh(&mut self, runtime: &McpRuntime, now: Instant, force: bool) -> bool {
        let due = force
            || self
                .read_at
                .is_none_or(|at| now.saturating_duration_since(at) >= REFRESH);
        if !due {
            return false;
        }
        let (rows, notes) = rows_of(runtime);
        self.read_at = Some(now);
        self.take(rows, notes)
    }

    /// [`Self::refresh`] with the rows read: whether anything changed.
    pub(crate) fn take(&mut self, rows: Vec<Row>, notes: usize) -> bool {
        let changed = rows != self.rows || notes != self.notes;
        self.rows = rows;
        self.notes = notes;
        let tools = self.tools.clone();
        self.settle();
        self.load_tools();
        changed || tools != self.tools
    }

    /// The menu reads the servers at its next refresh, whenever that is: a
    /// choice ran, and the status follows sooner than the next tick would
    /// show it (`host_menu_at_ms = 0`, `app_mcp_runtime.zig:713-714`).
    pub(crate) fn read_soon(&mut self) {
        self.read_at = None;
    }

    /// The servers the list shows, after the filter: the ones that work,
    /// then the ones that need attention, each by name, ignoring case; and
    /// where the second group starts, when it shows (`viewOf`,
    /// `menu.zig:273-296`).
    fn listed(&self) -> (Vec<&Row>, Option<usize>) {
        let filter = self.list_view_filter();
        let mut out: Vec<&Row> = Vec::new();
        let mut attention_start = 0;
        for attention in [false, true] {
            if attention {
                attention_start = out.len();
            }
            let start = out.len();
            out.extend(
                self.rows
                    .iter()
                    .filter(|row| row.attention == attention && matches(&row.name, filter)),
            );
            out[start..].sort_by_key(|row| row.name.to_ascii_lowercase());
        }
        let attention = (attention_start < out.len()).then_some(attention_start);
        (out, attention)
    }

    /// The filter the list is drawn with: the composer's on the list, the
    /// kept one elsewhere.
    fn list_view_filter(&self) -> &str {
        if self.screen == Screen::List {
            &self.filter
        } else {
            &self.list_filter
        }
    }

    /// Where the selection is in `rows`; the first row when it isn't there
    /// (`selectedIndex`, `menu.zig:234-239`).
    fn selected_index(&self, rows: &[&Row]) -> Option<usize> {
        if rows.is_empty() {
            return None;
        }
        Some(
            rows.iter()
                .position(|row| row.name == self.selected)
                .unwrap_or(0),
        )
    }

    /// The open server, found by name among all servers whatever the filter.
    fn open_row(&self) -> Option<&Row> {
        if self.screen == Screen::List {
            return None;
        }
        self.rows.iter().find(|row| row.name == self.selected)
    }

    /// The tools as the screen tells them: a server that needs a login or an
    /// approval says so, rather than as a failure (`toolsFor`, `menu.zig:169-176`).
    fn tools_for(&self, row: &Row) -> Tools {
        if let Tools::Ready(_) = self.tools {
            return self.tools.clone();
        }
        if row.waiting() {
            return Tools::NeedsApproval;
        }
        if row.needs_login {
            Tools::NeedsLogin
        } else {
            self.tools.clone()
        }
    }

    /// How many tools the filter keeps (`toolCount`, `menu.zig:107-116`).
    fn tool_count(&self) -> usize {
        match &self.tools {
            Tools::Ready(tools) => tools
                .iter()
                .filter(|tool| tool.matches(&self.filter))
                .count(),
            _ => 0,
        }
    }

    /// The `n`th tool the filter keeps (`nthTool`, `menu.zig:118-127`).
    fn nth_tool(&self, n: usize) -> Option<&Tool> {
        match &self.tools {
            Tools::Ready(tools) => tools
                .iter()
                .filter(|tool| tool.matches(&self.filter))
                .nth(n),
            _ => None,
        }
    }

    /// The open server's layout (`serverLayout`, `menu.zig:178-191`).
    fn layout(&self) -> Option<Layout> {
        let row = self.open_row()?;
        Some(Layout {
            // runs or http, file, status, protocol when known, and error when
            // there is one; then a blank.
            lead: 3
                + usize::from(row.version.is_some())
                + usize::from(row.shown_error().is_some())
                + 1,
            choices: row.choices().len(),
            // A blank, then the heading or the reason there are no tools.
            gap: 2,
            tools: self.tool_count(),
            window: SERVER_WINDOW,
        })
    }

    /// Keeps a server's screen valid after its choices or tools changed, or
    /// after the server went away (`settle`, `menu.zig:318-333`): a server
    /// gone takes its screen back to the list.
    fn settle(&mut self) {
        if self.screen == Screen::List {
            return;
        }
        let Some(layout) = self.layout() else {
            self.back_to_list();
            return;
        };
        self.cursor = if layout.selectable() > 0 {
            self.cursor.min(layout.selectable() - 1)
        } else {
            0
        };
        self.keep_visible(layout);
    }

    fn back_to_list(&mut self) {
        self.screen = Screen::List;
        self.cursor = 0;
        self.top = 0;
        self.answer_no = false;
        self.stop_tools();
    }

    /// Its tools aren't needed any more (`stopMenuTools`,
    /// `app_mcp_runtime.zig:819-832`).
    fn stop_tools(&mut self) {
        self.tools = Tools::Loading;
        self.loaded_as = None;
    }

    /// The open server's tools, once a list would have them; and again for
    /// a server that became usable since they couldn't load -- by being
    /// approved -- and only then: one moving between retrying and failed is
    /// left alone (`refreshHostMenu`, `app_mcp_runtime.zig:747-756`).
    fn load_tools(&mut self) {
        let Some(row) = self.open_row().cloned() else {
            return;
        };
        if !matches!(self.tools, Tools::Ready(_) | Tools::Loading) {
            let usable = !row.needs_login
                && matches!(row.connection, Connection::Ready | Connection::Disconnected)
                && !row.waiting()
                && !row.rejected();
            if !usable || self.loaded_as == Some(row.loaded_as()) {
                return;
            }
            self.tools = Tools::Loading;
            self.loaded_as = Some(row.loaded_as());
        }
        if self.tools != Tools::Loading {
            return;
        }
        self.tools = match row.found {
            Found::Tools(tools) => Tools::Ready(tools),
            Found::Held(reason) | Found::Failed(reason) => Tools::Failed(reason),
            Found::NeedsLogin => Tools::NeedsLogin,
            Found::Waiting => Tools::Loading,
        };
        if let Some(layout) = self.layout() {
            self.cursor = self.cursor.min(layout.selectable().saturating_sub(1));
            self.keep_visible(layout);
        }
    }

    fn select(&mut self, name: &str) {
        self.selected = name.to_string();
    }

    /// One key (`press`, `menu.zig:335-421`).
    pub(crate) fn press(&mut self, key: Key) -> Effect {
        match self.screen {
            Screen::List => {
                let (rows, _) = self.listed();
                let Some(at) = self.selected_index(&rows) else {
                    return if key == Key::Escape {
                        Effect::Close
                    } else {
                        Effect::None
                    };
                };
                let name = match key {
                    Key::Up => rows[at.saturating_sub(1)].name.clone(),
                    Key::Down => rows[(at + 1).min(rows.len() - 1)].name.clone(),
                    Key::Left | Key::Right => return Effect::None,
                    Key::Escape => return Effect::Close,
                    Key::Enter => {
                        let name = rows[at].name.clone();
                        self.select(&name);
                        self.screen = Screen::Server;
                        self.cursor = 0;
                        self.top = 0;
                        self.stop_tools();
                        self.loaded_as = self.open_row().map(Row::loaded_as);
                        self.load_tools();
                        return Effect::None;
                    }
                };
                self.select(&name);
                Effect::None
            }
            Screen::Server => {
                let Some(layout) = self.layout() else {
                    self.settle();
                    return Effect::None;
                };
                match key {
                    Key::Up => self.cursor = self.cursor.saturating_sub(1),
                    Key::Down => {
                        if self.cursor + 1 < layout.selectable() {
                            self.cursor += 1;
                        }
                    }
                    Key::Left | Key::Right => {}
                    Key::Enter => {
                        let Some(row) = self.open_row() else {
                            return Effect::None;
                        };
                        let Some(&choice) = row.choices().get(self.cursor) else {
                            return Effect::None;
                        };
                        self.screen = Screen::Confirm(choice);
                        self.answer_no = false;
                        *self.drawn.borrow_mut() = None;
                        return Effect::None;
                    }
                    Key::Escape => {
                        self.back_to_list();
                        return Effect::None;
                    }
                }
                self.keep_visible(layout);
                Effect::None
            }
            // Only the answer moves while a confirmation is open: ← chooses
            // yes and → no, and Enter on no is Esc.
            Screen::Confirm(choice) => {
                let Some(row) = self.open_row().cloned() else {
                    self.settle();
                    return Effect::None;
                };
                match key {
                    Key::Left if choice.asks_on_row() => self.answer_no = false,
                    Key::Right if choice.asks_on_row() => self.answer_no = true,
                    Key::Enter if choice.asks_on_row() && self.answer_no => {
                        self.screen = Screen::Server;
                        self.answer_no = false;
                    }
                    Key::Enter => return self.confirmed(choice, &row),
                    Key::Escape => {
                        self.screen = Screen::Server;
                        self.answer_no = false;
                    }
                    _ => {}
                }
                Effect::None
            }
        }
    }

    /// Yes to `choice` on `row`. A removed server has no screen to come back
    /// to. An approval approves the entry its screen drew, and only once all
    /// of it was on the screen; anything else approves nothing and says why.
    fn confirmed(&mut self, choice: Choice, row: &Row) -> Effect {
        let shown = command::typed(&row.name);
        let run = match choice {
            Choice::Remove => {
                self.back_to_list();
                return Effect::Run(Run::Remove {
                    name: row.name.clone(),
                    file: row.file(),
                });
            }
            Choice::Reject => Run::Reject {
                name: row.name.clone(),
            },
            Choice::Approve => {
                let drawn = self.drawn.borrow().clone();
                let Some(drawn) = drawn.filter(|drawn| drawn.name == row.name) else {
                    // No frame of it yet: the key waits for one.
                    return Effect::None;
                };
                self.screen = Screen::Server;
                if row.digest.as_deref() != Some(drawn.digest.as_str()) {
                    return Effect::Refused(format!(
                        "{shown} changed in {} since xfx showed it; nothing was approved.",
                        row.file().label()
                    ));
                }
                if !drawn.whole {
                    return Effect::Refused(format!(
                        "xfx couldn't show all of what {shown} {} on this screen, so nothing was approved.",
                        runs(row)
                    ));
                }
                Run::Approve {
                    name: row.name.clone(),
                    digest: drawn.digest,
                }
            }
        };
        self.screen = Screen::Server;
        Effect::Run(run)
    }

    fn keep_visible(&mut self, layout: Layout) {
        if layout.lines() <= layout.window || self.cursor == 0 {
            self.top = 0;
            return;
        }
        let at = layout.line(self.cursor);
        let shown = layout.shown();
        if at < self.top {
            self.top = at;
        }
        if at >= self.top + shown {
            self.top = at + 1 - shown;
        }
        self.top = self.top.min(layout.lines() - shown);
    }

    // -----------------------------------------------------------------------
    // rows
    // -----------------------------------------------------------------------

    /// The rows the panel needs (`menuRowCount`, presentation `:79-96`):
    /// at most `budget`, which is at most [`MAX_ROWS`] -- but for the
    /// approval's screen, which may take up to `room`, the whole of the
    /// space the screen gives a panel, to show all of what it trusts.
    pub(crate) fn height(&self, cols: u16, budget: u16, room: u16) -> u16 {
        let budget = budget.min(MAX_ROWS);
        if budget == 0 {
            return 0;
        }
        let wanted = match self.screen {
            Screen::List => {
                let (rows, attention) = self.listed();
                let shown = list_lines(rows.len(), attention).max(1);
                1 + if self.rows.is_empty() { 2 } else { shown } + usize::from(self.notes > 0)
            }
            Screen::Server => self.server_rows(),
            Screen::Confirm(choice) if choice.asks_on_row() => self.server_rows(),
            Screen::Confirm(_) => match self.open_row() {
                Some(row) => {
                    let wanted = 1 + confirm_lines(row, usize::from(cols)).len();
                    return u16::try_from(wanted)
                        .unwrap_or(u16::MAX)
                        .min(room.max(budget));
                }
                None => 1,
            },
        };
        u16::try_from(wanted).unwrap_or(u16::MAX).min(budget)
    }

    fn server_rows(&self) -> usize {
        match self.layout() {
            Some(layout) => 1 + layout.lines().min(SERVER_WINDOW),
            None => 1,
        }
    }

    /// The menu's rows, top first, for `row_count` rows of a `cols`-wide
    /// screen (`composeRow`, presentation `:160-170`).
    pub(crate) fn rows(&self, cols: u16, row_count: u16, palette: Palette) -> Vec<String> {
        let width = usize::from(cols);
        let count = usize::from(row_count);
        if width == 0 || count == 0 {
            return Vec::new();
        }
        let mut out = match self.screen {
            Screen::List => self.list_rows(width, count, palette),
            Screen::Server => self.server_screen(width, count, palette),
            Screen::Confirm(choice) if choice.asks_on_row() => {
                self.server_screen(width, count, palette)
            }
            Screen::Confirm(_) => self.confirm_rows(width, count, palette),
        };
        out.truncate(count);
        out
    }

    /// The list (`listRow`, presentation `:186-229`).
    fn list_rows(&self, width: usize, count: usize, palette: Palette) -> Vec<String> {
        let (rows, attention) = self.listed();
        let meta = if self.rows.is_empty() {
            "none yet".to_string()
        } else if !self.filter.is_empty() {
            format!("{} of {}", rows.len(), self.rows.len())
        } else {
            self.rows.len().to_string()
        };
        let mut out = vec![title("MCP servers", &meta, width, palette)];
        let note = (self.notes > 0).then(|| {
            text_row(
                &format!(
                    "{} config note{}: /mcp list shows {}",
                    self.notes,
                    if self.notes == 1 { "" } else { "s" },
                    if self.notes == 1 { "it" } else { "them" }
                ),
                width,
                palette.dim(),
                2,
                palette,
            )
        });
        let body = count
            .saturating_sub(1)
            .saturating_sub(usize::from(note.is_some()));
        if self.rows.is_empty() {
            out.push(text_row(
                "Add one with /mcp add NAME URL",
                width,
                "",
                2,
                palette,
            ));
            out.push(text_row(
                "or /mcp add NAME -- COMMAND",
                width,
                "",
                11,
                palette,
            ));
        } else if rows.is_empty() {
            out.push(text_row(
                &format!("No server matches \"{}\"", self.filter),
                width,
                palette.dim(),
                2,
                palette,
            ));
        } else {
            let selected = self.selected_index(&rows).unwrap_or(0);
            let heading = heading_lines(attention);
            let line_of = |i: usize| match attention {
                Some(first) if i >= first => i + heading,
                _ => i,
            };
            // The window keeps the selection in view.
            let selected_line = line_of(selected);
            let start = if selected_line < body {
                0
            } else {
                selected_line + 1 - body
            };
            let total = list_lines(rows.len(), attention);
            for line in (start..total).take(body) {
                match attention {
                    Some(first) if line >= first && line < first + heading => {
                        if line + 1 < first + heading {
                            out.push(String::new());
                        } else {
                            out.push(title(
                                "Needs attention",
                                &(rows.len() - first).to_string(),
                                width,
                                palette,
                            ));
                        }
                    }
                    Some(first) => {
                        let i = if line > first { line - heading } else { line };
                        out.push(list_row(rows[i], i == selected, width, palette));
                    }
                    None => out.push(list_row(rows[line], line == selected, width, palette)),
                }
            }
        }
        // The note on the panel's last row, below whatever the list left.
        if let Some(note) = note {
            out.resize(count - 1, String::new());
            out.push(note);
        }
        out
    }

    /// A server's screen (`serverRow`, presentation `:394-409`).
    fn server_screen(&self, width: usize, count: usize, palette: Palette) -> Vec<String> {
        let (Some(row), Some(layout)) = (self.open_row(), self.layout()) else {
            return Vec::new();
        };
        let mut out = vec![title(&command::typed(&row.name), "", width, palette)];
        // A filter typed since the last key can leave fewer items than the
        // selection's place; draw it on the last one left.
        let cursor = self.cursor.min(layout.selectable().saturating_sub(1));
        let window = count - 1;
        if layout.lines() <= window {
            out.extend(
                (0..layout.lines())
                    .map(|line| self.server_line(row, layout, cursor, line, width, palette)),
            );
            return out;
        }
        let shown = window.saturating_sub(1).max(1);
        let mut top = self.top.min(layout.lines() - shown);
        let at = layout.line(cursor);
        if at < top {
            top = at;
        }
        if at >= top + shown {
            top = at + 1 - shown;
        }
        if cursor == 0 {
            top = 0;
        }
        let mut lines = window;
        if top + lines < layout.lines() {
            lines -= 1;
        }
        lines = lines.min(layout.lines() - top);
        let below = layout.lines() - top - lines;
        out.extend(
            (top..top + lines)
                .map(|line| self.server_line(row, layout, cursor, line, width, palette)),
        );
        if below > 0 {
            out.push(text_row(
                &format!("{below} more below"),
                width,
                palette.dim(),
                2,
                palette,
            ));
        }
        out
    }

    /// Line `line` of a server's screen below its title (`serverLine`,
    /// presentation `:277-392`).
    fn server_line(
        &self,
        row: &Row,
        layout: Layout,
        cursor: usize,
        line: usize,
        width: usize,
        palette: Palette,
    ) -> String {
        if line < layout.lead {
            let facts = layout.lead - 1;
            if line >= facts {
                return String::new();
            }
            let protocol = usize::from(row.version.is_some());
            return match line {
                0 => fact_row(
                    if row.transport == Transport::Stdio {
                        "runs"
                    } else {
                        row.transport.tag()
                    },
                    &row.target,
                    width,
                    "",
                    palette,
                ),
                1 => fact_row("file", row.file().label(), width, "", palette),
                2 => fact_row("status", row.status_fact(), width, "", palette),
                3 if protocol == 1 => fact_row(
                    "protocol",
                    row.version.as_deref().unwrap_or_default(),
                    width,
                    "",
                    palette,
                ),
                _ => fact_row(
                    "error",
                    row.shown_error().unwrap_or_default(),
                    width,
                    palette.hint(),
                    palette,
                ),
            };
        }
        let choices = row.choices();
        let at = line - layout.lead;
        if let Some(&choice) = choices.get(at) {
            let selected = cursor == at;
            let mut out = String::new();
            out.push_str(if selected { palette.subtitle() } else { "" });
            out.push_str(if selected { "\u{203a} " } else { "  " });
            // Remove and Reject ask on their own row, answers included: the
            // chosen one is bright, and ← → move between them.
            if self.screen == Screen::Confirm(choice) {
                const ANSWERS: &str = "   yes  no";
                out.push_str(&ellipsized(
                    &command::safe_within(&question(row, choice), 4096),
                    width.saturating_sub(2).saturating_sub(ANSWERS.len()),
                ));
                out.push_str(palette.reset());
                out.push_str("   ");
                let (yes, no) = if self.answer_no {
                    (palette.dim(), palette.subtitle())
                } else {
                    (palette.subtitle(), palette.dim())
                };
                out.push_str(yes);
                out.push_str("yes");
                out.push_str(palette.reset());
                out.push_str("  ");
                out.push_str(no);
                out.push_str("no");
                out.push_str(palette.reset());
                return out;
            }
            out.push_str(choice.label());
            out.push_str(palette.reset());
            if width >= 56 {
                pad_to(&mut out, 16);
                out.push_str(palette.dim());
                out.push_str(&ellipsized(
                    &format!("/mcp {} {}", choice.verb(), command::typed(&row.name)),
                    width - 16,
                ));
                out.push_str(palette.reset());
            }
            return out;
        }
        let after = at - choices.len();
        if after == 0 {
            return String::new();
        }
        if after == 1 {
            let heading = match self.tools_for(row) {
                Tools::Loading => "Loading tools\u{2026}".to_string(),
                // Signing in is gated: xfx's sentence for it, where fx says
                // its tools show once you log in.
                Tools::NeedsLogin => command::AUTH_UNAVAILABLE.to_string(),
                Tools::NeedsApproval => "Its tools show here once you approve it.".to_string(),
                Tools::Failed(reason) => format!("Its tools couldn't load: {reason}"),
                Tools::Ready(tools) => {
                    if tools.is_empty() {
                        "It has no tools.".to_string()
                    } else if self.filter.is_empty() {
                        format!("Tools {}", tools.len())
                    } else if layout.tools == 0 {
                        "No tools match.".to_string()
                    } else {
                        format!("Tools {} of {}", layout.tools, tools.len())
                    }
                }
            };
            return text_row(&heading, width, palette.dim(), 2, palette);
        }
        let index = after - 2;
        let Some(tool) = self.nth_tool(index) else {
            return String::new();
        };
        let selected = cursor == choices.len() + index;
        let mut out = String::new();
        out.push_str(if selected { palette.subtitle() } else { "" });
        out.push_str(if selected { "\u{203a} " } else { "  " });
        out.push_str(&ellipsized(&command::safe_within(&tool.name, 4096), 20));
        out.push_str(palette.reset());
        pad_to(&mut out, 24);
        out.push_str(palette.dim());
        out.push_str(&ellipsized(
            &command::safe_within(&tool.description, 4096),
            width.saturating_sub(24),
        ));
        out.push_str(palette.reset());
        out
    }

    /// The approval's screen (`confirmRow`, presentation `:483-496`): the
    /// title keeps the status, since the facts are hidden; then a blank, the
    /// question, and all of what it trusts. Records what it drew.
    fn confirm_rows(&self, width: usize, count: usize, palette: Palette) -> Vec<String> {
        let Some(row) = self.open_row() else {
            return Vec::new();
        };
        let mut out = vec![title(
            &command::typed(&row.name),
            row.status_fact(),
            width,
            palette,
        )];
        let lines = confirm_lines(row, width);
        if let Some(digest) = &row.digest {
            *self.drawn.borrow_mut() = Some(Drawn {
                name: row.name.clone(),
                digest: digest.clone(),
                whole: lines.len() < count && width >= APPROVAL_MIN_COLS,
            });
        }
        for line in lines {
            out.push(match line {
                ConfirmLine::Question(text) => {
                    text_row(&text, width, palette.subtitle(), 2, palette)
                }
                ConfirmLine::Detail(text) => {
                    format!("{}{}", palette.dim(), spelled_row(&text, width, 2, palette))
                }
                ConfirmLine::Target(text) => spelled_row(&text, width, 4, palette),
            });
        }
        out
    }

    /// The hint row for where the menu is, widest first, the widest that
    /// fits (`hintVariants`, presentation `:506-522`).
    pub(crate) fn key_help(&self, cols: u16) -> String {
        const LIST: [&str; 4] = [
            "\u{2191}\u{2193} navigate     enter open     esc close",
            "\u{2191}\u{2193} move  enter open  esc",
            "enter open  esc close",
            "enter esc",
        ];
        const EMPTY: [&str; 1] = ["esc close"];
        const APPROVE: [&str; 3] = [
            "enter confirm     esc cancel",
            "enter confirm  esc cancel",
            "enter esc",
        ];
        const ON_TOOL: [&str; 3] = [
            "\u{2191}\u{2193} navigate     esc back",
            "\u{2191}\u{2193} move  esc back",
            "esc",
        ];
        const SERVER: [&str; 4] = [
            "\u{2191}\u{2193} navigate     enter run     esc back",
            "\u{2191}\u{2193} move  enter run  esc",
            "enter run  esc back",
            "enter esc",
        ];
        let server = || match self.open_row() {
            None => &["esc back"][..],
            Some(row) if self.cursor >= row.choices().len() => &ON_TOOL[..],
            Some(_) => &SERVER[..],
        };
        let variants: &[&str] = match self.screen {
            Screen::List if self.rows.is_empty() => &EMPTY,
            Screen::List => &LIST,
            // A question asked in place leaves the server's hint line as it
            // was; the row shows its answers.
            Screen::Confirm(choice) if choice.asks_on_row() => server(),
            Screen::Confirm(_) => &APPROVE,
            Screen::Server => server(),
        };
        let width = usize::from(cols);
        variants
            .iter()
            .copied()
            .find(|candidate| cells(candidate) <= width)
            .map(str::to_string)
            .unwrap_or_else(|| ellipsized(variants[variants.len() - 1], width))
    }
}

/// The list's lines below its title: a line per server, and before the
/// servers that need attention their heading, set apart by a blank line when
/// servers that work come first (`listLines`, presentation `:142-152`).
fn list_lines(rows: usize, attention: Option<usize>) -> usize {
    rows + heading_lines(attention)
}

fn heading_lines(attention: Option<usize>) -> usize {
    match attention {
        None => 0,
        Some(0) => 1,
        Some(_) => 2,
    }
}

/// `runs` or `connects to`, as the approval's question says it.
fn runs(row: &Row) -> &'static str {
    if row.transport == Transport::Stdio {
        "runs"
    } else {
        "connects to"
    }
}

/// The question Remove or Reject asks on its row (`question`, presentation
/// `:524-535`). xfx keeps no login, so there is none to remove with it.
fn question(row: &Row, choice: Choice) -> String {
    let name = command::typed(&row.name);
    match choice {
        Choice::Remove => format!("Remove {name} from {}?", row.file().label()),
        _ => format!("Reject {name}? xfx won't start it in this project."),
    }
}

/// One line of the approval's screen below its title.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ConfirmLine {
    Question(String),
    Detail(String),
    /// Spelled already.
    Target(String),
}

/// The approval's lines below the title: a blank, the question, and what it
/// trusts, wrapped at spaces and never cut (`confirmLines`, presentation
/// `:440-481`) -- spelled first, so a control spelled out is wrapped as the
/// cells it takes. Then the names of what the entry sets, which change what
/// runs as much: `PATH`, `LD_PRELOAD`.
fn confirm_lines(row: &Row, width: usize) -> Vec<ConfirmLine> {
    let mut out = vec![
        ConfirmLine::Detail(String::new()),
        ConfirmLine::Question(format!(
            "Approve {} for this project? It {}:",
            command::typed(&row.name),
            runs(row)
        )),
    ];
    let limit = width.saturating_sub(4).max(16);
    let mut rest = spelled(&row.target);
    while !rest.is_empty() {
        let (line, after) = cut_at(&rest, limit);
        out.push(ConfirmLine::Target(line.to_string()));
        rest = after.trim_start_matches(' ').to_string();
    }
    if !row.sets.is_empty() {
        let names: Vec<String> = row.sets.iter().map(|name| spelled(name)).collect();
        let mut rest = format!(
            "It {} {}.",
            if row.transport == Transport::Stdio {
                "sets"
            } else {
                "sends"
            },
            names.join(", ")
        );
        let limit = width.saturating_sub(2).max(16);
        while !rest.is_empty() {
            let (line, after) = cut_at(&rest, limit);
            out.push(ConfirmLine::Detail(line.to_string()));
            rest = after.trim_start_matches(' ').to_string();
        }
    }
    out
}

/// The narrowest screen the approval's lines are all drawn whole on: below
/// it [`confirm_lines`]' sixteen columns of a line do not fit beside their
/// indent.
const APPROVAL_MIN_COLS: usize = 20;

/// `text` cut to at most `limit` cells, at the last space in them when there
/// is one past its first character; the rest after it.
fn cut_at(text: &str, limit: usize) -> (&str, &str) {
    if cells(text) <= limit {
        return (text, "");
    }
    let mut end = 0;
    let mut width = 0;
    for (at, character) in text.char_indices() {
        let cell = cells(character.encode_utf8(&mut [0u8; 4]));
        if width + cell > limit {
            break;
        }
        width += cell;
        end = at + character.len_utf8();
    }
    let end = end.max(text.chars().next().map_or(0, char::len_utf8));
    match text[..end].rfind(' ') {
        Some(space) if space > 0 => (&text[..space], &text[space..]),
        _ => (&text[..end], &text[end..]),
    }
}

/// Every character of `raw` as [`crate::mcp::terminal_safe`] spells it, and
/// nothing cut: what an approval trusts is shown whole.
fn spelled(raw: &str) -> String {
    let mut buf = [0u8; 4];
    raw.chars()
        .map(|character| crate::mcp::terminal_safe(character.encode_utf8(&mut buf)))
        .collect()
}

/// `title` (presentation `:172-184`): the name bright in half the width, the
/// meta dim in what is left.
fn title(name: &str, meta: &str, width: usize, palette: Palette) -> String {
    let mut out = String::from("  ");
    out.push_str(palette.subtitle());
    out.push_str(&ellipsized(&command::safe_within(name, 4096), width / 2));
    out.push_str(palette.reset());
    out.push_str("  ");
    out.push_str(palette.dim());
    let used = cells(&out);
    out.push_str(&ellipsized(
        &command::safe_within(meta, 4096),
        width.saturating_sub(used),
    ));
    out.push_str(palette.reset());
    out
}

/// A server's row in the list (`serverListRow`, presentation `:231-260`):
/// the name, the status at its column, and the file's kind on a screen wide
/// enough.
fn list_row(row: &Row, selected: bool, width: usize, palette: Palette) -> String {
    let show_source = width >= 64;
    let name_width = (width / 4).clamp(12, 22);
    let status_col = 2 + name_width + 2;
    let source_col = status_col + 22 + 2;
    let mut out = String::new();
    // Like fx's other pickers: dim rows, the selected one bright.
    out.push_str(if selected {
        palette.subtitle()
    } else {
        palette.dim()
    });
    out.push_str(if selected { "\u{203a} " } else { "  " });
    out.push_str(&ellipsized(&command::typed(&row.name), name_width));
    out.push_str(palette.reset());
    pad_to(&mut out, status_col);
    // What needs the person stays bright so it stands out among dim rows.
    out.push_str(if row.bright {
        palette.hint()
    } else {
        palette.dim()
    });
    let status_width = if show_source {
        22
    } else {
        width.saturating_sub(status_col)
    };
    out.push_str(&ellipsized(
        &command::safe_within(&row.status, 4096),
        status_width,
    ));
    out.push_str(palette.reset());
    if show_source {
        pad_to(&mut out, source_col);
        out.push_str(palette.dim());
        out.push_str(if row.source == Source::Workspace {
            "project"
        } else {
            "profile"
        });
        out.push_str(palette.reset());
    }
    out
}

/// `composeFactRow` (`row_text.zig:136-150`): a dim label, then its value
/// from [`FACT_COL`]; both spelled.
fn fact_row(label: &str, value: &str, width: usize, style: &str, palette: Palette) -> String {
    let mut out = String::from(palette.dim());
    if width > 4 {
        out.push_str("  ");
    }
    let col = FACT_COL.min(width);
    out.push_str(&ellipsized(label, col.saturating_sub(2)));
    pad_to(&mut out, col);
    out.push_str(palette.reset());
    out.push_str(style);
    out.push_str(&ellipsized(
        &command::safe_within(value, 4096),
        width.saturating_sub(col),
    ));
    out.push_str(palette.reset());
    out
}

/// `composeTextRow` (`row_text.zig:152-161`): one styled line, spelled and
/// indented.
fn text_row(text: &str, width: usize, style: &str, indent: usize, palette: Palette) -> String {
    let indent = indent.min(width);
    format!(
        "{style}{}{}{}",
        " ".repeat(indent),
        ellipsized(&command::safe_within(text, 4096), width - indent),
        palette.reset()
    )
}

/// A line already spelled, indented: a piece of what an approval trusts.
fn spelled_row(text: &str, width: usize, indent: usize, palette: Palette) -> String {
    let indent = indent.min(width);
    format!(
        "{}{}{}",
        " ".repeat(indent),
        ellipsized(text, width - indent),
        palette.reset()
    )
}

/// The rows a screen `terminal_rows` tall gives the menu when its composer
/// is `input_extra` rows taller than one (`inlinePickerRowBudgetCapped`,
/// `picker_presentation.zig:760-770`, with [`MAX_ROWS`]): the band's fixed
/// rows and five rows of document left, never none. Uncapped, the room the
/// approval's screen may take.
pub(crate) fn room(terminal_rows: u16, input_extra: u16) -> u16 {
    terminal_rows
        .saturating_sub(5u16.saturating_add(input_extra))
        .saturating_sub(5)
        .max(1)
}

#[cfg(test)]
mod tests;
