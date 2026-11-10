//! The `/help` catalog: every command the session answers, by category, in
//! the band under the composer.
//!
//! Upstream's help menu (`vercel-labs/fx@c1db9191`,
//! `src/ui/footer/help_menu_presentation.zig`, state machine `HelpMenu` in
//! `src/core/slash_commands/command_specs.zig:191-251`), and it lives in the
//! slot the slash popup does ([`super::picker`]): between the rule under the
//! composer and the bottom rule, with its own key help where the hint row was.
//! Like the popup it is a **view of the composer** rather than a focus -- the
//! caret stays where the text is typed and only the six keys it binds
//! ([`HelpAction::of`]) are taken out of the stream -- but what the composer
//! holds means something different here: it is a search, not a command being
//! typed, so the slash popup does not open while the catalog is up.
//!
//! # What it looks like
//!
//! A header -- `Commands N`, then one tab for every category plus `All`, the
//! active one in brackets -- a blank row, then one row per command in view:
//! two columns of indent, the name padded to the widest name **in the filtered
//! set** plus four, and the summary. Unlike the popup the catalog is only as
//! tall as what it lists, at most twenty commands, and an empty list is one
//! row, `No commands found.`. The title and the marked row are the bold hint
//! grey, every other tab and row is dim, and the space between a name and its
//! summary carries no colour at all.
//!
//! # What it lists
//!
//! [`crate::interactive::SLASH_REGISTRY`], grouped by
//! [`crate::interactive::SLASH_CATEGORIES`] in upstream's order and in registry
//! order inside each group (`helpCatalogSpecAt`, `command_specs.zig:429-440`).
//! Only commands xfx answers, because a row here is an advertisement; a tab is
//! a category rather than a command, so a tab with nothing on it is shown and
//! says `Commands 0` -- upstream's `Agents` tab is one too.
//!
//! The filter is upstream's (`helpCatalogSpecMatches`, `:457-473`): the
//! composer split at whitespace, and every word a case-insensitive substring of
//! the name, an alias, the summary or the category. Upstream also searches a
//! `help_entry` (`/model <id-or-query>`); xfx's registry has no such field and
//! its summaries carry what argument grammar it has (`/model <id>`), so the
//! words that would have found a command there mostly find it here too.

use crate::interactive::{SlashSpec, SLASH_CATEGORIES, SLASH_REGISTRY};

use super::picker::{cells, edge_start, ellipsized, pad_to};
use super::theme::Palette;

/// The header and the blank row under it (`roomy_header_rows`,
/// `help_menu_presentation.zig:13`).
const HEADER_ROWS: u16 = 2;

/// The most commands shown at once (`max_visible_items`, `:14`).
pub(crate) const MOST_ITEMS: usize = 20;

/// The most rows the catalog is given: upstream sizes it with the resume
/// picker's cap (`surface_frame.zig:550-555`, `expanded_picker_row_budget`
/// over `resume_menu_presentation.max_inline_rows`), a header row, a gap row
/// and twenty items.
const MOST_ROWS: u16 = HEADER_ROWS + 20;

/// The band's rows the catalog cannot have (`inlinePickerRowBudgetCapped`'s
/// `fixed_rows`, `picker_presentation.zig:766`), before the composer's extra
/// rows are added to them.
const FIXED_ROWS: u16 = 5;

/// The document rows left on any screen tall enough to have them
/// (`minimum_transcript_rows`, `picker_presentation.zig:767`).
const MINIMUM_TRANSCRIPT_ROWS: u16 = 5;

/// What every command row starts with.
const INDENT: usize = 2;

/// The columns between the widest name and the summaries
/// (`inline_picker_column_gap_width`, `picker_presentation.zig:17`).
const COLUMN_GAP: usize = 4;

/// What separates the title and the tabs from each other.
const TAB_GAP: &str = "  ";

/// The tab that is every category at once.
const ALL: &str = "All";

/// What a list with nothing in it says (`composeEmptyRow`, `:269-276`).
const EMPTY: &str = "No commands found.";

/// What stands for the tabs a narrow header had no room for.
const ELLIPSIS: &str = "\u{2026}";

/// What the hint row says while the catalog is up, widest first
/// (`composeHelpMenuHintRow`, `src/ui/footer/input_presentation.zig:705-718`).
///
/// The first one that fits the screen is the one shown, and the last is shown
/// -- cut by the painter -- when none does.
const KEY_HELP: [&str; 5] = [
    "\u{2191}\u{2193} navigate     tab category     enter open     esc close",
    "\u{2191}\u{2193} navigate  tab category  enter open  esc close",
    "\u{2191}\u{2193} move  tab category  enter  esc",
    "tab category  enter open  esc",
    "tab enter esc",
];

/// What one keystroke means to the catalog.
///
/// Its own vocabulary rather than [`super::input::Action`], for the reason
/// [`super::picker::PickerAction`] is one: the shell translates, so a key the
/// catalog does not bind goes on to the composer -- which is what makes typing
/// a search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HelpAction {
    /// The previous command, wrapping to the last. Up.
    Up,
    /// The next one, wrapping to the first. Down.
    Down,
    /// A page of the composer's row cap up or down -- `true` for down --
    /// wrapping to the other end past either (`routeHelpMenuMove` with a
    /// page's delta, `input_completion_runtime.zig:209-226`, `:449-467`).
    /// Page Up and Page Down.
    Page(bool),
    /// The next tab, wrapping to `All`. Tab.
    NextCategory,
    /// The previous one. Shift-Tab.
    PreviousCategory,
    /// Run or complete what is chosen (`submitHelpMenuSelection`,
    /// `app_input_runtime.zig:2732-2762`). Enter.
    Open,
    /// Close it and empty the search (`cancelHelpMenu`, `:3260-3262`). Escape.
    Close,
}

impl HelpAction {
    /// What an ordinary decoded action means to the catalog, and `None` for
    /// the keystrokes that go on to the composer.
    pub(crate) fn of(action: super::input::Action) -> Option<Self> {
        use super::input::Action;
        match action {
            // `C-k` and `C-j` walk it too (`routePickerControlByte`,
            // `app_input_runtime.zig:1220-1235`, through `routeHelpMenuMove`).
            Action::Up | Action::ControlK => Some(Self::Up),
            Action::Down | Action::ControlJ => Some(Self::Down),
            Action::PageUp => Some(Self::Page(false)),
            Action::PageDown => Some(Self::Page(true)),
            Action::Tab => Some(Self::NextCategory),
            Action::BackTab => Some(Self::PreviousCategory),
            Action::Submit => Some(Self::Open),
            Action::Escape => Some(Self::Close),
            _ => None,
        }
    }
}

/// The catalog's shape for one row budget (`Layout`, `:25-32`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Layout {
    /// Every row it takes, header included.
    rows: u16,
    /// Whether the header and its blank row are drawn.
    header: bool,
    /// The first command in view, as an index into the filtered list.
    first: usize,
    /// How many commands are in view.
    visible: usize,
    /// The marked command, as an index into the filtered list.
    selected: usize,
}

/// The catalog: which tab is up, which command is marked, and what the search
/// is.
///
/// The search is the composer's text, held here as of the last edit
/// ([`Self::set_query`]) so that the rows and the height are a function of this
/// value alone -- which is what lets the band ask for both without being handed
/// the composer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HelpMenu {
    /// The commands it lists from: the live registry, or a test's.
    registry: &'static [SlashSpec],
    /// An index into [`SLASH_CATEGORIES`], or `None` for `All`.
    category: Option<usize>,
    /// An index into the filtered list. Kept in range by the readers rather
    /// than by every writer, as upstream's is (`selected_index % count`).
    selected: usize,
    /// The first command in view, as the last move left it: the window
    /// scrolls only when the mark reaches an edge (`list_window.zig:50-62`).
    window_start: usize,
    /// The composer's text.
    query: String,
}

impl HelpMenu {
    /// A catalog on `All`, the first command marked, searching for `query`
    /// (`HelpMenu.open`, `command_specs.zig:197-199`).
    pub(crate) fn open(query: &str) -> Self {
        Self::over(SLASH_REGISTRY, query)
    }

    /// The same, over a registry named by the caller: the seam the scrolling is
    /// tested through, since the live registry fits on any screen tall enough
    /// for a band.
    fn over(registry: &'static [SlashSpec], query: &str) -> Self {
        Self {
            registry,
            category: None,
            selected: 0,
            window_start: 0,
            query: query.to_string(),
        }
    }

    /// Takes the composer's text as the search.
    ///
    /// A search that changed puts the mark back on the first command and the
    /// window back at the top (`resetForQuery`, `:205-208`, called for every
    /// edit at `app_input_runtime.zig:2935-2938`); a keystroke that left the
    /// text alone -- a caret move -- leaves both where they were.
    pub(crate) fn set_query(&mut self, query: &str) {
        if self.query != query {
            self.query = query.to_string();
            self.reset();
        }
    }

    /// Where the mark and the window are, for [`Self::keep`].
    pub(crate) fn mark(&self) -> (usize, usize) {
        (self.selected, self.window_start)
    }

    /// Puts the mark and the window back where [`Self::mark`] read them,
    /// over whatever the search is now: fx's `insert_newline` syncs no
    /// catalog menu (`app_input_runtime.zig:513-521@481104fd`), so its
    /// catalog keeps both across a line break; the mark is read modulo what
    /// is listed ([`Self::selected`]), as fx's is.
    pub(crate) fn keep(&mut self, (selected, window_start): (usize, usize)) {
        self.selected = selected;
        self.window_start = window_start;
    }

    /// The mark on the first command and the window at the top.
    fn reset(&mut self) {
        self.selected = 0;
        self.window_start = 0;
    }

    /// The next tab, or the previous one, wrapping through `All`
    /// (`cycleCategory`, `:210-225`). The mark goes back to the top.
    pub(crate) fn cycle(&mut self, forward: bool) {
        let count = SLASH_CATEGORIES.len() + 1;
        let current = self.category.map_or(0, |index| index + 1);
        let next = if forward {
            (current + 1) % count
        } else {
            (current + count - 1) % count
        };
        self.category = next.checked_sub(1);
        self.reset();
    }

    /// Moves the mark one command, wrapping at both ends, and scrolls a window
    /// of `visible` commands only as far as keeps it in view (`move`,
    /// `:227-243`). `false`, and nothing moved, when there is nothing listed.
    #[cfg(test)]
    pub(crate) fn step(&mut self, down: bool, visible: u16) -> bool {
        self.step_by(if down { 1 } else { -1 }, visible)
    }

    /// The mark `delta` rows on, wrapping to the other end past either
    /// (`HelpMenu.move`, `command_specs.zig:232-248`).
    pub(crate) fn step_by(&mut self, delta: i64, visible: u16) -> bool {
        let count = self.items().len();
        if count == 0 {
            return false;
        }
        self.selected = super::picker::advanced(self.selected, count, delta);
        self.window_start = edge_start(self.window_start, count, self.selected, visible.max(1));
        true
    }

    /// The marked command, or `None` when nothing is listed
    /// (`selectedSpec`, `:245-249`).
    pub(crate) fn selected(&self) -> Option<&'static SlashSpec> {
        let items = self.items();
        let count = items.len();
        (count > 0).then(|| items[self.selected % count])
    }

    /// What the catalog lists: the active tab's commands -- every tab's, in
    /// category order, on `All` -- that the search finds.
    fn items(&self) -> Vec<&'static SlashSpec> {
        let words: Vec<String> = self
            .query
            .split([' ', '\t', '\r', '\n'])
            .filter(|word| !word.is_empty())
            .map(str::to_ascii_lowercase)
            .collect();
        let categories = match self.category {
            Some(index) => &SLASH_CATEGORIES[index..=index],
            None => SLASH_CATEGORIES,
        };
        let registry = self.registry;
        categories
            .iter()
            .flat_map(|category| {
                registry
                    .iter()
                    .filter(move |spec| spec.category == *category)
            })
            .filter(|spec| found(spec, &words))
            .collect()
    }

    /// The catalog's shape in `max_rows` rows (`buildLayout`, `:70-103`).
    ///
    /// The header only when there are more than two rows; an empty list is one
    /// row under it; otherwise the window starts where the last move left it,
    /// pulled back to the mark if the mark is above it and walked down until
    /// the mark is in it.
    fn layout(&self, count: usize, max_rows: u16) -> Layout {
        if max_rows == 0 {
            return Layout::default();
        }
        let header = max_rows > HEADER_ROWS;
        let body_start = if header { HEADER_ROWS } else { 0 };
        if count == 0 {
            return Layout {
                rows: max_rows.min(body_start + 1),
                header,
                ..Layout::default()
            };
        }
        let selected = self.selected % count;
        let budget = usize::from(max_rows - body_start);
        // `measureBody`: from `first`, as many commands as the rows and the
        // cap allow and the list still has.
        let measure = |first: usize| (count - first).min(budget).min(MOST_ITEMS);
        let mut first = self.window_start.min(count - 1).min(selected);
        let mut visible = measure(first);
        while selected >= first + visible && first < selected {
            first += 1;
            visible = measure(first);
        }
        if visible == 0 {
            first = selected;
            visible = measure(first);
        }
        // At most twenty commands under at most two rows of header.
        let shown = u16::try_from(visible).unwrap_or(u16::MAX);
        Layout {
            rows: body_start + shown,
            header,
            first,
            visible,
            selected,
        }
    }

    /// How many rows the band has to give it on a screen `terminal_rows` tall
    /// whose composer is `input_extra` rows taller than one (`menuRowCount`
    /// under `inlinePickerRowBudgetCapped`).
    pub(crate) fn height(&self, terminal_rows: u16, input_extra: u16) -> u16 {
        self.layout(self.items().len(), row_budget(terminal_rows, input_extra))
            .rows
    }

    /// How many commands are in view in `max_rows` rows, and never fewer than
    /// one: the window a move scrolls against
    /// (`visibleNavigationItemsForBudget`, `:38-44`).
    pub(crate) fn visible_items(&self, max_rows: u16) -> u16 {
        let visible = self.layout(self.items().len(), max_rows).visible;
        u16::try_from(visible.max(1)).unwrap_or(u16::MAX)
    }

    /// The catalog's rows in `max_rows` rows of a `cols`-wide screen, top
    /// first: exactly as many as [`Self::height`] gave the band when `max_rows`
    /// is that height (`composeHelpMenuRow`, `:46-68`).
    pub(crate) fn rows(&self, cols: u16, max_rows: u16, palette: Palette) -> Vec<String> {
        let items = self.items();
        let layout = self.layout(items.len(), max_rows);
        let mut rows = Vec::with_capacity(usize::from(layout.rows));
        if layout.rows == 0 {
            return rows;
        }
        if layout.header {
            rows.push(self.header(items.len(), cols, palette));
            rows.push(String::new());
        }
        if items.is_empty() {
            rows.push(format!(
                "{}{}{}",
                palette.dim(),
                ellipsized(EMPTY, usize::from(cols)),
                palette.reset()
            ));
            return rows;
        }
        let column = description_column(&items, cols);
        for (offset, spec) in items[layout.first..layout.first + layout.visible]
            .iter()
            .enumerate()
        {
            let marked = layout.first + offset == layout.selected;
            rows.push(item_row(spec, marked, column, cols, palette));
        }
        rows
    }

    /// `Commands N` and the tabs (`composeHeaderRow`, `:139-192`).
    ///
    /// Every tab when they fit. When they do not, the longest prefix of them
    /// that fits beside a dim `…` -- and the active tab after the ellipsis when
    /// the prefix cut it, so the tab that is up is always on the row. Narrower
    /// still, the title and the active tab; then the active tab alone.
    fn header(&self, count: usize, cols: u16, palette: Palette) -> String {
        let width = usize::from(cols);
        let title = format!("{}Commands {count}{}", palette.subtitle(), palette.reset());
        let labels: Vec<&str> = std::iter::once(ALL)
            .chain(SLASH_CATEGORIES.iter().copied())
            .collect();
        let active = self.category.map_or(0, |index| index + 1);
        let tab = |index: usize| painted_tab(labels[index], index == active, palette);

        let mut wide = title.clone();
        for index in 0..labels.len() {
            wide.push_str(TAB_GAP);
            wide.push_str(&tab(index));
        }
        if cells(&wide) <= width {
            return wide;
        }
        for prefix in (1..labels.len()).rev() {
            let mut packed = title.clone();
            for index in 0..prefix {
                packed.push_str(TAB_GAP);
                packed.push_str(&tab(index));
            }
            packed.push_str(TAB_GAP);
            packed.push_str(palette.dim());
            packed.push_str(ELLIPSIS);
            packed.push_str(palette.reset());
            if active >= prefix {
                packed.push_str(TAB_GAP);
                packed.push_str(&tab(active));
            }
            if cells(&packed) <= width {
                return packed;
            }
        }
        let compact = format!("{title}{TAB_GAP}{}", tab(active));
        if cells(&compact) <= width {
            return compact;
        }
        // Only on a screen narrower than any band fits on: the active tab, cut
        // inside its colour so the reset is never what the cut drops.
        format!(
            "{}[{}]{}",
            palette.subtitle(),
            super::frame::clip(labels[active], cols.saturating_sub(2)),
            palette.reset()
        )
    }
}

/// One tab: the active one bold in brackets, the rest dim
/// (`appendCategoryTab`, `:211-222`).
fn painted_tab(label: &str, active: bool, palette: Palette) -> String {
    if active {
        format!("{}[{label}]{}", palette.subtitle(), palette.reset())
    } else {
        format!("{}{label}{}", palette.dim(), palette.reset())
    }
}

/// One command's row (`composeCommandRow`, `:224-254`).
///
/// The indent, then the name in the row's style and closed, uncoloured spaces
/// out to the summary column, then the summary in the same style and closed.
/// A name too wide for its column is cut leaving the gap; a summary too wide
/// for the screen is cut at its edge.
fn item_row(spec: &SlashSpec, marked: bool, column: usize, cols: u16, palette: Palette) -> String {
    let width = usize::from(cols);
    let indent = if width <= INDENT { 0 } else { INDENT };
    let gap = if column >= indent + COLUMN_GAP {
        COLUMN_GAP
    } else {
        0
    };
    let style = if marked {
        palette.subtitle()
    } else {
        palette.dim()
    };
    let mut row = " ".repeat(indent);
    row.push_str(style);
    row.push_str(&ellipsized(
        spec.name,
        column.saturating_sub(indent).saturating_sub(gap),
    ));
    row.push_str(palette.reset());
    pad_to(&mut row, column);
    row.push_str(style);
    row.push_str(&ellipsized(spec.summary, width.saturating_sub(column)));
    row.push_str(palette.reset());
    row
}

/// Where the summaries start: past the indent, the widest name **in the
/// filtered list** -- not the window -- and the gap (`descriptionColumn`,
/// `:256-267`). So the column moves as the search narrows the list, and does
/// not move as the window scrolls.
fn description_column(items: &[&SlashSpec], cols: u16) -> usize {
    let width = usize::from(cols);
    let indent = if width <= INDENT { 0 } else { INDENT };
    let widest = items.iter().map(|spec| cells(spec.name)).max().unwrap_or(0);
    (indent + widest + COLUMN_GAP).min(width)
}

/// Whether every word of the search is somewhere in `spec`.
fn found(spec: &SlashSpec, words: &[String]) -> bool {
    words.iter().all(|word| {
        [spec.name, spec.summary, spec.category]
            .into_iter()
            .chain(spec.aliases.iter().copied())
            .any(|text| text.to_ascii_lowercase().contains(word.as_str()))
    })
}

/// The rows a screen `terminal_rows` tall gives the catalog when its composer
/// is `input_extra` rows taller than one (`inlinePickerRowBudgetCapped`,
/// `picker_presentation.zig:760-770`): what is left after the band's fixed
/// rows and five rows of document, at most [`MOST_ROWS`] and never none.
pub(super) fn row_budget(terminal_rows: u16, input_extra: u16) -> u16 {
    terminal_rows
        .saturating_sub(FIXED_ROWS.saturating_add(input_extra))
        .saturating_sub(MINIMUM_TRANSCRIPT_ROWS)
        .clamp(1, MOST_ROWS)
}

/// What the hint row says while the catalog is up, on a screen `cols` wide.
///
/// The widest of [`KEY_HELP`] that fits, and the narrowest when none does.
pub(crate) fn key_help(cols: u16) -> &'static str {
    KEY_HELP
        .iter()
        .copied()
        .find(|candidate| cells(candidate) <= usize::from(cols))
        .unwrap_or(KEY_HELP[KEY_HELP.len() - 1])
}

/// Whether the composer, as it stands, already names a command -- its own name
/// or an alias, with nothing after it but spaces or tabs
/// (`matchExact`/`matchesCommandToken`, `src/core/mods/registry.zig:59-68`).
///
/// What decides Enter's first branch: a command typed out in full is run as
/// typed, whatever row the search left marked.
pub(crate) fn names_exactly(text: &str) -> bool {
    let command = text.trim_end_matches([' ', '\t']);
    SLASH_REGISTRY
        .iter()
        .any(|spec| spec.name == command || spec.aliases.contains(&command))
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::interactive::Slash;

    use super::super::theme::Mode;

    /// The palette every row below is painted in: an undecided terminal's.
    const PALETTE: Palette = Palette { mode: Mode::Dark };

    /// Upstream's `selected_completion_style` (`core/shared/theme.zig:97`):
    /// the title, the active tab and the marked row.
    const BRIGHT: &str = "\u{1b}[1;38;5;255m";

    /// Upstream's `dim_style` (`core/shared/theme.zig:85`).
    const DIM: &str = "\u{1b}[38;5;245m";

    const RESET: &str = "\u{1b}[0m";

    /// A registry too long for a short screen: thirty commands, all General.
    static LONG: &[SlashSpec] = &[
        named("/c00"),
        named("/c01"),
        named("/c02"),
        named("/c03"),
        named("/c04"),
        named("/c05"),
        named("/c06"),
        named("/c07"),
        named("/c08"),
        named("/c09"),
        named("/c10"),
        named("/c11"),
        named("/c12"),
        named("/c13"),
        named("/c14"),
        named("/c15"),
        named("/c16"),
        named("/c17"),
        named("/c18"),
        named("/c19"),
        named("/c20"),
        named("/c21"),
        named("/c22"),
        named("/c23"),
        named("/c24"),
        named("/c25"),
        named("/c26"),
        named("/c27"),
        named("/c28"),
        named("/the-widest-name"),
    ];

    const fn named(name: &'static str) -> SlashSpec {
        SlashSpec {
            command: Slash::Help,
            name,
            aliases: &[],
            summary: "a summary",
            category: "General",
            has_args: false,
        }
    }

    /// The text of a row with its colours taken out.
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

    /// The names a catalog lists, in order.
    fn names(menu: &HelpMenu) -> Vec<&'static str> {
        menu.items().iter().map(|spec| spec.name).collect()
    }

    /// The names on the rows of a painted catalog, after its header.
    fn shown(rows: &[String]) -> Vec<String> {
        rows.iter()
            .skip(2)
            .map(|row| {
                plain(row)
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .to_string()
            })
            .collect()
    }

    /// The column a command row's summary starts in.
    fn summary_column(row: &str) -> usize {
        let text = plain(row);
        let name_end = 2 + text[2..].find(' ').expect("a gap after the name");
        name_end + (text[name_end..].len() - text[name_end..].trim_start().len())
    }

    /// The registry's names in a category, in registry order.
    fn in_category(category: &str) -> Vec<&'static str> {
        SLASH_REGISTRY
            .iter()
            .filter(|spec| spec.category == category)
            .map(|spec| spec.name)
            .collect()
    }

    /// The widest canonical name, which the summary column is measured from.
    fn widest() -> usize {
        SLASH_REGISTRY
            .iter()
            .map(|spec| spec.name.len())
            .max()
            .expect("a registry")
    }

    /// `All`: every category's names, in the categories' order.
    fn by_category() -> Vec<&'static str> {
        SLASH_CATEGORIES
            .iter()
            .flat_map(|category| in_category(category))
            .collect()
    }

    /// A catalog with `tabs` Tabs pressed.
    fn on_tab(tabs: usize) -> HelpMenu {
        let mut menu = HelpMenu::open("");
        for _ in 0..tabs {
            menu.cycle(true);
        }
        menu
    }

    #[test]
    fn all_lists_the_registry_by_category_in_upstreams_order() {
        // General's four in registry order, then Session, Account and Model:
        // the order of the tabs, not the order of the registry.
        assert_eq!(names(&HelpMenu::open("")), by_category());
        assert_eq!(names(&HelpMenu::open(""))[..2], ["/help", "/clear"]);
        for (tab, category) in SLASH_CATEGORIES.iter().enumerate() {
            assert_eq!(names(&on_tab(tab + 1)), in_category(category), "{category}");
        }
        // Every command is on exactly one tab, and `All` is the sum of them.
        let total: usize = (1..=SLASH_CATEGORIES.len())
            .map(|tabs| on_tab(tabs).items().len())
            .sum();
        assert_eq!(total, SLASH_REGISTRY.len());
    }

    #[test]
    fn tab_and_shift_tab_cycle_through_all_and_back_and_put_the_mark_on_top() {
        let mut menu = HelpMenu::open("");
        assert!(menu.step(true, 20));
        assert!(menu.step(true, 20));
        menu.cycle(true);
        assert_eq!(menu.category, Some(0), "General");
        assert_eq!((menu.selected, menu.window_start), (0, 0));
        // Shift-Tab twice from General is the last tab, through `All`.
        menu.cycle(false);
        assert_eq!(menu.category, None);
        menu.cycle(false);
        assert_eq!(menu.category, Some(SLASH_CATEGORIES.len() - 1), "Product");
        // And Tab from the last tab wraps to `All`.
        menu.cycle(true);
        assert_eq!(menu.category, None);
    }

    #[test]
    fn the_header_shows_every_tab_when_they_fit() {
        // `Commands 7` and the twelve tabs are exactly 120 cells, so upstream's
        // own rule (`<= width`) shows all of them.
        let menu = HelpMenu::open("");
        let row = menu.header(7, 120, PALETTE);
        let mut expected = format!("{BRIGHT}Commands 7{RESET}  {BRIGHT}[All]{RESET}");
        for label in SLASH_CATEGORIES {
            expected.push_str(&format!("  {DIM}{label}{RESET}"));
        }
        assert_eq!(row, expected);
        assert_eq!(cells(&row), 120);
        // One cell narrower and it packs.
        assert!(plain(&menu.header(7, 119, PALETTE)).ends_with("Extensions  \u{2026}"));
    }

    #[test]
    fn a_narrow_header_packs_a_prefix_and_an_ellipsis_and_keeps_the_active_tab() {
        let all = HelpMenu::open("").header(7, 80, PALETTE);
        assert_eq!(
            plain(&all),
            "Commands 7  [All]  General  Session  Account  Model  Appearance  Security  \u{2026}"
        );
        assert!(
            all.ends_with(&format!("  {DIM}\u{2026}{RESET}")),
            "the ellipsis is dim and closed: {all:?}"
        );
        assert!(cells(&all) <= 80);
        // The active tab past the prefix comes after the ellipsis, bracketed
        // and bright.
        let product = on_tab(SLASH_CATEGORIES.len()).header(0, 80, PALETTE);
        assert_eq!(
            plain(&product),
            "Commands 0  All  General  Session  Account  Model  Appearance  \u{2026}  [Product]"
        );
        assert!(product.ends_with(&format!("{BRIGHT}[Product]{RESET}")));
        // Too narrow for any prefix: the title and the active tab.
        let compact = on_tab(10).header(0, 24, PALETTE);
        assert_eq!(plain(&compact), "Commands 0  [Extensions]");
        // Narrower still: the active tab alone.
        assert_eq!(plain(&on_tab(10).header(0, 20, PALETTE)), "[Extensions]");
    }

    #[test]
    fn the_filter_wants_every_word_somewhere_and_ignores_case() {
        let found = |query: &str| names(&HelpMenu::open(query));
        // The category label, the summary and the name are all searched.
        assert_eq!(found("general"), in_category("General"));
        // `/status` by its summary's "session ID" (`commands.zig:455@481104fd`).
        assert_eq!(
            found("SESSION"),
            [
                "/status", "/new", "/reset", "/resume", "/rename", "/fork", "/undo", "/copy",
                "/compact"
            ]
        );
        // `/clear` by its summary's "processes", as upstream's `ses` finds it
        // (`fx-misc-40x120-help-query-ses.txt`).
        assert_eq!(
            found("ses"),
            [
                "/clear", "/status", "/new", "/reset", "/resume", "/rename", "/fork", "/undo",
                "/copy", "/compact"
            ]
        );
        assert_eq!(found("/version"), ["/version"]);
        // An alias finds its command, under its own name.
        assert_eq!(found("exit"), ["/quit"]);
        // Every word must be found, each anywhere: `/compact`'s "fresh
        // window" under Session finds it beside `/new`.
        assert_eq!(found("session fresh"), ["/new", "/compact"]);
        assert_eq!(found("  fresh \t session "), ["/new", "/compact"]);
        assert_eq!(found("fresh session start"), ["/new"]);
        // `/provider`'s alias finds it as `/exit` finds `/quit`; its summary
        // is upstream's now and names no provider of xfx's.
        assert_eq!(found("setup"), ["/provider"]);
        // `/usage`'s v0.0.16 summary, "show your provider's usage and spend"
        // (`commands.zig:454@481104fd`), has the word too.
        assert_eq!(found("provider"), ["/provider", "/usage"]);
        assert!(found("sessionzz").is_empty());
        // Nothing typed lists everything.
        assert_eq!(found("").len(), SLASH_REGISTRY.len());
        assert_eq!(found(" ").len(), SLASH_REGISTRY.len());
    }

    #[test]
    fn the_catalog_opens_with_its_header_a_blank_row_and_every_command_it_has_room_for() {
        let menu = HelpMenu::open("");
        // Upstream shows at most twenty at once (`max_visible_items`); past
        // that the window scrolls, so a longer registry fills it and no more.
        let visible = SLASH_REGISTRY.len().min(MOST_ITEMS);
        assert_eq!(
            usize::from(menu.height(40, 0)),
            2 + visible,
            "only as tall as what it lists, up to upstream's cap"
        );
        let rows = menu.rows(120, menu.height(40, 0), PALETTE);
        assert_eq!(rows.len(), 2 + visible);
        assert_eq!(rows[1], "", "the blank row under the header");
        // The marked row: name and summary bright, each closed, the spaces
        // between them uncoloured; the summary at 2 + the widest name + 4 --
        // `/permissions` since it was added, so read off the registry rather
        // than spelled.
        let column = 2 + widest() + 4;
        assert_eq!(
            rows[2],
            format!(
                "  {BRIGHT}/help{RESET}{}{BRIGHT}show available slash commands{RESET}",
                " ".repeat(column - 7)
            )
        );
        assert_eq!(
            rows[3],
            format!(
                "  {DIM}/clear{RESET}{}{DIM}{}{RESET}",
                " ".repeat(column - 8),
                SLASH_REGISTRY[1].summary
            )
        );
        assert_eq!(shown(&rows), by_category()[..visible]);
        for row in &rows[3..] {
            assert!(row.starts_with(&format!("  {DIM}")), "{row:?}");
            assert_eq!(summary_column(row), column, "{row:?}");
        }
    }

    #[test]
    fn the_summary_column_is_measured_over_the_filtered_list_not_the_window() {
        // The search narrows the list, and the column comes in with it.
        let help = HelpMenu::open("slash");
        assert_eq!(names(&help), ["/help"]);
        let rows = help.rows(120, help.height(40, 0), PALETTE);
        assert_eq!(summary_column(&rows[2]), 2 + 5 + 4);
        // A window too short for the widest name still leaves room for it:
        // fifteen rows give three commands, `/the-widest-name` is the
        // thirtieth, and the column is measured to it anyway.
        let menu = HelpMenu::over(LONG, "");
        assert_eq!(
            menu.height(15, 0),
            5,
            "a header, its blank row, three commands"
        );
        let rows = menu.rows(120, 5, PALETTE);
        assert_eq!(shown(&rows), ["/c00", "/c01", "/c02"]);
        assert_eq!(summary_column(&rows[2]), 2 + "/the-widest-name".len() + 4);
    }

    #[test]
    fn up_and_down_wrap_and_the_window_follows_the_mark() {
        let mut menu = HelpMenu::over(LONG, "");
        let visible = menu.visible_items(5);
        assert_eq!(visible, 3);
        // Up from the first command is the last, with the window at the end.
        assert!(menu.step(false, visible));
        let rows = menu.rows(120, 5, PALETTE);
        assert_eq!(shown(&rows), ["/c27", "/c28", "/the-widest-name"]);
        assert!(rows[4].starts_with(&format!("  {BRIGHT}")), "{rows:?}");
        // Down from the last is the first, and the window goes with it.
        assert!(menu.step(true, visible));
        let rows = menu.rows(120, 5, PALETTE);
        assert_eq!(shown(&rows), ["/c00", "/c01", "/c02"]);
        assert!(rows[2].starts_with(&format!("  {BRIGHT}")), "{rows:?}");
        // The window scrolls only at its edge: three Downs, then one Up.
        for _ in 0..3 {
            assert!(menu.step(true, visible));
        }
        assert_eq!(shown(&menu.rows(120, 5, PALETTE)), ["/c01", "/c02", "/c03"]);
        assert!(menu.step(false, visible));
        let rows = menu.rows(120, 5, PALETTE);
        assert_eq!(shown(&rows), ["/c01", "/c02", "/c03"]);
        assert!(rows[3].starts_with(&format!("  {BRIGHT}")), "{rows:?}");
        // The live registry wraps the same way: up from the first is the last
        // in the catalog's order.
        let mut live = HelpMenu::open("");
        assert!(live.step(false, 20));
        assert_eq!(
            live.selected().map(|spec| spec.name),
            by_category().last().copied()
        );
        assert!(live.step(true, 20));
        assert_eq!(live.selected().map(|spec| spec.name), Some("/help"));
    }

    #[test]
    fn a_new_search_puts_the_mark_back_on_top_and_a_caret_move_does_not() {
        let mut menu = HelpMenu::open("");
        assert!(menu.step(true, 20));
        menu.set_query("");
        assert_eq!(menu.selected, 1, "the same text is not a new search");
        menu.set_query("s");
        assert_eq!((menu.selected, menu.window_start), (0, 0));
    }

    #[test]
    fn an_empty_tab_says_so_and_has_nothing_to_open() {
        // Agents: upstream's own empty tab, and xfx files nothing under it.
        let mut agents = on_tab(9);
        assert_eq!(agents.category, Some(8));
        assert_eq!(agents.height(40, 0), 3);
        let rows = agents.rows(120, 3, PALETTE);
        assert!(plain(&rows[0]).starts_with("Commands 0  All"), "{rows:?}");
        assert!(plain(&rows[0]).contains("[Agents]"), "{rows:?}");
        assert_eq!(rows[1], "");
        // At column zero, dim, closed.
        assert_eq!(rows[2], format!("{DIM}No commands found.{RESET}"));
        assert_eq!(agents.selected(), None);
        assert!(!agents.step(true, 20), "a move with nothing to move over");
        // And a search that finds nothing is the same row.
        let none = HelpMenu::open("sessionzz");
        assert_eq!(
            none.rows(120, none.height(40, 0), PALETTE)[2],
            format!("{DIM}No commands found.{RESET}")
        );
    }

    #[test]
    fn rows_and_height_come_from_one_layout_on_every_screen() {
        for (rows, cols) in [(40_u16, 120_u16), (24, 80), (7, 20), (11, 30), (13, 40)] {
            for query in ["", "s", "/version", "zz"] {
                for tabs in [0, 1, 9] {
                    let mut menu = HelpMenu::open(query);
                    for _ in 0..tabs {
                        menu.cycle(true);
                    }
                    for input_extra in [0, 2] {
                        let height = menu.height(rows, input_extra);
                        let painted = menu.rows(cols, height, PALETTE);
                        assert_eq!(usize::from(height), painted.len(), "{query} {rows}x{cols}");
                        assert!(height >= 1, "never refused");
                        assert!(
                            painted
                                .iter()
                                .all(|row| super::super::wrap::width(row) <= cols),
                            "{query} on {rows}x{cols}: {painted:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn the_budget_is_upstreams_expanded_one() {
        // Five fixed rows, the composer's extra rows and five rows of document
        // off the screen; at most 22, at least one.
        for (rows, extra, budget) in [
            (40, 0, 22),
            (24, 0, 14),
            (24, 3, 11),
            (12, 0, 2),
            (11, 0, 1),
            (7, 0, 1),
            (60, 0, 22),
        ] {
            assert_eq!(
                row_budget(rows, extra),
                budget,
                "{rows} rows, {extra} extra"
            );
        }
        // Two rows or fewer: no header, the commands themselves.
        let menu = HelpMenu::open("");
        assert_eq!(menu.height(12, 0), 2);
        let rows = menu.rows(80, 2, PALETTE);
        assert!(rows[0].starts_with(&format!("  {BRIGHT}/help")), "{rows:?}");
        assert!(plain(&rows[1]).starts_with("  /clear"), "{rows:?}");
    }

    #[test]
    fn a_name_too_wide_for_a_narrow_screen_is_cut_before_the_summary_is() {
        let menu = HelpMenu::over(LONG, "widest");
        let rows = menu.rows(20, menu.height(24, 0), PALETTE);
        let row = plain(&rows[2]);
        // The column is the screen's width at most, so the name keeps its gap
        // and the summary gets nothing.
        assert_eq!(row, "  /the-widest-n\u{2026}    ");
        assert!(cells(&rows[2]) <= 20);
    }

    #[test]
    fn the_key_help_is_upstreams_and_fits_the_screen() {
        assert_eq!(
            key_help(120),
            "\u{2191}\u{2193} navigate     tab category     enter open     esc close"
        );
        assert_eq!(
            key_help(50),
            "\u{2191}\u{2193} navigate  tab category  enter open  esc close"
        );
        assert_eq!(
            key_help(40),
            "\u{2191}\u{2193} move  tab category  enter  esc"
        );
        assert_eq!(key_help(30), "tab category  enter open  esc");
        assert_eq!(key_help(20), "tab enter esc");
        for cols in 13..80 {
            assert!(cells(key_help(cols)) <= usize::from(cols), "{cols}");
        }
    }

    #[test]
    fn a_command_typed_out_in_full_is_exact_and_a_prefix_is_not() {
        assert!(names_exactly("/version"));
        assert!(names_exactly("/version \t"));
        assert!(names_exactly("/exit"), "an alias is a name");
        assert!(!names_exactly("/vers"));
        assert!(
            !names_exactly(" /version"),
            "only trailing blanks are forgiven"
        );
        assert!(!names_exactly("/version now"));
        assert!(!names_exactly("version"));
        assert!(!names_exactly(""));
    }

    #[test]
    fn the_catalog_binds_six_keys_and_lets_the_rest_through() {
        use super::super::input::Action;
        assert_eq!(HelpAction::of(Action::Up), Some(HelpAction::Up));
        assert_eq!(HelpAction::of(Action::Down), Some(HelpAction::Down));
        assert_eq!(HelpAction::of(Action::Tab), Some(HelpAction::NextCategory));
        assert_eq!(
            HelpAction::of(Action::BackTab),
            Some(HelpAction::PreviousCategory)
        );
        assert_eq!(HelpAction::of(Action::Submit), Some(HelpAction::Open));
        assert_eq!(HelpAction::of(Action::Escape), Some(HelpAction::Close));
        for typing in [
            Action::Backspace,
            Action::KillToStart,
            Action::Left,
            Action::Cancel,
            Action::Eof,
        ] {
            assert_eq!(HelpAction::of(typing), None, "{typing:?}");
        }
    }
}
