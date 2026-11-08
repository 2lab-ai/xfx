//! `/model`'s menu: the provider's models, by vendor, in the band under the
//! composer.
//!
//! Upstream's model menu (`vercel-labs/fx@c1db9191`,
//! `src/ui/footer/model_menu_presentation.zig`, state `ModelMenu` in
//! `src/core/app/model_cache_runtime.zig:128-262`), in the slot the slash
//! popup and the `/help` catalog use ([`super::picker`],
//! [`super::help_menu`]): between the rule under the composer and the bottom
//! rule, with its own key help where the hint row was. Like the catalog it is
//! a **view of the composer**: the composer, emptied when the menu opens, is
//! its search, and only the six keys [`ModelAction::of`] names are taken out
//! of the stream.
//!
//! # What it looks like
//!
//! `Models N` and the vendor tabs -- `All`, then `Anthropic`, `OpenAI`, `xAI`,
//! `Z.AI` and `Others`, each **only when the catalog has a model for it** and
//! only when it has models of more than one -- a blank row, one row per model,
//! and, under a blank row, a note on where the catalog came from. A model's
//! row is its id and, in one column shared by every row, the facts the catalog
//! published about it: `400K context · 128K output · Fast · Ultra`, with a fact
//! nobody published left out. The marked row's id is the bold hint grey and
//! everything else is dim. In place of the rows: `Loading models…`,
//! `Unable to load models.`, `No models available.` (the catalog is empty) or
//! `No models found.` (the search or the tab found nothing).
//!
//! # What it lists
//!
//! The rows the runtime sent (`super::bridge::UiEvent::CatalogLoaded`), in
//! the order they came: upstream's featured projection, applied before the
//! rows crossed the channel ([`crate::provider::model::picker_order`]). The
//! search keeps a row when it is in the id or in the vendor's name, ignoring
//! case (`modelMenuItemMatches`, `model_cache_runtime.zig:247-256`).

use crate::provider::model::{CatalogEntry, CatalogRetry};

use super::model_search;
use super::picker::{cells, edge_start, ellipsized, pad_to};
use super::theme::Palette;

/// The header row (`header_rows`, `model_menu_presentation.zig:13`).
const HEADER_ROWS: u16 = 1;

/// The blank row under the header (`roomy_top_gap_rows`, `:14`).
const TOP_GAP_ROWS: u16 = 1;

/// The blank row above the note (`status_gap_rows`, `:16`).
const NOTE_GAP_ROWS: u16 = 1;

/// The most models shown at once (`max_visible_items`, `:17`).
const MOST_ITEMS: usize = 20;

/// The most rows the menu is given (`max_inline_rows`, `:18`): the header, its
/// gap, twenty models, the note's gap and the note.
const MOST_ROWS: u16 = HEADER_ROWS + TOP_GAP_ROWS + 20 + NOTE_GAP_ROWS + 1;

/// The band's rows the menu cannot have (`inlinePickerRowBudgetCapped`'s
/// `fixed_rows`, `picker_presentation.zig:766`), before the composer's extra
/// rows are added to them.
const FIXED_ROWS: u16 = 5;

/// The document rows left on any screen tall enough to have them
/// (`minimum_transcript_rows`, `picker_presentation.zig:767`).
const MINIMUM_TRANSCRIPT_ROWS: u16 = 5;

/// What every model row starts with.
const INDENT: usize = 2;

/// The narrowest an id is allowed to be squeezed to before its facts give way
/// (`composeTitleRow`'s `8`, `:321`).
const NARROWEST_ID: usize = 8;

/// What separates the title, the tabs and the facts' column from what is
/// before them.
const GAP: &str = "  ";

/// What joins one fact to the next.
const FACT_SEPARATOR: &str = " \u{b7} ";

/// What stands for the tabs a narrow header had no room for, and for the part
/// of an id a narrow row had no room for.
const ELLIPSIS: &str = "\u{2026}";

/// What the hint row says while the menu is up, widest first
/// (`composeCatalogMenuHintRow`'s `provider_variants`,
/// `src/ui/footer/input_presentation.zig:827-833`).
const KEY_HELP: [&str; 5] = [
    "\u{2191}\u{2193} navigate     tab provider     enter use     esc close",
    "\u{2191}\u{2193} navigate  tab provider  enter use  esc close",
    "\u{2191}\u{2193} move  tab provider  enter  esc",
    "enter use  esc close",
    "enter esc",
];

/// What one keystroke means to the menu.
///
/// Its own vocabulary, for the reason [`super::help_menu::HelpAction`] is
/// one: the shell translates, so a key the menu does not bind goes on to the
/// composer -- which is what makes typing a search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModelAction {
    /// The previous model, wrapping to the last. Up.
    Up,
    /// The next one, wrapping to the first. Down.
    Down,
    /// A page of the composer's row cap, `true` for down, wrapping past
    /// either end (`routeModelMenuMove` with a page's delta,
    /// `input_completion_runtime.zig:209-226`, `:469-480`). Page Up and Page
    /// Down.
    Page(bool),
    /// The next vendor tab, wrapping to `All`. Tab.
    NextVendor,
    /// The previous one. Shift-Tab.
    PreviousVendor,
    /// Use the marked model (`submitModelMenuSelection`,
    /// `app_input_runtime.zig:2766-2785`). Enter.
    Use,
    /// Close it and empty the search (`cancelModelMenu`, `:3296-3298`).
    /// Escape.
    Close,
}

impl ModelAction {
    /// What an ordinary decoded action means to the menu, and `None` for the
    /// keystrokes that go on to the composer.
    pub(crate) fn of(action: super::input::Action) -> Option<Self> {
        use super::input::Action;
        match action {
            // `C-k` and `C-j` walk it too (`routePickerControlByte`,
            // `app_input_runtime.zig:1220-1235`, through `routeModelMenuMove`).
            Action::Up | Action::ControlK => Some(Self::Up),
            Action::Down | Action::ControlJ => Some(Self::Down),
            Action::PageUp => Some(Self::Page(false)),
            Action::PageDown => Some(Self::Page(true)),
            Action::Tab => Some(Self::NextVendor),
            Action::BackTab => Some(Self::PreviousVendor),
            Action::Submit => Some(Self::Use),
            Action::Escape => Some(Self::Close),
            _ => None,
        }
    }
}

/// One vendor tab (`ModelProviderFilter`, `model_cache_runtime.zig:107-114`),
/// in the order the header lists them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Vendor {
    All,
    Anthropic,
    OpenAi,
    Xai,
    Zai,
    Others,
}

impl Vendor {
    const TABS: [Vendor; 6] = [
        Vendor::All,
        Vendor::Anthropic,
        Vendor::OpenAi,
        Vendor::Xai,
        Vendor::Zai,
        Vendor::Others,
    ];

    /// What the tab says (`providerTabLabel`, `:255-265`).
    fn label(self) -> &'static str {
        match self {
            Vendor::All => "All",
            Vendor::Anthropic => "Anthropic",
            Vendor::OpenAi => "OpenAI",
            Vendor::Xai => "xAI",
            Vendor::Zai => "Z.AI",
            Vendor::Others => "Others",
        }
    }

    /// The tab a vendor's models are under (`providerFilter`, `:264-275`).
    fn of(vendor: &str) -> Self {
        if vendor.eq_ignore_ascii_case("anthropic") {
            Vendor::Anthropic
        } else if vendor.eq_ignore_ascii_case("openai") {
            Vendor::OpenAi
        } else if vendor.eq_ignore_ascii_case("xai") {
            Vendor::Xai
        } else if vendor.eq_ignore_ascii_case("zai") {
            Vendor::Zai
        } else {
            Vendor::Others
        }
    }
}

/// Where the rows are (`ModelMenuLoadState`, `model_cache_runtime.zig:87-91`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Load {
    /// Asked for; the runtime has not answered.
    Loading,
    /// The runtime's answer, which may be no rows at all.
    Ready,
    /// The runtime could not read the catalog, and whether asking again could
    /// change that.
    Failed(Option<CatalogRetry>),
}

/// One model as the menu shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Item {
    /// The whole `provider/model` id, which is all the search reads
    /// ([`model_search`]).
    id: String,
    /// The tab it is under: its vendor's, the vendor being what is in front of
    /// the id's first `/`, or nothing (`modelProvider`,
    /// `model_cache_runtime.zig:1148-1152@481104fd`).
    tab: Vendor,
    /// Its facts, joined; empty when the catalog published none.
    facts: String,
}

impl Item {
    fn of(entry: &CatalogEntry) -> Self {
        let vendor = match entry.id.split_once('/') {
            Some((vendor, _)) if !vendor.is_empty() => vendor,
            _ => "",
        };
        Self {
            tab: Vendor::of(vendor),
            id: entry.id.clone(),
            facts: facts(entry),
        }
    }
}

/// The facts upstream prints beside a model, in its order
/// (`appendCompactFacts`, `model_menu_presentation.zig:335-344`) -- and only
/// the ones the catalog published: a llmux row says its window or nothing.
fn facts(entry: &CatalogEntry) -> String {
    let mut facts: Vec<String> = Vec::new();
    if let Some(tokens) = entry.max_context {
        facts.push(token_fact(tokens, "context"));
    }
    if let Some(tokens) = entry.facts.max_output {
        facts.push(token_fact(tokens, "output"));
    }
    if entry.facts.fast {
        facts.push("Fast".to_string());
    }
    if entry.facts.ultrafast {
        facts.push("Ultra".to_string());
    }
    facts.join(FACT_SEPARATOR)
}

/// `400K context`, `1M context`, or the exact count when it is not a whole
/// number of either (`formatTokenFact`, `:379-387`).
fn token_fact(tokens: u64, suffix: &str) -> String {
    if tokens >= 1_000_000 && tokens.is_multiple_of(1_000_000) {
        format!("{}M {suffix}", tokens / 1_000_000)
    } else if tokens >= 1_000 && tokens.is_multiple_of(1_000) {
        format!("{}K {suffix}", tokens / 1_000)
    } else {
        format!("{tokens} {suffix}")
    }
}

/// The menu's shape for one row budget (`ModelMenuLayout`, `:20-92`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Layout {
    /// Every row it takes.
    rows: u16,
    /// Whether the first row is the header.
    header: bool,
    /// The row the state sentence is on, when there are no rows to list.
    state: Option<u16>,
    /// The row the note is on, when it is shown.
    note: Option<u16>,
    /// The row the first model is on.
    first_item_row: u16,
    /// How many models are in view.
    visible: usize,
    /// The marked model, as an index into the filtered list.
    selected: usize,
    /// How many models the tab and the search leave.
    count: usize,
}

/// The menu as `/settings`' Model row lists it ([`ModelMenu::inline`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Inline<'a> {
    /// Asked for; not answered yet.
    Loading,
    /// The catalog could not be read.
    Failed,
    /// The models the search finds -- maybe none -- with the mark and the
    /// first one in view as the menu's own moves left them.
    Listed {
        ids: Vec<&'a str>,
        selected: usize,
        window_start: usize,
    },
}

/// The menu: what it lists, which tab is up, which model is marked, and what
/// the search is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModelMenu {
    load: Load,
    items: Vec<Item>,
    /// Where the catalog came from, said under the list once it is loaded
    /// ([`crate::provider::model::catalog_note`]).
    note: Option<&'static str>,
    /// The tab that is up.
    tab: Vendor,
    /// An index into the filtered list, kept in range by the readers as
    /// upstream's is (`selected_index % count`).
    selected: usize,
    /// The first model in view, as the last move left it.
    window_start: usize,
    /// The composer's text.
    query: String,
}

impl ModelMenu {
    /// A menu still waiting for its rows (`openMenu` on a cache that is
    /// loading, `model_cache_runtime.zig:529-538`).
    pub(crate) fn loading(query: &str) -> Self {
        Self {
            load: Load::Loading,
            items: Vec::new(),
            note: None,
            tab: Vendor::All,
            selected: 0,
            window_start: 0,
            query: query.to_string(),
        }
    }

    /// A menu over rows already in hand.
    pub(crate) fn ready(entries: &[CatalogEntry], note: Option<&'static str>, query: &str) -> Self {
        let mut menu = Self::loading(query);
        menu.loaded(entries, note);
        menu
    }

    /// The runtime's rows arrived. Over a menu still waiting for its first,
    /// it lists them from the top, on `All`; over rows it already listed --
    /// a catalog read again while the menu is up -- it keeps the tab and the
    /// marked model, as upstream's does (`hydrateMenuSnapshot`,
    /// `model_cache_runtime.zig:1121-1146@481104fd`): "keep the provider tab
    /// and the selected model so the cursor does not jump". A tab the new
    /// rows do not show is `All`; a model they do not list leaves the mark at
    /// the same place, or the last row; and the window moves with the mark,
    /// so its row stays where it was on the screen.
    pub(crate) fn loaded(&mut self, entries: &[CatalogEntry], note: Option<&'static str>) {
        let was_ready = self.load == Load::Ready;
        let marked = self.selected().map(str::to_string);
        let (selected, window_start) = (self.selected, self.window_start);
        self.load = Load::Ready;
        self.items = entries.iter().map(Item::of).collect();
        self.note = note;
        if !was_ready {
            self.tab = Vendor::All;
            self.reset();
            return;
        }
        if !self.tabs().contains(&self.tab) {
            self.tab = Vendor::All;
        }
        let items = self.items();
        let at = marked
            .and_then(|id| items.iter().position(|item| item.id == id))
            .unwrap_or_else(|| selected.min(items.len().saturating_sub(1)));
        self.selected = at;
        self.window_start = (window_start + at).saturating_sub(selected);
    }

    /// What the menu says in its note's place under its rows: where the
    /// catalog came from, or that a refresh behind them failed
    /// (`loadedCatalogStatusText`, `model_menu_presentation.zig:403-433
    /// @481104fd`).
    pub(crate) fn set_note(&mut self, note: Option<&'static str>) {
        self.note = note;
    }

    /// The runtime could not read the catalog; `retry` is upstream's category
    /// for a failure the next `/model` may not repeat.
    pub(crate) fn failed(&mut self, retry: Option<CatalogRetry>) {
        self.load = Load::Failed(retry);
        self.items.clear();
        self.tab = Vendor::All;
        self.reset();
    }

    /// Takes the composer's text as the search. A search that changed puts
    /// the mark back on the first model (`setQuery`, `:148-154`); a keystroke
    /// that left the text alone leaves it where it was.
    pub(crate) fn set_query(&mut self, query: &str) {
        if self.query != query {
            self.query = query.to_string();
            self.reset();
        }
    }

    fn reset(&mut self) {
        self.selected = 0;
        self.window_start = 0;
    }

    /// The tabs the header shows (`modelProviderFilterAvailable`,
    /// `:277-285`): `All` always, and each vendor's only when the catalog has
    /// a model under it **and** models under more than one -- a catalog of
    /// one vendor has nothing to choose between.
    fn tabs(&self) -> Vec<Vendor> {
        let present: Vec<Vendor> = Vendor::TABS[1..]
            .iter()
            .copied()
            .filter(|tab| self.items.iter().any(|item| item.tab == *tab))
            .collect();
        let mut tabs = vec![Vendor::All];
        if present.len() > 1 {
            tabs.extend(present);
        }
        tabs
    }

    /// The next tab, or the previous one, wrapping through `All` and skipping
    /// the tabs the header does not show (`moveProvider`, `:182-206`). The mark
    /// goes back to the top. Nothing moves while the rows are not in.
    pub(crate) fn cycle(&mut self, forward: bool) -> bool {
        if self.load != Load::Ready {
            return false;
        }
        let tabs = self.tabs();
        let current = tabs.iter().position(|tab| *tab == self.tab).unwrap_or(0);
        let count = tabs.len();
        let next = if forward {
            (current + 1) % count
        } else {
            (current + count - 1) % count
        };
        if next == current {
            return false;
        }
        self.tab = tabs[next];
        self.reset();
        true
    }

    /// Moves the mark one model, wrapping at both ends, and scrolls a window of
    /// `visible` models only as far as keeps it in view
    /// (`moveVisibleItems`, `:166-180`). `false`, and nothing moved, when
    /// there is nothing listed.
    #[cfg(test)]
    pub(crate) fn step(&mut self, down: bool, visible: u16) -> bool {
        self.step_by(if down { 1 } else { -1 }, visible)
    }

    /// The mark `delta` rows on, wrapping to the other end past either
    /// (`moveVisibleItems`, `model_cache_runtime.zig:168-184`).
    pub(crate) fn step_by(&mut self, delta: i64, visible: u16) -> bool {
        let count = self.items().len();
        if self.load != Load::Ready || count == 0 {
            return false;
        }
        self.selected = super::picker::advanced(self.selected, count, delta);
        self.window_start = edge_start(self.window_start, count, self.selected, visible.max(1));
        true
    }

    /// What `/settings`' Model row shows of this menu under itself: the same
    /// state, the same filtered models, the same mark (`composeModelRow`,
    /// `settings_menu_presentation.zig:277-310`, which reads the model menu
    /// upstream's `/model` opens).
    pub(crate) fn inline(&self) -> Inline<'_> {
        match self.load {
            Load::Loading => Inline::Loading,
            Load::Failed(_) => Inline::Failed,
            Load::Ready => Inline::Listed {
                ids: self
                    .items()
                    .into_iter()
                    .map(|item| item.id.as_str())
                    .collect(),
                selected: self.selected,
                window_start: self.window_start,
            },
        }
    }

    /// The marked model's id, or `None` when nothing is listed
    /// (`selectedModelAlloc`, `:208-212`).
    pub(crate) fn selected(&self) -> Option<&str> {
        if self.load != Load::Ready {
            return None;
        }
        let items = self.items();
        let count = items.len();
        (count > 0).then(|| items[self.selected % count].id.as_str())
    }

    /// What the menu lists: the tab's models the search finds, better matches
    /// first and equal ones in the order the rows came (`modelMenuItemAt`,
    /// `model_cache_runtime.zig:298-315@481104fd`; [`model_search`]). The
    /// count, the rows, the mark, `/settings`' Model row and the mark kept
    /// across a revalidation ([`Self::loaded`], `modelMenuDisplayIndex`,
    /// `:317-338`) all read this one list.
    fn items(&self) -> Vec<&Item> {
        model_search::ranked(
            self.items
                .iter()
                .filter(|item| self.tab == Vendor::All || item.tab == self.tab),
            |item| item.id.as_str(),
            &self.query,
        )
    }

    /// The menu's shape in `budget` rows (`ModelMenuLayout.build`, `:31-91`).
    fn layout(&self, count: usize, budget: u16) -> Layout {
        if budget == 0 {
            return Layout::default();
        }
        let selected = if count > 0 { self.selected % count } else { 0 };
        if self.load != Load::Ready || count == 0 {
            let state = match budget {
                1 => 0,
                2 => 1,
                _ => HEADER_ROWS + TOP_GAP_ROWS,
            };
            let mut rows = state + 1;
            let note =
                (self.load == Load::Ready && self.note.is_some() && budget > rows + NOTE_GAP_ROWS)
                    .then_some(rows + NOTE_GAP_ROWS);
            if let Some(row) = note {
                rows = row + 1;
            }
            return Layout {
                rows,
                header: budget > 1,
                state: Some(state),
                note,
                selected,
                count,
                ..Layout::default()
            };
        }
        if budget <= 2 {
            return Layout {
                rows: budget,
                header: budget == 2,
                first_item_row: budget - 1,
                visible: 1,
                selected,
                count,
                ..Layout::default()
            };
        }
        let first_item_row = HEADER_ROWS + TOP_GAP_ROWS;
        let item_budget = usize::from(budget - first_item_row);
        let visible = count.min(item_budget.max(1)).min(MOST_ITEMS);
        let shown = u16::try_from(visible).unwrap_or(u16::MAX);
        let note = (self.note.is_some() && item_budget - visible > usize::from(NOTE_GAP_ROWS))
            .then_some(first_item_row + shown + NOTE_GAP_ROWS);
        Layout {
            rows: first_item_row + shown + note.map_or(0, |_| NOTE_GAP_ROWS + 1),
            header: true,
            state: None,
            note,
            first_item_row,
            visible,
            selected,
            count,
        }
    }

    /// How many rows the band has to give it on a screen `terminal_rows` tall
    /// whose composer is `input_extra` rows taller than one (`menuRowCount`
    /// under `inlinePickerRowBudgetCapped`).
    pub(crate) fn height(&self, terminal_rows: u16, input_extra: u16) -> u16 {
        self.layout(self.items().len(), row_budget(terminal_rows, input_extra))
            .rows
    }

    /// How many models are in view in `max_rows` rows, and never fewer than
    /// one: the window a move scrolls against
    /// (`visibleNavigationItemsForBudget`, `:98-100`).
    pub(crate) fn visible_items(&self, max_rows: u16) -> u16 {
        let visible = self.layout(self.items().len(), max_rows).visible;
        u16::try_from(visible.max(1)).unwrap_or(u16::MAX)
    }

    /// The menu's rows in `max_rows` rows of a `cols`-wide screen, top first:
    /// exactly as many as [`Self::height`] gave the band when `max_rows` is that
    /// height (`composeModelMenuRow`, `:102-153`). A gap row is empty.
    pub(crate) fn rows(&self, cols: u16, max_rows: u16, palette: Palette) -> Vec<String> {
        let items = self.items();
        let layout = self.layout(items.len(), max_rows);
        let mut rows = vec![String::new(); usize::from(layout.rows)];
        if layout.rows == 0 {
            return rows;
        }
        if layout.header {
            rows[0] = self.header(items.len(), cols, palette);
        }
        if let Some(state) = layout.state {
            rows[usize::from(state)] = dimmed(self.state_text(), cols, palette);
        }
        if let (Some(row), Some(note)) = (layout.note, self.note) {
            rows[usize::from(row)] = dimmed(note, cols, palette);
        }
        if layout.visible > 0 {
            let first = edge_start(
                self.window_start,
                layout.count,
                layout.selected,
                u16::try_from(layout.visible).unwrap_or(u16::MAX),
            );
            let column = self.facts_column(cols);
            for offset in 0..layout.visible {
                let Some(item) = items.get(first + offset) else {
                    break;
                };
                let row = usize::from(layout.first_item_row) + offset;
                rows[row] = item_row(
                    item,
                    first + offset == layout.selected,
                    column,
                    cols,
                    palette,
                );
            }
        }
        rows
    }

    /// What the menu says in place of its rows (`composeStateRow`, `:394-401`):
    /// for a failure asking again could change, upstream's `xfx retries
    /// automatically.` sentence instead of `Unable to load models.`
    /// (`retryableFailureText`, `:435-443@481104fd`).
    fn state_text(&self) -> &'static str {
        match self.load {
            Load::Loading => "Loading models\u{2026}",
            Load::Failed(Some(retry)) => retry.text(),
            Load::Failed(None) => "Unable to load models.",
            Load::Ready if self.items.is_empty() => "No models available.",
            Load::Ready => "No models found.",
        }
    }

    /// Where every row's facts start: past the indent, the widest id of
    /// **every** model -- not the filtered ones, so the column stays put as the
    /// search and the tabs narrow the list -- and two cells; pulled left when
    /// the widest facts would not fit after it, and no column at all when the
    /// screen has no room for an id and the facts side by side
    /// (`modelFactsColumn`, `:287-299`).
    fn facts_column(&self, cols: u16) -> Option<usize> {
        let width = usize::from(cols);
        let indent = if width <= INDENT { 0 } else { INDENT };
        let facts = self
            .items
            .iter()
            .map(|item| cells(&item.facts))
            .max()
            .unwrap_or(0);
        let longest = self
            .items
            .iter()
            .map(|item| cells(&item.id))
            .max()
            .unwrap_or(0)
            .max(NARROWEST_ID);
        if facts == 0 || width < indent + NARROWEST_ID + GAP.len() + facts {
            return None;
        }
        Some((indent + longest + GAP.len()).min(width - facts))
    }

    /// `Models N` and the tabs (`composeHeaderRow`, `:155-199`).
    ///
    /// The tabs that fit, grown outwards from the active one -- a dim `…` on
    /// a side that had to be cut -- so the tab that is up is always on the row;
    /// the title and the active tab alone when not even one more fits.
    fn header(&self, count: usize, cols: u16, palette: Palette) -> String {
        let width = usize::from(cols);
        let title = format!("{}Models {count}{}", palette.subtitle(), palette.reset());
        let title_width = cells(&title);
        let tabs = self.tabs();
        let active = tabs.iter().position(|tab| *tab == self.tab).unwrap_or(0);
        let tab_width =
            |index: usize| cells(tabs[index].label()) + 2 * usize::from(index == active);
        let range_width = |start: usize, end: usize| {
            let mut total = if start > 0 { 3 } else { 0 };
            for index in start..end {
                if index > start {
                    total += GAP.len();
                }
                total += tab_width(index);
            }
            if end < tabs.len() {
                total += 3;
            }
            total
        };
        let painted = |index: usize| painted_tab(tabs[index].label(), index == active, palette);
        if title_width + GAP.len() + tab_width(active) > width {
            return clipped(&format!("{title}{GAP}{}", painted(active)), cols, palette);
        }
        let (mut start, mut end) = (active, active + 1);
        loop {
            let mut grown = false;
            if end < tabs.len() && title_width + GAP.len() + range_width(start, end + 1) <= width {
                end += 1;
                grown = true;
            }
            if start > 0 && title_width + GAP.len() + range_width(start - 1, end) <= width {
                start -= 1;
                grown = true;
            }
            if !grown {
                break;
            }
        }
        let ellipsis = format!("{}{ELLIPSIS}{}", palette.dim(), palette.reset());
        let mut row = format!("{title}{GAP}");
        if start > 0 {
            row.push_str(&ellipsis);
            row.push_str(GAP);
        }
        for index in start..end {
            if index > start {
                row.push_str(GAP);
            }
            row.push_str(&painted(index));
        }
        if end < tabs.len() {
            row.push_str(GAP);
            row.push_str(&ellipsis);
        }
        clipped(&row, cols, palette)
    }
}

/// One tab: the active one bold in brackets, the rest dim
/// (`appendProviderTab`, `:232-238`).
fn painted_tab(label: &str, active: bool, palette: Palette) -> String {
    if active {
        format!("{}[{label}]{}", palette.subtitle(), palette.reset())
    } else {
        format!("{}{label}{}", palette.dim(), palette.reset())
    }
}

/// A row cut to the screen, the reset kept inside the cut
/// (`cloneClippedRow`, `:454-460`).
fn clipped(row: &str, cols: u16, palette: Palette) -> String {
    format!("{}{}", super::frame::clip(row, cols), palette.reset())
}

/// A whole row in the dim grey, ellipsized to the screen
/// (`composeDimmedRow`, `:445-452`).
fn dimmed(text: &str, cols: u16, palette: Palette) -> String {
    format!(
        "{}{}{}",
        palette.dim(),
        ellipsized(text, usize::from(cols)),
        palette.reset()
    )
}

/// One model's row (`composeTitleRow`, `:301-333`).
///
/// The indent, then the id in the row's style -- the marked one closed after
/// the id, every other one left dim through the padding, as upstream's are --
/// then, at the facts' column, the facts in the dim grey. An id too wide for
/// the room before the column loses its middle, so two long ids that share a
/// family still differ on the screen; facts too wide for the screen lose their
/// tail; and on a screen with no room for both the facts go.
fn item_row(
    item: &Item,
    marked: bool,
    column: Option<usize>,
    cols: u16,
    palette: Palette,
) -> String {
    let width = usize::from(cols);
    let indent = if width <= INDENT { 0 } else { INDENT };
    let start = column.unwrap_or(width);
    let show_facts = !item.facts.is_empty() && start >= indent + NARROWEST_ID + GAP.len();
    let id_budget = if show_facts {
        start - indent - GAP.len()
    } else {
        width.saturating_sub(indent)
    };
    let mut row = " ".repeat(indent);
    row.push_str(if marked {
        palette.subtitle()
    } else {
        palette.dim()
    });
    row.push_str(&middle_ellipsized(&item.id, id_budget));
    if marked {
        row.push_str(palette.reset());
    }
    if show_facts {
        pad_to(&mut row, start);
        row.push_str(palette.dim());
        row.push_str(&ellipsized(&item.facts, width - start));
    }
    if show_facts || !marked {
        row.push_str(palette.reset());
    }
    row
}

/// `text` in at most `budget` cells, its middle swapped for an ellipsis when
/// it does not fit: the first half rounded up, then the last
/// (`appendSingleLineMiddleEllipsized`, `row_text.zig:64-107`).
pub(super) fn middle_ellipsized(text: &str, budget: usize) -> String {
    if cells(text) <= budget {
        return text.to_string();
    }
    if budget <= 1 {
        return ellipsized(text, budget);
    }
    let content = budget - 1;
    let head_cells = content.div_ceil(2);
    let tail_cells = content - head_cells;
    let head = super::frame::clip(text, u16::try_from(head_cells).unwrap_or(u16::MAX));
    let mut tail: Vec<char> = Vec::new();
    let mut used = 0;
    for character in text.chars().rev() {
        let character_cells = cells(character.encode_utf8(&mut [0u8; 4]));
        if used + character_cells > tail_cells {
            break;
        }
        used += character_cells;
        tail.push(character);
    }
    let tail: String = tail.into_iter().rev().collect();
    format!("{head}{ELLIPSIS}{tail}")
}

/// The rows a screen `terminal_rows` tall gives the menu when its composer is
/// `input_extra` rows taller than one (`inlinePickerRowBudgetCapped`,
/// `picker_presentation.zig:760-770`): what is left after the band's fixed
/// rows and five rows of document, at most [`MOST_ROWS`] and never none.
fn row_budget(terminal_rows: u16, input_extra: u16) -> u16 {
    terminal_rows
        .saturating_sub(FIXED_ROWS.saturating_add(input_extra))
        .saturating_sub(MINIMUM_TRANSCRIPT_ROWS)
        .clamp(1, MOST_ROWS)
}

/// What the hint row says while the menu is up, on a screen `cols` wide: the
/// widest of [`KEY_HELP`] that fits, and the narrowest when none does.
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

    use crate::provider::model::{parse_gateway_catalog, picker_order, ModelFacts};

    use super::super::theme::Mode;

    const PALETTE: Palette = Palette { mode: Mode::Dark };

    /// Upstream's `selected_completion_style` (`core/shared/theme.zig:97`).
    const BRIGHT: &str = "\u{1b}[1;38;5;255m";

    /// Upstream's `dim_style` (`core/shared/theme.zig:85`).
    const DIM: &str = "\u{1b}[38;5;245m";

    const RESET: &str = "\u{1b}[0m";

    const NOTE: &str = "Note: Gateway catalog is authenticated with an API key";

    /// The research harness's catalog, as the runtime would send it.
    fn captured() -> Vec<CatalogEntry> {
        let entries = parse_gateway_catalog(
            r#"{"data":[
 {"id":"fake/model-a","type":"language","owned_by":"fake","released":1700000000,"tags":["tool-use"]},
 {"id":"openai/gpt-6-astra","type":"language","owned_by":"openai","released":1780000000,"tags":["tool-use"],"context_window":400000,"max_tokens":128000,"pricing":{"input_cache_read":"0.0000002","service_tiers":{"priority":{"input":"0.000004","output":"0.00002"},"ultrafast":{"input":"0.00006","output":"0.0003","input_cache_read":"0.000006"}}}},
 {"id":"anthropic/claude-opus-5","type":"language","owned_by":"anthropic","released":1775000000,"tags":["tool-use"],"context_window":1000000,"max_tokens":64000,"fast_options":[{"type":"toggle"}]},
 {"id":"xai/grok-4.7","type":"language","owned_by":"xai","released":1770000000,"tags":["tool-use"]}
]}"#,
        )
        .expect("a catalog");
        picker_order(&entries)
    }

    fn open() -> ModelMenu {
        ModelMenu::ready(&captured(), Some(NOTE), "")
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

    fn shown(menu: &ModelMenu, cols: u16, rows: u16) -> Vec<String> {
        menu.rows(cols, menu.height(rows, 0), PALETTE)
            .iter()
            .map(|row| plain(row))
            .collect()
    }

    fn llmux(id: &str, window: Option<u64>) -> CatalogEntry {
        CatalogEntry {
            id: id.to_string(),
            aliases: Vec::new(),
            name: None,
            efforts: Vec::new(),
            max_context: window,
            facts: ModelFacts::default(),
        }
    }

    #[test]
    fn the_menu_opens_as_upstreams_does_over_the_same_catalog() {
        // `fx-model-40x120-open`, row for row.
        let menu = open();
        assert_eq!(
            shown(&menu, 120, 40),
            [
                "Models 4  [All]  Anthropic  OpenAI  xAI  Others",
                "",
                "  openai/gpt-6-astra       400K context \u{b7} 128K output \u{b7} Fast \u{b7} Ultra",
                "  anthropic/claude-opus-5  1M context \u{b7} 64K output \u{b7} Fast",
                "  fake/model-a",
                "  xai/grok-4.7",
                "",
                NOTE,
            ]
        );
    }

    #[test]
    fn the_rows_are_painted_in_upstreams_two_greys() {
        let rows = open().rows(120, 8, PALETTE);
        assert_eq!(
            rows[0],
            format!(
                "{BRIGHT}Models 4{RESET}  {BRIGHT}[All]{RESET}  {DIM}Anthropic{RESET}  {DIM}OpenAI{RESET}  {DIM}xAI{RESET}  {DIM}Others{RESET}{RESET}"
            )
        );
        // The marked id bold and closed; its facts dim at the column.
        assert_eq!(
            rows[2],
            format!(
                "  {BRIGHT}openai/gpt-6-astra{RESET}       {DIM}400K context \u{b7} 128K output \u{b7} Fast \u{b7} Ultra{RESET}"
            )
        );
        // An unmarked id stays dim through the padding.
        assert_eq!(
            rows[3],
            format!("  {DIM}anthropic/claude-opus-5  {DIM}1M context \u{b7} 64K output \u{b7} Fast{RESET}")
        );
        // A row with no facts is the dim id, closed.
        assert_eq!(rows[4], format!("  {DIM}fake/model-a{RESET}"));
        assert_eq!(rows[7], format!("{DIM}{NOTE}{RESET}"));
    }

    #[test]
    fn down_and_up_wrap_and_tab_walks_the_vendors_the_catalog_has() {
        let mut menu = open();
        assert_eq!(menu.selected(), Some("openai/gpt-6-astra"));
        assert!(menu.step(true, 20));
        assert_eq!(menu.selected(), Some("anthropic/claude-opus-5"));
        // `fx-model-40x120-down5`: four more Downs from the second row wrap
        // round to it.
        for _ in 0..4 {
            menu.step(true, 20);
        }
        assert_eq!(menu.selected(), Some("anthropic/claude-opus-5"));
        menu.step(false, 20);
        menu.step(false, 20);
        assert_eq!(
            menu.selected(),
            Some("xai/grok-4.7"),
            "Up wraps to the last"
        );

        // `menu-tab1`, `menu-tab2`: Tab is the next vendor, the mark back on
        // top, and the count follows the tab.
        assert!(menu.cycle(true));
        assert_eq!(
            shown(&menu, 120, 40)[..3],
            [
                "Models 1  All  [Anthropic]  OpenAI  xAI  Others",
                "",
                "  anthropic/claude-opus-5  1M context \u{b7} 64K output \u{b7} Fast",
            ]
        );
        menu.cycle(true);
        assert_eq!(menu.selected(), Some("openai/gpt-6-astra"));
        // Z.AI has no model, so it is not a tab: xAI, then Others, then All.
        menu.cycle(true);
        assert_eq!(menu.selected(), Some("xai/grok-4.7"));
        menu.cycle(true);
        assert_eq!(menu.selected(), Some("fake/model-a"));
        menu.cycle(true);
        assert_eq!(menu.selected(), Some("openai/gpt-6-astra"), "back to All");
        // Shift-Tab goes the other way round.
        menu.cycle(false);
        assert_eq!(menu.selected(), Some("fake/model-a"));
    }

    #[test]
    fn a_search_that_finds_nothing_says_so_above_the_note() {
        // `fx-model2-40x120-menu-query`: `gro` on the OpenAI tab.
        let mut menu = open();
        menu.cycle(true);
        menu.cycle(true);
        menu.set_query("gro");
        assert_eq!(
            shown(&menu, 120, 40),
            [
                "Models 0  All  Anthropic  [OpenAI]  xAI  Others",
                "",
                "No models found.",
                "",
                NOTE,
            ]
        );
        assert_eq!(menu.selected(), None, "Enter has nothing to use");
        // `menu-btab`: Shift-Tab keeps the search.
        menu.cycle(false);
        assert_eq!(
            shown(&menu, 120, 40)[0],
            "Models 0  All  [Anthropic]  OpenAI  xAI  Others"
        );
        // On All the search finds grok -- by its vendor's name too.
        menu.cycle(false);
        assert_eq!(menu.selected(), Some("xai/grok-4.7"));
        menu.set_query("XAI");
        assert_eq!(menu.selected(), Some("xai/grok-4.7"));
    }

    #[test]
    fn the_states_before_and_without_rows() {
        let loading = ModelMenu::loading("");
        assert_eq!(
            shown(&loading, 120, 40),
            ["Models 0  [All]", "", "Loading models\u{2026}"]
        );
        assert_eq!(loading.selected(), None);
        let mut failed = ModelMenu::loading("");
        failed.failed(None);
        assert_eq!(
            shown(&failed, 120, 40),
            ["Models 0  [All]", "", "Unable to load models."]
        );
        // A failure asking again may not repeat says so, in upstream's
        // words (`retryableFailureText`, `model_menu_presentation.zig:435-443
        // @481104fd`).
        failed.failed(Some(CatalogRetry::Unreachable));
        assert_eq!(
            shown(&failed, 120, 40),
            [
                "Models 0  [All]",
                "",
                "Could not reach AI Gateway; xfx retries automatically."
            ]
        );
        failed.failed(Some(CatalogRetry::RateLimited));
        assert_eq!(
            shown(&failed, 120, 40)[2],
            "AI Gateway rate limited model discovery; xfx retries automatically."
        );
        let empty = ModelMenu::ready(&[], Some(NOTE), "");
        assert_eq!(
            shown(&empty, 120, 40),
            ["Models 0  [All]", "", "No models available.", "", NOTE]
        );
        let mut tab = ModelMenu::loading("");
        assert!(!tab.cycle(true), "no tab moves before the rows are in");
        assert!(!tab.step(true, 5));
    }

    #[test]
    fn a_catalog_of_one_vendor_shows_only_all_and_a_llmux_row_says_only_its_window() {
        let menu = ModelMenu::ready(
            &[
                llmux("claude-fable-5[1m]", Some(1_000_000)),
                llmux("grok-4.5", None),
            ],
            None,
            "",
        );
        assert_eq!(
            shown(&menu, 80, 40),
            [
                "Models 2  [All]",
                "",
                "  claude-fable-5[1m]  1M context",
                "  grok-4.5",
            ]
        );
    }

    #[test]
    fn facts_are_compact_and_exact_when_not_round() {
        assert_eq!(token_fact(400_000, "context"), "400K context");
        assert_eq!(token_fact(1_000_000, "context"), "1M context");
        assert_eq!(token_fact(1_500_000, "context"), "1500K context");
        assert_eq!(token_fact(131_072, "output"), "131072 output");
        assert_eq!(token_fact(999, "output"), "999 output");
    }

    #[test]
    fn a_narrow_screen_keeps_the_active_tab_and_squeezes_ids_in_the_middle() {
        let mut menu = open();
        for _ in 0..4 {
            menu.cycle(true);
        }
        let header = shown(&menu, 30, 40)[0].clone();
        assert!(header.contains("[Others]"), "{header}");
        assert!(header.contains('\u{2026}'), "{header}");
        assert!(cells(&header) <= 30, "{header}");

        assert_eq!(
            middle_ellipsized("provider/very-long-shared-family-alpha", 20),
            "provider/v\u{2026}ily-alpha"
        );
        assert_eq!(middle_ellipsized("short", 20), "short");
        // On a screen with no room for the facts beside the ids, they go --
        // the row padded to the screen and nothing after it, as upstream's
        // (`composeTitleRow` with no facts column).
        let rows = shown(&open(), 40, 40);
        assert_eq!(rows[2].trim_end(), "  openai/gpt-6-astra");
        for row in &rows {
            assert!(cells(row) <= 40, "{row}");
        }
    }

    #[test]
    fn twenty_models_at_most_and_the_window_follows_the_mark() {
        let many: Vec<CatalogEntry> = (0..30).map(|n| llmux(&format!("m-{n:02}"), None)).collect();
        let mut menu = ModelMenu::ready(&many, None, "");
        let rows = shown(&menu, 80, 40);
        assert_eq!(rows.len(), 2 + MOST_ITEMS);
        assert_eq!(rows[2], "  m-00");
        menu.step(false, menu.visible_items(menu.height(40, 0)));
        assert_eq!(menu.selected(), Some("m-29"));
        let rows = shown(&menu, 80, 40);
        assert_eq!(rows.last().map(String::as_str), Some("  m-29"));
        // A short screen shows fewer, and the note gives way first.
        let short = ModelMenu::ready(&many, Some(NOTE), "");
        assert_eq!(short.height(16, 0), 6);
        assert!(!shown(&short, 80, 16).contains(&NOTE.to_string()));
    }

    #[test]
    fn rows_and_height_come_from_one_layout_on_every_screen() {
        for menu in [
            open(),
            ModelMenu::loading(""),
            ModelMenu::ready(&[], Some(NOTE), ""),
        ] {
            for rows in 1..45 {
                let height = menu.height(rows, 0);
                assert!(height >= 1, "{rows}");
                assert_eq!(menu.rows(120, height, PALETTE).len(), usize::from(height));
            }
        }
        assert_eq!(row_budget(40, 0), 24);
        assert_eq!(row_budget(24, 0), 14);
        assert_eq!(row_budget(8, 3), 1);
    }

    #[test]
    fn the_key_help_is_upstreams_and_fits_the_screen() {
        assert_eq!(
            key_help(120),
            "\u{2191}\u{2193} navigate     tab provider     enter use     esc close"
        );
        assert_eq!(
            key_help(40),
            "\u{2191}\u{2193} move  tab provider  enter  esc"
        );
        assert_eq!(key_help(3), "enter esc");
    }

    #[test]
    fn the_menu_binds_six_keys_and_lets_the_rest_through() {
        use super::super::input::Action;
        assert_eq!(ModelAction::of(Action::Up), Some(ModelAction::Up));
        assert_eq!(ModelAction::of(Action::Down), Some(ModelAction::Down));
        assert_eq!(ModelAction::of(Action::Tab), Some(ModelAction::NextVendor));
        assert_eq!(
            ModelAction::of(Action::BackTab),
            Some(ModelAction::PreviousVendor)
        );
        assert_eq!(ModelAction::of(Action::Submit), Some(ModelAction::Use));
        assert_eq!(ModelAction::of(Action::Escape), Some(ModelAction::Close));
        assert_eq!(ModelAction::of(Action::Left), None);
        assert_eq!(ModelAction::of(Action::Backspace), None);
        // `C-k` and `C-j` are the same two moves; Alt-Enter is not.
        assert_eq!(ModelAction::of(Action::ControlK), Some(ModelAction::Up));
        assert_eq!(ModelAction::of(Action::ControlJ), Some(ModelAction::Down));
        assert_eq!(ModelAction::of(Action::InsertNewline), None);
    }

    #[test]
    fn a_revalidated_catalog_keeps_the_tab_and_the_marked_model() {
        // `hydrateMenuSnapshot` (`model_cache_runtime.zig:1121-1146@481104fd`):
        // "A revalidated catalog replaces rows under an open menu; keep the
        // provider tab and the selected model so the cursor does not jump" --
        // and a selection or tab the new rows lack falls back to the same
        // position in All. fx's own case, row for row (`:1599-1633`).
        let ids =
            |ids: &[&str]| -> Vec<CatalogEntry> { ids.iter().map(|id| llmux(id, None)).collect() };
        let mut menu = ModelMenu::ready(&ids(&["openai/a", "openai/b", "anthropic/c"]), None, "");
        // All, Anthropic, OpenAI: two steps on is OpenAI; one down, `openai/b`.
        assert!(menu.cycle(true));
        assert!(menu.cycle(true));
        assert!(menu.step(true, 20));
        assert_eq!(menu.selected(), Some("openai/b"));

        menu.loaded(
            &ids(&[
                "xai/new",
                "openai/new",
                "openai/a",
                "openai/b",
                "anthropic/c",
            ]),
            None,
        );
        assert_eq!(menu.selected(), Some("openai/b"), "the mark jumped");
        assert_eq!(
            menu.inline(),
            Inline::Listed {
                ids: vec!["openai/new", "openai/a", "openai/b"],
                selected: 2,
                window_start: 1,
            },
            "the tab, or the window under the mark, moved"
        );

        // Gone from the rows, and its tab with it: All, at the same place.
        menu.loaded(&ids(&["xai/new", "anthropic/c"]), None);
        assert_eq!(menu.selected(), Some("anthropic/c"));
        assert_eq!(
            menu.inline(),
            Inline::Listed {
                ids: vec!["xai/new", "anthropic/c"],
                selected: 1,
                window_start: 0,
            }
        );
    }

    /// The ids the menu lists, in its order.
    fn listed(menu: &ModelMenu) -> Vec<String> {
        match menu.inline() {
            Inline::Listed { ids, .. } => ids.iter().map(|id| id.to_string()).collect(),
            other => panic!("not listed: {other:?}"),
        }
    }

    #[test]
    fn the_search_finds_what_fxs_own_table_finds() {
        // fx's table (`model queries match the full id verbatim, by words, and
        // fuzzily`, `src/core/app/model_cache_runtime.zig:2060-2084@481104fd`),
        // each case as the menu shows it: listed for a substring, a words or a
        // fuzzy match, not for none. Which of the three it is shows in the
        // order ([`better_matches_list_first_and_the_mark_keeps_its_model_through_a_revalidation`]).
        const ID: &str = "anthropic/claude-sonnet-4.5";
        let lists = |id: &str, query: &str| {
            ModelMenu::ready(&[llmux(id, None)], None, query).selected() == Some(id)
        };
        for query in [
            // substring
            "",
            " \t",
            " Claude-Sonnet ",
            "ANTHROPIC/claude",
            // words
            "claude sonnet 4.5",
            "anthropic sonnet",
            "Sonnet45",
            "sonnet 4-5",
            "anthropic / sonnet",
            // fuzzy
            "cs45",
            "anthropic sonet",
        ] {
            assert!(lists(ID, query), "{query:?} finds {ID}");
        }
        assert!(
            lists("acme/gpt4omni", "omi"),
            "a word starts at a digit-letter turn"
        );
        for query in ["opus", "sonnet opus", "nthrpc", "- /"] {
            assert!(!lists(ID, query), "{query:?} finds nothing");
        }
        assert!(!lists("acme/gpt4omni", "mi"));
        assert!(!lists("", "a"));
        // Not fx's table; the same rule (`isIdWordStart`, `:438-447`): a case
        // change is not a word start (`p5-input-model-rank` `q-dsc`, `q-sc`).
        assert!(lists("acme/DeepSeekCoder", "dsc"));
        assert!(!lists("acme/DeepSeekCoder", "sc"));
    }

    #[test]
    fn better_matches_list_first_and_the_mark_keeps_its_model_through_a_revalidation() {
        // fx's own case, row for row (`model menu ranks fuzzy matches below
        // verbatim ones and keeps the selection`, `:2086-2123@481104fd`).
        let ids =
            |ids: &[&str]| -> Vec<CatalogEntry> { ids.iter().map(|id| llmux(id, None)).collect() };
        let entries = [
            "openai/gpt-4.5-preview",
            "openai/gpt-5",
            "anthropic/claude-opus-4.8",
            "acme/gpt5-lite",
        ];
        let mut menu = ModelMenu::ready(&ids(&entries), None, "gpt5");
        assert_eq!(
            listed(&menu),
            ["acme/gpt5-lite", "openai/gpt-5", "openai/gpt-4.5-preview"]
        );
        // All, Anthropic, OpenAI, Others: two on is OpenAI.
        assert!(menu.cycle(true));
        assert!(menu.cycle(true));
        assert_eq!(listed(&menu), ["openai/gpt-5", "openai/gpt-4.5-preview"]);
        assert!(menu.cycle(false));
        assert!(menu.cycle(false));

        assert!(menu.step(true, 3));
        assert_eq!(menu.selected(), Some("openai/gpt-5"));

        // "A revalidated catalog that adds a better match keeps the cursor on
        // the same model even though its row moves down."
        let mut revalidated = vec!["acme/gpt5-pro"];
        revalidated.extend(entries);
        menu.loaded(&ids(&revalidated), None);
        assert_eq!(menu.selected(), Some("openai/gpt-5"));
        assert_eq!(
            menu.inline(),
            Inline::Listed {
                ids: vec![
                    "acme/gpt5-pro",
                    "acme/gpt5-lite",
                    "openai/gpt-5",
                    "openai/gpt-4.5-preview",
                ],
                selected: 2,
                window_start: 1,
            }
        );

        // The mark is found in the ranked list, not in the catalog: first in
        // the catalog, gpt-4.5-preview is still the fourth match.
        assert!(menu.step(true, 3));
        assert_eq!(menu.selected(), Some("openai/gpt-4.5-preview"));
        menu.loaded(
            &ids(&[
                "openai/gpt-4.5-preview",
                "acme/gpt5-pro",
                "openai/gpt-5",
                "acme/gpt5-lite",
            ]),
            None,
        );
        assert_eq!(menu.selected(), Some("openai/gpt-4.5-preview"));
        assert!(
            matches!(menu.inline(), Inline::Listed { selected: 3, .. }),
            "{:?}",
            menu.inline()
        );
    }

    #[test]
    fn the_search_reads_the_whole_id_and_nothing_else_a_row_carries() {
        // `matchModelQuery(item.id, query)` (`:340-347,371-374@481104fd`): the
        // full `provider/model` id, so one query can name both -- and not the
        // vendor apart (f9ddaff8 dropped that), a name, an alias or an owner.
        let mut glm = llmux("zai/glm-5", None);
        glm.name = Some("Zhipu GLM".to_string());
        glm.aliases = vec!["zhipu".to_string()];
        glm.facts.owned_by = Some("zhipu-ai".to_string());
        let rows = [llmux("anthropic/claude-opus-5", None), glm];
        let first = |query: &str| {
            ModelMenu::ready(&rows, None, query)
                .selected()
                .map(str::to_string)
        };
        assert_eq!(
            first("anthropic opus").as_deref(),
            Some("anthropic/claude-opus-5")
        );
        assert_eq!(first("z.ai glm").as_deref(), Some("zai/glm-5"));
        assert_eq!(first("zhipu"), None);
        assert_eq!(first("Zhipu GLM"), None);
    }
}
