//! `/effort`'s menu: the reasoning effort levels the current model offers, in
//! the band under the composer.
//!
//! Upstream's Effort menu (`vercel-labs/fx@481104fd`, state
//! `src/core/input/effort_menu.zig`, rows
//! `src/ui/footer/effort_menu_presentation.zig`, key help
//! `src/ui/footer/input_presentation.zig:671-702`), in the slot `/model`'s menu
//! uses ([`super::model_menu`]): between the rule under the composer and the
//! bottom rule, with its own key help where the hint row was.
//!
//! Unlike `/model`'s menu it is **not** a view of the composer: it reads no
//! search, and an edit to the composer closes it while the edit still applies
//! (`dismissActiveMenusForComposerEdit`, `app_input_runtime.zig:1984-1990`).
//! Only the keys [`EffortAction::of`] names are taken out of the stream.
//!
//! # What it looks like
//!
//! `Effort N` in the bold hint grey and, two cells after it, the model in the
//! dim grey, its middle given up when it does not fit; a blank row; then one
//! row per level -- `default` first, then the model's levels in the order its
//! catalog row lists them -- the marked one bold, the rest dim, and `current`
//! in the dim grey two cells past the widest label on the row of the level in
//! use. A short footer gives up the blank row first, then the title, before it
//! gives up a level, and the levels then scroll with the mark
//! (`Layout.build`, `effort_menu_presentation.zig:17-39`).

use crate::provider::effort::Effort;
use crate::provider::model::CatalogEntry;

use super::input::Action;
use super::model_menu::middle_ellipsized;
use super::picker::{cells, edge_start, ellipsized, pad_to};
use super::theme::Palette;

/// The title row (`header_rows`, `effort_menu_presentation.zig:17`).
const HEADER_ROWS: u16 = 1;

/// The blank row under it (`gap_rows`, `:18`).
const GAP_ROWS: u16 = 1;

/// `default` and the most levels a model can declare
/// (`effort_menu.max_options`, `effort_menu.zig:11`:
/// `ReasoningEffort.max_options + 1`, `types.zig:2416`).
const MOST_OPTIONS: usize = 17;

/// The most rows the menu is given (`max_inline_rows`, `:20`).
const MOST_ROWS: u16 = HEADER_ROWS + GAP_ROWS + MOST_OPTIONS as u16;

/// The band's rows the menu cannot have, before the composer's extra rows
/// are added to them (`inlinePickerRowBudgetCapped`'s `fixed_rows`,
/// `picker_presentation.zig:799`).
const FIXED_ROWS: u16 = 5;

/// The document rows left on any screen tall enough to have them
/// (`minimum_transcript_rows`, `picker_presentation.zig:800`).
const MINIMUM_TRANSCRIPT_ROWS: u16 = 5;

/// What every level's row starts with, on a screen wider than it.
const INDENT: usize = 2;

/// What marks the level in use (`current_label`, `:19`).
const CURRENT: &str = "current";

/// What the hint row says while the menu is up, widest first
/// (`composeEffortMenuHintRow`, `input_presentation.zig:684-690`). The
/// fourth is never reached once the third does not fit; upstream lists it,
/// and so does this.
const KEY_HELP: [&str; 5] = [
    "\u{2191}\u{2193} navigate     enter use     esc close",
    "\u{2191}\u{2193} navigate  enter use  esc close",
    "\u{2191}\u{2193} move  enter  esc",
    "enter use  esc close",
    "enter esc",
];

/// What one keystroke means to the menu.
///
/// Its own vocabulary, as [`super::model_menu::ModelAction`] is: the shell
/// translates, and a key the menu does not bind goes on to the composer --
/// where an edit closes the menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EffortAction {
    /// The previous level, wrapping to the last. Up, `C-k`.
    Up,
    /// The next one, wrapping to the first. Down, `C-j`.
    Down,
    /// A page of the composer's row cap, `true` for down, wrapping as a step
    /// does (`move` takes any delta with `@mod`, `effort_menu.zig:53-57`).
    /// Page Up and Page Down.
    Page(bool),
    /// Apply the marked level (`submitEffortMenuSelection`,
    /// `app_input_runtime.zig:2876-2884`). Enter.
    Use,
    /// Close it (`cancelEffortMenu`, `:2869-2873`). Escape.
    Close,
}

impl EffortAction {
    /// What an ordinary decoded action means to the menu, and `None` for the
    /// keystrokes that go on to the composer.
    pub(crate) fn of(action: Action) -> Option<Self> {
        match action {
            // `C-k` and `C-j` walk it too (`routePickerControlByte` ->
            // `routeVisiblePickerMove` -> `routeEffortMenuMove`,
            // `input_completion_runtime.zig:396-400`).
            Action::Up | Action::ControlK => Some(Self::Up),
            Action::Down | Action::ControlJ => Some(Self::Down),
            Action::PageUp => Some(Self::Page(false)),
            Action::PageDown => Some(Self::Page(true)),
            Action::Submit => Some(Self::Use),
            Action::Escape => Some(Self::Close),
            _ => None,
        }
    }
}

/// The menu while it is up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EffortMenu {
    /// The model whose levels it lists, as the title shows it: one line, with
    /// nothing in it a terminal would obey.
    model: String,
    /// `default`, then the model's levels in the order its row lists them.
    options: Vec<Effort>,
    /// The level in use when it opened.
    current: usize,
    /// The marked level.
    selected: usize,
    /// The first level shown when they do not all fit.
    window_start: usize,
}

impl EffortMenu {
    /// The menu over `entry`'s levels for `model`, `current` marked and
    /// highlighted -- or `default` when the model does not offer it
    /// (`State.open`, `effort_menu.zig:30-49`). `None` for a row that lists no
    /// level, which has no menu.
    ///
    /// A level is offered only when it is one [`Effort::parse`] reads as a
    /// name, at most sixteen of them: so every label the menu draws is
    /// letters, digits, `-`, `_` and `.`, whatever a catalog sent.
    pub(crate) fn open(model: &str, entry: &CatalogEntry, current: &Effort) -> Option<Self> {
        let options: Vec<Effort> = std::iter::once(Effort::Auto)
            .chain(
                levels(entry)
                    .into_iter()
                    .take(MOST_OPTIONS - 1)
                    .map(Effort::Named),
            )
            .collect();
        if options.len() < 2 {
            return None;
        }
        let current = options
            .iter()
            .position(|option| option == current)
            .unwrap_or(0);
        Some(Self {
            model: one_line(model),
            options,
            current,
            selected: current,
            window_start: 0,
        })
    }

    /// The marked level.
    pub(crate) fn selected(&self) -> &Effort {
        &self.options[self.selected]
    }

    /// Whether `effort` is one of the levels it lists: the default always, a
    /// name only as the row spells it (`reasoningEffortSupported`,
    /// `model_capabilities.zig:191-197@481104fd`).
    pub(crate) fn offers(&self, effort: &Effort) -> bool {
        self.options.contains(effort)
    }

    /// What it lists, as a person is shown them: `default`, then the levels
    /// -- the list `/effort <level>`'s refusals end with
    /// (`session_commands.zig:1020-1025@481104fd`).
    pub(crate) fn levels(&self) -> Vec<&str> {
        self.options.iter().map(Effort::display).collect()
    }

    /// Moves the mark `delta` levels, wrapping past either end, and the
    /// window only as far as it takes to keep the mark in the `visible` rows
    /// (`move` and `routeEffortMenuMove`, `effort_menu.zig:53-57`,
    /// `input_completion_runtime.zig:519-528`). Whether anything changed.
    pub(crate) fn step_by(&mut self, delta: i64, visible: u16) -> bool {
        let count = i64::try_from(self.options.len()).unwrap_or(i64::MAX);
        let at = i64::try_from(self.selected).unwrap_or(0);
        let next = usize::try_from((at + delta).rem_euclid(count)).unwrap_or(0);
        let start = edge_start(self.window_start, self.options.len(), next, visible.max(1));
        let changed = next != self.selected || start != self.window_start;
        self.selected = next;
        self.window_start = start;
        changed
    }

    /// The rows the menu takes on a screen `terminal_rows` tall whose
    /// composer is `input_extra` rows taller than one (`menuRowCount`,
    /// `:41-43`).
    pub(crate) fn height(&self, terminal_rows: u16, input_extra: u16) -> u16 {
        self.layout(row_budget(terminal_rows, input_extra)).rows()
    }

    /// How many levels are in view in `max_rows` rows, and never fewer than
    /// one: the window a move scrolls against (`visibleItems`, `:45-47`).
    pub(crate) fn visible_items(&self, max_rows: u16) -> u16 {
        self.layout(max_rows).visible.max(1)
    }

    /// The menu's rows in `max_rows` rows of a `cols`-wide screen, top first:
    /// exactly as many as [`Self::height`] gave the band when `max_rows` is
    /// that height (`composeEffortMenuRow`, `:57-88`). The gap row is empty.
    pub(crate) fn rows(&self, cols: u16, max_rows: u16, palette: Palette) -> Vec<String> {
        let layout = self.layout(max_rows);
        let mut rows = vec![String::new(); usize::from(layout.rows())];
        if cols == 0 || layout.rows() == 0 {
            return rows;
        }
        if layout.header {
            rows[0] = self.header(cols, palette);
        }
        let first = edge_start(
            self.window_start,
            self.options.len(),
            self.selected,
            layout.visible.max(1),
        );
        let width = usize::from(cols);
        let indent = if width <= INDENT { 0 } else { INDENT };
        let marker = indent + self.widest_label() + 2;
        for offset in 0..usize::from(layout.visible) {
            let index = first + offset;
            let Some(option) = self.options.get(index) else {
                break;
            };
            let mut row = " ".repeat(indent);
            row.push_str(if index == self.selected {
                palette.subtitle()
            } else {
                palette.dim()
            });
            row.push_str(&ellipsized(option.display(), width.saturating_sub(indent)));
            row.push_str(palette.reset());
            if index == self.current && marker + CURRENT.len() <= width {
                pad_to(&mut row, marker);
                row.push_str(palette.dim());
                row.push_str(CURRENT);
                row.push_str(palette.reset());
            }
            rows[usize::from(layout.first_item_row) + offset] = row;
        }
        rows
    }

    /// `Effort N` and the model (`composeHeaderRow`, `:90-106`).
    fn header(&self, cols: u16, palette: Palette) -> String {
        let width = usize::from(cols);
        let title = format!("Effort {}", self.options.len());
        let mut row = format!(
            "{}{}{}",
            palette.subtitle(),
            super::frame::clip(&title, cols),
            palette.reset()
        );
        let title_width = cells(&title);
        if !self.model.is_empty() && title_width + 2 < width {
            row.push_str("  ");
            row.push_str(palette.dim());
            row.push_str(&middle_ellipsized(&self.model, width - title_width - 2));
            row.push_str(palette.reset());
        }
        row
    }

    /// The widest level's label, in cells (`labelColumnWidth`, `:108-112`).
    fn widest_label(&self) -> usize {
        self.options
            .iter()
            .map(|option| cells(option.display()))
            .max()
            .unwrap_or(0)
    }

    /// Where everything goes in `row_budget` rows (`Layout.build`, `:24-35`).
    fn layout(&self, row_budget: u16) -> Layout {
        let count = u16::try_from(self.options.len()).unwrap_or(u16::MAX);
        if row_budget == 0 || count == 0 {
            return Layout::default();
        }
        if row_budget == 1 {
            return Layout {
                visible: 1,
                ..Layout::default()
            };
        }
        let first_item_row = if row_budget >= 3 {
            HEADER_ROWS + GAP_ROWS
        } else {
            HEADER_ROWS
        };
        Layout {
            header: true,
            first_item_row,
            visible: count.min(row_budget - first_item_row),
        }
    }
}

/// Where the menu's parts go in the rows it is given.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Layout {
    header: bool,
    first_item_row: u16,
    visible: u16,
}

impl Layout {
    fn rows(self) -> u16 {
        self.first_item_row + self.visible
    }
}

/// The levels `entry` lists that are names upstream would parse, in its
/// order: the default spellings and anything else [`Effort::parse`] does not
/// read as a name are left out.
fn levels(entry: &CatalogEntry) -> Vec<String> {
    entry
        .efforts
        .iter()
        .filter(|level| matches!(Effort::parse(level), Some(Effort::Named(_))))
        .cloned()
        .collect()
}

/// `text` as one line a terminal obeys nothing in: the channel's inert
/// spelling ([`super::bridge::inert`]), and the line breaks it keeps turned
/// into spaces as well.
pub(crate) fn one_line(text: &str) -> String {
    super::bridge::inert(text)
        .chars()
        .map(|character| {
            if matches!(character, '\n' | '\r') {
                ' '
            } else {
                character
            }
        })
        .collect()
}

/// The rows a screen `terminal_rows` tall gives the menu when its composer is
/// `input_extra` rows taller than one (`inlinePickerRowBudgetCapped`,
/// `picker_presentation.zig:793-803`): what is left after the band's fixed
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

    use super::super::theme::{Depth, Mode};

    const PALETTE: Palette = Palette {
        mode: Mode::Dark,
        depth: Depth::Ansi256,
    };

    /// Upstream's `selected_completion_style` (`core/shared/theme.zig:97`).
    const BRIGHT: &str = "\u{1b}[1;38;5;255m";

    /// Upstream's `dim_style` (`core/shared/theme.zig:85`).
    const DIM: &str = "\u{1b}[38;5;245m";

    const RESET: &str = "\u{1b}[0m";

    fn entry(efforts: &[&str]) -> CatalogEntry {
        CatalogEntry {
            id: "openai/gpt-6-astra".to_string(),
            aliases: Vec::new(),
            name: None,
            efforts: efforts.iter().map(|effort| effort.to_string()).collect(),
            max_context: None,
            facts: Default::default(),
        }
    }

    fn astra(current: &Effort) -> EffortMenu {
        EffortMenu::open(
            "openai/gpt-6-astra",
            &entry(&["low", "medium", "high", "xhigh"]),
            current,
        )
        .expect("astra lists levels")
    }

    fn plain(row: &str) -> String {
        let mut out = String::new();
        let mut chars = row.chars();
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
    fn it_lists_the_default_then_the_rows_levels_with_the_one_in_use_marked() {
        // `new:effort-menu/menu` at 40x120: the title, a gap, five levels.
        let menu = astra(&Effort::Auto);
        let rows = menu.rows(120, menu.height(40, 0), PALETTE);
        let shown: Vec<String> = rows.iter().map(|row| plain(row)).collect();
        assert_eq!(
            shown,
            [
                "Effort 5  openai/gpt-6-astra",
                "",
                "  default  current",
                "  low",
                "  medium",
                "  high",
                "  xhigh",
            ]
        );
        assert_eq!(
            rows[0],
            format!("{BRIGHT}Effort 5{RESET}  {DIM}openai/gpt-6-astra{RESET}")
        );
        assert_eq!(
            rows[2],
            format!("  {BRIGHT}default{RESET}  {DIM}current{RESET}")
        );
        assert_eq!(rows[3], format!("  {DIM}low{RESET}"));
    }

    #[test]
    fn the_mark_opens_on_the_level_in_use_and_on_default_for_one_the_model_lacks() {
        let menu = astra(&Effort::Named("medium".to_string()));
        assert_eq!(menu.selected(), &Effort::Named("medium".to_string()));
        let rows = menu.rows(120, menu.height(40, 0), PALETTE);
        assert_eq!(plain(&rows[4]), "  medium   current");
        // `effort_menu.zig:45`: a level the model does not offer marks
        // `default`, and nothing is marked `current` but it.
        let menu = astra(&Effort::Named("max".to_string()));
        assert_eq!(menu.selected(), &Effort::Auto);
        let rows = menu.rows(120, menu.height(40, 0), PALETTE);
        assert_eq!(plain(&rows[2]), "  default  current");
    }

    #[test]
    fn up_and_down_wrap_past_either_end() {
        // `menu-down`, then `menu-wrap`: Down to `low`, Up twice to `xhigh`.
        let mut menu = astra(&Effort::Auto);
        let visible = menu.visible_items(menu.height(40, 0));
        assert!(menu.step_by(1, visible));
        assert_eq!(menu.selected(), &Effort::Named("low".to_string()));
        menu.step_by(-1, visible);
        menu.step_by(-1, visible);
        assert_eq!(menu.selected(), &Effort::Named("xhigh".to_string()));
        // A page wraps by the count, as `@mod` does.
        menu.step_by(7, visible);
        assert_eq!(menu.selected(), &Effort::Named("low".to_string()));
    }

    #[test]
    fn a_short_footer_drops_the_gap_then_the_title_and_scrolls_the_levels() {
        // `new:effort-short/menu` at 12 rows: the budget is two -- the title
        // and one level, no gap.
        let mut menu = astra(&Effort::Auto);
        let budget = menu.height(12, 0);
        assert_eq!(budget, 2);
        let rows = menu.rows(120, budget, PALETTE);
        assert_eq!(
            rows.iter().map(|row| plain(row)).collect::<Vec<_>>(),
            ["Effort 5  openai/gpt-6-astra", "  default  current"]
        );
        // `menu-down5` wraps back to `default`; `menu-down6` shows `low` alone.
        let visible = menu.visible_items(budget);
        for _ in 0..5 {
            menu.step_by(1, visible);
        }
        assert_eq!(
            plain(&menu.rows(120, budget, PALETTE)[1]),
            "  default  current"
        );
        menu.step_by(1, visible);
        assert_eq!(plain(&menu.rows(120, budget, PALETTE)[1]), "  low");
        // One row: no title, the marked level alone.
        assert_eq!(
            menu.rows(120, 1, PALETTE)
                .iter()
                .map(|row| plain(row))
                .collect::<Vec<_>>(),
            ["  low"]
        );
    }

    #[test]
    fn the_marker_and_the_model_go_when_they_do_not_fit() {
        let menu = astra(&Effort::Auto);
        // `  default  current` is 18 cells: at 17 the marker goes.
        assert_eq!(plain(&menu.rows(18, 7, PALETTE)[2]), "  default  current");
        assert_eq!(plain(&menu.rows(17, 7, PALETTE)[2]), "  default");
        // The model gives up its middle, and goes once two cells do not fit.
        assert_eq!(
            plain(&menu.rows(20, 7, PALETTE)[0]),
            "Effort 5  opena\u{2026}stra"
        );
        assert_eq!(plain(&menu.rows(10, 7, PALETTE)[0]), "Effort 5");
    }

    #[test]
    fn a_row_without_levels_has_no_menu_and_only_parsable_names_are_offered() {
        assert!(EffortMenu::open("fake/model-a", &entry(&[]), &Effort::Auto).is_none());
        // An llmux row is not held to the Gateway's rule when it is read
        // (`crate::provider::model`), so the menu is: nothing a terminal would
        // obey reaches a row, and the default's spellings are not offered twice.
        let menu = EffortMenu::open(
            "odd\u{1b}[2J\nmodel",
            &entry(&["low", "evil\u{1b}[2J", "two words", "auto", "high"]),
            &Effort::Auto,
        )
        .expect("two levels survive");
        let rows = menu.rows(120, menu.height(40, 0), PALETTE);
        let shown: Vec<String> = rows.iter().map(|row| plain(row)).collect();
        assert_eq!(shown[0], "Effort 3  odd [2J model");
        assert_eq!(&shown[2..], ["  default  current", "  low", "  high"]);
        assert!(!rows.concat().contains("\u{1b}[2J"));
    }

    #[test]
    fn the_key_help_is_the_widest_that_fits() {
        assert_eq!(
            key_help(120),
            "\u{2191}\u{2193} navigate     enter use     esc close"
        );
        assert_eq!(
            key_help(38),
            "\u{2191}\u{2193} navigate  enter use  esc close"
        );
        assert_eq!(key_help(32), "\u{2191}\u{2193} move  enter  esc");
        assert_eq!(key_help(18), "enter esc");
    }

    #[test]
    fn only_its_keys_are_taken() {
        assert_eq!(EffortAction::of(Action::Up), Some(EffortAction::Up));
        assert_eq!(EffortAction::of(Action::ControlJ), Some(EffortAction::Down));
        assert_eq!(EffortAction::of(Action::Submit), Some(EffortAction::Use));
        assert_eq!(EffortAction::of(Action::Escape), Some(EffortAction::Close));
        assert_eq!(EffortAction::of(Action::Tab), None);
        assert_eq!(EffortAction::of(Action::Backspace), None);
    }
}
