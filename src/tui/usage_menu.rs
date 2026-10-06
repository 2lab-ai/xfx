//! `/usage` on this surface: the dashboard that takes the composer's place.
//!
//! What the numbers are is [`crate::usage`]'s. This module is upstream's
//! compact usage menu (`vercel-labs/fx@c1db9191
//! src/ui/footer/compact_command_menu_presentation.zig:108-1008`, its state in
//! `src/core/session/usage_menu.zig`): which window and which model are
//! marked, and every row the band shows for them, between its two rules.
//!
//! # What it looks like
//!
//! A header -- `Usage` and the four windows, the one shown bracketed --
//! then a status row that says how complete the window is, the overview
//! (tokens, spend, requests; input, output, reasoning; cache read and write),
//! the session's own clock and edits on the session scope, and one row per
//! model: its tokens, its share of them, its spend. Enter opens the marked
//! model's detail line under it. The composer row is not shown while it is up,
//! and every keystroke is the dashboard's (`routeActiveModalInput`,
//! `app_input_runtime.zig:1366-1395`).
//!
//! # Left and Right look reversed, and are
//!
//! The header lists the widest window first; Left and Right step along
//! upstream's enum, which runs the other way and stops at both ends
//! (`Scope.previous`/`next`, `usage_report.zig:44-60`). So Left from `30 days`
//! lands on `7 days`, to its right on the screen, and Right from `30 days`
//! does nothing (`fx-usage-40x120-left`, `right1`). Tab and Shift-Tab walk the
//! header itself, wrapping. Copied as upstream ships it.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;

use crate::usage::{
    compact, duration, grouped, money, share, Completeness, Coverage, Ledger, ModelUsage, Scope,
    SessionActivity, Snapshot, Totals,
};

use super::input::Action;
use super::picker::{cells, ellipsized, pad_to};
use super::theme::{Depth, Mode, Palette};

/// The columns between two of the dashboard's columns (`column_gap`).
const COLUMN_GAP: usize = 4;

/// The most model rows the dashboard lists (`max_usage_model_rows`).
const MAX_MODEL_ROWS: usize = 20;

/// The marked model's prefix, and every other row's.
const MARK: &str = "\u{276f} ";
const UNMARKED: &str = "  ";

/// What the hint row says while the dashboard is up, widest first
/// (`composeCompactCommandMenuHintRow`, `input_presentation.zig:766-790`).
const KEY_HELP: [&str; 3] = [
    "tab scope     \u{2191}\u{2193} model     enter expand     r refresh     esc close",
    "tab scope  \u{2191}\u{2193} model  enter expand  r refresh  esc",
    "tab \u{2191}\u{2193}  enter  r  esc",
];

/// The status row's sentences (`composeUsageStatusRow`, `:436-451`).
const REFRESH_FAILED: &str = "Refresh failed \u{b7} showing previous data";
const NOT_STARTED: &str = "Tracking has not started";
const PARTIAL: &str = "Partial data \u{b7} some usage may be missing";
const PARTIAL_NARROW: &str = "Partial data";
/// Upstream's `Local fx activity`, with this product's name in it.
const COMPLETE: &str = "Local xfx activity";

/// What stands in the dashboard before a window has been read, or once
/// reading it failed (`:170-178`).
const LOADING: &str = "Loading usage";
const UNAVAILABLE: &str = "Usage unavailable \u{b7} press r to retry";

/// The name the reading thread is given, for a debugger or a panic message.
const THREAD_NAME: &str = "xfx-usage";

/// Why a read could not even be asked for: a session with no profile to read
/// -- not reachable from a real session, which refuses to open without a home,
/// and the answer for a shell a test built without one.
const NO_PROFILE: &str = "no home directory is set";

/// One read of the session logs, as the reading thread hands it back.
#[derive(Debug)]
pub(crate) struct Read {
    /// When it was read: the moment every rolling window ends at.
    pub(crate) at_ms: i64,
    pub(crate) ledger: Result<Ledger, String>,
}

/// Reads the ledger **off the thread that holds the terminal**.
///
/// Upstream refreshes its rolling windows on a background thread and reads only
/// the session scope where it is (`requestUsageDashboardRefresh`,
/// `app_commands.zig:1157-1175`). Here the read is a replay of every session
/// log the widest window can see, which is file reads and JSON on a scale the
/// user's history decides -- a stall the band would show as a frozen screen.
/// So it goes to one thread of its own, started on the first `/usage`, and
/// what it read comes back on a channel the UI reads once a turn of its loop,
/// the way [`super::sound`]'s saves do.
#[derive(Debug)]
pub(crate) struct Reader {
    profile: Option<PathBuf>,
    thread: Option<(Sender<i64>, JoinHandle<()>)>,
    done: Sender<Read>,
    results: Receiver<Read>,
    /// How many reads a reader built by a test was asked for: one that starts
    /// no thread, so what it answers is only ever what the test delivers.
    #[cfg(test)]
    asked: Option<usize>,
}

impl Reader {
    pub(crate) fn new(profile: Option<PathBuf>) -> Self {
        let (done, results) = mpsc::channel();
        Self {
            profile,
            thread: None,
            done,
            results,
            #[cfg(test)]
            asked: None,
        }
    }

    /// A reader that reads nothing: asked, it counts the ask, and answers
    /// only with what [`Self::deliver`] hands it -- so a case about what the
    /// UI does with a read is not racing a real one.
    #[cfg(test)]
    pub(crate) fn by_hand() -> Self {
        let mut reader = Self::new(Some(PathBuf::new()));
        reader.asked = Some(0);
        reader
    }

    /// How many reads a [`Self::by_hand`] reader was asked for.
    #[cfg(test)]
    pub(crate) fn asked(&self) -> usize {
        self.asked.unwrap_or(0)
    }

    /// Asks for one read, as of `now_ms`, and returns without waiting.
    pub(crate) fn request(&mut self, now_ms: i64) -> Result<(), String> {
        let Some(profile) = self.profile.clone() else {
            return Err(NO_PROFILE.to_string());
        };
        #[cfg(test)]
        if let Some(asked) = self.asked.as_mut() {
            *asked += 1;
            return Ok(());
        }
        if self.thread.is_none() {
            let (asks, asked) = mpsc::channel::<i64>();
            let done = self.done.clone();
            let thread = std::thread::Builder::new()
                .name(THREAD_NAME.to_string())
                .spawn(move || {
                    for at_ms in asked {
                        let ledger = crate::usage::read_ledger(&profile, at_ms)
                            .map_err(|err| err.to_string());
                        // Nobody left to tell is a session that has ended.
                        let _ = done.send(Read { at_ms, ledger });
                    }
                })
                .map_err(|err| format!("no thread to read usage with: {err}"))?;
            self.thread = Some((asks, thread));
        }
        let (asks, _) = self.thread.as_ref().expect("the reader was just started");
        // The thread is gone only after a panic in it; it is not started
        // again, because whatever killed it would kill the next.
        asks.send(now_ms)
            .map_err(|_| "the usage reader stopped".to_string())
    }

    /// A read that has finished, or `None` when none has since the last call.
    pub(crate) fn finished(&self) -> Option<Read> {
        self.results.try_recv().ok()
    }

    /// Stands in for the thread, for the cases about what the UI does with a
    /// read.
    #[cfg(test)]
    pub(crate) fn deliver(&self, read: Read) {
        let _ = self.done.send(read);
    }
}

impl Drop for Reader {
    /// Lets a read in flight finish before the session is gone, and the
    /// thread end before the signal handlers do: the shell is dropped ahead of
    /// `super::signals::release`, whose safety argument is that no handler can
    /// be running on a thread other than the UI's when the dispositions go
    /// back -- and a thread spawned from the UI thread outside a wait can be
    /// handed one ([`super::sound`]'s writer is joined for the same reason).
    fn drop(&mut self) {
        if let Some((asks, thread)) = self.thread.take() {
            drop(asks);
            let _ = thread.join();
        }
    }
}

/// What one keystroke means to the dashboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MenuAction {
    /// Tab, and Shift-Tab with `false`: the next window along the header,
    /// wrapping.
    Cycle(bool),
    /// Left: one step towards the session, stopping there.
    Previous,
    /// Right: one step towards 30 days, stopping there.
    Next,
    /// Up and Ctrl-K, and Down and Ctrl-J with `true`: the model above or
    /// below, stopping at the ends (`moveModel`, `usage_menu.zig:131-145`).
    Move(bool),
    /// Enter: open or close the marked model's detail line.
    Expand,
    /// `r`: read the window again.
    Reload,
    /// Escape.
    Close,
}

impl MenuAction {
    /// What a decoded keystroke means here, and `None` for every one the
    /// dashboard swallows without meaning anything by it.
    ///
    /// Ctrl-J and Ctrl-K are upstream's too (`app_input_runtime.zig:1386-1393`,
    /// bytes 10 and 11), and arrive as the actions those bytes decode to on the
    /// composer. Shift-Tab reaches here as the permission-mode cycle's key and
    /// is the dashboard's while it is up (`:2667-2670`).
    pub(crate) fn of_action(action: Action) -> Option<Self> {
        match action {
            Action::Tab => Some(Self::Cycle(true)),
            Action::BackTab => Some(Self::Cycle(false)),
            Action::Left => Some(Self::Previous),
            Action::Right => Some(Self::Next),
            Action::Up | Action::KillToEnd => Some(Self::Move(false)),
            Action::Down | Action::InsertNewline => Some(Self::Move(true)),
            Action::Submit => Some(Self::Expand),
            Action::Escape => Some(Self::Close),
            _ => None,
        }
    }

    /// What a typed character means here: `r` or `R`, and nothing else.
    pub(crate) fn of_text(character: char) -> Option<Self> {
        matches!(character, 'r' | 'R').then_some(Self::Reload)
    }
}

/// The dashboard: which window, which model, and what it is showing.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Dashboard {
    /// The window asked for, which is the one the header brackets -- even
    /// while it is still being read (`navigationScope`).
    scope: Scope,
    /// The window being shown, or `None` while the first read is out.
    snapshot: Option<Snapshot>,
    /// The last read failed; what is shown, if anything, is older.
    refresh_failed: bool,
    selected: usize,
    expanded: Option<usize>,
    /// The first model row in view.
    window_start: usize,
}

impl Dashboard {
    /// The dashboard on `scope`, showing `snapshot` when there is one to show
    /// at once (`openOwned`) and `Loading usage` when there is not
    /// (`openLoading`, `usage_menu.zig:15-55`).
    pub(crate) fn open(scope: Scope, snapshot: Option<Snapshot>) -> Self {
        Self {
            scope: snapshot.as_ref().map_or(scope, |shown| shown.scope),
            snapshot,
            refresh_failed: false,
            selected: 0,
            expanded: None,
            window_start: 0,
        }
    }

    /// The window the header brackets.
    pub(crate) fn scope(&self) -> Scope {
        self.scope
    }

    /// Moves to `scope` with nothing to show for it yet: the selection goes,
    /// and the body says `Loading usage` (`setLoadingScope`, `:56-69`).
    pub(crate) fn loading(&mut self, scope: Scope) {
        *self = Self::open(scope, None);
    }

    /// Shows `snapshot`, keeping the marked and the open model by **name**
    /// when the new window still has them (`replaceOwned`, `:76-111`).
    pub(crate) fn show(&mut self, snapshot: Snapshot) {
        let name = |snapshot: &Option<Snapshot>, index: Option<usize>| -> Option<String> {
            let models = &snapshot.as_ref()?.models;
            models
                .get(index?.min(models.len().checked_sub(1)?))
                .map(|m| m.model.clone())
        };
        let marked = name(&self.snapshot, Some(self.selected));
        let opened = self.expanded.and_then(|index| {
            self.snapshot
                .as_ref()
                .and_then(|shown| shown.models.get(index))
                .map(|model| model.model.clone())
        });
        let find = |wanted: Option<String>| {
            let wanted = wanted?;
            snapshot
                .models
                .iter()
                .position(|model| model.model == wanted)
        };
        self.selected =
            find(marked).unwrap_or(self.selected.min(snapshot.models.len().saturating_sub(1)));
        self.expanded = find(opened);
        self.window_start = self.window_start.min(self.selected);
        self.scope = snapshot.scope;
        self.snapshot = Some(snapshot);
        self.refresh_failed = false;
    }

    /// A read of `scope` failed: the header says the window that was asked
    /// for, and whatever was shown stays, marked old (`recordRefreshFailure`,
    /// `:113-123`).
    pub(crate) fn failed(&mut self, scope: Scope) {
        self.refresh_failed = true;
        self.scope = scope;
    }

    /// Moves the mark one model, stopping at the ends, and keeps it among the
    /// `visible` model rows. Answers whether it moved.
    pub(crate) fn step(&mut self, down: bool, visible: usize) -> bool {
        let count = self.models().len();
        if count == 0 {
            return false;
        }
        let next = if down {
            (self.selected + 1).min(count - 1)
        } else {
            self.selected.saturating_sub(1)
        };
        if next == self.selected {
            return false;
        }
        self.selected = next;
        self.keep_visible(visible);
        true
    }

    /// Opens the marked model's detail line, or closes it (`toggleExpanded`,
    /// `:147-155`). Answers whether there was a model to open.
    pub(crate) fn toggle(&mut self, visible: usize) -> bool {
        let count = self.models().len();
        if count == 0 {
            return false;
        }
        let selected = self.selected.min(count - 1);
        self.expanded = if self.expanded == Some(selected) {
            None
        } else {
            Some(selected)
        };
        self.keep_visible(visible);
        true
    }

    fn keep_visible(&mut self, visible: usize) {
        let visible = visible.max(1);
        if self.selected < self.window_start {
            self.window_start = self.selected;
        } else if self.selected >= self.window_start + visible {
            self.window_start = self.selected - (visible - 1);
        }
    }

    fn models(&self) -> &[ModelUsage] {
        self.snapshot
            .as_ref()
            .map_or(&[][..], |snapshot| snapshot.models.as_slice())
    }

    /// How many rows the dashboard wants on a screen `cols` wide
    /// (`usageDesiredRowCount`, `:142-154`).
    pub(crate) fn wanted_rows(&self, cols: u16) -> u16 {
        let Some(snapshot) = self.snapshot.as_ref() else {
            return 2;
        };
        let count = if snapshot.totals.is_none() {
            2 + if snapshot.activity.is_some() { 3 } else { 0 }
        } else {
            let model_rows =
                snapshot.models.len().min(MAX_MODEL_ROWS) + usize::from(self.expanded.is_some());
            Layout::of(snapshot, cols).model_start + model_rows.max(1)
        };
        u16::try_from(count).unwrap_or(u16::MAX)
    }

    /// How many rows the band gives it on a screen `rows` x `cols`: what it
    /// wants, and never more than the screen less three
    /// (`surface_frame.zig:586-592`). At least one.
    pub(crate) fn height(&self, rows: u16, cols: u16) -> u16 {
        self.wanted_rows(cols).min(rows.saturating_sub(3)).max(1)
    }

    /// How many model rows a panel `visible` rows tall shows
    /// (`usageVisibleModelItems`, `:520-538`): what Up, Down and Enter keep the
    /// mark among.
    pub(crate) fn visible_models(&self, visible: u16, cols: u16) -> usize {
        let Some(snapshot) = self.snapshot.as_ref() else {
            return 0;
        };
        if snapshot.models.is_empty() || snapshot.totals.is_none() {
            return 0;
        }
        let start = if visible == 1 {
            0
        } else {
            let natural = Layout::of(snapshot, cols).model_start;
            if usize::from(visible) < natural + 1 {
                constrained_model_start(visible)
            } else {
                natural
            }
        };
        let area = usize::from(visible).saturating_sub(start);
        if area == 0 {
            return 0;
        }
        snapshot.models.len().min(MAX_MODEL_ROWS).min(
            area.saturating_sub(usize::from(self.expanded.is_some()))
                .max(1),
        )
    }

    /// The panel row the mark is on, for the caret: the marked model's when
    /// it is in view, else the header.
    pub(crate) fn marked_row(&self, visible: u16, cols: u16) -> u16 {
        // Which palette paints them does not move a row.
        let any = Palette {
            mode: Mode::Dark,
            depth: Depth::Ansi256,
        };
        let rows = self.rows(cols, visible, any);
        rows.iter()
            .position(|row| row.contains(MARK))
            .and_then(|at| u16::try_from(at).ok())
            .unwrap_or(0)
    }

    /// The dashboard's rows, top first, for a panel `visible` rows tall
    /// (`composeUsageRow`, `:156-255`).
    pub(crate) fn rows(&self, cols: u16, visible: u16, palette: Palette) -> Vec<String> {
        let paint = Paint { cols, palette };
        if visible <= 1 {
            return vec![self.priority_row(&paint)];
        }
        let mut rows = vec![header(self.scope, &paint)];
        let Some(snapshot) = self.snapshot.as_ref() else {
            rows.push(paint.dim(if self.refresh_failed {
                UNAVAILABLE
            } else {
                LOADING
            }));
            return rows;
        };
        let Some(totals) = snapshot.totals else {
            rows.push(paint.dim(self.status(snapshot, cols)));
            if let Some(activity) = snapshot.activity {
                rows.extend(activity_rows(activity, &paint));
            }
            rows.truncate(usize::from(visible));
            return rows;
        };
        let layout = Layout::of(snapshot, cols);
        if usize::from(visible) < layout.model_start + 1 {
            return self.constrained_rows(snapshot, totals, visible, &paint);
        }
        rows.push(paint.dim(self.status(snapshot, cols)));
        while rows.len() < layout.overview_start {
            rows.push(String::new());
        }
        for row in 0..layout.overview_rows {
            rows.push(overview_row(totals, layout.overview, row, &paint));
        }
        if let (Some(start), Some(activity)) = (layout.activity_start, snapshot.activity) {
            while rows.len() < start {
                rows.push(String::new());
            }
            rows.extend(activity_rows(activity, &paint));
        }
        while rows.len() < layout.models_header {
            rows.push(String::new());
        }
        rows.push(paint.heading(&models_heading(snapshot, cols)));
        if layout.columns {
            if let Some(columns) = ModelColumns::of(snapshot, cols) {
                rows.push(columns.row("Model", "Tokens", "Share", "Spend", false, &paint));
            }
        }
        if snapshot.models.is_empty() {
            rows.push(paint.dim("  No model usage in this scope."));
        } else {
            let room = usize::from(visible).saturating_sub(layout.model_start);
            rows.extend(self.model_rows(snapshot, totals, room, &paint));
        }
        rows.truncate(usize::from(visible));
        rows
    }

    /// What the status row says about `snapshot`.
    fn status(&self, snapshot: &Snapshot, cols: u16) -> &'static str {
        if self.refresh_failed {
            REFRESH_FAILED
        } else if snapshot.coverage == Coverage::NotStarted {
            NOT_STARTED
        } else if snapshot.coverage == Coverage::Partial
            || snapshot.completeness != Completeness::Complete
        {
            if cols < 48 {
                PARTIAL_NARROW
            } else {
                PARTIAL
            }
        } else {
            COMPLETE
        }
    }

    /// A one-row panel: the marked model when there is one, else the status
    /// (`composeUsagePriorityRow`, `:257-282`).
    fn priority_row(&self, paint: &Paint) -> String {
        let Some(snapshot) = self.snapshot.as_ref() else {
            return paint.dim(if self.refresh_failed {
                UNAVAILABLE
            } else {
                LOADING
            });
        };
        match snapshot.totals {
            Some(totals) if !snapshot.models.is_empty() => self
                .model_rows(snapshot, totals, 1, paint)
                .into_iter()
                .next()
                .unwrap_or_default(),
            _ => paint.dim(self.status(snapshot, paint.cols)),
        }
    }

    /// A panel too short for the whole layout: header, status, a one-line
    /// summary, the models header, then the models (`composeConstrainedUsageRow`,
    /// `:335-364`).
    fn constrained_rows(
        &self,
        snapshot: &Snapshot,
        totals: Totals,
        visible: u16,
        paint: &Paint,
    ) -> Vec<String> {
        let start = constrained_model_start(visible);
        let mut rows = vec![header(self.scope, paint)];
        if start > 1 {
            rows.push(paint.dim(self.status(snapshot, paint.cols)));
        }
        if start > 2 {
            rows.push(paint.dim(&format!(
                "{} tokens \u{b7} {}",
                compact(totals.total_tokens),
                money(totals.total_cost)
            )));
        }
        if start > 3 {
            rows.push(paint.heading(&models_heading(snapshot, paint.cols)));
        }
        let room = usize::from(visible).saturating_sub(start);
        if snapshot.models.is_empty() {
            rows.push(paint.dim(self.status(snapshot, paint.cols)));
        } else {
            rows.extend(self.model_rows(snapshot, totals, room, paint));
        }
        rows.truncate(usize::from(visible));
        rows
    }

    /// The model rows that fit in `room` rows, the marked one in view and the
    /// open one followed by its detail line (`composeUsageModelRow`, `:453-518`).
    fn model_rows(
        &self,
        snapshot: &Snapshot,
        totals: Totals,
        room: usize,
        paint: &Paint,
    ) -> Vec<String> {
        let count = snapshot.models.len();
        let selected = self.selected.min(count - 1);
        let shown = count.min(MAX_MODEL_ROWS).min(
            room.saturating_sub(usize::from(self.expanded.is_some()))
                .max(1),
        );
        let max_start = count - shown;
        let start = self
            .window_start
            .min(selected)
            .max(selected.saturating_sub(shown - 1))
            .min(max_start);
        let mode = ModelMode::of(snapshot, paint.cols);
        let mut rows = Vec::new();
        for (index, model) in snapshot.models.iter().enumerate().skip(start) {
            if rows.len() >= room {
                break;
            }
            rows.push(model_row(
                snapshot,
                totals,
                model,
                index == selected,
                mode,
                paint,
            ));
            if self.expanded == Some(index) && rows.len() < room {
                rows.push(paint.dim(&detail(&model.totals)));
            }
        }
        rows
    }
}

/// The models heading: with the count from 48 columns up
/// (`composeUsageModelsHeader`, `:718-729`).
fn models_heading(snapshot: &Snapshot, cols: u16) -> String {
    if cols < 48 {
        "Models".to_string()
    } else {
        format!("Models {}", snapshot.models.len())
    }
}

/// Where a constrained panel's model rows begin (`constrainedUsageModelStart`).
fn constrained_model_start(visible: u16) -> usize {
    if visible <= 4 {
        usize::from(visible.saturating_sub(1))
    } else {
        4
    }
}

/// What the hint row says while the dashboard is up, on a screen `cols` wide:
/// the widest of [`KEY_HELP`] that fits, and the narrowest when none does.
pub(crate) fn key_help(cols: u16) -> &'static str {
    KEY_HELP
        .iter()
        .copied()
        .find(|candidate| cells(candidate) <= usize::from(cols))
        .unwrap_or(KEY_HELP[KEY_HELP.len() - 1])
}

/// The screen width and palette every row is painted for.
struct Paint {
    cols: u16,
    palette: Palette,
}

impl Paint {
    fn width(&self) -> usize {
        usize::from(self.cols)
    }

    /// `text` in one style, ellipsized to the screen (`composeStyledRow`).
    fn styled(&self, style: &str, text: &str) -> String {
        format!(
            "{style}{}{}",
            ellipsized(text, self.width()),
            self.palette.reset()
        )
    }

    fn dim(&self, text: &str) -> String {
        self.styled(self.palette.dim(), text)
    }

    /// Upstream's `system_notice_label_style`: bold, in the label grey.
    fn heading(&self, text: &str) -> String {
        self.styled(self.palette.notice_heading(), text)
    }

    fn indent(&self) -> usize {
        if self.cols <= 2 {
            0
        } else {
            2
        }
    }
}

/// The header: `Usage` and every window, the shown one bracketed and bold
/// (`composeUsageHeaderRow`, `:284-333`). On a screen under 64 columns, or
/// one too narrow for all four, only the shown one.
fn header(active: Scope, paint: &Paint) -> String {
    let palette = paint.palette;
    let tab = |scope: Scope| {
        if scope == active {
            format!(
                "{}[{}]{}",
                palette.subtitle(),
                scope.label(),
                palette.reset()
            )
        } else {
            format!("{}{}{}", palette.dim(), scope.label(), palette.reset())
        }
    };
    let title = format!("{}Usage{}", palette.subtitle(), palette.reset());
    let mut wide = title.clone();
    for scope in Scope::HEADER {
        wide.push_str("  ");
        wide.push_str(&tab(scope));
    }
    if paint.cols >= 64 && cells(&wide) <= paint.width() {
        return wide;
    }
    let compact = format!("{title}  {}", tab(active));
    format!(
        "{}{}",
        super::frame::clip(&compact, paint.cols),
        palette.reset()
    )
}

/// The session's block: its heading, its clock, its edits
/// (`composeCompactSessionActivityRow`, `:387-434`).
fn activity_rows(activity: SessionActivity, paint: &Paint) -> Vec<String> {
    vec![
        paint.heading("Session activity"),
        paint.dim(&format!(
            "API {} \u{b7} Wall {}",
            duration(activity.api_duration_ms),
            duration(activity.wall_duration_ms)
        )),
        paint.dim(&format!(
            "Code +{} \u{b7} -{}",
            activity.lines_added, activity.lines_removed
        )),
    ]
}

/// How the overview is laid out on a screen of some width
/// (`UsageOverviewMode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Overview {
    /// Three rows of three aligned columns: 110 columns and room for them.
    Wide { second: usize, third: usize },
    /// Three rows, each its cells joined by ` · `.
    Medium,
    /// Two rows: tokens and spend, then requests.
    Narrow,
}

/// Where each part of the dashboard starts (`usageLayout`, `:557-591`).
#[derive(Debug, Clone, Copy)]
struct Layout {
    overview: Overview,
    overview_start: usize,
    overview_rows: usize,
    activity_start: Option<usize>,
    models_header: usize,
    columns: bool,
    model_start: usize,
}

impl Layout {
    fn of(snapshot: &Snapshot, cols: u16) -> Self {
        let totals = snapshot.totals.unwrap_or(EMPTY_TOTALS);
        let overview = match overview_columns(totals, cols) {
            Some((second, third)) if cols >= 110 => Overview::Wide { second, third },
            _ if cols >= 48 => Overview::Medium,
            _ => Overview::Narrow,
        };
        let narrow = overview == Overview::Narrow;
        let overview_start = if narrow { 2 } else { 3 };
        let overview_rows = if narrow { 2 } else { 3 };
        let mut cursor = overview_start + overview_rows;
        let mut activity_start = None;
        if snapshot.activity.is_some() {
            if !narrow {
                cursor += 1;
            }
            activity_start = Some(cursor);
            cursor += 3;
        }
        if !narrow {
            cursor += 1;
        }
        let models_header = cursor;
        cursor += 1;
        let columns = ModelMode::of(snapshot, cols) == ModelMode::Columns;
        if columns {
            cursor += 1;
        }
        Self {
            overview,
            overview_start,
            overview_rows,
            activity_start,
            models_header,
            columns,
            model_start: cursor,
        }
    }
}

/// Totals of nothing, for a layout asked of a snapshot that has none.
const EMPTY_TOTALS: Totals = Totals {
    total_tokens: 0,
    input_tokens: 0,
    output_tokens: 0,
    cache_read_tokens: 0,
    cache_write_tokens: 0,
    reasoning_tokens: None,
    request_count: None,
    total_cost: 0.0,
};

/// The overview's three cells on `row` (`usageOverviewCells`, `:630-672`).
fn overview_cells(totals: Totals, narrow: bool, row: usize) -> [String; 3] {
    let tokens = |value: u64, label: &str| format!("{} {label}", compact(value));
    let requests = || match totals.request_count {
        Some(count) => format!(
            "{} {}",
            grouped(count),
            if count == 1 { "request" } else { "requests" }
        ),
        None => "Requests unavailable".to_string(),
    };
    if narrow {
        return match row {
            0 => [
                tokens(totals.total_tokens, "tokens"),
                money(totals.total_cost),
                String::new(),
            ],
            _ => [requests(), String::new(), String::new()],
        };
    }
    match row {
        0 => [
            tokens(totals.total_tokens, "tokens"),
            format!("{} spent", money(totals.total_cost)),
            requests(),
        ],
        1 => [
            tokens(totals.input_tokens, "input"),
            tokens(totals.output_tokens, "output"),
            match totals.reasoning_tokens {
                Some(value) => tokens(value, "reasoning"),
                None => "Reasoning unavailable".to_string(),
            },
        ],
        _ => [
            tokens(totals.cache_read_tokens, "cache read"),
            tokens(totals.cache_write_tokens, "cache write"),
            String::new(),
        ],
    }
}

/// Where the overview's second and third columns start, or `None` when the
/// three do not fit (`usageOverviewColumns`, `:674-695`).
fn overview_columns(totals: Totals, cols: u16) -> Option<(usize, usize)> {
    let mut widths = [0usize; 3];
    for row in 0..3 {
        for (width, cell) in widths.iter_mut().zip(overview_cells(totals, false, row)) {
            *width = (*width).max(cells(&cell));
        }
    }
    let indent = if cols <= 2 { 0 } else { 2 };
    let second = indent + widths[0] + COLUMN_GAP;
    let third = second + widths[1] + COLUMN_GAP;
    (third + widths[2] <= usize::from(cols)).then_some((second, third))
}

/// One overview row (`composeUsageOverviewRow`, `:593-628`).
fn overview_row(totals: Totals, overview: Overview, row: usize, paint: &Paint) -> String {
    let [first, second, third] = overview_cells(totals, overview == Overview::Narrow, row);
    match overview {
        // `composeUsageMetricColumns`, `:697-716`.
        Overview::Wide {
            second: at_second,
            third: at_third,
        } => {
            let mut text = String::from(paint.palette.dim());
            text.push_str(&" ".repeat(paint.indent()));
            text.push_str(&ellipsized(&first, at_second.saturating_sub(3)));
            pad_to(&mut text, at_second);
            text.push_str(&ellipsized(
                &second,
                at_third.saturating_sub(at_second).saturating_sub(1),
            ));
            if !third.is_empty() {
                pad_to(&mut text, at_third);
                text.push_str(&ellipsized(&third, paint.width().saturating_sub(at_third)));
            }
            text.push_str(paint.palette.reset());
            text
        }
        Overview::Medium | Overview::Narrow => {
            let text = match (second.is_empty(), third.is_empty()) {
                (_, false) => format!("{first} \u{b7} {second} \u{b7} {third}"),
                (false, true) => format!("{first} \u{b7} {second}"),
                (true, true) => first,
            };
            paint.dim(&text)
        }
    }
}

/// How the model rows are laid out (`UsageModelMode`, `:820-825`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModelMode {
    /// Name, tokens, share and spend in aligned columns, under a header row.
    Columns,
    /// The name, then `tokens · share · spend` at one column.
    Facts(usize),
    /// The name, then `tokens · spend`.
    Compact(usize),
    /// The name alone.
    NameOnly,
}

impl ModelMode {
    fn of(snapshot: &Snapshot, cols: u16) -> Self {
        if cols >= 110 && ModelColumns::of(snapshot, cols).is_some() {
            return Self::Columns;
        }
        if cols >= 48 {
            if let Some(at) = info_column(snapshot, cols, true) {
                return Self::Facts(at);
            }
        }
        match info_column(snapshot, cols, false) {
            Some(at) => Self::Compact(at),
            None => Self::NameOnly,
        }
    }
}

/// Where the model table's columns start (`usageModelColumns`, `:827-853`).
#[derive(Debug, Clone, Copy)]
struct ModelColumns {
    token: usize,
    share: usize,
    spend: usize,
}

impl ModelColumns {
    fn of(snapshot: &Snapshot, cols: u16) -> Option<Self> {
        let total = snapshot.totals.map_or(0, |totals| totals.total_tokens);
        let indent = if cols <= 2 { 0 } else { 2 };
        let mut longest = cells("Model");
        let (mut token_width, mut share_width, mut spend_width) =
            (cells("Tokens"), cells("Share"), cells("Spend"));
        for model in &snapshot.models {
            longest = longest.max(cells(&printable(&model.model)));
            token_width = token_width.max(cells(&compact(model.totals.total_tokens)));
            share_width = share_width.max(cells(&share(model.totals.total_tokens, total)));
            spend_width = spend_width.max(cells(&money(model.totals.total_cost)));
        }
        let fixed = COLUMN_GAP * 3 + token_width + share_width + spend_width;
        let width = usize::from(cols);
        if width < indent + 10 + fixed {
            return None;
        }
        let name_width = longest.min(width - indent - fixed);
        let token = indent + name_width + COLUMN_GAP;
        let share = token + token_width + COLUMN_GAP;
        let spend = share + share_width + COLUMN_GAP;
        Some(Self {
            token,
            share,
            spend,
        })
    }

    /// One row of the table (`composeUsageModelColumnsRow`, `:792-818`): the
    /// name in the row's own style -- bold behind `❯` when marked -- and the
    /// three numbers dim.
    fn row(
        &self,
        name: &str,
        tokens: &str,
        share: &str,
        spend: &str,
        marked: bool,
        paint: &Paint,
    ) -> String {
        let palette = paint.palette;
        let mut row = String::from(if marked {
            palette.notice_heading()
        } else {
            palette.dim()
        });
        row.push_str(super::frame::clip(
            if marked { MARK } else { UNMARKED },
            paint.cols,
        ));
        let used = cells(&row);
        row.push_str(&ellipsized(
            name,
            self.token.saturating_sub(used).saturating_sub(1),
        ));
        row.push_str(palette.reset());
        pad_to(&mut row, self.token);
        row.push_str(palette.dim());
        row.push_str(&ellipsized(
            tokens,
            self.share.saturating_sub(self.token).saturating_sub(1),
        ));
        pad_to(&mut row, self.share);
        row.push_str(&ellipsized(
            share,
            self.spend.saturating_sub(self.share).saturating_sub(1),
        ));
        pad_to(&mut row, self.spend);
        row.push_str(&ellipsized(spend, paint.width().saturating_sub(self.spend)));
        row.push_str(palette.reset());
        row
    }
}

/// Where a narrower screen puts a model's facts, or `None` when they do not
/// fit beside a name (`usageModelInfoColumn`, `:855-878`).
fn info_column(snapshot: &Snapshot, cols: u16, with_share: bool) -> Option<usize> {
    let total = snapshot.totals.map_or(0, |totals| totals.total_tokens);
    let indent = if cols <= 2 { 0 } else { 2 };
    let mut longest = 0;
    let mut widest = 0;
    for model in &snapshot.models {
        longest = longest.max(cells(&printable(&model.model)));
        widest = widest.max(cells(&facts(model, total, with_share)));
    }
    let width = usize::from(cols);
    if widest == 0 || width < indent + 8 + COLUMN_GAP + widest {
        return None;
    }
    Some((indent + longest + COLUMN_GAP).min(width - widest))
}

/// A model's numbers on one string (`formatUsageModelFacts`, `:880-908`).
fn facts(model: &ModelUsage, total: u64, with_share: bool) -> String {
    let tokens = compact(model.totals.total_tokens);
    let spend = money(model.totals.total_cost);
    if with_share {
        format!(
            "{tokens} \u{b7} {} \u{b7} {spend}",
            share(model.totals.total_tokens, total)
        )
    } else {
        format!("{tokens} \u{b7} {spend}")
    }
}

/// One model's row, in whichever layout the screen allows
/// (`composeUsageModelSummaryRow`, `:750-790`).
fn model_row(
    snapshot: &Snapshot,
    totals: Totals,
    model: &ModelUsage,
    marked: bool,
    mode: ModelMode,
    paint: &Paint,
) -> String {
    let name = printable(&model.model);
    match mode {
        ModelMode::Columns => match ModelColumns::of(snapshot, paint.cols) {
            Some(columns) => columns.row(
                &name,
                &compact(model.totals.total_tokens),
                &share(model.totals.total_tokens, totals.total_tokens),
                &money(model.totals.total_cost),
                marked,
                paint,
            ),
            None => action_row(&name, "", marked, None, paint),
        },
        ModelMode::Facts(at) => action_row(
            &name,
            &facts(model, totals.total_tokens, true),
            marked,
            Some(at),
            paint,
        ),
        ModelMode::Compact(at) => action_row(
            &name,
            &facts(model, totals.total_tokens, false),
            marked,
            Some(at),
            paint,
        ),
        ModelMode::NameOnly => action_row(&name, "", marked, None, paint),
    }
}

/// A name with its facts at `info_at` (`composeWorkspaceActionRow`,
/// `:1161-1206`).
fn action_row(
    name: &str,
    info: &str,
    marked: bool,
    info_at: Option<usize>,
    paint: &Paint,
) -> String {
    let palette = paint.palette;
    let width = paint.width();
    let mut row = String::from(if marked {
        palette.notice_heading()
    } else {
        palette.dim()
    });
    row.push_str(super::frame::clip(
        if marked { MARK } else { UNMARKED },
        paint.cols,
    ));
    let used = cells(&row);
    if used >= width {
        row.push_str(palette.reset());
        return row;
    }
    let Some(at) = info_at.filter(|at| *at > used + 2 && !info.is_empty()) else {
        row.push_str(&ellipsized(name, width - used));
        row.push_str(palette.reset());
        return row;
    };
    row.push_str(&ellipsized(name, at - used - 1));
    row.push_str(palette.reset());
    if cells(&row) >= at {
        return row;
    }
    pad_to(&mut row, at);
    row.push_str(palette.dim());
    row.push_str(&ellipsized(info, width.saturating_sub(at)));
    row.push_str(palette.reset());
    row
}

/// The marked model's detail line (`formatUsageModelDetail`, `:910-934`).
fn detail(totals: &Totals) -> String {
    format!(
        "Input {} \u{b7} Output {} \u{b7} Cache {}/{} \u{b7} Reasoning {} \u{b7} Requests {} \u{b7} {}",
        compact(totals.input_tokens),
        compact(totals.output_tokens),
        compact(totals.cache_read_tokens),
        compact(totals.cache_write_tokens),
        totals
            .reasoning_tokens
            .map_or_else(|| "n/a".to_string(), compact),
        totals
            .request_count
            .map_or_else(|| "n/a".to_string(), grouped),
        money(totals.total_cost)
    )
}

/// A model's name with nothing in it a terminal would obey: the name is the
/// requested model's when the provider named none, and that is whatever was
/// typed at `/model`.
fn printable(name: &str) -> String {
    super::bridge::inert(name).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{RecordedUsage, RequestUsage};
    use crate::usage::session_snapshot;

    const NOW: i64 = 40 * 24 * 60 * 60 * 1000;

    fn request(
        model: &str,
        input: u64,
        output: u64,
        cache: u64,
        reasoning: u64,
        cost: f64,
    ) -> RequestUsage {
        RequestUsage {
            at_ms: NOW - 60_000,
            duration_ms: 300,
            model: model.to_string(),
            input_tokens: input,
            output_tokens: output,
            cache_read_tokens: cache,
            cache_write_tokens: 0,
            reasoning_tokens: Some(reasoning),
            cost: Some(cost),
            generation_id: None,
        }
    }

    /// The fx capture's two billed turns (`billed-turns.json`).
    fn billed() -> RecordedUsage {
        RecordedUsage {
            requests: vec![
                request("fake/model-a", 12000, 800, 4000, 0, 0.0125),
                request("anthropic/claude-opus-5", 52000, 3100, 30000, 900, 0.421),
            ],
            ..RecordedUsage::default()
        }
    }

    fn rolling(usage: &RecordedUsage, scope: Scope) -> Snapshot {
        Ledger::from_sessions([usage], &[], Some(NOW - 120_000)).snapshot(scope, NOW)
    }

    const PALETTE: Palette = Palette {
        mode: Mode::Dark,
        depth: Depth::Ansi256,
    };

    /// The rows as plain text, colours taken out.
    fn text(dashboard: &Dashboard, cols: u16, visible: u16) -> Vec<String> {
        dashboard
            .rows(cols, visible, PALETTE)
            .iter()
            .map(|row| plain(row))
            .collect()
    }

    fn plain(row: &str) -> String {
        let mut out = String::new();
        let mut chars = row.chars().peekable();
        while let Some(character) = chars.next() {
            if character == '\u{1b}' {
                for next in chars.by_ref() {
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
                continue;
            }
            out.push(character);
        }
        out
    }

    #[test]
    fn the_thirty_day_window_reads_as_fxs_capture_does() {
        // `fx-usage3-40x120-d30`, rows 4-14.
        let dashboard = Dashboard::open(Scope::Days30, Some(rolling(&billed(), Scope::Days30)));
        assert_eq!(dashboard.wanted_rows(120), 11);
        assert_eq!(
            text(&dashboard, 120, 11),
            [
                "Usage  [30 days]  7 days  24 hours  Session",
                "Partial data \u{b7} some usage may be missing",
                "",
                "  67.9K tokens      $0.43 spent      2 requests",
                "  64K input         3.9K output      900 reasoning",
                "  34K cache read    0 cache write",
                "",
                "Models 2",
                "  Model                      Tokens    Share    Spend",
                "\u{276f} anthropic/claude-opus-5    55.1K     81.1%    $0.42",
                "  fake/model-a               12.8K     18.9%    $0.01",
            ]
        );
    }

    #[test]
    fn enter_opens_the_marked_model_and_down_moves_the_mark_not_the_line() {
        // `fx-usage3-40x120-d30-expand`, then `-d30-down`.
        let mut dashboard = Dashboard::open(Scope::Days30, Some(rolling(&billed(), Scope::Days30)));
        assert!(dashboard.toggle(20));
        assert_eq!(dashboard.wanted_rows(120), 12);
        let rows = text(&dashboard, 120, 12);
        assert_eq!(
            rows[10],
            "Input 52K \u{b7} Output 3.1K \u{b7} Cache 30K/0 \u{b7} Reasoning 900 \u{b7} Requests 1 \u{b7} $0.42"
        );
        assert!(dashboard.step(true, 20));
        let rows = text(&dashboard, 120, 12);
        assert_eq!(
            rows[9],
            "  anthropic/claude-opus-5    55.1K     81.1%    $0.42"
        );
        assert!(rows[10].starts_with("Input 52K"), "{rows:?}");
        assert_eq!(
            rows[11],
            "\u{276f} fake/model-a               12.8K     18.9%    $0.01"
        );
        // The end stops it; it does not wrap.
        assert!(!dashboard.step(true, 20));
        assert!(dashboard.step(false, 20));
        assert!(!dashboard.step(false, 20));
    }

    #[test]
    fn the_session_scope_adds_its_clock_and_keeps_the_marked_model_by_name() {
        // `fx-usage3-40x120-session`: the mark stayed on fake/model-a across
        // three Tabs, and its detail line with it.
        let mut dashboard = Dashboard::open(Scope::Days30, Some(rolling(&billed(), Scope::Days30)));
        dashboard.step(true, 20);
        dashboard.toggle(20);
        dashboard.show(session_snapshot(&billed(), Some(NOW - 17_400), NOW));
        assert_eq!(dashboard.scope(), Scope::Session);
        let rows = text(&dashboard, 120, 16);
        assert_eq!(rows[0], "Usage  30 days  7 days  24 hours  [Session]");
        assert_eq!(rows[1], "Local xfx activity");
        assert_eq!(
            &rows[6..10],
            [
                "",
                "Session activity",
                "API 0s \u{b7} Wall 17s",
                "Code +0 \u{b7} -0"
            ]
        );
        assert_eq!(rows[11], "Models 2");
        assert_eq!(
            rows[14],
            "\u{276f} fake/model-a               12.8K     18.9%    $0.01"
        );
        assert!(
            rows[15].starts_with("Input 12K \u{b7} Output 800 \u{b7} Cache 4K/0"),
            "{rows:?}"
        );
    }

    #[test]
    fn nothing_recorded_yet_is_two_rows() {
        // `fx-usage-40x120-empty`.
        let ledger = Ledger::from_sessions(std::iter::empty(), &[], None);
        let dashboard = Dashboard::open(Scope::Days30, Some(ledger.snapshot(Scope::Days30, NOW)));
        assert_eq!(dashboard.wanted_rows(120), 2);
        assert_eq!(
            text(&dashboard, 120, 2),
            [
                "Usage  [30 days]  7 days  24 hours  Session",
                "Tracking has not started"
            ]
        );
    }

    #[test]
    fn a_window_of_unpriced_requests_is_partial_with_no_models() {
        // `fx-usage-40x120-open`, after one turn the fake gateway did not bill.
        let mut unbilled = billed();
        unbilled.requests.truncate(1);
        unbilled.requests[0].cost = None;
        let dashboard = Dashboard::open(Scope::Days30, Some(rolling(&unbilled, Scope::Days30)));
        assert_eq!(
            text(&dashboard, 120, 10),
            [
                "Usage  [30 days]  7 days  24 hours  Session",
                "Partial data \u{b7} some usage may be missing",
                "",
                "  0 tokens        $0.00 spent      0 requests",
                "  0 input         0 output         Reasoning unavailable",
                "  0 cache read    0 cache write",
                "",
                "Models 0",
                "  Model    Tokens    Share    Spend",
                "  No model usage in this scope.",
            ]
        );
    }

    #[test]
    fn loading_and_failing_say_so_in_the_body() {
        let mut dashboard = Dashboard::open(Scope::Days30, None);
        assert_eq!(dashboard.wanted_rows(120), 2);
        assert_eq!(text(&dashboard, 120, 2)[1], "Loading usage");
        dashboard.failed(Scope::Days7);
        assert_eq!(
            text(&dashboard, 120, 2),
            [
                "Usage  30 days  [7 days]  24 hours  Session",
                "Usage unavailable \u{b7} press r to retry"
            ]
        );
        // With something already shown, it stays, marked old.
        let mut dashboard = Dashboard::open(Scope::Days30, Some(rolling(&billed(), Scope::Days30)));
        dashboard.failed(Scope::Days30);
        assert_eq!(
            text(&dashboard, 120, 11)[1],
            "Refresh failed \u{b7} showing previous data"
        );
    }

    #[test]
    fn the_colours_are_upstreams() {
        let mut dashboard = Dashboard::open(Scope::Days30, Some(rolling(&billed(), Scope::Days30)));
        dashboard.toggle(20);
        let palette = PALETTE;
        let rows = dashboard.rows(120, 12, palette);
        // The title and the shown window bold in the hint grey, the others dim.
        assert!(
            rows[0].starts_with(&format!(
                "{}Usage{}  {}[30 days]",
                palette.subtitle(),
                palette.reset(),
                palette.subtitle()
            )),
            "{:?}",
            rows[0]
        );
        assert!(
            rows[0].contains(&format!("{}7 days", palette.dim())),
            "{:?}",
            rows[0]
        );
        // The models heading and the marked name bold in the label grey.
        assert!(
            rows[7].starts_with(palette.notice_heading()),
            "{:?}",
            rows[7]
        );
        assert!(
            rows[9].starts_with(&format!("{}\u{276f} anthropic", palette.notice_heading())),
            "{:?}",
            rows[9]
        );
        // Its numbers dim, after the name's colour is closed.
        assert!(
            rows[9].contains(&format!("{}    {}55.1K", palette.reset(), palette.dim())),
            "{:?}",
            rows[9]
        );
        // Everything else dim.
        for at in [1, 3, 4, 5, 8, 10, 11] {
            assert!(rows[at].starts_with(palette.dim()), "{at}: {:?}", rows[at]);
        }
    }

    #[test]
    fn a_narrow_screen_folds_the_overview_and_the_header() {
        let dashboard = Dashboard::open(Scope::Days30, Some(rolling(&billed(), Scope::Days30)));
        let rows = text(&dashboard, 60, 20);
        assert_eq!(rows[0], "Usage  [30 days]");
        assert_eq!(rows[3], "67.9K tokens \u{b7} $0.43 spent \u{b7} 2 requests");
        // No column header under `Models` below 110 columns: the facts sit at
        // one column after the name.
        assert_eq!(rows[7], "Models 2");
        assert_eq!(
            rows[8],
            "\u{276f} anthropic/claude-opus-5    55.1K \u{b7} 81.1% \u{b7} $0.42"
        );
        let rows = text(&dashboard, 40, 20);
        assert_eq!(rows[1], "Partial data");
        assert_eq!(rows[2], "67.9K tokens \u{b7} $0.43");
        assert_eq!(rows[3], "2 requests");
        assert_eq!(rows[4], "Models");
    }

    #[test]
    fn a_short_panel_keeps_the_mark_in_view() {
        let dashboard = Dashboard::open(Scope::Days30, Some(rolling(&billed(), Scope::Days30)));
        let rows = text(&dashboard, 120, 6);
        assert_eq!(rows.len(), 6);
        assert_eq!(rows[2], "67.9K tokens \u{b7} $0.43");
        assert_eq!(rows[3], "Models 2");
        assert!(rows[4].starts_with("\u{276f} anthropic"), "{rows:?}");
        let one = text(&dashboard, 120, 1);
        assert!(one[0].starts_with("\u{276f} anthropic"), "{one:?}");
        assert_eq!(dashboard.height(10, 120), 7);
        assert_eq!(dashboard.height(2, 120), 1);
    }

    #[test]
    fn keys_mean_upstreams_things() {
        assert_eq!(
            MenuAction::of_action(Action::Tab),
            Some(MenuAction::Cycle(true))
        );
        assert_eq!(
            MenuAction::of_action(Action::BackTab),
            Some(MenuAction::Cycle(false))
        );
        assert_eq!(
            MenuAction::of_action(Action::Left),
            Some(MenuAction::Previous)
        );
        assert_eq!(MenuAction::of_action(Action::Right), Some(MenuAction::Next));
        assert_eq!(
            MenuAction::of_action(Action::KillToEnd),
            Some(MenuAction::Move(false))
        );
        assert_eq!(
            MenuAction::of_action(Action::InsertNewline),
            Some(MenuAction::Move(true))
        );
        assert_eq!(
            MenuAction::of_action(Action::Submit),
            Some(MenuAction::Expand)
        );
        assert_eq!(
            MenuAction::of_action(Action::Escape),
            Some(MenuAction::Close)
        );
        assert_eq!(MenuAction::of_action(Action::Cancel), None);
        assert_eq!(MenuAction::of_text('r'), Some(MenuAction::Reload));
        assert_eq!(MenuAction::of_text('R'), Some(MenuAction::Reload));
        assert_eq!(MenuAction::of_text('q'), None);
        assert_eq!(key_help(120), KEY_HELP[0]);
        assert_eq!(key_help(50), KEY_HELP[1]);
        assert_eq!(key_help(20), KEY_HELP[2]);
    }
}
