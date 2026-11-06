//! `/usage` on this surface: fx v0.0.16's df-style dashboard, AI Gateway half.
//!
//! What the numbers are, and when they are fetched, is
//! [`crate::usage::dashboard`]'s. This module is the screen over one of its
//! [`Snapshot`]s: upstream's navigation state (`State`,
//! `vercel-labs/fx@481104fd src/core/session/usage_menu.zig:8-131`) and every
//! row it draws (`src/ui/footer/usage_presentation.zig:19-878,1558-1625`), with
//! the hint row's three Gateway lists (`input_presentation.zig:794-813`).
//!
//! # What it looks like
//!
//! A heading, `usage: <who>` with the status in the right corner (`updated 2m
//! ago`, `updating…`, a failure in normal weight); the period table (`today`,
//! `7d`, `month`); the budget table when the cache holds one; the model table
//! for the period Tab picks, at most [`MAX_MODEL_ROWS`] rows and a dim `N more`
//! row summing everything below the window. Enter opens the selected model's
//! detail screen: its period, tokens and perf tables. Lowercase headers, the
//! table name bold and the column labels dim, numbers right-aligned under
//! them, the selected model bold behind a `❯`, no colour. Tables are as wide
//! as their content; a narrow screen drops columns before it cuts a name.
//!
//! # Not drawn
//!
//! The Codex and Grok views (`usage_presentation.zig:879-1556`) and their two
//! hint lists (`input_presentation.zig:798-805`): xfx has neither sign-in
//! (U9).
//!
//! # Where it is not fx
//!
//! - **A model name is spelled, not replaced.** fx shows `?` for a control,
//!   C1, bidi or zero-width character in a name (`safeText`, `:367-385`); here
//!   every name goes through [`crate::mcp::terminal_safe`], which spells it
//!   (`\x1b`, `\u{202e}`), so a name with a hidden character is not shown as
//!   another name. Widths are measured on what is drawn.
//! - **The no-sign-in text names no `/login`**, which xfx has not (the sign-in
//!   waits on the user): `(set AI_GATEWAY_API_KEY)`.
//! - **A provider setting no turn can be sent with** -- an unreadable
//!   `provider`, a refused `XFX_GATEWAY_URL` -- is xfx's own state, with no fx
//!   twin ([`Unavailable::ProviderRejected`]): its line sends the user to the
//!   turn's refusal, which names it, in fx's shape for a sign-in that needs a
//!   refresh (`(send a message to renew it, then reopen /usage)`,
//!   `usage_presentation.zig:496`).

use crate::usage::dashboard::{Snapshot, Status};
use crate::usage::fixed;
use crate::usage::report::{month_day, Failure, ModelRow, Totals, Unavailable};
use crate::usage::view::Period;

use super::input::Action;
use super::picker::cells;
use super::theme::Palette;

/// The most model rows on screen at once; the window scrolls over the rest
/// (`max_model_rows`, `usage_presentation.zig:19`).
pub(crate) const MAX_MODEL_ROWS: usize = 10;

/// What stands before an unselected model's name (`gutter`, `:613`).
const GUTTER: usize = 2;

/// The detail screen's rows (`detail_rows`, `:139`): heading and blank, then
/// three tables of a header and three periods, a blank between them.
const DETAIL_ROWS: usize = 2 + 4 + 1 + 4 + 1 + 4;

/// The rows a message-only screen takes: heading, blank, message.
const MESSAGE_ROWS: usize = 3;

/// The selected model's prefix.
const MARK: &str = "\u{276f} ";

/// `usage: ` before the heading's left part.
const PREFIX_WIDTH: usize = "usage: ".len();

/// The least of the left part the status is drawn beside (`min_left`).
const MIN_LEFT: usize = 12;

/// What the hint row says, widest first, for each kind of screen
/// (`composeCompactCommandMenuHintRow`'s `.usage` lists,
/// `input_presentation.zig:794-813`).
const MESSAGE_HELP: [&str; 3] = ["esc close", "esc close", "esc"];
const DETAIL_HELP: [&str; 3] = [
    "\u{2191}\u{2193} next model     esc back     r refresh",
    "\u{2191}\u{2193} model  esc back  r refresh",
    "\u{2191}\u{2193}  esc  r",
];
const TABLE_HELP: [&str; 3] = [
    "\u{2191}\u{2193} model     enter detail     tab period     r refresh     esc close",
    "\u{2191}\u{2193} model  enter detail  tab period  r refresh  esc",
    "\u{2191}\u{2193}  enter  tab  r  esc",
];

/// What one keystroke means to the dashboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MenuAction {
    /// Tab and Right: the next period, wrapping (`Period.next`).
    Next,
    /// Shift-Tab and Left: the previous period, wrapping.
    Previous,
    /// Up and Ctrl-K, and Down and Ctrl-J with `true`: the model above or
    /// below, stopping at the ends (`moveModel`, `usage_menu.zig:63-77`).
    Move(bool),
    /// Enter: the selected model's detail screen.
    Detail,
    /// `r`: fetch now, unless a fetch is in flight.
    Reload,
    /// Escape: back to the tables, or close.
    Close,
}

impl MenuAction {
    /// What a decoded keystroke means here, and `None` for every one the
    /// dashboard swallows without meaning anything by it.
    ///
    /// Shift-Tab reaches here as the permission-mode cycle's key and is the
    /// dashboard's while it is up (`toggle_permission_mode`,
    /// `app_input_runtime.zig:2241-2244`); Ctrl-J and Ctrl-K are bytes 10 and
    /// 11 (`:1204-1211`); Left and Right step the period both ways, wrapping
    /// (`:2228-2239`).
    pub(crate) fn of_action(action: Action) -> Option<Self> {
        match action {
            Action::Tab | Action::Right => Some(Self::Next),
            Action::BackTab | Action::Left => Some(Self::Previous),
            Action::Up | Action::ControlK => Some(Self::Move(false)),
            Action::Down | Action::ControlJ => Some(Self::Move(true)),
            Action::Submit => Some(Self::Detail),
            Action::Escape => Some(Self::Close),
            _ => None,
        }
    }

    /// What a typed character means here: `r` or `R`, and nothing else
    /// (`app_input_runtime.zig:1200-1203`).
    pub(crate) fn of_text(character: char) -> Option<Self> {
        matches!(character, 'r' | 'R').then_some(Self::Reload)
    }
}

/// The dashboard on screen: which period the model table lists, which model is
/// selected, whether its detail screen is open, and the snapshot it draws
/// (`State`, `usage_menu.zig:8-18`).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Screen {
    period: Period,
    selected: usize,
    window_start: usize,
    detail: bool,
    snapshot: Snapshot,
}

impl Screen {
    /// The dashboard opened on `snapshot`, its model table on `period` -- the
    /// one it listed when last closed -- and nothing else kept (`open`,
    /// `usage_menu.zig:21-29`).
    pub(crate) fn open(period: Period, snapshot: Snapshot) -> Self {
        Self {
            period,
            selected: 0,
            window_start: 0,
            detail: false,
            snapshot,
        }
    }

    /// The period the model table lists, kept across opens.
    pub(crate) fn period(&self) -> Period {
        self.period
    }

    /// The snapshot on screen.
    pub(crate) fn shown(&self) -> &Snapshot {
        &self.snapshot
    }

    /// Draws `snapshot` instead, keeping the selected model by name when it is
    /// still listed; a detail screen whose model left the list goes back to
    /// the tables (`replace`, `usage_menu.zig:33-49`).
    pub(crate) fn replace(&mut self, snapshot: Snapshot) {
        let held = self.selected_name().map(str::to_owned);
        let models = &snapshot.of(self.period).models;
        let found = held
            .as_deref()
            .and_then(|name| models.iter().position(|model| model.model == name));
        self.selected = found.unwrap_or_else(|| self.selected.min(models.len().saturating_sub(1)));
        self.window_start = self.window_start.min(self.selected);
        if found.is_none() {
            self.detail = false;
        }
        self.snapshot = snapshot;
    }

    /// Lists `period` in the model table; the selection starts over. Refused
    /// on the detail screen. Answers whether anything changed (`setPeriod`,
    /// `usage_menu.zig:54-60`).
    pub(crate) fn set_period(&mut self, period: Period) -> bool {
        if self.detail || period == self.period {
            return false;
        }
        self.period = period;
        self.selected = 0;
        self.window_start = 0;
        true
    }

    /// Moves the selection one model, stopping at the ends, and keeps it among
    /// the `visible` model rows -- on the tables, or on the detail screen,
    /// where it is the next model's details (`moveModel`,
    /// `usage_menu.zig:63-77`). Answers whether it moved.
    pub(crate) fn move_model(&mut self, down: bool, visible: usize) -> bool {
        let count = self.listed().len();
        if count == 0 {
            return false;
        }
        let next = if down {
            self.selected.saturating_add(1).min(count - 1)
        } else {
            self.selected.saturating_sub(1)
        };
        if next == self.selected {
            return false;
        }
        self.selected = next;
        let visible = visible.max(1);
        if self.selected < self.window_start {
            self.window_start = self.selected;
        } else if self.selected >= self.window_start.saturating_add(visible) {
            self.window_start = self.selected - (visible - 1);
        }
        true
    }

    /// Enter: the selected model's detail screen, when a model is listed
    /// (`openDetail`, `usage_menu.zig:80-84`).
    pub(crate) fn open_detail(&mut self) -> bool {
        if self.detail || self.listed().is_empty() {
            return false;
        }
        self.detail = true;
        true
    }

    /// Escape on the detail screen: back to the tables (`closeDetail`,
    /// `usage_menu.zig:87-91`). `false` on the tables, where Escape closes.
    pub(crate) fn close_detail(&mut self) -> bool {
        std::mem::replace(&mut self.detail, false)
    }

    fn listed(&self) -> &[ModelRow] {
        &self.snapshot.of(self.period).models
    }

    fn selected_name(&self) -> Option<&str> {
        self.listed()
            .get(self.selected)
            .map(|model| model.model.as_str())
    }

    /// Nothing to tabulate: not signed in, or nothing cached yet
    /// (`messageOnly`, `usage_presentation.zig:93-101`).
    fn message_only(&self) -> bool {
        if matches!(self.snapshot.status, Status::Unavailable(_)) {
            return true;
        }
        if Period::ALL
            .iter()
            .any(|period| !self.snapshot.of(*period).empty())
        {
            return false;
        }
        self.snapshot.status != Status::Ready
    }

    fn has_budget(&self) -> bool {
        self.snapshot.budget().is_some()
    }

    /// The tables' rows above the first model row (`tableRows`, `:133-137`):
    /// heading, blank, period header and three rows, blank, the budget's
    /// header, row and blank when there is one, the model header.
    fn table_rows(&self) -> usize {
        2 + 4 + 1 + if self.has_budget() { 3 } else { 0 } + 1
    }

    /// The rows the dashboard wants (`desiredRowCount`, `:141-150`).
    pub(crate) fn desired_rows(&self) -> usize {
        if self.message_only() {
            return MESSAGE_ROWS;
        }
        if self.detail {
            return DETAIL_ROWS;
        }
        let models = self.listed().len();
        if models == 0 {
            return self.table_rows() + 1;
        }
        self.table_rows() + models.min(MAX_MODEL_ROWS) + usize::from(models > MAX_MODEL_ROWS)
    }

    /// The rows the band gives it on a screen `rows` tall: what it wants, and
    /// never more than the screen less three
    /// (`compact_command_menu_presentation.zig:41-46`). At least one.
    pub(crate) fn height(&self, rows: u16) -> u16 {
        u16::try_from(self.desired_rows())
            .unwrap_or(u16::MAX)
            .min(rows.saturating_sub(3))
            .max(1)
    }

    /// How many model rows fit in `visible` rows, keeping one for the `N more`
    /// line when not every model fits (`visibleModelItems`, `:153-159`): what
    /// the arrows keep the selection among.
    pub(crate) fn visible_models(&self, visible: u16) -> usize {
        let models = self.listed().len();
        let area = usize::from(visible).saturating_sub(self.table_rows());
        if models <= area.min(MAX_MODEL_ROWS) {
            return models.max(1);
        }
        area.saturating_sub(1).min(MAX_MODEL_ROWS).max(1)
    }

    /// Which rows the dashboard draws in `visible` rows (`layout`,
    /// `:161-265`, the Gateway half).
    fn layout(&self, visible: u16) -> Vec<Line> {
        let mut lines = vec![Line::Heading, Line::Blank];
        if self.message_only() {
            lines.push(Line::Message);
            return lines;
        }
        if self.detail {
            lines.push(Line::DetailPeriodHeader);
            lines.extend(Period::ALL.map(Line::DetailPeriod));
            lines.push(Line::Blank);
            lines.push(Line::TokensHeader);
            lines.extend(Period::ALL.map(Line::Tokens));
            lines.push(Line::Blank);
            lines.push(Line::PerfHeader);
            lines.extend(Period::ALL.map(Line::Perf));
            return lines;
        }
        lines.push(Line::PeriodHeader);
        lines.extend(Period::ALL.map(Line::Period));
        lines.push(Line::Blank);
        if self.has_budget() {
            lines.push(Line::BudgetHeader);
            lines.push(Line::Budget);
            lines.push(Line::Blank);
        }
        lines.push(Line::ModelHeader);
        let models = self.listed().len();
        if models == 0 {
            lines.push(Line::NoModels);
            return lines;
        }
        let shown = self.visible_models(visible);
        let start = self.window_start.min(models.saturating_sub(shown));
        let end = start.saturating_add(shown).min(models);
        lines.extend((start..end).map(Line::Model));
        if end < models {
            lines.push(Line::More(end));
        }
        lines
    }

    /// The dashboard's rows, top first, for a panel `visible` rows tall on a
    /// screen `cols` wide (`composeRow`, `:267-314`). No row passes `cols`.
    pub(crate) fn rows(&self, cols: u16, visible: u16, palette: Palette) -> Vec<String> {
        let paint = Paint {
            limit: usize::from(cols),
            palette,
        };
        let mut lines = self.layout(visible);
        lines.truncate(usize::from(visible));
        lines
            .into_iter()
            .map(|line| {
                let row = self.compose(line, &paint);
                if row.is_empty() {
                    row
                } else {
                    format!("{}{}", super::frame::clip(&row, cols), palette.reset())
                }
            })
            .collect()
    }

    /// The panel row the selection is on, for the caret: the selected model's
    /// when it is in view, else the heading.
    pub(crate) fn marked_row(&self, visible: u16) -> u16 {
        if self.detail || self.message_only() {
            return 0;
        }
        self.layout(visible)
            .iter()
            .take(usize::from(visible))
            .position(|line| *line == Line::Model(self.selected))
            .and_then(|at| u16::try_from(at).ok())
            .unwrap_or(0)
    }

    /// What the hint row says on a screen `cols` wide: the widest of this
    /// screen's list that fits, else its narrowest.
    pub(crate) fn key_help(&self, cols: u16) -> &'static str {
        let list = if self.message_only() {
            &MESSAGE_HELP
        } else if self.detail {
            &DETAIL_HELP
        } else {
            &TABLE_HELP
        };
        list.iter()
            .copied()
            .find(|candidate| cells(candidate) <= usize::from(cols))
            .unwrap_or(list[list.len() - 1])
    }

    fn compose(&self, line: Line, paint: &Paint) -> String {
        match line {
            Line::Heading => self.heading(paint),
            Line::Blank => String::new(),
            Line::Message => paint.text_row(self.message()),
            Line::PeriodHeader => paint.cells(&PERIOD_HEADER, Kind::Header),
            Line::Period(period) => self.period_row(period, paint),
            Line::BudgetHeader => {
                paint.cells(&budget_columns(&BUDGET_HEADER, paint.limit), Kind::Header)
            }
            Line::Budget => self.budget_row(paint),
            Line::ModelHeader => self.model_header(paint),
            Line::Model(index) => self.model_row(index, paint),
            Line::More(first) => self.more_row(first, paint),
            Line::NoModels => paint.text_row(no_models_text(self.period)),
            Line::DetailPeriodHeader => paint.cells(&DETAIL_PERIOD_HEADER, Kind::Header),
            Line::DetailPeriod(period) => self.detail_period_row(period, paint),
            Line::TokensHeader => paint.cells(tokens_header(paint.limit), Kind::Header),
            Line::Tokens(period) => self.tokens_row(period, paint),
            Line::PerfHeader => paint.cells(perf_header(paint.limit), Kind::Header),
            Line::Perf(period) => self.perf_row(period, paint),
        }
    }

    // ---------------------------------------------------------------- heading

    /// `usage: <who>` on the left, the status in the right corner; a narrow
    /// screen shortens the left part first and drops the status only when even
    /// a short name would not fit (`composeHeading`, `:389-416`).
    fn heading(&self, paint: &Paint) -> String {
        let limit = paint.limit;
        let left = self.heading_left();
        let status = self.heading_status();
        let status_width = cells(&status);
        let show_status = status_width > 0 && PREFIX_WIDTH + MIN_LEFT + 2 + status_width <= limit;
        let left_room = if show_status {
            limit - PREFIX_WIDTH - 2 - status_width
        } else {
            limit.saturating_sub(PREFIX_WIDTH)
        };
        let palette = paint.palette;
        let shown = super::model_menu::middle_ellipsized(&left, left_room);
        let mut row = format!("{}usage:{} {shown}", palette.bold(), palette.reset());
        if show_status {
            let used = PREFIX_WIDTH + cells(&shown);
            row.push_str(&" ".repeat(limit.saturating_sub(used + status_width)));
            // A failure reads in normal weight; routine freshness stays dim.
            if matches!(self.snapshot.status, Status::Failed(_)) {
                row.push_str(&status);
            } else {
                row.push_str(palette.dim());
                row.push_str(&status);
                row.push_str(palette.reset());
            }
        }
        row
    }

    /// Whose numbers these are, or on a detail screen which model
    /// (`headingLeft`, `:420-444`). The label names a variable, never a key.
    fn heading_left(&self) -> String {
        if let Status::Unavailable(reason) = self.snapshot.status {
            return unavailable_heading(reason).to_string();
        }
        if self.detail {
            let models = self.listed();
            if let Some(model) = models.get(self.selected) {
                return format!(
                    "{}, model {} of {}",
                    crate::mcp::terminal_safe(&model.model),
                    self.selected + 1,
                    models.len()
                );
            }
        }
        if self.snapshot.label.is_empty() {
            return "AI Gateway".to_string();
        }
        self.snapshot.label.to_string()
    }

    /// The heading's right corner (`headingStatus`, `:446-463`).
    fn heading_status(&self) -> String {
        let ago = self
            .snapshot
            .fetched_at_ms()
            .map(|at| ago_text(self.snapshot.now_ms.saturating_sub(at)));
        match self.snapshot.status {
            Status::Fetching if self.message_only() => "fetching\u{2026}".to_string(),
            Status::Fetching => "updating\u{2026}".to_string(),
            Status::Ready => match ago {
                Some(ago) => format!("updated {ago}"),
                None => "nothing fetched today".to_string(),
            },
            Status::Failed(failure) => match ago {
                Some(ago) => format!("{}, last updated {ago}", failure_text(failure)),
                None => failure_text(failure).to_string(),
            },
            Status::Unavailable(_) => String::new(),
        }
    }

    /// The one line under the heading when there is nothing to tabulate
    /// (`composeMessage`, `:487-504`).
    fn message(&self) -> &'static str {
        match self.snapshot.status {
            Status::Unavailable(reason) => unavailable_message(reason),
            Status::Fetching => "fetching usage from AI Gateway",
            Status::Failed(_) => "(press r to retry)",
            Status::Ready => "no usage yet",
        }
    }

    // ----------------------------------------------------------------- period

    fn period_row(&self, period: Period, paint: &Paint) -> String {
        let totals = &self.snapshot.of(period).totals;
        let (spent, requests, tokens) = (
            money(totals.total_cost),
            grouped(totals.request_count),
            compact(totals.total_tokens()),
        );
        let cells = [
            Cell::left(period.label(), 8),
            Cell::right(&spent, 11),
            Cell::right(&requests, 10),
            Cell::right(&tokens, 8),
        ];
        paint.cells(&cells, Kind::Data)
    }

    // ----------------------------------------------------------------- budget

    /// `user  $316.00  $65.97  $250.03  21%  nov 1` (`composeBudgetRow`,
    /// `:570-596`).
    fn budget_row(&self, paint: &Paint) -> String {
        let Some(budget) = self.snapshot.budget() else {
            return String::new();
        };
        // A rounded integer, at most 999; `as` takes a negative or NaN ratio
        // to 0 where fx's `@intFromFloat` would trap.
        let use_pct = if budget.limit > 0.0 {
            (budget.spent / budget.limit * 100.0).round().min(999.0) as u64
        } else {
            0
        };
        let resets = self
            .snapshot
            .budget_reset_day()
            .map_or_else(|| "-".to_string(), short_day);
        let (size, used, avail, pct) = (
            money(budget.limit),
            money(budget.spent),
            money((budget.limit - budget.spent).max(0.0)),
            format!("{use_pct}%"),
        );
        let full = [
            Cell::left("user", 6),
            Cell::right(&size, 12),
            Cell::right(&used, 11),
            Cell::right(&avail, 12),
            Cell::right(&pct, 6),
            Cell::right("", 2),
            Cell::left(&resets, 6),
        ];
        paint.cells(&budget_columns(&full, paint.limit), Kind::Data)
    }

    // ----------------------------------------------------------------- models

    /// The name column is as wide as the longest listed name (or the header),
    /// so the table never stretches; share, then tokens, give way first
    /// (`modelLayout`, `:625-636`). Measured on the names as drawn.
    fn model_layout(&self, limit: usize) -> (ModelColumns, usize) {
        let name_width = self
            .listed()
            .iter()
            .map(|model| cells(&crate::mcp::terminal_safe(&model.model)))
            .fold(cells(&model_header_text(self.period)), usize::max);
        let room = limit.saturating_sub(GUTTER);
        for columns in [
            ModelColumns::Full,
            ModelColumns::NoShare,
            ModelColumns::Small,
        ] {
            if name_width + columns.width() <= room {
                return (columns, name_width);
            }
        }
        (
            ModelColumns::Small,
            room.saturating_sub(ModelColumns::Small.width()).max(8),
        )
    }

    fn model_header(&self, paint: &Paint) -> String {
        let (columns, name_width) = self.model_layout(paint.limit);
        let mut row = " ".repeat(GUTTER);
        let header = model_header_text(self.period);
        let used = paint.append(
            &mut row,
            &[Cell::left(&header, name_width)],
            Kind::Header,
            GUTTER,
        );
        let labels = columns.cells("spent", "share", "requests", "tokens");
        paint.append(&mut row, &labels, Kind::Labels, used);
        row
    }

    fn model_row(&self, index: usize, paint: &Paint) -> String {
        let Some(model) = self.listed().get(index) else {
            return String::new();
        };
        let total = self.snapshot.of(self.period).totals.total_cost;
        let (columns, name_width) = self.model_layout(paint.limit);
        let numbers = Numbers::of(&model.totals, total);
        let selected = index == self.selected;
        let palette = paint.palette;
        let mut row = if selected {
            format!("{}{MARK}", palette.bold())
        } else {
            " ".repeat(GUTTER)
        };
        append_name(
            &mut row,
            &crate::mcp::terminal_safe(&model.model),
            name_width,
        );
        paint.append(
            &mut row,
            &numbers.cells(columns),
            Kind::Data,
            GUTTER + name_width,
        );
        if selected {
            row.push_str(palette.reset());
        }
        row
    }

    /// The models below the window, summed, in dim (`composeMoreRow`,
    /// `:718-748`). Rows above the window are not in it.
    fn more_row(&self, first: usize, paint: &Paint) -> String {
        let models = self.listed();
        let mut sum = Totals::default();
        for model in models.iter().skip(first) {
            sum.add(&model.totals);
        }
        let total = self.snapshot.of(self.period).totals.total_cost;
        let (columns, name_width) = self.model_layout(paint.limit);
        let numbers = Numbers::of(&sum, total);
        let palette = paint.palette;
        let mut row = format!("{}{}", palette.dim(), " ".repeat(GUTTER));
        let label = format!("{} more", models.len().saturating_sub(first));
        append_name(&mut row, &label, name_width);
        paint.append(
            &mut row,
            &numbers.cells(columns),
            Kind::Data,
            GUTTER + name_width,
        );
        row.push_str(palette.reset());
        row
    }

    // ----------------------------------------------------------------- detail

    /// The selected model's row for `period`, or `None` when it had no usage
    /// then (`detailRow`, `:756-762`).
    fn detail_row(&self, period: Period) -> Option<&ModelRow> {
        let name = self.selected_name()?;
        self.snapshot
            .of(period)
            .models
            .iter()
            .find(|model| model.model == name)
    }

    fn detail_period_row(&self, period: Period, paint: &Paint) -> String {
        let total = self.snapshot.of(period).totals.total_cost;
        let numbers = self
            .detail_row(period)
            .map_or_else(Numbers::none, |row| Numbers::of(&row.totals, total));
        let cells = [
            Cell::left(period.label(), 8),
            Cell::right(&numbers.spent, 11),
            Cell::right(&numbers.share, 8),
            Cell::right(&numbers.requests, 10),
            Cell::right(&numbers.tokens, 8),
        ];
        paint.cells(&cells, Kind::Data)
    }

    fn tokens_row(&self, period: Period, paint: &Paint) -> String {
        let header = tokens_header(paint.limit);
        let values: [String; 5] = match self.detail_row(period) {
            Some(row) => [
                row.totals.input_tokens,
                row.totals.output_tokens,
                row.totals.reasoning_tokens,
                row.totals.cache_read_tokens,
                row.totals.cache_write_tokens,
            ]
            .map(compact),
            None => std::array::from_fn(|_| "-".to_string()),
        };
        let mut cells = vec![Cell::left(period.label(), header[0].width)];
        cells.extend(
            values
                .iter()
                .zip(&header[1..])
                .map(|(value, column)| Cell::right(value, column.width)),
        );
        paint.cells(&cells, Kind::Data)
    }

    fn perf_row(&self, period: Period, paint: &Paint) -> String {
        let header = perf_header(paint.limit);
        let totals = self.detail_row(period).map(|row| &row.totals);
        let hit = percent(totals.and_then(Totals::cache_hit_pct));
        let ttft = duration(totals.and_then(Totals::p90_ttft_ms));
        let tps = totals
            .and_then(Totals::p90_tps)
            .map_or_else(|| "-".to_string(), |tps| fixed(tps, 0));
        let errors = percent(totals.and_then(Totals::error_pct));
        let cells = [
            Cell::left(period.label(), header[0].width),
            Cell::right(&hit, header[1].width),
            Cell::right(&ttft, header[2].width),
            Cell::right(&tps, header[3].width),
            Cell::right(&errors, header[4].width),
        ];
        paint.cells(&cells, Kind::Data)
    }
}

/// One row of the dashboard (`Line`, `usage_presentation.zig:31-76`, the
/// Gateway half).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Line {
    Heading,
    Blank,
    /// One line of guidance when there is nothing to tabulate.
    Message,
    PeriodHeader,
    Period(Period),
    BudgetHeader,
    Budget,
    ModelHeader,
    Model(usize),
    /// The models below the window, summed; carries the first one's index.
    More(usize),
    NoModels,
    DetailPeriodHeader,
    DetailPeriod(Period),
    TokensHeader,
    Tokens(Period),
    PerfHeader,
    Perf(Period),
}

/// The heading for a sign-in nothing can be fetched with (`headingLeft`,
/// `:422-427`).
fn unavailable_heading(reason: Unavailable) -> &'static str {
    match reason {
        Unavailable::NoGatewaySignIn => "not signed in to AI Gateway",
        Unavailable::UnsupportedProvider => "this provider's usage isn't shown yet",
        Unavailable::ProviderRejected => "the provider settings can't be used",
    }
}

/// What to do about it (`composeMessage`, `:492-497`). fx's no-sign-in text
/// names `/login`, which xfx has not.
fn unavailable_message(reason: Unavailable) -> &'static str {
    match reason {
        Unavailable::NoGatewaySignIn => "(set AI_GATEWAY_API_KEY)",
        Unavailable::UnsupportedProvider => {
            "(use /provider and pick AI Gateway to see AI Gateway usage)"
        }
        Unavailable::ProviderRejected => "(send a message to see why, then reopen /usage)",
    }
}

/// What a failed fetch says (`failureText`, `:474-485`).
fn failure_text(failure: Failure) -> &'static str {
    match failure {
        Failure::Network => "couldn't reach AI Gateway",
        Failure::Refused => "AI Gateway refused this sign-in",
        Failure::NotAvailable => "usage reports aren't available for this team",
        Failure::RateLimited => "rate limited, try again shortly",
        Failure::Server => "AI Gateway error",
        Failure::BadResponse => "unexpected AI Gateway response",
        Failure::Cache => "couldn't save the usage cache",
    }
}

/// An empty model table, per period (`noModelsText`, `:506-512`).
fn no_models_text(period: Period) -> &'static str {
    match period {
        Period::Today => "no usage today (UTC)",
        Period::Days7 => "no usage in the last 7 days",
        Period::Month => "no usage this month",
    }
}

fn model_header_text(period: Period) -> String {
    format!("model ({})", period.label())
}

/// `just now`, `2m ago`, `3h ago`, `2d ago` (`agoText`, `:466-472`); a clock
/// that reads earlier than the fetch is `just now`.
fn ago_text(elapsed_ms: i64) -> String {
    const MINUTE_MS: i64 = 60 * 1000;
    let minutes = elapsed_ms.max(0).div_euclid(MINUTE_MS);
    if minutes < 1 {
        "just now".to_string()
    } else if minutes < 60 {
        format!("{minutes}m ago")
    } else if minutes < 24 * 60 {
        format!("{}h ago", minutes / 60)
    } else {
        format!("{}d ago", minutes / (24 * 60))
    }
}

/// `nov 1` (`shortDay`, `:598-603`).
fn short_day(day: i64) -> String {
    const MONTHS: [&str; 12] = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    let date = month_day(day);
    let month = usize::try_from(date.month - 1)
        .ok()
        .and_then(|at| MONTHS.get(at))
        .copied()
        .unwrap_or("");
    format!("{month} {}", date.day)
}

// ------------------------------------------------------------------- columns

/// One padded cell (`Cell`, `:316-320`).
#[derive(Debug, Clone, Copy)]
struct Cell<'a> {
    text: &'a str,
    width: usize,
    left: bool,
}

impl<'a> Cell<'a> {
    const fn left(text: &'a str, width: usize) -> Self {
        Self {
            text,
            width,
            left: true,
        }
    }

    const fn right(text: &'a str, width: usize) -> Self {
        Self {
            text,
            width,
            left: false,
        }
    }
}

/// How a run of cells is styled (`Kind`, `:322-332`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// A table's header row: the first cell (the table name) bold, the column
    /// labels dim.
    Header,
    /// More column labels on the same header row: all dim.
    Labels,
    Data,
}

const PERIOD_HEADER: [Cell<'static>; 4] = [
    Cell::left("period", 8),
    Cell::right("spent", 11),
    Cell::right("requests", 10),
    Cell::right("tokens", 8),
];

const BUDGET_HEADER: [Cell<'static>; 7] = [
    Cell::left("budget", 6),
    Cell::right("size", 12),
    Cell::right("used", 11),
    Cell::right("avail", 12),
    Cell::right("use%", 6),
    Cell::right("", 2),
    Cell::left("resets", 6),
];

const DETAIL_PERIOD_HEADER: [Cell<'static>; 5] = [
    Cell::left("period", 8),
    Cell::right("spent", 11),
    Cell::right("share", 8),
    Cell::right("requests", 10),
    Cell::right("tokens", 8),
];

const TOKENS_WIDE: [Cell<'static>; 6] = [
    Cell::left("tokens", 8),
    Cell::right("input", 9),
    Cell::right("output", 9),
    Cell::right("reasoning", 11),
    Cell::right("cache read", 12),
    Cell::right("cache write", 13),
];

const TOKENS_NARROW: [Cell<'static>; 6] = [
    Cell::left("tokens", 6),
    Cell::right("in", 7),
    Cell::right("out", 7),
    Cell::right("reason", 8),
    Cell::right("cache rd", 10),
    Cell::right("cache wr", 10),
];

const PERF_WIDE: [Cell<'static>; 5] = [
    Cell::left("perf", 8),
    Cell::right("cache hit", 11),
    Cell::right("p90 ttft", 10),
    Cell::right("p90 tok/s", 11),
    Cell::right("errors", 8),
];

/// 38 columns, so every perf column shows down to 40-column screens.
const PERF_NARROW: [Cell<'static>; 5] = [
    Cell::left("perf", 5),
    Cell::right("cache hit", 10),
    Cell::right("p90 ttft", 9),
    Cell::right("tok/s", 7),
    Cell::right("errors", 7),
];

fn cells_width(cells: &[Cell<'_>]) -> usize {
    cells.iter().map(|cell| cell.width).sum()
}

fn tokens_header(limit: usize) -> &'static [Cell<'static>] {
    if limit >= cells_width(&TOKENS_WIDE) {
        &TOKENS_WIDE
    } else {
        &TOKENS_NARROW
    }
}

fn perf_header(limit: usize) -> &'static [Cell<'static>] {
    if limit >= cells_width(&PERF_WIDE) {
        &PERF_WIDE
    } else {
        &PERF_NARROW
    }
}

/// The budget's columns at `limit` (`budgetColumns`, `:539-545`): all of them
/// from 55 columns, no `resets` from 47, and below that no `size` either.
fn budget_columns<'a>(full: &[Cell<'a>; 7], limit: usize) -> Vec<Cell<'a>> {
    if limit >= 55 {
        full.to_vec()
    } else if limit >= 47 {
        full[..5].to_vec()
    } else {
        vec![full[0], full[2], full[3], full[4]]
    }
}

/// Which number columns the model table keeps (`ModelColumns`, `:606`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModelColumns {
    Full,
    NoShare,
    Small,
}

impl ModelColumns {
    /// Their width (`modelColumnsWidth`, `:615-621`).
    fn width(self) -> usize {
        match self {
            Self::Full => 11 + 8 + 10 + 8,
            Self::NoShare => 11 + 10 + 8,
            Self::Small => 11 + 10,
        }
    }

    /// `spent`, then what is kept of share, requests and tokens
    /// (`modelCells`, `:642-661`).
    fn cells<'a>(
        self,
        spent: &'a str,
        share: &'a str,
        requests: &'a str,
        tokens: &'a str,
    ) -> Vec<Cell<'a>> {
        let spent = Cell::right(spent, 11);
        match self {
            Self::Full => vec![
                spent,
                Cell::right(share, 8),
                Cell::right(requests, 10),
                Cell::right(tokens, 8),
            ],
            Self::NoShare => vec![spent, Cell::right(requests, 10), Cell::right(tokens, 8)],
            Self::Small => vec![spent, Cell::right(requests, 10)],
        }
    }
}

/// A model's numbers in one period, formatted.
struct Numbers {
    spent: String,
    share: String,
    requests: String,
    tokens: String,
}

impl Numbers {
    fn of(totals: &Totals, period_cost: f64) -> Self {
        Self {
            spent: money(totals.total_cost),
            share: percent(share_of(totals.total_cost, period_cost)),
            requests: grouped(totals.request_count),
            tokens: compact(totals.total_tokens()),
        }
    }

    /// A period the model had no usage in: dashes.
    fn none() -> Self {
        Self {
            spent: "-".to_string(),
            share: "-".to_string(),
            requests: "-".to_string(),
            tokens: "-".to_string(),
        }
    }

    fn cells(&self, columns: ModelColumns) -> Vec<Cell<'_>> {
        columns.cells(&self.spent, &self.share, &self.requests, &self.tokens)
    }
}

/// `name`, already spelled safe, middle-ellipsized into `width` columns and
/// padded to them (`appendName`, `:709-716`).
fn append_name(row: &mut String, name: &str, width: usize) {
    let shown = super::model_menu::middle_ellipsized(name, width);
    let pad = width.saturating_sub(cells(&shown));
    row.push_str(&shown);
    row.push_str(&" ".repeat(pad));
}

/// The screen width and palette every row is painted for.
struct Paint {
    limit: usize,
    palette: Palette,
}

impl Paint {
    /// `cells` padded to their widths, stopping before a cell that would pass
    /// the limit (`composeCells`, `:334-341`).
    fn cells(&self, cells: &[Cell<'_>], kind: Kind) -> String {
        let mut row = String::new();
        self.append(&mut row, cells, kind, 0);
        row
    }

    /// Appends `cells` from column `used`; answers the column it stopped at
    /// (`appendCells`, `:343-365`).
    fn append(&self, row: &mut String, cells: &[Cell<'_>], kind: Kind, used: usize) -> usize {
        let mut used = used;
        for (index, cell) in cells.iter().enumerate() {
            if used + cell.width > self.limit {
                break;
            }
            let pad = " ".repeat(cell.width.saturating_sub(super::picker::cells(cell.text)));
            let style = match kind {
                Kind::Header if index == 0 => self.palette.bold(),
                Kind::Header | Kind::Labels => self.palette.dim(),
                Kind::Data => "",
            };
            if !cell.left {
                row.push_str(&pad);
            }
            if style.is_empty() {
                row.push_str(cell.text);
            } else {
                row.push_str(style);
                row.push_str(cell.text);
                row.push_str(self.palette.reset());
            }
            if cell.left {
                row.push_str(&pad);
            }
            used += cell.width;
        }
        used
    }

    /// One dim line of guidance, two columns in, cut with an ellipsis
    /// (`composeTextRow`, `row_text.zig:153-161`).
    fn text_row(&self, text: &str) -> String {
        let indent = GUTTER.min(self.limit);
        format!(
            "{}{}{}{}",
            self.palette.dim(),
            " ".repeat(indent),
            super::picker::ellipsized(text, self.limit - indent),
            self.palette.reset()
        )
    }
}

// -------------------------------------------------------------------- format
//
// Every `{d:.N}` of fx's is [`fixed`]: Zig rounds the shortest decimal that
// reads back as the number, half away from zero, where Rust's `{:.N}` rounds
// the exact binary value to even (`14.5` is `15` there, `14` here).

/// `part` of `whole` as a percent, or `None` with nothing to share
/// (`shareOf`, `:1560-1563`).
fn share_of(part: f64, whole: f64) -> Option<f64> {
    (whole > 0.0).then(|| part / whole * 100.0)
}

/// `81.0%`, or `-` (`optionalPercent`, `:1569-1572`).
fn percent(value: Option<f64>) -> String {
    value.map_or_else(|| "-".to_string(), |value| format!("{}%", fixed(value, 1)))
}

/// `850ms`, `2.1s`, or `-` (`duration`, `:1575-1579`).
fn duration(ms: Option<f64>) -> String {
    match ms {
        None => "-".to_string(),
        Some(ms) if ms < 1000.0 => format!("{}ms", fixed(ms, 0)),
        Some(ms) => format!("{}s", fixed(ms / 1000.0, 1)),
    }
}

/// `$1,234.56`, or four decimals below one dollar (`money`, `:1582-1588`).
fn money(value: f64) -> String {
    if value < 1.0 {
        return format!("${}", fixed(value, 4));
    }
    // Finite and not negative in a valid entry; a sum that overflowed to
    // infinity saturates rather than wraps.
    let cents = (value * 100.0).round() as u64;
    format!("${}.{:02}", grouped(cents / 100), cents % 100)
}

/// `3,665` (`grouped`, `:1591-1604`).
fn grouped(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// `950`, `12K`, `6.9M`, `1.2B` (`compact`, `:1607-1625`).
fn compact(value: u64) -> String {
    const UNITS: [(u64, &str); 4] = [
        (1_000_000_000_000, "T"),
        (1_000_000_000, "B"),
        (1_000_000, "M"),
        (1_000, "K"),
    ];
    for (div, suffix) in UNITS {
        if value >= div {
            let scaled = value as f64 / div as f64;
            let places = if scaled < 10.0 { 1 } else { 0 };
            return format!("{}{suffix}", fixed(scaled, places));
        }
    }
    value.to_string()
}

#[cfg(test)]
mod tests;
