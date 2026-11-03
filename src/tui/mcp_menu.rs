//! `/mcp`'s menu: the servers, their tools, and a server's resources and
//! prompts, under the composer.
//!
//! Upstream's footer menu (`vercel-labs/fx@c1db9191
//! src/ui/footer/mcp_menu_presentation.zig`, its state in
//! `src/core/mcp/menu_state.zig`, its hint rows in `input_presentation.zig:
//! 607-692`), as `cap/fx-mcp-{empty,fixture}-40x120-menu*` show it: a header
//! with the server count and four tabs, a row under it that says what the
//! section is about, then one row per item, between the band's two rules with
//! the composer above.
//!
//! The screens this build draws: **browse** in all four sections, a server's
//! **details**, **help** (`c`), and the **confirmation** a removal or a
//! rejection asks for (`Remove this profile MCP server? press enter to
//! confirm.`). Each change goes through the same runtime job its `/mcp`
//! command does, so the menu and the command cannot disagree.
//!
//! **Gated keys answer, and change nothing.** `s` would start Slack's browser
//! sign-in and `enter` on a remote server would start its own: both say
//! upstream's sentence for an unavailable sign-in in the feedback row
//! (`docs/parity.md`). `a`, which opens upstream's add form, puts
//! `/mcp add ` in the composer instead and closes the menu, so a server is
//! still added by a line the person reads before it runs.

use crate::mcp::config::{Admission, Source, Transport};
use crate::mcp::runtime::{Catalog, Prompt, Resource, ServerView, Status};

use super::input::Action;
use super::picker::{cells, ellipsized, pad_to};
use super::theme::Palette;

/// The header row and the row under it (`roomy_header_rows`).
const HEADER_ROWS: u16 = 2;

/// The most rows the menu takes (`max_inline_rows`).
pub(crate) const MAX_ROWS: u16 = 12;

/// The details screen's value column (`composeFactRow`, `:580-592`).
const VALUE_COLUMN: usize = 22;

/// The four tabs, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Section {
    Servers,
    Tools,
    Resources,
    Prompts,
}

impl Section {
    const ALL: [Self; 4] = [Self::Servers, Self::Tools, Self::Resources, Self::Prompts];

    fn label(self) -> &'static str {
        match self {
            Self::Servers => "Servers",
            Self::Tools => "Tools",
            Self::Resources => "Resources",
            Self::Prompts => "Prompts",
        }
    }

    fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|section| *section == self)
            .unwrap_or(0)
    }
}

/// What a confirmation is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Confirm {
    Remove,
    Reject,
    ApproveAll,
    Reset,
}

impl Confirm {
    /// `confirmationText` (`:144-153`).
    fn text(self) -> &'static str {
        match self {
            Self::Remove => "Remove this profile MCP server? press enter to confirm.",
            Self::Reject => "Reject this project MCP server? press enter to confirm.",
            Self::ApproveAll => "Approve all pending project MCP servers? press enter to confirm.",
            Self::Reset => "Reset all project MCP choices? press enter to confirm.",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Screen {
    Browse,
    Details,
    Info,
    Confirm(Confirm),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Load {
    Ready,
    Loading,
    Failed,
}

/// What the shell does about a keystroke the menu took.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Effect {
    /// Nothing beyond a repaint.
    Repaint,
    /// The keystroke means nothing here.
    Ignored,
    /// The menu is closed.
    Close,
    /// Read this section's catalog, for this request, from this server.
    Load {
        request: u64,
        section: crate::mcp::runtime::Section,
        server: Option<String>,
    },
    /// Reload the configuration, and tell the menu.
    Reload,
    /// Run this `/mcp` subcommand.
    Command(String),
    /// Put this line in the composer and close the menu.
    Compose(String),
    /// Run this trust choice, announced as the command would announce it.
    Trust {
        action: crate::mcp::config::TrustAction,
        body: String,
    },
    /// Write these rows into the transcript, whole.
    Show(String),
}

/// The menu's state.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct McpMenu {
    section: Section,
    screen: Screen,
    selected: usize,
    selected_server: usize,
    window_start: usize,
    load: Load,
    request: u64,
    tools: Vec<String>,
    resources: Vec<Resource>,
    prompts: Vec<Prompt>,
    feedback: Option<String>,
    filter: Option<String>,
    /// The project entry the details screen last drew: what `a` approves,
    /// once it has been shown whole. Set while drawing, which takes `&self`;
    /// a key reads it against the entry as it is now.
    drawn: std::cell::RefCell<Option<Drawn>>,
    /// The pending servers the approve-all confirmation was opened over,
    /// each with its digest: what its Enter approves. Fixed when it opens --
    /// a redraw only compares against it.
    listed: Option<Vec<(String, String)>>,
    /// A frame of the confirmation found the pending servers other than
    /// `listed`: it said so, and Enter approves nothing.
    listed_changed: std::cell::Cell<bool>,
    /// The pending servers the servers tab last drew, each with its digest:
    /// the list a person sees before pressing `p`, and so what `p` binds.
    browsed: std::cell::RefCell<Option<Vec<(String, String)>>>,
}

/// A project entry the details screen drew.
#[derive(Debug, Clone, PartialEq)]
struct Drawn {
    name: String,
    digest: String,
    /// Every row of it was in the last frame drawn, and nothing has written
    /// over the screen since.
    on_screen: bool,
    /// `v` wrote it whole into the transcript, which no later frame undoes.
    by_v: bool,
}

impl Drawn {
    fn whole(&self) -> bool {
        self.on_screen || self.by_v
    }
}

/// What the details screen says when `a` meets an entry other than the one
/// it drew: nothing is approved, and the next frame shows the new one.
const ENTRY_CHANGED: &str = "This entry changed since it was drawn; check it and press a again.";
/// What the approve-all confirmation says once the pending servers differ
/// from the ones it was opened over: Enter then approves nothing.
const PENDING_CHANGED: &str = "The pending servers changed. Press Esc to review them again.";
/// What it says when `a` meets an entry the screen did not show whole.
const ENTRY_NOT_SHOWN: &str = "This entry is not shown whole: press v to see all of it, then a.";

impl McpMenu {
    /// A menu open on the servers, the first one marked (`.open`,
    /// `menu_state.zig:217-224`).
    pub(crate) fn open() -> Self {
        Self {
            section: Section::Servers,
            screen: Screen::Browse,
            selected: 0,
            selected_server: 0,
            window_start: 0,
            load: Load::Ready,
            request: 0,
            tools: Vec::new(),
            resources: Vec::new(),
            prompts: Vec::new(),
            feedback: None,
            filter: None,
            drawn: std::cell::RefCell::new(None),
            listed: None,
            listed_changed: std::cell::Cell::new(false),
            browsed: std::cell::RefCell::new(None),
        }
    }

    /// Something wrote over the screen, or a frame did not land: what the
    /// details last drew is no longer known to be on it. What `v` wrote into
    /// the transcript still was.
    pub(crate) fn forget_drawn(&self) {
        if let Some(drawn) = self.drawn.borrow_mut().as_mut() {
            drawn.on_screen = false;
        }
    }

    #[cfg(test)]
    pub(crate) fn screen(&self) -> Screen {
        self.screen
    }

    /// The feedback row says `text` (`menu_feedback`).
    pub(crate) fn tell(&mut self, text: String) {
        self.feedback = Some(text);
    }

    /// What a catalog read found, when it is the read this menu is waiting on.
    pub(crate) fn loaded(&mut self, request: u64, result: Result<Catalog, String>) {
        if request != self.request || self.load != Load::Loading {
            return;
        }
        match result {
            Ok(Catalog::Tools(tools)) => self.tools = tools,
            Ok(Catalog::Resources(resources)) => self.resources = resources,
            Ok(Catalog::Prompts(prompts)) => self.prompts = prompts,
            Err(message) => {
                self.feedback = Some(message);
                self.load = Load::Failed;
                return;
            }
        }
        self.feedback = None;
        self.load = Load::Ready;
    }

    fn query(&self) -> &str {
        self.filter.as_deref().unwrap_or_default()
    }

    /// The items the section shows, after the filter (`itemCount`).
    fn items(&self, servers: &[ServerView]) -> Vec<(String, String)> {
        let query = self.query().to_ascii_lowercase();
        let matches = |text: &str| query.is_empty() || text.to_ascii_lowercase().contains(&query);
        match self.section {
            Section::Servers => servers
                .iter()
                .map(|server| (server.name.clone(), String::new()))
                .collect(),
            Section::Tools => self
                .tools
                .iter()
                .filter(|tool| matches(tool))
                .map(|tool| (tool.clone(), "Tool".to_string()))
                .collect(),
            Section::Resources => self
                .resources
                .iter()
                .filter(|resource| matches(&resource.uri) || matches(&resource.name))
                .map(|resource| {
                    let description = resource
                        .title
                        .clone()
                        .or_else(|| resource.description.clone())
                        .unwrap_or_else(|| {
                            if resource.template {
                                "Template".to_string()
                            } else {
                                resource.name.clone()
                            }
                        });
                    (resource.uri.clone(), description)
                })
                .collect(),
            Section::Prompts => self
                .prompts
                .iter()
                .filter(|prompt| matches(&prompt.name))
                .map(|prompt| {
                    (
                        prompt.title.clone().unwrap_or_else(|| prompt.name.clone()),
                        prompt
                            .description
                            .clone()
                            .unwrap_or_else(|| "Prompt".to_string()),
                    )
                })
                .collect(),
        }
    }

    /// The server the menu is about: the marked one on the servers tab, the
    /// one marked there last everywhere else.
    fn server<'a>(&self, servers: &'a [ServerView]) -> Option<&'a ServerView> {
        servers.get(self.selected_server.min(servers.len().saturating_sub(1)))
    }

    /// How many rows the menu takes in `max_rows` of a `cols`-wide screen
    /// (`menuRowCount`, `:18-44`). A project server's details may take more
    /// than upstream's twelve: what approving it would run is shown whole
    /// when the screen has the rows ([`entry_rows`]).
    pub(crate) fn height(&self, servers: &[ServerView], cols: u16, max_rows: u16) -> u16 {
        let entry = match (self.screen, self.server(servers)) {
            (Screen::Details, Some(server)) => entry_rows(server, usize::from(cols)).len(),
            _ => 0,
        };
        let max_rows = if entry > 0 {
            max_rows
        } else {
            max_rows.min(MAX_ROWS)
        };
        if max_rows == 0 {
            return 0;
        }
        let header = if max_rows > 2 { HEADER_ROWS } else { 0 };
        let budget = max_rows - header;
        if budget == 0 {
            return header;
        }
        let body = match self.screen {
            Screen::Browse => u16::try_from(self.items(servers).len().max(1)).unwrap_or(u16::MAX),
            Screen::Details => match self.server(servers) {
                Some(server) => {
                    let facts = if server.failure.is_some() { 8 } else { 7 };
                    u16::try_from(facts + entry).unwrap_or(u16::MAX)
                }
                None => 1,
            },
            Screen::Info => 7,
            Screen::Confirm(_) => 1,
        };
        header + body.min(budget)
    }

    /// The menu's rows, top first, for `row_count` rows of a `cols`-wide
    /// screen (`composeMcpMenuRow`, `:52-131`).
    pub(crate) fn rows(
        &self,
        servers: &[ServerView],
        issues: &[String],
        cols: u16,
        row_count: u16,
        palette: Palette,
    ) -> Vec<String> {
        let width = usize::from(cols);
        let mut rows = Vec::with_capacity(usize::from(row_count));
        if width == 0 || row_count == 0 {
            return rows;
        }
        let show_header = row_count > 2;
        if show_header {
            rows.push(self.header(servers.len(), width, palette));
            rows.push(self.second_row(servers, issues, width, palette));
        }
        let body = usize::from(row_count - if show_header { HEADER_ROWS } else { 0 });
        match self.screen {
            Screen::Browse => {
                if self.section == Section::Servers {
                    *self.browsed.borrow_mut() = Some(pending_of(servers));
                }
                rows.extend(self.browse_rows(servers, body, width, show_header, palette))
            }
            Screen::Details => rows.extend(self.details_rows(servers, width, body, palette)),
            Screen::Info => rows.extend(info_rows(width, palette)),
            Screen::Confirm(confirm) => {
                let changed = confirm == Confirm::ApproveAll
                    && self.listed.as_ref() != Some(&pending_of(servers));
                if changed {
                    self.listed_changed.set(true);
                }
                rows.push(text_row(
                    if changed || (confirm == Confirm::ApproveAll && self.listed_changed.get()) {
                        PENDING_CHANGED
                    } else {
                        confirm.text()
                    },
                    width,
                    palette.notice_label(),
                    0,
                    palette,
                ))
            }
        }
        rows.truncate(usize::from(row_count));
        rows
    }

    /// `composeHeader` (`:155-178`): `MCP n` and the tabs, or the title and
    /// the active tab alone when they do not fit.
    fn header(&self, count: usize, width: usize, palette: Palette) -> String {
        let title = format!("{}MCP {count}{}", palette.subtitle(), palette.reset());
        let tab = |section: Section| {
            if section == self.section {
                format!(
                    "{}[{}]{}",
                    palette.subtitle(),
                    section.label(),
                    palette.reset()
                )
            } else {
                format!("{}{}{}", palette.dim(), section.label(), palette.reset())
            }
        };
        let mut wide = title.clone();
        for section in Section::ALL {
            wide.push_str("  ");
            wide.push_str(&tab(section));
        }
        if cells(&wide) <= width {
            return wide;
        }
        let compact = format!("{title}  {}", tab(self.section));
        if cells(&compact) <= width {
            compact
        } else {
            format!(
                "{}{}{}",
                palette.subtitle(),
                ellipsized(&format!("MCP {count}"), width),
                palette.reset()
            )
        }
    }

    /// The row under the header (`:62-104`).
    fn second_row(
        &self,
        servers: &[ServerView],
        issues: &[String],
        width: usize,
        palette: Palette,
    ) -> String {
        if let Some(feedback) = &self.feedback {
            let style = if self.load == Load::Failed {
                palette.notice_label()
            } else {
                palette.dim()
            };
            return text_row(feedback, width, style, 2, palette);
        }
        if let Some(filter) = self.filter.as_ref().filter(|filter| !filter.is_empty()) {
            return text_row(
                &format!("Filter: {filter}"),
                width,
                palette.dim(),
                2,
                palette,
            );
        }
        if self.screen == Screen::Browse && self.section == Section::Servers && !issues.is_empty() {
            let summary = if issues.len() == 1 {
                "1 project MCP configuration error. Run /mcp list for details. ".to_string()
            } else {
                format!(
                    "{} project MCP configuration errors. Run /mcp list for details. ",
                    issues.len()
                )
            };
            return text_row(
                &format!("{summary}{}", issues[0]),
                width,
                palette.notice_label(),
                2,
                palette,
            );
        }
        match self.section {
            Section::Resources | Section::Prompts => match self.server(servers) {
                Some(server) => fact_row("Server", &server.name, width, palette),
                None => String::new(),
            },
            Section::Tools => text_row("All available servers", width, palette.dim(), 2, palette),
            Section::Servers => String::new(),
        }
    }

    /// `composeBrowseRow` (`:208-293`).
    fn browse_rows(
        &self,
        servers: &[ServerView],
        visible: usize,
        width: usize,
        feedback_in_header: bool,
        palette: Palette,
    ) -> Vec<String> {
        if self.load == Load::Loading && self.section != Section::Servers {
            return vec![text_row(
                "Loading MCP catalog\u{2026}",
                width,
                palette.dim(),
                2,
                palette,
            )];
        }
        if self.load == Load::Failed && self.feedback.is_some() && self.section != Section::Servers
        {
            if feedback_in_header {
                return vec![String::new()];
            }
            let feedback = self.feedback.as_deref().unwrap_or_default();
            return vec![text_row(
                feedback,
                width,
                palette.notice_label(),
                2,
                palette,
            )];
        }
        let items = self.items(servers);
        if items.is_empty() {
            let text = if !self.query().is_empty() {
                "No matching MCP items."
            } else {
                match self.section {
                    Section::Servers => "No MCP servers configured.",
                    Section::Tools => "No MCP tools available.",
                    Section::Resources => "No MCP resources available.",
                    Section::Prompts => "No MCP prompts available.",
                }
            };
            return vec![text_row(text, width, palette.dim(), 2, palette)];
        }
        let visible = visible.min(items.len()).max(1);
        let start = window_start(self.window_start, self.selected, items.len(), visible);
        (start..items.len())
            .take(visible)
            .map(|index| {
                let marked = index == self.selected;
                match self.section {
                    Section::Servers => server_row(&servers[index], marked, width, palette),
                    _ => catalog_row(&items[index].0, &items[index].1, marked, width, palette),
                }
            })
            .collect()
    }

    /// `composeDetailsRow` (`:434-466`).
    fn details_rows(
        &self,
        servers: &[ServerView],
        width: usize,
        body: usize,
        palette: Palette,
    ) -> Vec<String> {
        let Some(server) = self.server(servers) else {
            return vec![text_row(
                "MCP server is no longer available.",
                width,
                palette.notice_label(),
                2,
                palette,
            )];
        };
        let capabilities = server.capability_summary();
        let source = match server.source {
            Source::Profile => "Profile \u{b7} ~/.xfx/mcp.json",
            Source::Workspace => "Project \u{b7} .mcp.json",
        };
        let mut rows = vec![
            fact_row("Server", &server.name, width, palette),
            fact_row("State", server.state_label(), width, palette),
            fact_row("Source", source, width, palette),
            fact_row("Transport", server.transport.tag(), width, palette),
            fact_row(
                "Policy",
                if server.required {
                    "required"
                } else {
                    "optional"
                },
                width,
                palette,
            ),
            fact_row(
                "Protocol",
                server.protocol_version.as_deref().unwrap_or("unavailable"),
                width,
                palette,
            ),
            fact_row("Capabilities", &capabilities, width, palette),
        ];
        if let Some(failure) = &server.failure {
            rows.push(fact_row("Error", failure, width, palette));
        }
        // What approving it would run or reach, whole -- not through a fact
        // row's clip -- or, on a screen too short for all of it, as much as
        // fits and how to see the rest.
        let entry: Vec<String> = entry_rows(server, width)
            .into_iter()
            .map(|row| format!("{}{row}{}", palette.subtitle(), palette.reset()))
            .collect();
        // The facts cut to the rows the panel has, so nothing below is cut
        // again by the menu's own truncation; then an entry that does not fit
        // gets its row saying how to see it -- whenever the panel has a row
        // at all -- in place of the last fact if it must.
        rows.truncate(body);
        let mut room = body - rows.len();
        let fits = entry.len() <= room;
        if !fits && room == 0 && rows.pop().is_some() {
            room = 1;
        }
        let mut drawn = self.drawn.borrow_mut();
        *drawn = server.entry.as_ref().map(|entry| {
            let by_v = drawn.as_ref().is_some_and(|drawn| {
                drawn.by_v && drawn.name == server.name && drawn.digest == entry.digest
            });
            Drawn {
                name: server.name.clone(),
                digest: entry.digest.clone(),
                on_screen: fits,
                by_v,
            }
        });
        drop(drawn);
        if fits {
            rows.extend(entry);
        } else if room > 0 {
            let more = entry.len() - (room - 1);
            rows.extend(entry.into_iter().take(room - 1));
            rows.push(text_row(
                &format!("\u{2026} {more} more rows: press v to see all"),
                width,
                palette.notice_label(),
                2,
                palette,
            ));
        }
        rows
    }

    /// One keystroke (`menu_state.zig:215-420`, the key routing in
    /// `app_input_runtime.zig:1098-1111`).
    pub(crate) fn key(
        &mut self,
        action: KeyAction,
        servers: &[ServerView],
        visible: usize,
    ) -> Effect {
        match (self.screen, action) {
            // What the confirmation was opened over -- unless a frame of it,
            // or the servers as they are now, found the pending set changed:
            // then it approves nothing and goes back to the list.
            (Screen::Confirm(Confirm::ApproveAll), KeyAction::Enter) => {
                self.screen = Screen::Browse;
                let changed = self.listed_changed.replace(false);
                let Some(listed) = self.listed.take() else {
                    return Effect::Repaint;
                };
                if changed || listed != pending_of(servers) {
                    return Effect::Repaint;
                }
                Effect::Trust {
                    action: crate::mcp::config::TrustAction::ApproveListed(listed),
                    body: "Approving all project MCP servers for this workspace.".to_string(),
                }
            }
            (Screen::Confirm(confirm), KeyAction::Enter) => {
                let name = self.server(servers).map(|server| server.name.clone());
                self.screen = Screen::Browse;
                let command = match (confirm, name) {
                    (Confirm::Remove, Some(name)) => format!("remove {name}"),
                    (Confirm::Reject, Some(name)) => format!("trust reject {name}"),
                    (Confirm::Reset, _) => "trust reset".to_string(),
                    _ => return Effect::Repaint,
                };
                Effect::Command(command)
            }
            (Screen::Confirm(_) | Screen::Details | Screen::Info, KeyAction::Escape) => {
                self.screen = Screen::Browse;
                Effect::Repaint
            }
            (Screen::Confirm(_), _) => Effect::Ignored,
            (Screen::Info, _) => Effect::Ignored,
            (Screen::Details, KeyAction::Char(character)) => self.details_key(character, servers),
            (Screen::Details, KeyAction::Enter) => match self.server(servers) {
                Some(server)
                    if server.transport != Transport::Stdio
                        && server.status() != Status::Connecting =>
                {
                    self.feedback = Some(crate::mcp::command::AUTH_UNAVAILABLE.to_string());
                    Effect::Repaint
                }
                _ => Effect::Ignored,
            },
            (Screen::Details, KeyAction::Up | KeyAction::Down) => {
                self.step(action == KeyAction::Down, servers.len(), servers.len());
                self.selected_server = self.selected;
                Effect::Repaint
            }
            (Screen::Details, _) => Effect::Ignored,
            (Screen::Browse, KeyAction::Escape) => {
                if self.filter.is_some() {
                    self.filter = None;
                    self.selected = 0;
                    self.window_start = 0;
                    return Effect::Repaint;
                }
                Effect::Close
            }
            (Screen::Browse, KeyAction::Up | KeyAction::Down) => {
                let count = self.items(servers).len();
                self.step(action == KeyAction::Down, count, visible.max(1));
                if self.section == Section::Servers {
                    self.selected_server = self.selected;
                }
                Effect::Repaint
            }
            (Screen::Browse, KeyAction::Next | KeyAction::Previous) => {
                self.cycle(action == KeyAction::Next, servers)
            }
            (Screen::Browse, KeyAction::Enter) => {
                if self.section == Section::Servers && !servers.is_empty() {
                    *self.drawn.borrow_mut() = None;
                    self.screen = Screen::Details;
                    return Effect::Repaint;
                }
                Effect::Ignored
            }
            (Screen::Browse, KeyAction::Backspace) => {
                if let Some(filter) = self.filter.as_mut() {
                    filter.pop();
                    self.selected = 0;
                    self.window_start = 0;
                    return Effect::Repaint;
                }
                Effect::Ignored
            }
            (Screen::Browse, KeyAction::Char(character)) => {
                if let Some(filter) = self.filter.as_mut() {
                    if filter.len() < 128 && (' '..='~').contains(&character) {
                        filter.push(character);
                        self.selected = 0;
                        self.window_start = 0;
                    }
                    return Effect::Repaint;
                }
                self.browse_key(character)
            }
        }
    }

    fn browse_key(&mut self, character: char) -> Effect {
        match (self.section, character) {
            (Section::Servers, 's') => {
                self.feedback = Some(crate::mcp::command::AUTH_UNAVAILABLE.to_string());
                Effect::Repaint
            }
            (Section::Servers, 'a') => Effect::Compose("/mcp add ".to_string()),
            (Section::Servers, 'r') => {
                self.feedback = Some("Reloading MCP configuration\u{2026}".to_string());
                self.load = Load::Ready;
                Effect::Reload
            }
            (Section::Servers, 'c') => {
                self.screen = Screen::Info;
                Effect::Repaint
            }
            // The list the servers tab last drew -- what the person pressing
            // `p` was looking at. Before it has drawn one there is no such
            // list: the key only asks for the frame.
            (Section::Servers, 'p') => {
                let Some(browsed) = self.browsed.borrow().clone() else {
                    return Effect::Repaint;
                };
                self.listed = Some(browsed);
                self.listed_changed.set(false);
                self.screen = Screen::Confirm(Confirm::ApproveAll);
                Effect::Repaint
            }
            (Section::Servers, 'z') => {
                self.screen = Screen::Confirm(Confirm::Reset);
                Effect::Repaint
            }
            (Section::Tools | Section::Resources | Section::Prompts, '/') => {
                self.filter = Some(String::new());
                self.selected = 0;
                self.window_start = 0;
                Effect::Repaint
            }
            _ => Effect::Ignored,
        }
    }

    /// A key on a server's details (`serverActionAvailable`,
    /// `menu_state.zig:48-59`, and the help screen's list of them).
    fn details_key(&mut self, character: char, servers: &[ServerView]) -> Effect {
        let Some(server) = self.server(servers) else {
            return Effect::Ignored;
        };
        match character {
            'd' if server.source == Source::Profile => {
                self.screen = Screen::Confirm(Confirm::Remove);
                Effect::Repaint
            }
            'x' if server.source == Source::Workspace
                && server.admission != Some(Admission::Rejected) =>
            {
                self.screen = Screen::Confirm(Confirm::Reject);
                Effect::Repaint
            }
            // The entry this screen last drew -- and only while it is still
            // the entry: one a reload read after the frame was never seen,
            // so `a` approves nothing and the next frame shows it.
            'a' if server.admission == Some(Admission::Pending) => {
                let Some(entry) = &server.entry else {
                    return Effect::Ignored;
                };
                let drawn = self.drawn.borrow().clone();
                match drawn {
                    Some(drawn) if drawn.name == server.name && drawn.digest == entry.digest => {
                        if !drawn.whole() {
                            self.feedback = Some(ENTRY_NOT_SHOWN.to_string());
                            return Effect::Repaint;
                        }
                    }
                    Some(drawn) if drawn.name == server.name => {
                        self.feedback = Some(ENTRY_CHANGED.to_string());
                        return Effect::Repaint;
                    }
                    _ => {
                        self.feedback = Some(ENTRY_NOT_SHOWN.to_string());
                        return Effect::Repaint;
                    }
                }
                self.feedback = None;
                self.screen = Screen::Browse;
                Effect::Trust {
                    body: format!(
                        "Approving project MCP server '{}'.",
                        crate::mcp::terminal_safe(&server.name)
                    ),
                    action: crate::mcp::config::TrustAction::ApproveShown {
                        name: server.name.clone(),
                        digest: entry.digest.clone(),
                    },
                }
            }
            // Written whole into the transcript: shown, so `a` may approve it.
            'v' => match &server.entry {
                Some(entry) => {
                    let mut drawn = self.drawn.borrow_mut();
                    let on_screen = drawn.as_ref().is_some_and(|drawn| {
                        drawn.on_screen && drawn.name == server.name && drawn.digest == entry.digest
                    });
                    *drawn = Some(Drawn {
                        name: server.name.clone(),
                        digest: entry.digest.clone(),
                        on_screen,
                        by_v: true,
                    });
                    drop(drawn);
                    Effect::Show(format!(
                        "Project MCP server '{}' in .mcp.json:\n  {}\n",
                        crate::mcp::terminal_safe(&server.name),
                        entry.rows.join("\n  ")
                    ))
                }
                None => Effect::Ignored,
            },
            'c' => {
                self.screen = Screen::Info;
                Effect::Repaint
            }
            _ => Effect::Ignored,
        }
    }

    /// Moves the mark one item, wrapping (`.move`, `menu_state.zig:241-260`).
    fn step(&mut self, down: bool, count: usize, visible: usize) {
        if count == 0 {
            return;
        }
        self.selected = if down {
            (self.selected + 1) % count
        } else if self.selected == 0 {
            count - 1
        } else {
            self.selected - 1
        };
        self.window_start = window_start(self.window_start, self.selected, count, visible.max(1));
    }

    /// Changes the tab (`.cycle_section`, `menu_state.zig:270-293`): a
    /// catalog section reads its items afresh.
    fn cycle(&mut self, forward: bool, servers: &[ServerView]) -> Effect {
        let index = self.section.index();
        let next = if forward {
            (index + 1) % 4
        } else {
            (index + 3) % 4
        };
        self.section = Section::ALL[next];
        self.screen = Screen::Browse;
        self.selected = 0;
        self.window_start = 0;
        self.filter = None;
        self.feedback = None;
        if self.section == Section::Servers {
            self.selected = self.selected_server;
            self.load = Load::Ready;
            return Effect::Repaint;
        }
        self.request += 1;
        self.load = Load::Loading;
        let section = match self.section {
            Section::Tools => crate::mcp::runtime::Section::Tools,
            Section::Resources => crate::mcp::runtime::Section::Resources,
            _ => crate::mcp::runtime::Section::Prompts,
        };
        Effect::Load {
            request: self.request,
            section,
            server: self.server(servers).map(|server| server.name.clone()),
        }
    }

    /// The hint row (`composeMcpMenuHintRow`, `input_presentation.zig:607-692`).
    pub(crate) fn key_help(&self, servers: &[ServerView], cols: u16) -> String {
        const ROOT: [&str; 3] = [
            "\u{2191}\u{2193} move  tab section  enter inspect  s add Slack  a add  r reload  c help  esc close",
            "\u{2191}\u{2193} move  tab section  enter  s Slack  a add  r reload  c help  esc",
            "tab enter s a r c esc",
        ];
        const CATALOG: [&str; 3] = [
            "\u{2191}\u{2193} navigate     tab section     enter open     / filter     esc back",
            "\u{2191}\u{2193} move  tab section  enter  / filter  esc",
            "tab enter / esc",
        ];
        const CONFIRM: [&str; 3] = [
            "enter confirm     esc cancel",
            "enter confirm  esc",
            "enter esc",
        ];
        const INFO: [&str; 3] = ["esc back", "esc", "esc"];
        let mut details = String::new();
        if let Some(server) = self.server(servers) {
            for (available, label) in [
                (
                    server.transport != Transport::Stdio
                        && server.status() != Status::Connecting
                        && matches!(server.status(), Status::NeedsAuth | Status::Unavailable),
                    "enter sign in  ",
                ),
                (server.admission == Some(Admission::Pending), "a approve  "),
                (server.entry.is_some(), "v show whole  "),
                (
                    server.source == Source::Workspace
                        && server.admission != Some(Admission::Rejected),
                    "x reject  ",
                ),
                (server.source == Source::Profile, "d remove  "),
            ] {
                if available {
                    details.push_str(label);
                }
            }
        }
        details.push_str("c help  esc back");
        // `v` stays in every width: on a screen too short for the entry it is
        // the only way to see it, and so to approve it.
        let entry = self
            .server(servers)
            .is_some_and(|server| server.entry.is_some());
        let (medium, narrow) = if entry {
            ("v show whole  c help  esc back", "v c esc")
        } else {
            ("c help  esc back", "c esc")
        };
        let details_variants = [details.as_str(), medium, narrow];
        let variants: [&str; 3] = match self.screen {
            Screen::Browse if self.section == Section::Servers => ROOT,
            Screen::Browse => CATALOG,
            Screen::Details => details_variants,
            Screen::Info => INFO,
            Screen::Confirm(_) => CONFIRM,
        };
        let width = usize::from(cols);
        variants
            .iter()
            .copied()
            .find(|candidate| cells(candidate) <= width)
            .map(str::to_string)
            .unwrap_or_else(|| ellipsized(variants[2], width))
    }
}

/// A keystroke, in the menu's vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeyAction {
    Up,
    Down,
    Next,
    Previous,
    Enter,
    Escape,
    Backspace,
    Char(char),
}

impl KeyAction {
    /// What a decoded action means here; `None` for one the menu swallows.
    /// Tab and Right go to the next tab, Shift-Tab and Left to the one
    /// before, as upstream's cycle does.
    pub(crate) fn of(action: Action) -> Option<Self> {
        match action {
            Action::Up | Action::ControlK => Some(Self::Up),
            Action::Down | Action::ControlJ => Some(Self::Down),
            Action::Tab | Action::Right => Some(Self::Next),
            Action::BackTab | Action::Left => Some(Self::Previous),
            Action::Submit => Some(Self::Enter),
            Action::Escape => Some(Self::Escape),
            Action::Backspace => Some(Self::Backspace),
            _ => None,
        }
    }
}

/// The first item a `visible`-row window shows (`visibleWindowStart`, `:308-320`).
fn window_start(requested: usize, selected: usize, count: usize, visible: usize) -> usize {
    if count == 0 {
        return 0;
    }
    let visible = visible.min(count).max(1);
    let selected = selected.min(count - 1);
    let mut start = requested.min(count - visible);
    if selected < start {
        start = selected;
    }
    if selected >= start + visible {
        start = selected + 1 - visible;
    }
    start.min(count - visible)
}

/// `composeTextRow` (`:594-608`).
fn text_row(text: &str, width: usize, style: &str, indent: usize, palette: Palette) -> String {
    let indent = indent.min(width);
    format!(
        "{style}{}{}{}",
        " ".repeat(indent),
        ellipsized(&crate::mcp::terminal_safe(text), width - indent),
        palette.reset()
    )
}

/// `composeFactRow` (`:580-592`): the label dim, the value bright at column 22.
fn fact_row(label: &str, value: &str, width: usize, palette: Palette) -> String {
    let mut row = String::from(palette.dim());
    if width > 4 {
        row.push_str("  ");
    }
    let column = VALUE_COLUMN.min(width);
    row.push_str(&ellipsized(label, column.saturating_sub(2)));
    pad_to(&mut row, column);
    row.push_str(palette.subtitle());
    row.push_str(&ellipsized(
        &crate::mcp::terminal_safe(value),
        width - column,
    ));
    row.push_str(palette.reset());
    row
}

/// `composeServerRow` (`:362-402`): the name, the state at column 24, and
/// `transport · source` at column 46, bright when marked.
fn server_row(server: &ServerView, marked: bool, width: usize, palette: Palette) -> String {
    let mut row = String::new();
    if width > 4 {
        row.push_str("  ");
    }
    row.push_str(if marked {
        palette.subtitle()
    } else {
        palette.dim()
    });
    let name_width = if width >= 80 {
        20
    } else if width >= 56 {
        16
    } else {
        width.saturating_sub(2)
    };
    row.push_str(&ellipsized(
        &crate::mcp::terminal_safe(&server.name),
        name_width,
    ));
    if width > name_width + 4 {
        pad_to(&mut row, name_width + 4);
        let state_width = if width >= 80 {
            20
        } else {
            width - name_width - 4
        };
        row.push_str(&ellipsized(server.state_label(), state_width));
    }
    if width >= 80 {
        pad_to(&mut row, 46);
        let source = match server.source {
            Source::Profile => "Profile",
            Source::Workspace => "Project",
        };
        row.push_str(&ellipsized(
            &format!("{} \u{b7} {source}", server.transport.label()),
            width - 46,
        ));
    }
    row.push_str(palette.reset());
    row
}

/// `composeCatalogRow` (`:322-353`): the identity, and its description at
/// column 38 on a screen wide enough.
fn catalog_row(
    identity: &str,
    description: &str,
    marked: bool,
    width: usize,
    palette: Palette,
) -> String {
    let mut row = String::new();
    if width > 4 {
        row.push_str("  ");
    }
    row.push_str(if marked {
        palette.subtitle()
    } else {
        palette.dim()
    });
    if width >= 76 {
        let identity_width = 34.min(width - 4);
        row.push_str(&ellipsized(
            &crate::mcp::terminal_safe(identity),
            identity_width,
        ));
        pad_to(&mut row, identity_width + 4);
        row.push_str(&ellipsized(
            &crate::mcp::terminal_safe(description),
            width - identity_width - 4,
        ));
    } else {
        row.push_str(&ellipsized(
            &crate::mcp::terminal_safe(identity),
            width.saturating_sub(2),
        ));
    }
    row.push_str(palette.reset());
    row
}

/// `composeInfoRow` (`:295-306`), with this build's file and keys.
fn info_rows(width: usize, palette: Palette) -> Vec<String> {
    vec![
        fact_row("Profile config", "~/.xfx/mcp.json", width, palette),
        fact_row("Project config", "<workspace>/.mcp.json", width, palette),
        text_row(
            "servers: s add Slack \u{b7} a add \u{b7} r reload \u{b7} enter inspect",
            width,
            palette.dim(),
            2,
            palette,
        ),
        text_row(
            "project trust: p approve all \u{b7} z reset",
            width,
            palette.dim(),
            2,
            palette,
        ),
        text_row(
            "details: enter sign in \u{b7} l logout \u{b7} d remove \u{b7} v show whole",
            width,
            palette.dim(),
            2,
            palette,
        ),
        text_row(
            "project details: a approve \u{b7} x reject",
            width,
            palette.dim(),
            2,
            palette,
        ),
        text_row(
            "catalogs: / filter \u{b7} enter open \u{b7} i insert preview",
            width,
            palette.dim(),
            2,
            palette,
        ),
    ]
}

/// Every pending project server in `servers`, with the digest of its entry:
/// what an approve-all is over.
fn pending_of(servers: &[ServerView]) -> Vec<(String, String)> {
    servers
        .iter()
        .filter(|server| server.admission == Some(Admission::Pending))
        .filter_map(|server| {
            let entry = server.entry.as_ref()?;
            Some((server.name.clone(), entry.digest.clone()))
        })
        .collect()
}

/// A project server's entry ([`crate::mcp::runtime::ShownEntry`]) as the
/// details screen lays it out: each row whole, wrapped onto as many screen
/// rows as it needs, two columns in. Each row is already terminal-safe.
fn entry_rows(server: &ServerView, width: usize) -> Vec<String> {
    let Some(entry) = &server.entry else {
        return Vec::new();
    };
    let cols = u16::try_from(width.saturating_sub(2).max(1)).unwrap_or(u16::MAX);
    entry
        .rows
        .iter()
        .flat_map(|row| {
            super::wrap::wrap(row, cols)
                .into_iter()
                .map(|cut| format!("  {}", row[cut.start..cut.end].trim_end()))
                .collect::<Vec<_>>()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::theme::{Depth, Mode};
    use super::*;
    use crate::mcp::runtime::{Capabilities, Connection};

    const PALETTE: Palette = Palette {
        mode: Mode::Dark,
        depth: Depth::Ansi256,
    };

    fn plain(row: &str) -> String {
        let mut out = String::new();
        let mut chars = row.chars();
        while let Some(c) = chars.next() {
            if c == '\u{1b}' {
                for next in chars.by_ref() {
                    if next == 'm' {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    fn view(
        name: &str,
        source: Source,
        connection: Connection,
        admission: Option<Admission>,
    ) -> ServerView {
        ServerView {
            name: name.into(),
            source,
            transport: Transport::Stdio,
            admission,
            connection,
            needs_auth: false,
            required: false,
            reloading: false,
            protocol_version: Some("2024-11-05".into()),
            capabilities: Capabilities::default(),
            tool_count: (connection == Connection::Ready).then_some(1),
            tools_held: usize::from(connection == Connection::Ready),
            listed: Default::default(),
            failure: None,
            entry: None,
        }
    }

    #[test]
    fn the_empty_menu_is_upstreams_three_rows_and_root_hint() {
        // `cap/fx-mcp-empty-40x120-menu.txt`, rows 5-7 and 9.
        let menu = McpMenu::open();
        assert_eq!(menu.height(&[], 120, 20), 3);
        let rows: Vec<String> = menu
            .rows(&[], &[], 120, 3, PALETTE)
            .iter()
            .map(|row| plain(row))
            .collect();
        assert_eq!(
            rows,
            [
                "MCP 0  [Servers]  Tools  Resources  Prompts",
                "",
                "  No MCP servers configured."
            ]
        );
        assert_eq!(
            menu.key_help(&[], 120),
            "\u{2191}\u{2193} move  tab section  enter inspect  s add Slack  a add  r reload  c help  esc close"
        );
    }

    #[test]
    fn two_servers_are_upstreams_columns_and_the_mark_follows_down() {
        // `cap/fx-mcp-fixture-40x120-menu{,-down}.txt`, rows 17-20.
        let servers = [
            view("fixture", Source::Profile, Connection::Ready, None),
            view(
                "proj",
                Source::Workspace,
                Connection::Disabled,
                Some(Admission::Rejected),
            ),
        ];
        let mut menu = McpMenu::open();
        assert_eq!(menu.height(&servers, 120, 20), 4);
        let rows = menu.rows(&servers, &[], 120, 4, PALETTE);
        assert_eq!(
            plain(&rows[2]),
            format!(
                "  fixture{}Ready{}stdio \u{b7} Profile",
                " ".repeat(15),
                " ".repeat(17)
            )
        );
        assert_eq!(
            plain(&rows[3]),
            format!(
                "  proj{}Disabled{}stdio \u{b7} Project",
                " ".repeat(18),
                " ".repeat(14)
            )
        );
        assert!(rows[2].contains(PALETTE.subtitle()));
        assert!(rows[3].contains(PALETTE.dim()));
        assert_eq!(menu.key(KeyAction::Down, &servers, 2), Effect::Repaint);
        let rows = menu.rows(&servers, &[], 120, 4, PALETTE);
        assert!(rows[2].contains(PALETTE.dim()) && rows[3].contains(PALETTE.subtitle()));
    }

    #[test]
    fn a_catalog_tab_loads_for_the_marked_server_and_a_failure_is_its_second_row() {
        // `cap/fx-mcp-fixture-40x120-menu-{tools,resources}.txt`.
        let servers = [
            view("fixture", Source::Profile, Connection::Ready, None),
            view(
                "proj",
                Source::Workspace,
                Connection::Disabled,
                Some(Admission::Rejected),
            ),
        ];
        let mut menu = McpMenu::open();
        let Effect::Load {
            request, server, ..
        } = menu.key(KeyAction::Next, &servers, 2)
        else {
            panic!("the tools tab reads its catalog");
        };
        assert_eq!(server.as_deref(), Some("fixture"));
        menu.loaded(request, Ok(Catalog::Tools(vec!["mcp_fixture_echo".into()])));
        assert_eq!(menu.height(&servers, 120, 20), 3);
        let rows: Vec<String> = menu
            .rows(&servers, &[], 120, 3, PALETTE)
            .iter()
            .map(|row| plain(row))
            .collect();
        assert_eq!(
            rows,
            [
                "MCP 2  Servers  [Tools]  Resources  Prompts".to_string(),
                "  All available servers".to_string(),
                format!("  mcp_fixture_echo{}Tool", " ".repeat(20)),
            ]
        );
        assert_eq!(
            menu.key_help(&servers, 120),
            "\u{2191}\u{2193} navigate     tab section     enter open     / filter     esc back"
        );
        let Effect::Load { request, .. } = menu.key(KeyAction::Next, &servers, 1) else {
            panic!("the resources tab reads its catalog");
        };
        menu.loaded(
            request,
            Err("MCP catalog failed: McpWorkspaceApprovalRequired".into()),
        );
        let rows = menu.rows(&servers, &[], 120, 3, PALETTE);
        assert_eq!(
            plain(&rows[1]),
            "  MCP catalog failed: McpWorkspaceApprovalRequired"
        );
        assert!(rows[1].starts_with(PALETTE.notice_label()));
        assert_eq!(plain(&rows[2]), "");
    }

    #[test]
    fn removal_asks_first_and_then_runs_the_command() {
        let servers = [view("fixture", Source::Profile, Connection::Ready, None)];
        let mut menu = McpMenu::open();
        assert_eq!(menu.key(KeyAction::Enter, &servers, 1), Effect::Repaint);
        assert_eq!(menu.screen(), Screen::Details);
        assert_eq!(menu.key(KeyAction::Char('d'), &servers, 1), Effect::Repaint);
        let rows = menu.rows(&servers, &[], 120, 3, PALETTE);
        assert_eq!(
            plain(&rows[2]),
            "Remove this profile MCP server? press enter to confirm."
        );
        assert_eq!(
            menu.key(KeyAction::Enter, &servers, 1),
            Effect::Command("remove fixture".into())
        );
        assert_eq!(menu.key(KeyAction::Char('s'), &servers, 1), Effect::Repaint);
        let rows = menu.rows(&servers, &[], 120, 3, PALETTE);
        assert_eq!(
            plain(&rows[1]),
            "  Interactive MCP authentication is unavailable here."
        );
    }

    #[test]
    fn a_hostile_project_server_name_is_spelled_out_on_the_issue_row() {
        // A `.mcp.json` key that would retitle the window and reverse what
        // follows it is no valid server name (`config.rs`'s issue for it
        // carries the name as written); the menu's issue row spells it out.
        let dir = tempfile::tempdir().expect("dir");
        std::fs::write(
            dir.path().join(crate::mcp::config::WORKSPACE_FILE),
            serde_json::json!({"mcpServers": {"\u{1b}]0;owned\u{7}\u{202e}live": {"command": "x"}}})
                .to_string(),
        )
        .unwrap();
        let issues = crate::mcp::config::load_workspace(
            dir.path(),
            &crate::mcp::config::Choices::default(),
            &|_| None,
        )
        .issues;
        assert_eq!(issues.len(), 1, "{issues:?}");
        let rows = McpMenu::open().rows(&[], &issues, 120, 3, PALETTE);
        let row = &rows[1];
        assert!(
            !row.contains(['\u{7}', '\u{202e}']) && !row.contains("\u{1b}]"),
            "{row:?}"
        );
        assert_eq!(
            plain(row),
            r"  1 project MCP configuration error. Run /mcp list for details. \x1b]0;owned\x07\u{202e}live: McpInvalidServerName"
        );
    }

    #[test]
    fn details_show_a_rewritten_project_entry_and_a_approves_only_what_they_drew() {
        // With the prompts dismissed (Escape), the details screen is how a
        // person gets back to an entry the file changed under them: it shows
        // what the entry would run, and `a` approves that entry -- not
        // whatever the file says by the time the answer is saved.
        use crate::mcp::config::{entry_digest, TrustAction};
        use crate::mcp::runtime::Job;
        let home = tempfile::tempdir().expect("home");
        let workspace = home.path().join("work");
        std::fs::create_dir_all(&workspace).expect("workspace");
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/support/mcp_fixture.sh")
            .to_string_lossy()
            .into_owned();
        let entry = |mark: &str| serde_json::json!({"command": "/bin/sh", "args": [fixture, mark]});
        let write = |mark: &str| {
            std::fs::write(
                workspace.join(".mcp.json"),
                serde_json::json!({"mcpServers": {"proj": entry(mark)}}).to_string(),
            )
            .expect("write .mcp.json")
        };
        let settings = home.path().join(".xfx").join("settings.json");
        let runtime = crate::mcp::McpRuntime::new(crate::mcp::Paths {
            profile: Some(home.path().join(".xfx").join("mcp.json")),
            settings: Some(settings.clone()),
            workspace: workspace.clone(),
        });
        let proj = |runtime: &crate::mcp::McpRuntime| {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
            loop {
                let view = runtime
                    .views()
                    .into_iter()
                    .find(|view| view.name == "proj")
                    .expect("proj");
                if view.connection != Connection::Connecting {
                    return view;
                }
                assert!(std::time::Instant::now() < deadline, "proj never settled");
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        };
        write("first");
        runtime.start();
        bounded_run(
            &runtime,
            Job::Trust {
                action: TrustAction::Approve("proj".into()),
                body: "Approving project MCP server 'proj'.".into(),
            },
        );
        assert_eq!(proj(&runtime).status(), Status::Ready);
        assert_eq!(
            proj(&runtime).entry,
            None,
            "an approved entry is expanded: not shown"
        );
        runtime.suppress_prompts();
        write("second");
        bounded_run(&runtime, Job::Command("reload".into()));
        assert_eq!(proj(&runtime).state_label(), "Pending trust");

        let views = runtime.views();
        let mut menu = McpMenu::open();
        assert_eq!(menu.key(KeyAction::Enter, &views, 5), Effect::Repaint);
        let command = format!("command: /bin/sh {fixture} second");
        let rows: Vec<String> = menu
            .rows(&views, &[], 200, 30, PALETTE)
            .iter()
            .map(|row| plain(row))
            .collect();
        assert!(rows.contains(&format!("  {command}")), "{rows:#?}");
        assert_eq!(
            menu.key(KeyAction::Char('v'), &views, 5),
            Effect::Show(format!(
                "Project MCP server 'proj' in .mcp.json:\n  {command}\n"
            ))
        );
        let shown = entry_digest(&entry("second"));
        let Effect::Trust { action, body } = menu.key(KeyAction::Char('a'), &views, 5) else {
            panic!("`a` on a pending project server is a trust choice")
        };
        assert_eq!(
            action,
            TrustAction::ApproveShown {
                name: "proj".into(),
                digest: shown.clone()
            }
        );
        // The file changes again before the answer is saved: the approval is
        // for what the screen drew, and the newer entry still asks.
        write("third");
        bounded_run(&runtime, Job::Command("reload".into()));
        bounded_run(&runtime, Job::Trust { action, body });
        assert_eq!(proj(&runtime).state_label(), "Pending trust");
        let saved: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&settings).expect("settings"))
                .expect("json");
        assert_eq!(
            saved["workspaces"][workspace.to_str().unwrap()]["enabledMcpjsonServerDigests"]["proj"],
            serde_json::json!(shown)
        );
        runtime.shutdown();
    }

    #[test]
    fn details_too_short_for_an_entry_say_how_much_more_and_how_to_see_it() {
        let mut server = view(
            "p",
            Source::Workspace,
            Connection::Disabled,
            Some(Admission::Pending),
        );
        let command = format!("command: /bin/sh -c '{}'", "w ".repeat(150));
        server.entry = Some(std::sync::Arc::new(crate::mcp::runtime::ShownEntry {
            rows: vec![command.clone()],
            digest: "sha256:p".into(),
        }));
        let servers = [server];
        let mut menu = McpMenu::open();
        menu.key(KeyAction::Enter, &servers, 5);
        // Seven facts and the entry, wrapped at 38 columns: whole when the
        // screen has the rows, past upstream's twelve.
        let wrapped = entry_rows(&servers[0], 40).len();
        assert!(wrapped >= 8, "{wrapped}");
        assert_eq!(
            menu.height(&servers, 40, 40),
            u16::try_from(2 + 7 + wrapped).unwrap()
        );
        let whole: Vec<String> = menu
            .rows(&servers, &[], 40, 40, PALETTE)
            .iter()
            .map(|row| plain(row))
            .collect();
        assert_eq!(whole.len(), 2 + 7 + wrapped, "{whole:#?}");
        // Four rows short of that: as much as fits, and the rest named.
        let short: Vec<String> = menu
            .rows(
                &servers,
                &[],
                40,
                u16::try_from(2 + 7 + wrapped - 4).unwrap(),
                PALETTE,
            )
            .iter()
            .map(|row| plain(row))
            .collect();
        assert_eq!(
            short.last().map(String::as_str),
            Some("  \u{2026} 5 more rows: press v to see all"),
            "{short:#?}"
        );
    }

    #[test]
    fn details_with_no_room_for_the_entry_say_press_v_and_a_approves_only_what_was_whole() {
        // Seven facts fill a nine-row panel's body (two header rows): with
        // no room left the entry is not drawn, so the last fact gives way to
        // the row that says how to see it, and `a` approves only an entry
        // shown whole -- on the screen, or written into the transcript by `v`.
        let mut server = view(
            "p",
            Source::Workspace,
            Connection::Disabled,
            Some(Admission::Pending),
        );
        server.entry = Some(std::sync::Arc::new(crate::mcp::runtime::ShownEntry {
            rows: vec![format!("command: /bin/sh -c '{}'", "w ".repeat(60))],
            digest: "sha256:p".into(),
        }));
        let servers = [server];
        let wrapped = entry_rows(&servers[0], 80).len();
        assert!(wrapped >= 2, "{wrapped}");
        let approves = |menu: &mut McpMenu| {
            matches!(
                menu.key(KeyAction::Char('a'), &servers, 5),
                Effect::Trust { .. }
            )
        };
        // Every height from one row up: below nine the facts themselves are
        // cut to the panel, and the row saying how to see the entry is still
        // the last one drawn.
        for (row_count, room) in (1u16..=9).map(|rows| (rows, 0usize)).chain([(10, 1)]) {
            let mut menu = McpMenu::open();
            menu.key(KeyAction::Enter, &servers, 5);
            let rows: Vec<String> = menu
                .rows(&servers, &[], 80, row_count, PALETTE)
                .iter()
                .map(|row| plain(row))
                .collect();
            assert_eq!(rows.len(), usize::from(row_count), "room {room}: {rows:#?}");
            assert_eq!(
                rows.last().map(String::as_str),
                Some(format!("  \u{2026} {wrapped} more rows: press v to see all").as_str()),
                "room {room}: {rows:#?}"
            );
            assert!(
                !approves(&mut menu),
                "room {room}: approved an entry not shown"
            );
            let said: Vec<String> = menu
                .rows(&servers, &[], 80, row_count, PALETTE)
                .iter()
                .map(|row| plain(row))
                .collect();
            if row_count > 2 {
                assert_eq!(
                    said[1],
                    format!("  {ENTRY_NOT_SHOWN}"),
                    "room {room}: {said:#?}"
                );
            }
            // `v` shows it whole, a frame is drawn, and `a` approves it: what
            // `v` showed outlasts the frames after it.
            assert!(matches!(
                menu.key(KeyAction::Char('v'), &servers, 5),
                Effect::Show(_)
            ));
            menu.rows(&servers, &[], 80, row_count, PALETTE);
            assert!(approves(&mut menu), "rows {row_count}: `a` after `v`");
        }
        // Drawn whole on a screen with the rows: `a` approves it at once.
        let mut menu = McpMenu::open();
        menu.key(KeyAction::Enter, &servers, 5);
        menu.rows(&servers, &[], 80, 30, PALETTE);
        // The hint names `v` however narrow it gets.
        for cols in [20, 40, 120] {
            assert!(menu.key_help(&servers, cols).contains('v'), "{cols}");
        }
        assert!(approves(&mut menu));
    }

    /// `runtime.run(job)`, failing the case loudly if it has not returned
    /// within a minute rather than hanging the whole run.
    fn bounded_run(
        runtime: &std::sync::Arc<crate::mcp::McpRuntime>,
        job: crate::mcp::runtime::Job,
    ) {
        let (done, finished) = std::sync::mpsc::channel();
        let runtime = std::sync::Arc::clone(runtime);
        std::thread::spawn(move || {
            runtime.run(job);
            let _ = done.send(());
        });
        finished
            .recv_timeout(std::time::Duration::from_secs(60))
            .expect("an MCP job did not finish within a minute");
    }

    /// A runtime over a temporary workspace whose `.mcp.json` the case
    /// rewrites, and the settings file the choices land in.
    struct Workspace {
        _home: tempfile::TempDir,
        root: std::path::PathBuf,
        settings: std::path::PathBuf,
        runtime: std::sync::Arc<crate::mcp::McpRuntime>,
    }

    impl Workspace {
        fn new() -> Self {
            let home = tempfile::tempdir().expect("home");
            let root = home.path().join("work");
            std::fs::create_dir_all(&root).expect("workspace");
            let settings = home.path().join(".xfx").join("settings.json");
            let runtime = crate::mcp::McpRuntime::new(crate::mcp::Paths {
                profile: Some(home.path().join(".xfx").join("mcp.json")),
                settings: Some(settings.clone()),
                workspace: root.clone(),
            });
            Self {
                _home: home,
                root,
                settings,
                runtime,
            }
        }

        /// Writes `servers` as the workspace's `.mcp.json` and reads it.
        fn write(&self, servers: serde_json::Value) {
            std::fs::write(
                self.root.join(".mcp.json"),
                serde_json::json!({"mcpServers": servers}).to_string(),
            )
            .expect("write .mcp.json");
            bounded_run(
                &self.runtime,
                crate::mcp::runtime::Job::Command("reload".into()),
            );
        }

        /// The approvals saved, by name.
        fn approved(&self) -> serde_json::Value {
            let saved: serde_json::Value = std::fs::read_to_string(&self.settings)
                .ok()
                .and_then(|text| serde_json::from_str(&text).ok())
                .unwrap_or_default();
            saved["workspaces"][self.root.to_str().unwrap()]["enabledMcpjsonServerDigests"].clone()
        }

        /// Runs what the menu asked for, as the shell would.
        fn apply(&self, effect: Effect) {
            use crate::mcp::runtime::Job;
            match effect {
                Effect::Command(line) => bounded_run(&self.runtime, Job::Command(line)),
                Effect::Trust { action, body } => {
                    bounded_run(&self.runtime, Job::Trust { action, body })
                }
                other => panic!("not a trust choice: {other:?}"),
            }
        }

        fn notices(&self) -> Vec<String> {
            self.runtime
                .take_events()
                .into_iter()
                .filter_map(|event| match event {
                    crate::mcp::runtime::Event::Notice { body, .. } => Some(body),
                    _ => None,
                })
                .collect()
        }
    }

    /// An entry that never starts: `exit 0` with a mark in its arguments.
    fn quiet(mark: &str) -> serde_json::Value {
        serde_json::json!({"command": "/bin/sh", "args": ["-c", "exit 0", mark]})
    }

    #[test]
    fn the_details_show_a_literal_the_child_is_given_as_the_prompt_does() {
        // `"PATH": "./x:${PATH}"` makes `command: node` run `./x/node`. The
        // details and `v` are where a person checks an entry before `a`:
        // they show the prefix as the prompt does, from the same rows.
        let workspace = Workspace::new();
        workspace.write(serde_json::json!({"proj": {
            "command": "node",
            "env": {"PATH": "./x:${PATH}"},
        }}));
        let views = workspace.runtime.views();
        let mut menu = McpMenu::open();
        assert_eq!(menu.key(KeyAction::Enter, &views, 5), Effect::Repaint);
        let rows: Vec<String> = menu
            .rows(&views, &[], 120, 30, PALETTE)
            .iter()
            .map(|row| plain(row))
            .collect();
        assert!(
            rows.contains(&"  env: PATH=./x:${PATH}".to_string()),
            "{rows:#?}"
        );
        assert_eq!(
            menu.key(KeyAction::Char('v'), &views, 5),
            Effect::Show(
                "Project MCP server 'proj' in .mcp.json:\n  command: node\n  env: PATH=./x:${PATH}\n"
                    .to_string()
            )
        );
        workspace.runtime.shutdown();
    }

    #[test]
    fn approve_all_approves_the_servers_its_confirmation_drew_not_ones_read_after() {
        use crate::mcp::config::entry_digest;
        for between in ["draw and enter", "enter and execution"] {
            let workspace = Workspace::new();
            workspace.write(serde_json::json!({"a": quiet("a1")}));
            let mut menu = McpMenu::open();
            let drawn = workspace.runtime.views();
            menu.rows(&drawn, &[], 120, 12, PALETTE);
            assert_eq!(menu.key(KeyAction::Char('p'), &drawn, 5), Effect::Repaint);
            let rows: Vec<String> = menu
                .rows(&drawn, &[], 120, 12, PALETTE)
                .iter()
                .map(|row| plain(row))
                .collect();
            assert!(
                rows.iter().any(|row| row.contains("Approve all")),
                "{rows:#?}"
            );
            // `a` is rewritten and `b` added: neither was in what was drawn.
            let rewrite =
                || workspace.write(serde_json::json!({"a": quiet("a2"), "b": quiet("b1")}));
            if between == "draw and enter" {
                rewrite();
                // A frame after the reload: it must not make the new set
                // the one Enter approves.
                menu.rows(&workspace.runtime.views(), &[], 120, 12, PALETTE);
            }
            let effect = menu.key(KeyAction::Enter, &workspace.runtime.views(), 5);
            if between == "draw and enter" {
                // The pending servers are not the ones it was opened over:
                // Enter approves nothing and goes back to the list.
                assert_eq!(effect, Effect::Repaint, "{between}");
                assert_eq!(menu.screen(), Screen::Browse, "{between}");
            } else {
                rewrite();
                let _ = workspace.notices();
                workspace.apply(effect);
                // The job runs on what the runtime reads then: neither the
                // rewritten nor the added server was listed, and both are
                // named as still waiting.
                let said = workspace.notices();
                for expected in [
                    "'a' changed since it was listed",
                    "'b' was added since the list was drawn",
                ] {
                    assert!(
                        said.iter().any(|body| body.contains(expected)),
                        "{between}: {expected}: {said:?}"
                    );
                }
            }
            assert_eq!(
                workspace.approved(),
                serde_json::Value::Null,
                "{between}: approved what the confirmation never drew"
            );
            // Drawn as it is now, it approves what it drew.
            let drawn = workspace.runtime.views();
            menu.rows(&drawn, &[], 120, 12, PALETTE);
            menu.key(KeyAction::Char('p'), &drawn, 5);
            menu.rows(&drawn, &[], 120, 12, PALETTE);
            let effect = menu.key(KeyAction::Enter, &drawn, 5);
            workspace.apply(effect);
            assert_eq!(
                workspace.approved(),
                serde_json::json!({
                    "a": entry_digest(&quiet("a2")),
                    "b": entry_digest(&quiet("b1")),
                }),
                "{between}"
            );
            workspace.runtime.shutdown();
        }
    }

    #[test]
    fn a_resize_keeps_an_entry_still_drawn_whole_and_a_reload_after_v_does_not() {
        let entry = |digest: &str| {
            let mut server = view(
                "p",
                Source::Workspace,
                Connection::Disabled,
                Some(Admission::Pending),
            );
            server.entry = Some(std::sync::Arc::new(crate::mcp::runtime::ShownEntry {
                rows: vec![format!("command: /bin/sh -c '{}'", "w ".repeat(30))],
                digest: digest.into(),
            }));
            [server]
        };
        let approves = |menu: &mut McpMenu, servers: &[ServerView]| {
            matches!(
                menu.key(KeyAction::Char('a'), servers, 5),
                Effect::Trust { .. }
            )
        };
        let servers = entry("sha256:a");
        let open = || {
            let mut menu = McpMenu::open();
            menu.key(KeyAction::Enter, &servers, 5);
            menu
        };
        // Drawn whole, then redrawn at another size that still fits it: the
        // digest has not changed, so `a` approves.
        let mut menu = open();
        menu.rows(&servers, &[], 120, 30, PALETTE);
        menu.rows(&servers, &[], 60, 30, PALETTE);
        assert!(approves(&mut menu, &servers));
        // Redrawn at a size that cuts it: no longer on the screen whole.
        let mut menu = open();
        menu.rows(&servers, &[], 120, 30, PALETTE);
        menu.rows(&servers, &[], 120, 9, PALETTE);
        assert!(!approves(&mut menu, &servers));
        // Something wrote over the screen: what was drawn is not known to be
        // there any more.
        let mut menu = open();
        menu.rows(&servers, &[], 120, 30, PALETTE);
        menu.forget_drawn();
        assert!(!approves(&mut menu, &servers));
        // `v`, then a reload reads a different entry: `a` refuses it.
        let mut menu = open();
        menu.rows(&servers, &[], 120, 9, PALETTE);
        assert!(matches!(
            menu.key(KeyAction::Char('v'), &servers, 5),
            Effect::Show(_)
        ));
        let reread = entry("sha256:b");
        assert!(!approves(&mut menu, &reread));
        let rows: Vec<String> = menu
            .rows(&reread, &[], 120, 9, PALETTE)
            .iter()
            .map(|row| plain(row))
            .collect();
        assert_eq!(rows[1], format!("  {ENTRY_CHANGED}"), "{rows:#?}");
    }

    #[test]
    fn approve_all_is_frozen_when_its_confirmation_opens_and_a_change_refuses_it() {
        // The confirmation is one sentence; a redraw after a reload looks the
        // same. So what Enter approves is fixed when it opens, and a frame
        // that finds the pending servers changed says so and approves nothing.
        let workspace = Workspace::new();
        workspace.write(serde_json::json!({"a": quiet("a1")}));
        let mut menu = McpMenu::open();
        let opened = workspace.runtime.views();
        menu.rows(&opened, &[], 120, 12, PALETTE);
        assert_eq!(menu.key(KeyAction::Char('p'), &opened, 5), Effect::Repaint);
        menu.rows(&opened, &[], 120, 12, PALETTE);
        workspace.write(serde_json::json!({"a": quiet("a2")}));
        let now = workspace.runtime.views();
        let rows: Vec<String> = menu
            .rows(&now, &[], 120, 12, PALETTE)
            .iter()
            .map(|row| plain(row))
            .collect();
        let effect = menu.key(KeyAction::Enter, &now, 5);
        if matches!(effect, Effect::Trust { .. } | Effect::Command(_)) {
            workspace.apply(effect);
        }
        assert_eq!(
            workspace.approved(),
            serde_json::Value::Null,
            "approved a digest the confirmation was not opened over"
        );
        assert!(
            rows.iter().any(|row| row.contains(PENDING_CHANGED)),
            "{rows:#?}"
        );
        assert_eq!(menu.screen(), Screen::Browse, "Enter goes back to the list");
        workspace.runtime.shutdown();
    }

    #[test]
    fn p_binds_the_list_the_servers_tab_drew_and_says_when_it_no_longer_stands() {
        // The person pressing `p` saw the servers tab's last frame. A reload
        // after it, with no frame between, changes what is pending: the
        // confirmation is over the list that was drawn, so it says the
        // pending servers changed and approves nothing.
        let workspace = Workspace::new();
        workspace.write(serde_json::json!({"a": quiet("a1")}));
        let mut menu = McpMenu::open();
        menu.rows(&workspace.runtime.views(), &[], 120, 12, PALETTE);
        workspace.write(serde_json::json!({"a": quiet("a2")}));
        let now = workspace.runtime.views();
        assert_eq!(menu.key(KeyAction::Char('p'), &now, 5), Effect::Repaint);
        let rows: Vec<String> = menu
            .rows(&now, &[], 120, 12, PALETTE)
            .iter()
            .map(|row| plain(row))
            .collect();
        assert!(
            rows.iter().any(|row| row.contains(PENDING_CHANGED)),
            "{rows:#?}"
        );
        assert_eq!(menu.key(KeyAction::Enter, &now, 5), Effect::Repaint);
        assert_eq!(workspace.approved(), serde_json::Value::Null);
        workspace.runtime.shutdown();
    }

    #[test]
    fn p_before_the_servers_tab_has_been_drawn_approves_nothing() {
        // `p` binds the list the servers tab drew; before it has drawn one
        // there is no list a person saw, so `p` only asks for a frame and
        // Enter after it approves nothing.
        let workspace = Workspace::new();
        workspace.write(serde_json::json!({"a": quiet("a1")}));
        let views = workspace.runtime.views();
        let mut menu = McpMenu::open();
        assert_eq!(menu.key(KeyAction::Char('p'), &views, 5), Effect::Repaint);
        let effect = menu.key(KeyAction::Enter, &views, 5);
        assert!(
            !matches!(effect, Effect::Trust { .. } | Effect::Command(_)),
            "{effect:?}"
        );
        if matches!(effect, Effect::Trust { .. } | Effect::Command(_)) {
            workspace.apply(effect);
        }
        assert_eq!(workspace.approved(), serde_json::Value::Null);
        workspace.runtime.shutdown();
    }

    #[test]
    fn enter_compares_the_pending_servers_itself_when_no_frame_came_after_the_reload() {
        // A reload between the confirmation's frame and Enter, with no frame
        // after it: no frame said the set changed, so Enter's own comparison
        // is what refuses.
        let workspace = Workspace::new();
        workspace.write(serde_json::json!({"a": quiet("a1")}));
        let mut menu = McpMenu::open();
        let drawn = workspace.runtime.views();
        menu.rows(&drawn, &[], 120, 12, PALETTE);
        assert_eq!(menu.key(KeyAction::Char('p'), &drawn, 5), Effect::Repaint);
        let rows: Vec<String> = menu
            .rows(&drawn, &[], 120, 12, PALETTE)
            .iter()
            .map(|row| plain(row))
            .collect();
        assert!(
            !rows.iter().any(|row| row.contains(PENDING_CHANGED)),
            "{rows:#?}"
        );
        workspace.write(serde_json::json!({"a": quiet("a2")}));
        let effect = menu.key(KeyAction::Enter, &workspace.runtime.views(), 5);
        assert_eq!(effect, Effect::Repaint, "{effect:?}");
        assert_eq!(menu.screen(), Screen::Browse);
        assert_eq!(workspace.approved(), serde_json::Value::Null);
        workspace.runtime.shutdown();
    }
}
