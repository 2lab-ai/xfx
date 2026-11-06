//! `/fork`'s menu: the session's prompts, oldest first, in the band under the
//! composer.
//!
//! Upstream's Fork menu (`vercel-labs/fx@481104fd`, commit `e4348723`; state
//! `ForkMenu`, `src/core/app/app_session_runtime.zig:573-600`, rows
//! `src/ui/footer/fork_menu_presentation.zig`, key help
//! `src/ui/footer/input_presentation.zig:704-735`), in the slot `/model`'s
//! menu uses ([`super::model_menu`]): between the rule under the composer and
//! the bottom rule, with its own key help where the hint row was.
//!
//! Like `/effort`'s menu it is **not** a view of the composer: it reads no
//! search, and an edit to the composer closes it while the edit still applies
//! (`dismissActiveMenusForComposerEdit`, `app_input_runtime.zig:1984-1990`).
//! Only the keys [`ForkAction::of`] names are taken out of the stream.
//!
//! # What it looks like
//!
//! No title. Each prompt in at most three rows, two cells in, broken at its
//! own line breaks and after the last blank that fits, the third row of a
//! longer one cut one cell short and ended with `…`
//! ([`message_layout`]); the selected prompt's rows bold, the rest dim. It
//! opens on the newest, with the newest rows in view, and Up and Down stop at
//! either end rather than wrap. At most six prompts' worth of rows, eighteen,
//! are shown; the window moves only as far as it must to keep the selected
//! prompt whole.
//!
//! # Where the text comes from
//!
//! A prompt is read back out of the session's log, which is a file: it reached
//! the band through the channel, which made it inert
//! ([`super::bridge`]'s `made_inert`), and each row is made so again here
//! before it is painted -- a tab drawn as the space upstream draws it, and no
//! character a terminal would obey.

use super::input::Action;
use super::picker::cells;
use super::theme::Palette;

/// The most rows one prompt is shown in (`max_message_layout_rows`,
/// `render_input.zig:439`).
const PROMPT_ROWS: usize = 3;

/// The most rows the menu is given: six prompts of three rows
/// (`max_inline_rows`, `fork_menu_presentation.zig:11`).
const MOST_ROWS: u16 = 6 * PROMPT_ROWS as u16;

/// The band's rows the menu cannot have, before the composer's extra rows
/// are added to them (`inlinePickerRowBudgetCapped`'s `fixed_rows`,
/// `picker_presentation.zig:799`).
const FIXED_ROWS: u16 = 5;

/// The document rows left on any screen tall enough to have them
/// (`minimum_transcript_rows`, `picker_presentation.zig:800`).
const MINIMUM_TRANSCRIPT_ROWS: u16 = 5;

/// What every row starts with on a screen wider than four cells
/// (`indentWidth`, `fork_menu_presentation.zig:13-15`).
const INDENT: u16 = 2;

/// What ends the last row of a prompt that does not fit.
const ELLIPSIS: char = '\u{2026}';

/// What the hint row says while the menu is up, widest first
/// (`composeForkMenuHintRow`, `input_presentation.zig:714-720`). The fourth
/// is never reached once the third does not fit; upstream lists it, and so
/// does this.
const KEY_HELP: [&str; 5] = [
    "\u{2191}\u{2193} navigate     enter fork     esc close",
    "\u{2191}\u{2193} navigate  enter fork  esc close",
    "\u{2191}\u{2193} move  enter  esc",
    "enter fork  esc close",
    "enter esc",
];

/// One prompt a fork can start before: the turn it opened, counted from 1,
/// and what was typed (`Session.ForkPoint`, `session_adapter.zig:2383-2386`).
///
/// The text is the log's, the prompt as it was sent; the images it carried
/// are not part of it, as upstream puts back only the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ForkPoint {
    pub(crate) turn: usize,
    pub(crate) prompt: String,
}

/// What the runtime answers a `/fork` with (`openForkMenu`,
/// `app_session_runtime.zig:2072-2096`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ForkPoints {
    /// The conversation's log stopped taking what it was given, so what it
    /// holds is not what the screen shows: upstream's `/fork needs a saved
    /// session`.
    Unsaved,
    /// The prompts a fork can start before, oldest first -- none when nothing
    /// has been asked yet, which is upstream's `nothing to fork yet`.
    Points(Vec<ForkPoint>),
}

/// What one keystroke means to the menu.
///
/// Its own vocabulary, as [`super::model_menu::ModelAction`] is: the shell
/// translates, and a key the menu does not bind goes on to the composer --
/// where an edit closes the menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ForkAction {
    /// The prompt before, stopping at the oldest. Up, `C-k`.
    Up,
    /// The prompt after, stopping at the newest. Down, `C-j`.
    Down,
    /// A page of the composer's row cap, `true` for down, clamped as a step
    /// is (`move` clamps any delta, `app_session_runtime.zig:588-592`). Page
    /// Up and Page Down.
    Page(bool),
    /// Fork before the selected prompt (`submitForkMenuSelection`,
    /// `app_input_runtime.zig:1667-1690`). Enter.
    Fork,
    /// Close it (`cancelForkMenu`, `:2862-2867`). Escape.
    Close,
}

impl ForkAction {
    /// What an ordinary decoded action means to the menu, and `None` for the
    /// keystrokes that go on to the composer.
    pub(crate) fn of(action: Action) -> Option<Self> {
        match action {
            // `C-k` and `C-j` walk it too (`routePickerControlByte` ->
            // `routeVisiblePickerMove` -> `routeForkMenuMove`,
            // `input_completion_runtime.zig:390-406`).
            Action::Up | Action::ControlK => Some(Self::Up),
            Action::Down | Action::ControlJ => Some(Self::Down),
            Action::PageUp => Some(Self::Page(false)),
            Action::PageDown => Some(Self::Page(true)),
            Action::Submit => Some(Self::Fork),
            Action::Escape => Some(Self::Close),
            _ => None,
        }
    }
}

/// The menu while it is up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ForkMenu {
    /// The prompts, oldest first.
    points: Vec<ForkPoint>,
    /// The selected one.
    selected: usize,
    /// The first menu row shown, counted over every prompt's rows. Past the
    /// end when it opens, which shows the newest
    /// (`window_start = maxInt`, `app_session_runtime.zig:578`).
    window_start: usize,
}

impl ForkMenu {
    /// The menu over `points`, the newest selected (`openForkMenu`,
    /// `app_session_runtime.zig:2091-2093`); `None` when there is none, which
    /// has no menu.
    pub(crate) fn open(points: Vec<ForkPoint>) -> Option<Self> {
        let selected = points.len().checked_sub(1)?;
        Some(Self {
            points,
            selected,
            window_start: usize::MAX,
        })
    }

    /// The selected prompt.
    pub(crate) fn selected(&self) -> &ForkPoint {
        &self.points[self.selected]
    }

    /// Moves the selection `delta` prompts, stopping at the oldest and the
    /// newest, and the window only as far as it takes to show the selected
    /// prompt whole in `budget` rows of a `cols`-wide screen (`move` and
    /// `routeForkMenuMove`, `app_session_runtime.zig:588-592`,
    /// `input_completion_runtime.zig:506-517`). Whether anything changed.
    pub(crate) fn step_by(&mut self, delta: i64, cols: u16, budget: u16) -> bool {
        let last = i64::try_from(self.points.len().saturating_sub(1)).unwrap_or(i64::MAX);
        let at = i64::try_from(self.selected).unwrap_or(0);
        let next = usize::try_from(at.saturating_add(delta).clamp(0, last)).unwrap_or(0);
        let before = (self.selected, self.window_start);
        self.selected = next;
        self.window_start = self.first_row(cols, budget);
        before != (self.selected, self.window_start)
    }

    /// The rows the menu takes on a `cols`-wide screen `terminal_rows` tall
    /// whose composer is `input_extra` rows taller than one (`menuRowCount`,
    /// `fork_menu_presentation.zig:39-41`, under
    /// `inlinePickerRowBudgetCapped`, `surface_frame.zig:583-588`).
    pub(crate) fn height(&self, cols: u16, terminal_rows: u16, input_extra: u16) -> u16 {
        let total = self.measure(cols).total;
        let budget = row_budget(terminal_rows, input_extra);
        u16::try_from(total).map_or(budget, |total| total.min(budget))
    }

    /// The rows the band gives the menu's window on such a screen, whatever
    /// the menu holds: what a move scrolls against
    /// (`inlineMenuRowBudget(app, max_inline_rows)`,
    /// `input_completion_runtime.zig:514`).
    pub(crate) fn budget(terminal_rows: u16, input_extra: u16) -> u16 {
        row_budget(terminal_rows, input_extra)
    }

    /// The menu's rows in `max_rows` rows of a `cols`-wide screen, top first
    /// (`composeForkMenuRow`, `fork_menu_presentation.zig:54-78`): exactly as
    /// many as [`Self::height`] gave the band when `max_rows` is that height.
    pub(crate) fn rows(&self, cols: u16, max_rows: u16, palette: Palette) -> Vec<String> {
        let shown = usize::from(max_rows).min(self.measure(cols).total);
        let mut rows = vec![String::new(); shown];
        if cols == 0 {
            return rows;
        }
        let start = self.first_row(cols, max_rows);
        let indent = " ".repeat(usize::from(indent(cols)));
        let mut line = 0usize;
        for (index, point) in self.points.iter().enumerate() {
            let layout = message_layout(&point.prompt, content_width(cols), PROMPT_ROWS);
            for text in layout.painted() {
                if let Some(row) = line.checked_sub(start).and_then(|slot| rows.get_mut(slot)) {
                    let style = if index == self.selected {
                        palette.subtitle()
                    } else {
                        palette.dim()
                    };
                    *row = format!("{indent}{style}{text}{}", palette.reset());
                }
                line += 1;
            }
        }
        rows
    }

    /// The first row in view: [`Self::window_start`], moved only as far as it
    /// takes to show the whole selected prompt in `budget` rows, so the list
    /// scrolls at its edges (`windowStart`, `fork_menu_presentation.zig:45-52`).
    fn first_row(&self, cols: u16, budget: u16) -> usize {
        let lines = self.measure(cols);
        let height = usize::from(budget).min(lines.total);
        let mut start = self.window_start.min(lines.total - height);
        if lines.selected_start < start {
            start = lines.selected_start;
        }
        if lines.selected_end > start + height {
            start = lines.selected_start.min(lines.selected_end - height);
        }
        start
    }

    /// How many rows every prompt takes, and where the selected one's are
    /// (`Lines.measure`, `fork_menu_presentation.zig:28-36`).
    fn measure(&self, cols: u16) -> Lines {
        let width = content_width(cols);
        let mut lines = Lines::default();
        for (index, point) in self.points.iter().enumerate() {
            if index == self.selected {
                lines.selected_start = lines.total;
            }
            lines.total += message_layout(&point.prompt, width, PROMPT_ROWS).rows.len();
            if index == self.selected {
                lines.selected_end = lines.total;
            }
        }
        lines
    }
}

/// The menu's rows, every prompt's in order, and where the selected one sits.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Lines {
    total: usize,
    selected_start: usize,
    selected_end: usize,
}

/// The cells a row's text may take on a `cols`-wide screen: the width less
/// the indent (`promptLayout`, `fork_menu_presentation.zig:17-19`).
fn content_width(cols: u16) -> usize {
    usize::from(cols.saturating_sub(indent(cols)))
}

/// Two cells on a screen wider than four, none otherwise (`indentWidth`).
fn indent(cols: u16) -> u16 {
    if cols > 4 {
        INDENT
    } else {
        0
    }
}

/// A prompt laid out in rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Layout {
    /// The rows' text, as cut from the prompt.
    rows: Vec<String>,
    /// The cells each row may take.
    width: usize,
    /// Whether the prompt went on past the last row.
    truncated: bool,
}

impl Layout {
    /// The rows as the menu paints them (`appendMessageLayoutRow`,
    /// `input_presentation.zig:80-96`): a tab drawn as a space, nothing in
    /// them a terminal would obey, each cut to the width -- the last one a
    /// cell short and ended with `…` when the prompt went on.
    fn painted(&self) -> Vec<String> {
        let count = self.rows.len();
        self.rows
            .iter()
            .enumerate()
            .map(|(index, row)| {
                let ellipsis = self.truncated && index + 1 == count && self.width > 0;
                // A tab is a control character, so the channel's spelling
                // draws it as the space upstream draws it.
                let safe = super::bridge::inert(row);
                let budget = u16::try_from(self.width - usize::from(ellipsis)).unwrap_or(u16::MAX);
                let mut text = super::frame::clip(&safe, budget).to_string();
                if ellipsis {
                    text.push(ELLIPSIS);
                }
                text
            })
            .collect()
    }
}

/// `message` in at most `row_limit` rows of `width` cells
/// (`steering_message_layout`, `render_input.zig:466-525@481104fd`, with no
/// boundary to wait for).
///
/// A row ends at a line break -- `\n`, `\r` or `\r\n` -- or where the next
/// character would not fit: there, at the blanks that follow when it is one,
/// and otherwise back at the last blank of the row, when the row has one past
/// its first character. A tab is one cell. A prompt that goes on past the
/// last row, or a character too wide for a row of its own, ends the layout
/// with that row as it broke, but no further than where the cells it held
/// were still fewer than the width, so the `…` fits after it.
///
/// Upstream's own function, ported for this menu rather than shared with the
/// waiting-prompt banner, which still keeps the layout of the release it was
/// ported at (`super::shell`'s `banner_layout`).
pub(crate) fn message_layout(message: &str, width: usize, row_limit: usize) -> Layout {
    let bytes = message.as_bytes();
    let blank = |at: usize| matches!(bytes.get(at), Some(b' ' | b'\t'));
    let line_break = |at: usize| matches!(bytes.get(at), Some(b'\n' | b'\r'));
    let limit = row_limit.min(PROMPT_ROWS);
    let mut layout = Layout {
        rows: Vec::new(),
        width,
        truncated: false,
    };
    let mut offset = 0usize;
    while layout.rows.len() < limit {
        let start = offset;
        let mut used = 0usize;
        let mut ellipsis_end = start;
        let mut last_blank: Option<usize> = None;
        while offset < bytes.len() && !line_break(offset) {
            let Some(unit) = super::wrap::first_cluster(&message[offset..]) else {
                break;
            };
            let unit_width = if unit == "\t" {
                1
            } else {
                usize::from(super::wrap::width(unit))
            };
            if unit_width > width - used {
                break;
            }
            if blank(offset) {
                last_blank = Some(offset);
            }
            used += unit_width;
            offset += unit.len();
            if used < width {
                ellipsis_end = offset;
            }
        }
        let mut end = offset;
        let soft = blank(offset);
        while blank(offset) {
            offset += 1;
        }
        let hard = line_break(offset);
        if hard {
            let carriage = bytes[offset] == b'\r';
            offset += 1;
            if carriage && bytes.get(offset) == Some(&b'\n') {
                offset += 1;
            }
        } else if !soft && offset < bytes.len() {
            if let Some(at) = last_blank.filter(|at| *at > start) {
                end = at;
                offset = at + 1;
            }
        }
        layout.rows.push(message[start..end].to_string());
        if offset == bytes.len() {
            break;
        }
        if layout.rows.len() == limit || (!hard && end == start) {
            if let Some(last) = layout.rows.last_mut() {
                *last = message[start..end.min(ellipsis_end)].to_string();
            }
            layout.truncated = true;
            break;
        }
    }
    layout
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

    fn point(turn: usize, prompt: &str) -> ForkPoint {
        ForkPoint {
            turn,
            prompt: prompt.to_string(),
        }
    }

    fn menu(points: &[(usize, &str)], selected: usize) -> ForkMenu {
        let mut menu = ForkMenu::open(
            points
                .iter()
                .map(|(turn, text)| point(*turn, text))
                .collect(),
        )
        .expect("points");
        menu.selected = selected;
        menu
    }

    /// A row without its colours, and whether it is the selected style.
    fn read(row: &str) -> (String, bool) {
        let selected = row.contains(PALETTE.subtitle());
        let mut text = String::new();
        let mut chars = row.chars();
        while let Some(character) = chars.next() {
            if character == '\u{1b}' {
                for next in chars.by_ref() {
                    if next == 'm' {
                        break;
                    }
                }
                continue;
            }
            text.push(character);
        }
        (text, selected)
    }

    #[test]
    fn each_prompt_shows_in_at_most_three_rows_and_a_cut_one_ends_with_an_ellipsis() {
        // Upstream's own case (`fork_menu_presentation.zig:103-117@481104fd`).
        let menu = menu(
            &[
                (
                    1,
                    "one two three four five six seven eight nine ten eleven twelve",
                ),
                (2, "short\nsecond line"),
            ],
            1,
        );
        assert_eq!(menu.height(16, 40, 0), 5);
        let rows: Vec<(String, bool)> = menu
            .rows(16, MOST_ROWS, PALETTE)
            .iter()
            .map(|row| read(row))
            .collect();
        assert_eq!(
            rows,
            [
                ("  one two three".to_string(), false),
                ("  four five six".to_string(), false),
                ("  seven eight\u{2026}".to_string(), false),
                ("  short".to_string(), true),
                ("  second line".to_string(), true),
            ]
        );
    }

    #[test]
    fn it_opens_on_the_newest_at_the_bottom_and_scrolls_only_past_an_edge() {
        // Upstream's own case (`fork_menu_presentation.zig:119-151@481104fd`).
        let mut menu = ForkMenu::open(vec![
            point(1, "first"),
            point(2, "second"),
            point(3, "third"),
        ])
        .expect("points");
        let rows = |menu: &ForkMenu, budget: u16| -> Vec<(String, bool)> {
            menu.rows(40, budget, PALETTE)
                .iter()
                .map(|row| read(row))
                .collect()
        };
        assert_eq!(
            rows(&menu, 3),
            [
                ("  first".to_string(), false),
                ("  second".to_string(), false),
                ("  third".to_string(), true),
            ]
        );
        // Two rows show the two newest; moving up keeps them until the
        // selection passes the top, then the list scrolls by one -- and it
        // stops at the oldest.
        let tops = |menu: &ForkMenu| {
            let shown = rows(menu, 2);
            (shown[0].0.clone(), shown[1].0.clone())
        };
        assert_eq!(tops(&menu), ("  second".to_string(), "  third".to_string()));
        for (delta, top, bottom) in [
            (-1, "  second", "  third"),
            (-1, "  first", "  second"),
            (-1, "  first", "  second"),
            (1, "  first", "  second"),
        ] {
            menu.step_by(delta, 40, 2);
            assert_eq!(tops(&menu), (top.to_string(), bottom.to_string()));
        }
        assert_eq!(menu.selected().turn, 2);
        assert!(menu.step_by(-5, 40, 2));
        assert_eq!(menu.selected().turn, 1);
        assert!(
            !menu.step_by(-1, 40, 2),
            "a step past the oldest changed something"
        );
    }

    #[test]
    fn a_row_is_cut_where_the_ellipsis_still_fits_and_a_tab_is_one_cell() {
        // `steering_message_layout`: a third row that broke at a blank keeps
        // every character that leaves a cell for `…`; a tab counts one cell
        // and is drawn as a space; `\r\n` is one break.
        let layout = message_layout("aaaa bbbb cccc dddd eeee", 9, 3);
        assert_eq!(layout.rows, ["aaaa bbbb", "cccc dddd", "eeee"]);
        assert!(!layout.truncated);
        let layout = message_layout("aaaa bbbb cccc dddd eeee ffff gggg", 9, 3);
        assert_eq!(
            layout.painted(),
            ["aaaa bbbb", "cccc dddd", "eeee fff\u{2026}"]
        );
        let layout = message_layout("a\tb\r\nc", 9, 3);
        assert_eq!(layout.painted(), ["a b", "c"]);
        // A word wider than a row is cut at the row, and the last row a cell
        // short of it.
        let layout = message_layout("abcdefghijklmnopq", 5, 3);
        assert_eq!(layout.painted(), ["abcde", "fghij", "klmn\u{2026}"]);
        // A character too wide for a row of its own ends the layout there.
        let layout = message_layout("a\u{4e00}b", 1, 3);
        assert_eq!(layout.painted(), ["a", "\u{2026}"]);
        // Wide characters are two cells each.
        let layout = message_layout("\u{4e00}\u{4e01}\u{4e02}\u{4e03}", 5, 3);
        assert_eq!(layout.painted(), ["\u{4e00}\u{4e01}", "\u{4e02}\u{4e03}"]);
    }

    #[test]
    fn a_prompt_draws_nothing_a_terminal_would_obey() {
        // Read raw, with only this module's own three sequences taken out --
        // not through [`read`], whose skip from an ESC to the next `m` would
        // swallow the very sequence this looks for.
        let menu = menu(&[(1, "x\u{1b}]0;pwned\u{7}y\u{9b}31m\u{1b}[2Jz")], 0);
        let rows = menu.rows(80, MOST_ROWS, PALETTE);
        assert_eq!(rows.len(), 1);
        let payload = rows[0]
            .strip_prefix(&format!("  {}", PALETTE.subtitle()))
            .and_then(|row| row.strip_suffix(PALETTE.reset()))
            .expect("the indent, the selected style, the text, the reset");
        assert_eq!(payload, "x ]0;pwned y 31m [2Jz");
    }

    #[test]
    fn the_key_help_narrows_with_the_screen() {
        assert_eq!(
            key_help(120),
            "\u{2191}\u{2193} navigate     enter fork     esc close"
        );
        assert_eq!(
            key_help(35),
            "\u{2191}\u{2193} navigate  enter fork  esc close"
        );
        assert_eq!(key_help(20), "\u{2191}\u{2193} move  enter  esc");
        assert_eq!(key_help(5), "enter esc");
    }

    #[test]
    fn the_band_gives_it_at_most_six_prompts_of_rows_and_never_none() {
        assert_eq!(row_budget(40, 0), MOST_ROWS);
        assert_eq!(row_budget(12, 0), 2);
        assert_eq!(row_budget(5, 0), 1);
        assert_eq!(
            ForkMenu::open(vec![point(1, "x")])
                .expect("one")
                .height(120, 40, 0),
            1
        );
    }
}
