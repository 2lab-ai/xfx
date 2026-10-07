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
}

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

    /// How many rows the menu takes in `max_rows` (`menuRowCount`, `:18-44`).
    pub(crate) fn height(&self, servers: &[ServerView], max_rows: u16) -> u16 {
        let max_rows = max_rows.min(MAX_ROWS);
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
                Some(server) if server.failure.is_some() => 8,
                Some(_) => 7,
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
                rows.extend(self.browse_rows(servers, body, width, show_header, palette))
            }
            Screen::Details => rows.extend(self.details_rows(servers, width, palette)),
            Screen::Info => rows.extend(info_rows(width, palette)),
            Screen::Confirm(confirm) => rows.push(text_row(
                confirm.text(),
                width,
                palette.notice_label(),
                0,
                palette,
            )),
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
    fn details_rows(&self, servers: &[ServerView], width: usize, palette: Palette) -> Vec<String> {
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
            (Screen::Confirm(confirm), KeyAction::Enter) => {
                let name = self.server(servers).map(|server| server.name.clone());
                self.screen = Screen::Browse;
                let command = match (confirm, name) {
                    (Confirm::Remove, Some(name)) => format!("remove {name}"),
                    (Confirm::Reject, Some(name)) => format!("trust reject {name}"),
                    (Confirm::ApproveAll, _) => "trust approve-all".to_string(),
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
            (Section::Servers, 'p') => {
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
            'a' if server.admission == Some(Admission::Pending) => {
                let name = server.name.clone();
                self.screen = Screen::Browse;
                Effect::Command(format!("trust approve {name}"))
            }
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
        let details_variants = [details.as_str(), "c help  esc back", "c esc"];
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
            "details: enter sign in \u{b7} l logout \u{b7} d remove",
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
        }
    }

    #[test]
    fn the_empty_menu_is_upstreams_three_rows_and_root_hint() {
        // `cap/fx-mcp-empty-40x120-menu.txt`, rows 5-7 and 9.
        let menu = McpMenu::open();
        assert_eq!(menu.height(&[], 20), 3);
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
        assert_eq!(menu.height(&servers, 20), 4);
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
        assert_eq!(menu.height(&servers, 20), 3);
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
}
