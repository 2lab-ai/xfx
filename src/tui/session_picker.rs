//! The `/resume` picker: the saved sessions a prompt could continue, in the
//! band under the composer.
//!
//! Upstream's session picker (`vercel-labs/fx@c1db9191`,
//! `src/ui/footer/resume_menu_presentation.zig`, state in
//! `src/core/app/app_session_runtime.zig:440-560`), in the slot the `/help`
//! catalog uses ([`super::help_menu`]) and like it a **view of the composer**:
//! what is typed goes into the composer and is the picker's search, and only
//! the keys it binds ([`SessionAction::of`]) are taken out of the stream.
//!
//! # What it looks like
//!
//! A header -- `Sessions N` and the two scope tabs, the active one in brackets
//! -- a blank row, then one row per session: two columns of indent, the title
//! padded to the widest title **in the filtered list**, four columns of gap,
//! and `<workspace> · <age> · N turn(s)` in fixed columns, the age right-aligned
//! in its own (`composeTitleRow`, `:340-415`). The marked row is the bold hint
//! grey, title and metadata both; every other row is dim. A screen too narrow
//! to give the title twelve columns beside the metadata drops the metadata and
//! gives the title the row; a title too wide for its column loses its middle to
//! an ellipsis.
//!
//! # What it lists
//!
//! What the runtime thread read out of the session store
//! ([`crate::session::SessionStore::list`]) for both scopes, newest first,
//! without the session this one is in and without sessions that hold no turn
//! (`hasResumableContent`, `session_store_types.zig:71-73`). Read there rather
//! than here because a listing reads a manifest per session and this module is
//! on the thread that holds the terminal; the picker opens on `Loading
//! sessions…` and fills when the listing arrives ([`Self::listed`]).
//!
//! The search is upstream's (`matchesQuery`, `session_catalog.zig:79-88`): the
//! composer trimmed, a case-insensitive substring of the title or of the
//! workspace path. Upstream also searches a preview of the conversation; xfx's
//! store keeps no preview, so a word only the conversation contains finds
//! nothing here.
//!
//! # Pages
//!
//! Upstream lists a page at a time -- as many sessions as the screen has rows
//! less seven, and never fewer than ten (`resumePageLimitForRows`,
//! `app_session_runtime.zig:1188-1193`) -- with `↓ Load more` under the last
//! one while there are more (`composeLoadMoreRow`,
//! `resume_menu_presentation.zig:417-436`). Walking onto the last loaded
//! session loads the next page (`maybePrefetchMoreSessions`, `:2808-2820`), as
//! Enter on the row does (`loadMoreSessionPicker`). The search and the header's
//! count are about the loaded pages only; a scope change starts from one page
//! again. xfx's runtime reads the list whole, up to the store's listing cap
//! ([`crate::session::store::MAX_LIST_LIMIT`]), so a page is loaded here, at
//! once -- the `↓ Loading more…` upstream shows while its next page is read
//! has nothing to wait for.

use super::picker::{cells, edge_start, pad_to};
use super::theme::Palette;

/// The header row (`header_rows`, `resume_menu_presentation.zig:15`).
const HEADER_ROWS: u16 = 1;

/// The blank row under it, which a failed resume writes its sentence on
/// (`roomy_top_gap_rows`, `:16`; `feedback_row = 1`, `:153`).
const GAP_ROWS: u16 = 1;

/// The most sessions shown at once (`max_visible_items`, `:18`).
const MOST_ITEMS: usize = 20;

/// What every row starts with.
const INDENT: &str = "  ";

/// The columns between the widest title and the metadata
/// (`inline_picker_column_gap_width`, `picker_presentation.zig:17`).
const COLUMN_GAP: usize = 4;

/// The narrowest a title column may be with the metadata beside it
/// (`minimum_title_column_width`, `:20`).
const MINIMUM_TITLE_COLUMN: usize = 12;

/// What separates the metadata's three columns.
const SEPARATOR: &str = " \u{b7} ";

/// What separates the title and the tabs.
const TAB_GAP: &str = "  ";

/// What a session with no title is called (`session_display_metadata.zig:14`),
/// and what the store's own listing calls one too.
pub(crate) const UNTITLED: &str = "Untitled session";

/// What the list says while the listing is on its way, when it could not be
/// read, and when it has nothing in it (`composeStateRow`, `:438-452`).
const LOADING: &str = "Loading sessions\u{2026}";
const UNREADABLE: &str = "Unable to load sessions.";
const EMPTY: &str = "No sessions found.";

/// What a compact row with a failure under it ends with (`:483`).
const RETRY: &str = " \u{b7} retry";

/// The row under the last loaded session while there are more (`:432`).
const LOAD_MORE: &str = "\u{2193} Load more";

/// The fewest sessions a page holds (`default_resume_page_limit`,
/// `session_store.zig:423`).
const LEAST_PAGE: usize = 10;

/// The rows a page leaves to the composer, the rules, the hint row, the
/// header, the gap and the load-more row (`resumePageLimitForRows`).
const PAGE_CHROME_ROWS: u16 = 7;

const ELLIPSIS: &str = "\u{2026}";

/// What the hint row says while the picker is up, widest first
/// (`input_presentation.zig:834-840`, the `scope` variants). The first that
/// fits is shown, and the last -- cut by the painter -- when none does.
const KEY_HELP: [&str; 5] = [
    "\u{2191}\u{2193} navigate     tab scope     enter resume     esc close",
    "\u{2191}\u{2193} navigate  tab scope  enter resume  esc close",
    "\u{2191}\u{2193} move  tab scope  enter  esc",
    "enter resume  esc close",
    "enter esc",
];

/// One saved session, as the picker lists it.
///
/// The store's summary less what a row does not show, and only text: the
/// runtime builds it, and the channel it crosses makes every string of it inert
/// on the way (`super::bridge`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ListedSession {
    pub(crate) id: String,
    /// The title the store publishes for it -- a `/rename` if it had one, its
    /// first prompt if not -- or `None`.
    pub(crate) title: Option<String>,
    /// Where it is bound now: what the row names by its last component and the
    /// search reads whole.
    pub(crate) workspace: String,
    pub(crate) updated_at_ms: i64,
    pub(crate) turns: u64,
}

impl ListedSession {
    /// The title a row shows (`displayTitle`).
    fn title(&self) -> &str {
        self.title
            .as_deref()
            .filter(|title| !title.trim().is_empty())
            .unwrap_or(UNTITLED)
    }

    /// The workspace's last component (`sessionWorkspaceLabel`, `:309-311`).
    fn workspace_label(&self) -> &str {
        std::path::Path::new(&self.workspace)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(&self.workspace)
    }

    /// `1 turn`, `2 turns` (`sessionTurnsText`, `:313-319`).
    fn turns_text(&self) -> String {
        format!(
            "{} {}",
            self.turns,
            if self.turns == 1 { "turn" } else { "turns" }
        )
    }

    /// Whether the search finds it (`matchesQuery`).
    fn matches(&self, query: &str) -> bool {
        let query = query.trim_matches([' ', '\t', '\r', '\n']);
        if query.is_empty() {
            return true;
        }
        let query = query.to_ascii_lowercase();
        self.title().to_ascii_lowercase().contains(&query)
            || self.workspace.to_ascii_lowercase().contains(&query)
    }
}

/// Both scopes' sessions, and the moment they were read at.
///
/// The clock travels with the rows so that a row's age is a function of this
/// value and not of when the band happens to be painted: two frames of the
/// same listing say the same thing, and a test can say what they say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Listing {
    pub(crate) current: Vec<ListedSession>,
    pub(crate) all: Vec<ListedSession>,
    pub(crate) now_ms: i64,
}

/// Which sessions the picker is showing (`session_catalog.Scope`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Scope {
    /// Bound to the workspace this session runs in. What it opens on
    /// (`openSessionPicker`, `app_session_runtime.zig:2552-2554`).
    Current,
    /// Every session in the store.
    All,
}

/// Why a session could not be resumed, as the row under the header says it
/// (`composeSelectionFailureRow`, `:454-478`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResumeFailure {
    /// Another xfx holds its writer lock.
    OpenElsewhere,
    /// Anything else: gone, damaged, insecure, unreadable.
    Unavailable,
}

impl ResumeFailure {
    /// Upstream's sentence, with this product's name in it where upstream's
    /// names its own -- the sentence tells the user to go and close it there.
    ///
    /// Upstream's third, `This session is being updated. Wait a moment, then
    /// press enter to retry.`, has no xfx counterpart: xfx's store reads
    /// exactly the bytes a manifest publishes, so a log caught mid-write is a
    /// tail the resume ignores rather than a session it refuses.
    fn sentence(self) -> &'static str {
        match self {
            Self::OpenElsewhere => {
                "This session is open in another xfx. Close it there, then press enter to retry."
            }
            Self::Unavailable => "Unable to resume this session.",
        }
    }
}

/// Where the listing stands (`load_state`).
#[derive(Debug, Clone, PartialEq, Eq)]
enum Load {
    Loading,
    Failed,
    Ready(Listing),
}

/// What one keystroke means to the picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionAction {
    /// The previous session, stopping at the first. Up, and `C-k`
    /// (`routePickerControlByte`, `app_input_runtime.zig:1220-1235`).
    Up,
    /// The next one, stopping at the last. Down, and `C-j`.
    Down,
    /// A page down (`true`) or up, stopping at the end: PgDn and PgUp, a page
    /// being the composer's row cap (`routeComposerVertical`,
    /// `input_completion_runtime.zig:209-227`, into `moveVisibleItems`).
    Page(bool),
    /// The other scope. Tab and Shift-Tab both (`app_input_runtime.zig:1178`,
    /// `:1490`): with two scopes, either direction is the other one.
    Scope,
    /// Resume the marked session. Enter.
    Resume,
    /// Close it and empty the search (`cancelSessionMenu`,
    /// `app_input_runtime.zig:3300-3307`). Escape.
    Close,
}

impl SessionAction {
    /// What an ordinary decoded action means to the picker, and `None` for the
    /// keystrokes that go on to the composer.
    pub(crate) fn of(action: super::input::Action) -> Option<Self> {
        use super::input::Action;
        match action {
            Action::Up | Action::ControlK => Some(Self::Up),
            Action::Down | Action::ControlJ => Some(Self::Down),
            Action::PageUp => Some(Self::Page(false)),
            Action::PageDown => Some(Self::Page(true)),
            Action::Tab | Action::BackTab => Some(Self::Scope),
            Action::Submit => Some(Self::Resume),
            Action::Escape => Some(Self::Close),
            _ => None,
        }
    }
}

/// How many sessions a page holds on a screen `terminal_rows` tall
/// (`resumePageLimitForRows`, `app_session_runtime.zig:1188-1193`).
pub(crate) fn page_for(terminal_rows: u16) -> usize {
    usize::from(terminal_rows.saturating_sub(PAGE_CHROME_ROWS)).max(LEAST_PAGE)
}

/// The picker's shape for one row budget (`SessionMenuLayout`, `:24-196`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Layout {
    rows: u16,
    header: bool,
    /// The row the first session is on, counted from the picker's top.
    first_row: u16,
    /// How many rows the mark moves over in view: the sessions, and the
    /// load-more row when there is one (`visible_items`).
    visible: usize,
    /// How many sessions are in view (`visible_session_items`).
    sessions: usize,
    /// The row `↓ Load more` is on, while there is a page to load.
    load_more_row: Option<u16>,
    /// The row a failed resume is said on, when it is said on a row of its own.
    feedback_row: Option<u16>,
    /// Whether it is said at the end of the marked row instead.
    feedback_inline: bool,
}

/// The picker: which scope is up, which session is marked, and what the search
/// is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionPicker {
    scope: Scope,
    load: Load,
    /// An index into the filtered list.
    selected: usize,
    /// The first session in view, as the last move left it.
    window_start: usize,
    /// The composer's text.
    query: String,
    /// Why the last Enter did not resume, and which session it was about.
    failure: Option<(String, ResumeFailure)>,
    /// The session an Enter has asked the runtime for, until it answers. A
    /// second Enter meanwhile asks for nothing.
    pending: Option<String>,
    /// How many sessions a page holds ([`page_for`]).
    page: usize,
    /// How many of the scope's sessions are loaded: whole pages.
    loaded: usize,
}

impl SessionPicker {
    /// A picker on the current workspace, waiting for its listing, searching
    /// for `query` (`openSessionPickerWithScope`: the query is seeded from the
    /// composer), in pages of `page` sessions ([`page_for`]).
    pub(crate) fn open(query: &str, page: usize) -> Self {
        let page = page.max(1);
        Self {
            scope: Scope::Current,
            load: Load::Loading,
            selected: 0,
            window_start: 0,
            query: query.to_string(),
            failure: None,
            pending: None,
            page,
            loaded: page,
        }
    }

    /// The listing the runtime read, or `None` when it could not read one: its
    /// first page.
    pub(crate) fn listed(&mut self, listing: Option<Listing>) {
        self.load = listing.map_or(Load::Failed, Load::Ready);
        self.loaded = self.page;
        self.reset();
    }

    /// Takes the composer's text as the search. A search that changed puts the
    /// mark back on the first session and forgets a failure that was about the
    /// old one (`setQuery`, `app_session_runtime.zig:513-520`).
    pub(crate) fn set_query(&mut self, query: &str) {
        if self.query != query {
            self.query = query.to_string();
            self.reset();
        }
    }

    fn reset(&mut self) {
        self.selected = 0;
        self.window_start = 0;
        self.failure = None;
    }

    /// The other scope, from the top and from its first page
    /// (`toggleSessionPickerScope`, which re-opens the picker on it).
    pub(crate) fn toggle_scope(&mut self) {
        self.scope = match self.scope {
            Scope::Current => Scope::All,
            Scope::All => Scope::Current,
        };
        self.loaded = self.page;
        self.reset();
    }

    /// Moves the mark one row, **stopping** at either end rather than
    /// wrapping (`moveVisibleItems`, `:538-552`) -- the load-more row is the
    /// last one while there is a page to load -- and scrolls a window of
    /// `visible` rows only as far as keeps it in view. A mark that reaches the
    /// last loaded session loads the next page (`maybePrefetchMoreSessions`).
    /// `false` when there is no listing to move in.
    pub(crate) fn step(&mut self, down: bool, visible: u16) -> bool {
        self.step_by(if down { 1 } else { -1 }, visible)
    }

    /// Moves the mark `delta` rows, stopping at either end, as
    /// [`Self::step`] does one -- and loads the next page as it does.
    pub(crate) fn step_by(&mut self, delta: i64, visible: u16) -> bool {
        let Load::Ready(_) = &self.load else {
            return false;
        };
        self.failure = None;
        let rows = self.items().len() + usize::from(self.more());
        if rows == 0 {
            return true;
        }
        let current = i64::try_from(self.selected.min(rows - 1)).unwrap_or(i64::MAX);
        let last = i64::try_from(rows - 1).unwrap_or(i64::MAX);
        self.selected = usize::try_from(current.saturating_add(delta).clamp(0, last)).unwrap_or(0);
        self.follow(visible);
        if self.more() && self.selected + 1 >= self.items().len() {
            self.load_more(visible);
        }
        true
    }

    /// Whether the mark is on `↓ Load more` (`isLoadMoreSelected`).
    pub(crate) fn on_load_more(&self) -> bool {
        matches!(self.load, Load::Ready(_)) && self.more() && self.selected == self.items().len()
    }

    /// Loads the next page (`scheduleMoreSessions` and
    /// `applySessionPickerCatalogPage`, `app_session_runtime.zig:2784-2806`,
    /// `:708-783`). A mark that was on the load-more row moves to the first
    /// session the page added, or stays on the row -- or on the last session
    /// once there is none -- when the search finds none of them
    /// (`selectionAfterLoadedPage`, `:785-793`).
    pub(crate) fn load_more(&mut self, visible: u16) {
        let before = self.items().len();
        let on_the_row = self.selected == before;
        self.loaded = self.loaded.saturating_add(self.page);
        if on_the_row {
            let now = self.items().len();
            self.selected = if now > before {
                before
            } else if self.more() {
                now
            } else {
                now.saturating_sub(1)
            };
        }
        self.follow(visible);
    }

    /// Keeps the marked row in a window of `visible` rows
    /// (`syncWindowStart`, `:554-570`): the load-more row is one of them, and
    /// with the mark on it the window ends on the last loaded session.
    fn follow(&mut self, visible: u16) {
        let count = self.items().len();
        let sessions = usize::from(visible).saturating_sub(usize::from(self.more() && visible > 0));
        self.window_start = if count == 0 || sessions == 0 {
            0
        } else if self.selected >= count {
            count.saturating_sub(sessions)
        } else {
            edge_start(
                self.window_start,
                count,
                self.selected,
                u16::try_from(sessions).unwrap_or(u16::MAX),
            )
        };
    }

    /// Whether the scope has sessions past the pages loaded.
    fn more(&self) -> bool {
        self.scope_sessions().len() > self.loaded
    }

    /// The marked session's id, when there is a listed one to resume and no
    /// resume is already on its way (`selectedId`, `:499-504`).
    pub(crate) fn selected(&self) -> Option<String> {
        if self.pending.is_some() {
            return None;
        }
        let items = self.items();
        items.get(self.selected).map(|session| session.id.clone())
    }

    /// Records that the runtime has been asked to resume `id`.
    pub(crate) fn resuming(&mut self, id: String) {
        self.failure = None;
        self.pending = Some(id);
    }

    /// The runtime could not resume `id`: the row under the header says why,
    /// and Enter may ask again.
    pub(crate) fn failed(&mut self, id: String, failure: ResumeFailure) {
        self.pending = None;
        self.failure = Some((id, failure));
    }

    /// The scope's sessions, every page.
    fn scope_sessions(&self) -> &[ListedSession] {
        let Load::Ready(listing) = &self.load else {
            return &[];
        };
        match self.scope {
            Scope::Current => &listing.current,
            Scope::All => &listing.all,
        }
    }

    /// What the picker lists: the loaded sessions the search finds.
    fn items(&self) -> Vec<&ListedSession> {
        self.scope_sessions()
            .iter()
            .take(self.loaded)
            .filter(|session| session.matches(&self.query))
            .collect()
    }

    fn now_ms(&self) -> i64 {
        match &self.load {
            Load::Ready(listing) => listing.now_ms,
            _ => 0,
        }
    }

    /// The picker's shape in `budget` rows for `count` sessions, with the
    /// load-more row under them when `more` (`SessionMenuLayout.build`).
    fn layout(&self, count: usize, more: bool, budget: u16) -> Layout {
        if budget == 0 {
            return Layout::default();
        }
        let failed = self.failure.is_some();
        let rows = count + usize::from(more);
        if !matches!(self.load, Load::Ready(_)) || rows == 0 {
            // A header, the gap, and the one row that says why there is
            // nothing to list -- the last row of whatever was given.
            return Layout {
                rows: budget.min(HEADER_ROWS + GAP_ROWS + 1),
                header: budget > 1,
                ..Layout::default()
            };
        }
        if budget == 1 {
            return Layout {
                rows: 1,
                visible: 1,
                sessions: usize::from(count > 0),
                ..Layout::default()
            };
        }
        if failed && budget == 2 {
            return Layout {
                rows: 2,
                header: true,
                first_row: 1,
                visible: 1,
                sessions: 1,
                feedback_inline: true,
                ..Layout::default()
            };
        }
        if failed && budget == 3 {
            return Layout {
                rows: 3,
                header: true,
                first_row: 1,
                visible: 1,
                sessions: 1,
                feedback_row: Some(2),
                ..Layout::default()
            };
        }
        if failed && more && budget <= 5 {
            return Layout {
                rows: budget,
                header: true,
                first_row: 2,
                visible: 2,
                sessions: 1,
                load_more_row: Some(budget - 1),
                feedback_row: Some(1),
                feedback_inline: false,
            };
        }
        if more {
            return self.paged(count, budget, failed);
        }
        if budget == 2 {
            return Layout {
                rows: 2,
                header: true,
                first_row: 1,
                visible: 1,
                sessions: 1,
                ..Layout::default()
            };
        }
        let first_row = HEADER_ROWS + GAP_ROWS;
        let room = usize::from(budget - first_row);
        let visible = count.min(room.max(1)).min(MOST_ITEMS);
        let shown = u16::try_from(visible).unwrap_or(u16::MAX);
        Layout {
            rows: first_row + shown,
            header: true,
            first_row,
            visible,
            sessions: visible,
            load_more_row: None,
            feedback_row: failed.then_some(1),
            feedback_inline: false,
        }
    }

    /// The shape with a page still to load (`buildPaged`, `:155-192`): the
    /// sessions that fit above one last row, and `↓ Load more` on it.
    fn paged(&self, count: usize, budget: u16, failed: bool) -> Layout {
        if budget == 2 {
            // One row under the header: the session, or the load-more row
            // when that is what is marked.
            let on_the_row = self.selected >= count;
            return Layout {
                rows: 2,
                header: true,
                first_row: 1,
                visible: 1,
                sessions: usize::from(!on_the_row),
                load_more_row: on_the_row.then_some(1),
                feedback_row: None,
                feedback_inline: false,
            };
        }
        let first_row = HEADER_ROWS + GAP_ROWS;
        let room = usize::from(budget.saturating_sub(first_row).saturating_sub(1));
        let sessions = count.min(room).min(MOST_ITEMS - 1);
        let load_more_row = first_row + u16::try_from(sessions).unwrap_or(u16::MAX);
        Layout {
            rows: budget.min(load_more_row + 1),
            header: true,
            first_row,
            visible: sessions + 1,
            sessions,
            load_more_row: Some(load_more_row),
            feedback_row: failed.then_some(1),
            feedback_inline: false,
        }
    }

    /// How many rows the band has to give it on a screen `terminal_rows` tall
    /// whose composer is `input_extra` rows taller than one (`menuRowCount`).
    pub(crate) fn height(&self, terminal_rows: u16, input_extra: u16) -> u16 {
        self.layout(
            self.items().len(),
            self.more(),
            super::help_menu::row_budget(terminal_rows, input_extra),
        )
        .rows
    }

    /// How many rows the mark moves over in view in `max_rows` rows -- the
    /// sessions, and the load-more row -- and never fewer than one: the
    /// window a move scrolls against (`visibleNavigationItemsForBudget`).
    pub(crate) fn visible_items(&self, max_rows: u16) -> u16 {
        let visible = self
            .layout(self.items().len(), self.more(), max_rows)
            .visible;
        u16::try_from(visible.max(1)).unwrap_or(u16::MAX)
    }

    /// The picker's rows in `max_rows` rows of a `cols`-wide screen, top first
    /// (`composeSessionMenuRow`, `:206-254`).
    pub(crate) fn rows(&self, cols: u16, max_rows: u16, palette: Palette) -> Vec<String> {
        let items = self.items();
        let more = self.more();
        let layout = self.layout(items.len(), more, max_rows);
        let mut rows = vec![String::new(); usize::from(layout.rows)];
        if layout.rows == 0 {
            return rows;
        }
        if layout.header {
            rows[0] = self.header(items.len(), cols, palette);
        }
        if !matches!(self.load, Load::Ready(_)) || (items.is_empty() && !more) {
            let last = usize::from(layout.rows) - 1;
            if !(layout.header && last == 0) {
                rows[last] = state_row(self.state_message(), cols, palette);
            }
            return rows;
        }
        if let (Some(row), Some((_, failure))) = (layout.feedback_row, self.failure.as_ref()) {
            rows[usize::from(row)] = failure_row(*failure, cols, palette);
        }
        let navigation = items.len() + usize::from(more);
        let selected = self.selected.min(navigation - 1);
        if let Some(row) = layout.load_more_row.filter(|row| *row < layout.rows) {
            rows[usize::from(row)] = load_more_row(selected == items.len(), cols, palette);
        }
        if items.is_empty() || layout.sessions == 0 {
            return rows;
        }
        let columns = Columns::of(&items, self.now_ms());
        // `sessionWindowStart`, `:256-267`.
        let start = if selected >= items.len() {
            items.len().saturating_sub(layout.sessions)
        } else if items.len() <= layout.sessions {
            0
        } else {
            edge_start(
                self.window_start,
                items.len(),
                selected,
                u16::try_from(layout.sessions).unwrap_or(u16::MAX),
            )
        };
        for offset in 0..layout.sessions {
            let index = start + offset;
            let Some(session) = items.get(index) else {
                break;
            };
            let marked = index == selected;
            let row = if layout.feedback_inline {
                let budget = cols.saturating_sub(u16::try_from(cells(RETRY)).unwrap_or(0));
                let mut row = title_row(session, marked, self.now_ms(), &columns, budget, palette);
                row.push_str(palette.notice_label());
                row.push_str(super::frame::clip(
                    RETRY,
                    cols.min(u16::try_from(cells(RETRY)).unwrap_or(0)),
                ));
                row.push_str(palette.reset());
                row
            } else {
                title_row(session, marked, self.now_ms(), &columns, cols, palette)
            };
            rows[usize::from(layout.first_row) + offset] = row;
        }
        rows
    }

    /// What the picker says when it has nothing to list.
    fn state_message(&self) -> &'static str {
        match self.load {
            Load::Loading => LOADING,
            Load::Failed => UNREADABLE,
            Load::Ready(_) => EMPTY,
        }
    }

    /// `Sessions N` and the two scope tabs, or the title and the active tab
    /// alone when both do not fit (`composeHeaderRow`, `:268-305`).
    fn header(&self, count: usize, cols: u16, palette: Palette) -> String {
        let title = format!("{}Sessions {count}{}", palette.subtitle(), palette.reset());
        let tab = |scope: Scope| {
            let label = match scope {
                Scope::Current => "Current workspace",
                Scope::All => "All workspaces",
            };
            if scope == self.scope {
                format!("{}[{label}]{}", palette.subtitle(), palette.reset())
            } else {
                format!("{}{label}{}", palette.dim(), palette.reset())
            }
        };
        let wide = format!(
            "{title}{TAB_GAP}{}{TAB_GAP}{}",
            tab(Scope::Current),
            tab(Scope::All)
        );
        let row = if cells(&wide) <= usize::from(cols) {
            wide
        } else {
            format!("{title}{TAB_GAP}{}", tab(self.scope))
        };
        clipped(&row, cols, palette)
    }
}

/// `row` in at most `cols` cells, closed even when the cut fell inside a
/// colour: the painter stops a row at the screen's edge, and a reset lost
/// behind that edge would leave the colour open on the rows under the band.
fn clipped(row: &str, cols: u16, palette: Palette) -> String {
    if cells(row) <= usize::from(cols) {
        return row.to_string();
    }
    format!("{}{}", super::frame::clip(row, cols), palette.reset())
}

/// The metadata's column widths, measured over **every** session the search
/// finds rather than over the ones in view, so moving the mark never moves a
/// column (`matchingColumns`, `:321-338`).
#[derive(Debug, Default)]
struct Columns {
    title: usize,
    workspace: usize,
    age: usize,
    turns: usize,
}

impl Columns {
    fn of(items: &[&ListedSession], now_ms: i64) -> Self {
        let mut columns = Self::default();
        for session in items {
            columns.title = columns.title.max(cells(&one_line(session.title())));
            columns.workspace = columns.workspace.max(cells(session.workspace_label()));
            columns.age = columns.age.max(cells(&age(session.updated_at_ms, now_ms)));
            columns.turns = columns.turns.max(cells(&session.turns_text()));
        }
        columns
    }
}

/// One session's row (`composeTitleRow`, `:340-415`).
fn title_row(
    session: &ListedSession,
    marked: bool,
    now_ms: i64,
    columns: &Columns,
    cols: u16,
    palette: Palette,
) -> String {
    let width = usize::from(cols);
    let mut row = String::new();
    if width > 4 {
        row.push_str(INDENT);
    }
    let prefix = cells(&row);
    let title = one_line(session.title());
    let age = age(session.updated_at_ms, now_ms);
    let workspace = session.workspace_label();
    let turns = session.turns_text();
    let workspace_col = columns.workspace.max(cells(workspace));
    let age_col = columns.age.max(cells(&age));
    let turns_col = columns.turns.max(cells(&turns));
    let metadata = workspace_col + cells(SEPARATOR) + age_col + cells(SEPARATOR) + turns_col;
    // One cell short of the edge, as upstream's content width is.
    let content = width.saturating_sub(1);
    let available = content.saturating_sub(prefix + COLUMN_GAP + metadata);
    let show_metadata = metadata > 0 && available >= MINIMUM_TITLE_COLUMN;
    let measured = columns.title.max(cells(&title));
    let title_col = if show_metadata {
        measured.min(available)
    } else {
        width.saturating_sub(prefix)
    };
    let style = if marked {
        palette.subtitle()
    } else {
        palette.dim()
    };
    row.push_str(style);
    row.push_str(&middle_ellipsized(&title, title_col));
    row.push_str(palette.reset());
    if show_metadata {
        pad_to(&mut row, prefix + title_col + COLUMN_GAP);
        row.push_str(style);
        row.push_str(workspace);
        row.push_str(&" ".repeat(workspace_col - cells(workspace)));
        row.push_str(SEPARATOR);
        row.push_str(&" ".repeat(age_col - cells(&age)));
        row.push_str(&age);
        row.push_str(SEPARATOR);
        row.push_str(&turns);
        row.push_str(palette.reset());
    }
    row
}

/// `↓ Load more`, in the marked row's style when it is marked
/// (`composeLoadMoreRow`, `:417-436`).
fn load_more_row(marked: bool, cols: u16, palette: Palette) -> String {
    let width = usize::from(cols);
    let indent = if width <= 4 { 0 } else { INDENT.len() };
    let mut row = INDENT[..indent].to_string();
    row.push_str(if marked {
        palette.subtitle()
    } else {
        palette.dim()
    });
    row.push_str(&super::picker::ellipsized(
        LOAD_MORE,
        width.saturating_sub(indent),
    ));
    row.push_str(palette.reset());
    row
}

/// The row that says why there is nothing to list (`composeStateRow`).
fn state_row(message: &str, cols: u16, palette: Palette) -> String {
    let mut row = String::new();
    if cols > 4 {
        row.push_str(INDENT);
    }
    row.push_str(palette.dim());
    row.push_str(super::frame::clip(message, cols.saturating_sub(2)));
    row.push_str(palette.reset());
    row
}

/// The row that says why the last Enter did not resume, in upstream's
/// `red_style` -- the outcome label's grey in either theme
/// (`theme.zig:88,131`; [`Palette::notice_label`]).
fn failure_row(failure: ResumeFailure, cols: u16, palette: Palette) -> String {
    let width = usize::from(cols);
    let mut row = String::new();
    let indent = if width > 4 { INDENT.len() } else { 0 };
    row.push_str(&INDENT[..indent]);
    row.push_str(palette.notice_label());
    row.push_str(&super::picker::ellipsized(
        failure.sentence(),
        width.saturating_sub(indent),
    ));
    row.push_str(palette.reset());
    row
}

/// A title on one row: a line break is a space (`appendProjectedSingleLine`,
/// `row_text.zig:81-85`).
fn one_line(text: &str) -> String {
    text.replace(['\n', '\r'], " ")
}

/// `text` in at most `budget` cells, its middle swapped for an ellipsis when
/// it does not fit -- the head gets the odd cell
/// (`appendSingleLineMiddleEllipsized`, `row_text.zig:64-108`).
fn middle_ellipsized(text: &str, budget: usize) -> String {
    if budget == 0 {
        return String::new();
    }
    if cells(text) <= budget {
        return text.to_string();
    }
    if budget == 1 {
        return ELLIPSIS.to_string();
    }
    let content = budget - 1;
    let head = content.div_ceil(2);
    let tail = content - head;
    let head = super::frame::clip(text, u16::try_from(head).unwrap_or(u16::MAX));
    format!("{head}{ELLIPSIS}{}", suffix_by_width(text, tail))
}

/// The longest tail of `text` that is at most `budget` cells wide, cut only
/// between grapheme clusters.
fn suffix_by_width(text: &str, budget: usize) -> &str {
    let mut starts: Vec<(usize, usize)> = Vec::new();
    let mut at = 0;
    while at < text.len() {
        let Some(cluster) = super::wrap::first_cluster(&text[at..]) else {
            break;
        };
        starts.push((at, cells(cluster)));
        at += cluster.len();
    }
    let mut used = 0;
    let mut start = text.len();
    for (offset, width) in starts.into_iter().rev() {
        if used + width > budget {
            break;
        }
        used += width;
        start = offset;
    }
    &text[start..]
}

/// How long ago a session was last written, in the picker's compact words:
/// `now`, `5m`, `2h`, `1d` (`relativeActivityAgeCompact`,
/// `session_catalog.zig:67-77`).
fn age(updated_at_ms: i64, now_ms: i64) -> String {
    const MINUTE: i64 = 60_000;
    const HOUR: i64 = 60 * MINUTE;
    const DAY: i64 = 24 * HOUR;
    let delta = now_ms.saturating_sub(updated_at_ms).max(0);
    if delta < MINUTE {
        "now".to_string()
    } else if delta < HOUR {
        format!("{}m", delta / MINUTE)
    } else if delta < DAY {
        format!("{}h", delta / HOUR)
    } else {
        format!("{}d", delta / DAY)
    }
}

/// What the hint row says while the picker is up, on a screen `cols` wide.
pub(crate) fn key_help(cols: u16) -> &'static str {
    KEY_HELP
        .iter()
        .copied()
        .find(|candidate| cells(candidate) <= usize::from(cols))
        .unwrap_or(KEY_HELP[KEY_HELP.len() - 1])
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::super::theme::{Depth, Mode};

    const PALETTE: Palette = Palette {
        mode: Mode::Dark,
        depth: Depth::Ansi256,
    };

    /// Upstream's `selected_completion_style` and `dim_style`
    /// (`core/shared/theme.zig:97`, `:85`), and the reset.
    const BRIGHT: &str = "\u{1b}[1;38;5;255m";
    const DIM: &str = "\u{1b}[38;5;245m";
    /// Upstream's `red_style` on a dark screen (`theme.zig:88`).
    const RED: &str = "\u{1b}[38;5;252m";
    const RESET: &str = "\u{1b}[0m";

    const NOW: i64 = 1_800_000_000_000;

    fn session(id: &str, title: Option<&str>, workspace: &str, ago_ms: i64) -> ListedSession {
        ListedSession {
            id: id.to_string(),
            title: title.map(str::to_string),
            workspace: workspace.to_string(),
            updated_at_ms: NOW - ago_ms,
            turns: 1,
        }
    }

    /// The research capture's three sessions (`fx-picker-40x120-list3`),
    /// newest first, with an older one in another workspace for `All`.
    fn three() -> Listing {
        let current = vec![
            session("c", Some("third question gamma"), "/x/work", 0),
            session("b", Some("second question beta"), "/x/work", 0),
            session(
                "a",
                Some("Renamed alpha session with a much longer title than the others"),
                "/x/work",
                0,
            ),
        ];
        let mut all = current.clone();
        all.push(ListedSession {
            turns: 3,
            ..session("o", Some("elsewhere"), "/y/other", 2 * 3_600_000)
        });
        Listing {
            current,
            all,
            now_ms: NOW,
        }
    }

    fn ready(listing: Listing) -> SessionPicker {
        let mut picker = SessionPicker::open("", page_for(40));
        picker.listed(Some(listing));
        picker
    }

    fn plain(row: &str) -> String {
        let mut out = String::new();
        let mut rest = row;
        while !rest.is_empty() {
            if let Some(len) = super::super::pacer::escape_at(rest) {
                rest = &rest[len..];
                continue;
            }
            let mut characters = rest.chars();
            out.extend(characters.next());
            rest = characters.as_str();
        }
        out
    }

    /// `n` sessions in the current workspace, `question 1` the newest.
    fn many(n: usize) -> Listing {
        let current: Vec<ListedSession> = (1..=n)
            .map(|i| {
                session(
                    &format!("s{i}"),
                    Some(&format!("question {i}")),
                    "/x/work",
                    0,
                )
            })
            .collect();
        Listing {
            all: current.clone(),
            current,
            now_ms: NOW,
        }
    }

    fn texts(picker: &SessionPicker, rows: u16) -> Vec<String> {
        picker
            .rows(80, rows, PALETTE)
            .iter()
            .map(|row| plain(row))
            .collect()
    }

    #[test]
    fn a_page_is_the_screen_less_seven_rows_and_never_fewer_than_ten() {
        // `resumePageLimitForRows`, `app_session_runtime.zig:1188-1193`.
        assert_eq!(page_for(24), 17);
        assert_eq!(page_for(40), 33);
        assert_eq!(page_for(14), 10);
        assert_eq!(page_for(0), 10);
    }

    #[test]
    fn more_sessions_than_a_page_end_on_load_more_and_reaching_the_last_loads_the_next() {
        // `fx-resume-paging-24x80`: eighteen sessions, pages of seventeen, a
        // band of fourteen rows -- the header, the gap, eleven sessions and
        // `↓ Load more`.
        let mut picker = SessionPicker::open("", page_for(24));
        picker.listed(Some(many(18)));
        let rows = texts(&picker, 14);
        assert_eq!(rows.len(), 14);
        assert!(
            rows[0].starts_with("Sessions 17  [Current workspace]"),
            "{rows:?}"
        );
        assert!(rows[2].starts_with("  question 1 "), "{rows:?}");
        assert!(rows[12].starts_with("  question 11 "), "{rows:?}");
        assert_eq!(rows[13], "  \u{2193} Load more");
        // Dim until marked, and in the marked row's style once it is.
        assert_eq!(
            picker.rows(80, 14, PALETTE)[13],
            format!("  {DIM}\u{2193} Load more{RESET}")
        );

        // Down fourteen: the window follows, the load-more row stays last.
        let visible = picker.visible_items(14);
        assert_eq!(visible, 12);
        for _ in 0..14 {
            picker.step(true, visible);
        }
        let rows = texts(&picker, 14);
        assert!(rows[2].starts_with("  question 5 "), "{rows:?}");
        assert!(rows[12].starts_with("  question 15 "), "{rows:?}");
        assert_eq!(rows[13], "  \u{2193} Load more");

        // One more reaches the last loaded session: the next page loads, the
        // count says so and the load-more row is gone.
        picker.step(true, visible);
        picker.step(true, visible);
        // The window stays where the mark had it and gains the row the
        // load-more row gave up (`fx-resume-paging-24x80-down16-loaded`).
        let rows = texts(&picker, 14);
        assert!(rows[0].starts_with("Sessions 18 "), "{rows:?}");
        assert_eq!(rows.len(), 14, "{rows:?}");
        assert!(rows[2].starts_with("  question 7 "), "{rows:?}");
        assert!(rows[13].starts_with("  question 18 "), "{rows:?}");
        // And it stops on the last.
        picker.step(true, visible);
        picker.step(true, visible);
        assert_eq!(picker.selected().as_deref(), Some("s18"));
    }

    #[test]
    fn the_search_and_a_scope_change_keep_to_the_pages_loaded() {
        // `fx-resume-paging-24x80-query-ve`: five of the seventeen loaded
        // match, and the load-more row stays under them.
        let mut picker = SessionPicker::open("", page_for(24));
        let mut listing = many(18);
        for (session, word) in listing.current.iter_mut().zip([
            "seventeen",
            "twelve",
            "eleven",
            "seven",
            "five",
            "a",
            "b",
            "c",
            "d",
            "e",
            "f",
            "g",
            "h",
            "i",
            "j",
            "k",
            "l",
            "eighteen-ve",
        ]) {
            session.title = Some(format!("question {word}"));
        }
        picker.listed(Some(listing));
        picker.set_query("ve");
        let rows = texts(&picker, 14);
        assert!(rows[0].starts_with("Sessions 5 "), "{rows:?}");
        assert_eq!(rows.len(), 8, "{rows:?}");
        assert_eq!(rows[7], "  \u{2193} Load more");
        // The scope's other tab starts from one page again.
        picker.set_query("");
        let visible = picker.visible_items(14);
        for _ in 0..17 {
            picker.step(true, visible);
        }
        assert!(texts(&picker, 14)[0].starts_with("Sessions 18 "));
        picker.toggle_scope();
        assert!(texts(&picker, 14)[0].starts_with("Sessions 17 "));
    }

    #[test]
    fn enter_on_load_more_loads_the_page_and_marks_its_first_session() {
        // The mark only rests on the row when a page the walk loaded held
        // nothing the search finds; Enter there loads the next and marks the
        // first session it added (`selectionAfterLoadedPage`,
        // `app_session_runtime.zig:785-793`).
        let mut picker = SessionPicker::open("alpha", 1);
        let mut listing = many(3);
        for (session, title) in
            listing
                .current
                .iter_mut()
                .zip(["alpha one", "beta two", "alpha three"])
        {
            session.title = Some(title.to_string());
        }
        picker.listed(Some(listing));
        let visible = picker.visible_items(14);
        picker.step(true, visible);
        assert!(picker.on_load_more());
        assert_eq!(texts(&picker, 14)[3], "  \u{2193} Load more");
        assert_eq!(
            picker.selected(),
            None,
            "the row is not a session to resume"
        );
        assert_eq!(
            picker.rows(80, 14, PALETTE)[3],
            format!("  {BRIGHT}\u{2193} Load more{RESET}")
        );
        picker.load_more(visible);
        assert_eq!(picker.selected().as_deref(), Some("s3"));
        assert!(!picker.on_load_more());
    }

    #[test]
    fn three_sessions_are_upstreams_rows_text_and_colour() {
        // `fx-picker-40x120-list3.ansi`, rows 5-9: the header, the gap, and
        // three rows whose metadata starts four columns past the widest title.
        let picker = ready(three());
        let rows = picker.rows(120, 22, PALETTE);
        assert_eq!(rows.len(), 5);
        assert_eq!(
            rows[0],
            format!(
                "{BRIGHT}Sessions 3{RESET}  {BRIGHT}[Current workspace]{RESET}  {DIM}All workspaces{RESET}"
            )
        );
        assert_eq!(rows[1], "");
        let gap = |title: &str| " ".repeat(62 + 4 - title.len());
        assert_eq!(
            rows[2],
            format!(
                "  {BRIGHT}third question gamma{RESET}{}{BRIGHT}work \u{b7} now \u{b7} 1 turn{RESET}",
                gap("third question gamma")
            )
        );
        assert_eq!(
            rows[3],
            format!(
                "  {DIM}second question beta{RESET}{}{DIM}work \u{b7} now \u{b7} 1 turn{RESET}",
                gap("second question beta")
            )
        );
        assert_eq!(
            plain(&rows[4]),
            "  Renamed alpha session with a much longer title than the others    work \u{b7} now \u{b7} 1 turn"
        );
        assert_eq!(picker.height(40, 0), 5);
    }

    #[test]
    fn the_mark_stops_at_both_ends_rather_than_wrapping() {
        // `list3-down2` and `list3-down3-wrap` are the same screen.
        let mut picker = ready(three());
        let marked = |picker: &SessionPicker| {
            picker
                .rows(120, 22, PALETTE)
                .iter()
                .position(|row| row.starts_with(&format!("  {BRIGHT}")))
        };
        assert_eq!(marked(&picker), Some(2));
        assert!(picker.step(true, 20));
        assert_eq!(marked(&picker), Some(3));
        picker.step(true, 20);
        picker.step(true, 20);
        assert_eq!(marked(&picker), Some(4), "Down wrapped off the last row");
        assert_eq!(picker.selected().as_deref(), Some("a"));
        picker.step(false, 20);
        picker.step(false, 20);
        picker.step(false, 20);
        assert_eq!(marked(&picker), Some(2), "Up wrapped off the first row");
        assert_eq!(picker.selected().as_deref(), Some("c"));
    }

    #[test]
    fn a_page_moves_the_mark_that_far_and_stops_at_the_end() {
        // Upstream's page delta is the composer's row cap, clamped by
        // `moveVisibleItems` as a single step is.
        let mut picker = ready(three());
        assert_eq!(picker.selected().as_deref(), Some("c"));
        assert!(picker.step_by(10, 20));
        assert_eq!(
            picker.selected().as_deref(),
            Some("a"),
            "stopped at the last"
        );
        assert!(picker.step_by(-10, 20));
        assert_eq!(
            picker.selected().as_deref(),
            Some("c"),
            "stopped at the first"
        );
        assert!(picker.step_by(2, 20));
        assert_eq!(picker.selected().as_deref(), Some("a"));
    }

    #[test]
    fn tab_shows_every_workspace_and_back_from_the_top() {
        let mut picker = ready(three());
        picker.step(true, 20);
        picker.toggle_scope();
        let rows = picker.rows(120, 22, PALETTE);
        assert_eq!(
            rows[0],
            format!(
                "{BRIGHT}Sessions 4{RESET}  {DIM}Current workspace{RESET}  {BRIGHT}[All workspaces]{RESET}"
            )
        );
        // The mark went back to the top, and the columns grew for the session
        // in the other workspace: a wider workspace, an age right-aligned in
        // its column, a plural.
        assert_eq!(picker.selected().as_deref(), Some("c"));
        let title = |text: &str| " ".repeat(62 + 4 - text.len());
        assert_eq!(
            plain(&rows[2]),
            format!(
                "  third question gamma{}work  \u{b7} now \u{b7} 1 turn",
                title("third question gamma")
            )
        );
        assert_eq!(
            plain(&rows[5]),
            format!(
                "  elsewhere{}other \u{b7}  2h \u{b7} 3 turns",
                title("elsewhere")
            )
        );
        picker.toggle_scope();
        assert!(picker.rows(120, 22, PALETTE)[0].contains("[Current workspace]"));
    }

    #[test]
    fn typing_filters_by_title_or_workspace_path_and_starts_from_the_top() {
        // `list3-query-beta`.
        let mut picker = ready(three());
        picker.step(true, 20);
        picker.set_query("beta");
        let rows = picker.rows(120, 22, PALETTE);
        assert!(rows[0].contains("Sessions 1"), "{rows:?}");
        assert_eq!(rows.len(), 3);
        // One row: the title column is that row's own width now.
        assert_eq!(
            plain(&rows[2]),
            "  second question beta    work \u{b7} now \u{b7} 1 turn"
        );
        assert_eq!(picker.selected().as_deref(), Some("b"));
        // A path is searched whole, case-insensitively, and with the blanks
        // around the search trimmed.
        picker.toggle_scope();
        picker.set_query("  Y/OTHER ");
        assert_eq!(picker.selected().as_deref(), Some("o"));
        // xfx keeps no preview, so the answer's words find nothing.
        picker.set_query("ALPHA answer");
        assert_eq!(picker.selected(), None);
    }

    #[test]
    fn nothing_to_list_is_one_dim_row_and_enter_has_nothing_to_resume() {
        // `fx-busy-40x120-resume-empty.ansi`: the header, the gap, and the
        // sentence two columns in.
        let picker = ready(Listing {
            current: Vec::new(),
            all: Vec::new(),
            now_ms: NOW,
        });
        let rows = picker.rows(120, 22, PALETTE);
        assert_eq!(
            rows,
            vec![
                format!(
                    "{BRIGHT}Sessions 0{RESET}  {BRIGHT}[Current workspace]{RESET}  {DIM}All workspaces{RESET}"
                ),
                String::new(),
                format!("  {DIM}No sessions found.{RESET}"),
            ]
        );
        assert_eq!(picker.selected(), None);
        let mut moved = picker.clone();
        assert!(moved.step(true, 20));
        assert_eq!(moved, picker, "a move in an empty list changed something");
        // A search that finds nothing says the same.
        let mut searched = ready(three());
        searched.set_query("zzz");
        assert_eq!(
            plain(&searched.rows(120, 22, PALETTE)[2]),
            "  No sessions found."
        );
    }

    #[test]
    fn a_picker_waiting_for_its_listing_says_so_and_one_that_failed_says_that() {
        let loading = SessionPicker::open("", page_for(40));
        assert_eq!(
            plain(&loading.rows(120, 22, PALETTE)[2]),
            "  Loading sessions\u{2026}"
        );
        assert_eq!(loading.selected(), None);
        let mut failed = SessionPicker::open("", page_for(40));
        failed.listed(None);
        assert_eq!(
            plain(&failed.rows(120, 22, PALETTE)[2]),
            "  Unable to load sessions."
        );
    }

    #[test]
    fn a_failed_resume_is_said_in_red_on_the_row_under_the_header_and_enter_may_retry() {
        let mut picker = ready(three());
        let id = picker.selected().expect("a session is marked");
        picker.resuming(id.clone());
        assert_eq!(picker.selected(), None, "a second Enter asked again");
        picker.failed(id.clone(), ResumeFailure::OpenElsewhere);
        let rows = picker.rows(120, 22, PALETTE);
        assert_eq!(
            rows[1],
            format!(
                "  {RED}This session is open in another xfx. Close it there, then press enter to retry.{RESET}"
            )
        );
        assert_eq!(picker.selected(), Some(id));
        // A move forgets it.
        picker.step(true, 20);
        assert_eq!(picker.rows(120, 22, PALETTE)[1], "");
        picker.failed("b".to_string(), ResumeFailure::Unavailable);
        assert_eq!(
            plain(&picker.rows(120, 22, PALETTE)[1]),
            "  Unable to resume this session."
        );
    }

    #[test]
    fn a_narrow_screen_drops_the_metadata_before_the_title_goes_under_twelve_columns() {
        let picker = ready(three());
        // 2 + 12 + 4 + 19 = 37 cells of content, plus the one upstream keeps
        // free: at 38 columns the metadata still fits beside a twelve-column
        // title, cut in the middle.
        let rows = picker.rows(38, 22, PALETTE);
        assert_eq!(
            plain(&rows[4]),
            "  Rename\u{2026}thers    work \u{b7} now \u{b7} 1 turn"
        );
        // One column fewer and the title has the row.
        let rows = picker.rows(37, 22, PALETTE);
        assert_eq!(
            plain(&rows[4]),
            "  Renamed alpha ses\u{2026}e than the others"
        );
        assert_eq!(plain(&rows[2]), "  third question gamma");
    }

    #[test]
    fn an_untitled_session_and_the_age_buckets_read_as_upstreams_do() {
        assert_eq!(age(NOW, NOW), "now");
        assert_eq!(age(NOW - 59_999, NOW), "now");
        assert_eq!(age(NOW - 60_000, NOW), "1m");
        assert_eq!(age(NOW - 3_599_999, NOW), "59m");
        assert_eq!(age(NOW - 3_600_000, NOW), "1h");
        assert_eq!(age(NOW - 86_400_000, NOW), "1d");
        assert_eq!(age(NOW + 5_000, NOW), "now", "a clock that went back");
        let untitled = ready(Listing {
            current: vec![session("u", None, "/x/work", 0)],
            all: Vec::new(),
            now_ms: NOW,
        });
        assert!(
            plain(&untitled.rows(120, 22, PALETTE)[2]).starts_with("  Untitled session    work")
        );
    }

    #[test]
    fn a_long_list_scrolls_a_window_of_twenty_and_a_short_screen_shows_fewer() {
        let many: Vec<ListedSession> = (0..30)
            .map(|n| {
                session(
                    &format!("s{n:02}"),
                    Some(&format!("session {n:02}")),
                    "/x/w",
                    0,
                )
            })
            .collect();
        let mut picker = ready(Listing {
            current: many.clone(),
            all: many,
            now_ms: NOW,
        });
        assert_eq!(picker.height(40, 0), 22);
        for _ in 0..25 {
            picker.step(true, 20);
        }
        let rows = picker.rows(120, 22, PALETTE);
        assert!(
            plain(&rows[2]).starts_with("  session 06"),
            "{:?}",
            plain(&rows[2])
        );
        assert!(rows[21].contains(&format!("{BRIGHT}session 25")));
        // A budget of one row: no header, the marked session alone.
        let rows = picker.rows(120, 1, PALETTE);
        assert_eq!(rows.len(), 1);
        assert!(plain(&rows[0]).starts_with("  session 25"));
    }

    #[test]
    fn the_hint_row_gives_way_by_width_in_upstreams_order() {
        assert_eq!(
            key_help(120),
            "\u{2191}\u{2193} navigate     tab scope     enter resume     esc close"
        );
        assert_eq!(
            key_help(50),
            "\u{2191}\u{2193} navigate  tab scope  enter resume  esc close"
        );
        assert_eq!(key_help(30), "\u{2191}\u{2193} move  tab scope  enter  esc");
        assert_eq!(key_help(24), "enter resume  esc close");
        assert_eq!(key_help(10), "enter esc");
        assert_eq!(key_help(3), "enter esc");
    }

    #[test]
    fn only_these_keys_are_the_pickers() {
        use super::super::input::Action;
        assert_eq!(SessionAction::of(Action::Up), Some(SessionAction::Up));
        assert_eq!(SessionAction::of(Action::Down), Some(SessionAction::Down));
        assert_eq!(SessionAction::of(Action::ControlK), Some(SessionAction::Up));
        assert_eq!(
            SessionAction::of(Action::ControlJ),
            Some(SessionAction::Down)
        );
        assert_eq!(
            SessionAction::of(Action::PageUp),
            Some(SessionAction::Page(false))
        );
        assert_eq!(
            SessionAction::of(Action::PageDown),
            Some(SessionAction::Page(true))
        );
        assert_eq!(SessionAction::of(Action::Tab), Some(SessionAction::Scope));
        assert_eq!(
            SessionAction::of(Action::BackTab),
            Some(SessionAction::Scope)
        );
        assert_eq!(
            SessionAction::of(Action::Submit),
            Some(SessionAction::Resume)
        );
        assert_eq!(
            SessionAction::of(Action::Escape),
            Some(SessionAction::Close)
        );
        assert_eq!(SessionAction::of(Action::Backspace), None);
    }
}
