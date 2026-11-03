//! `/workspace`'s compact picker: the workspace, how many directories are
//! added, and one row per thing the command can do.
//!
//! Upstream's `WorkspaceMenu` (`vercel-labs/fx@c1db9191
//! src/core/workspace/workspace_menu.zig`) as
//! `compact_command_menu_presentation.zig:1009-1238` paints it, between the
//! band's two rules and in place of the composer: a bold `Workspace` title, a
//! blank row, two pinned rows -- `Primary` and the workspace's path,
//! `Additional directories` and `n / 16` -- a blank row, and then the choices:
//! `Add directory…`, one row per saved directory with its status, and `Clear
//! saved directories` when there is one to clear. The marked choice carries a
//! `❯` and is bright; the rest are dim.
//!
//! # Enter writes a command; it does not run one
//!
//! A choice is a command **put in the composer** -- `/workspace add `,
//! `/workspace remove <path>`, `/workspace clear` -- and the picker closes
//! (`prepareWorkspaceMenuCommand`, `app_input_runtime.zig:2580-2609`). Nothing
//! is changed until the user submits that line, so the two destructive
//! choices always pass through a line the user can read and edit first.

use std::path::Path;

use crate::workspace::access::{terminal_safe, Entry, MAX_ADDITIONAL_DIRECTORIES};

use super::input::Action;
use super::picker::{cells, ellipsized, pad_to};
use super::theme::Palette;

/// The picker's title row.
const TITLE: &str = "Workspace";

/// The title, a blank, `Primary`, `Additional directories` and a blank
/// (`workspace_pinned_rows`, `compact_command_menu_presentation.zig:17`).
const PINNED_ROWS: u16 = 5;

/// The columns between a label and what is said about it (`column_gap`).
const COLUMN_GAP: usize = 4;

/// The first choice, and what it does.
const ADD: (&str, &str) = ("Add directory\u{2026}", "Grant access to another directory");

/// The last choice, when there is something to clear.
const CLEAR: (&str, &str) = (
    "Clear saved directories",
    "Remove every saved additional directory",
);

/// The widest pinned label, which places both pinned values.
const WIDEST_SUMMARY_LABEL: &str = "Additional directories";

/// The marked choice's lead (`composeWorkspaceActionRow`, `:1162-1206`).
const MARK: &str = "\u{276f} ";

/// What the hint row says while the picker is up, widest first
/// (`composeCompactCommandMenuHintRow`, `input_presentation.zig:782-786`).
const KEY_HELP: [&str; 3] = [
    "\u{2191}\u{2193} navigate     enter use     esc close",
    "\u{2191}\u{2193} move  enter use  esc",
    "enter esc",
];

/// What one keystroke means to the picker.
///
/// The picker's own vocabulary, as `/statusline`'s has one
/// (`super::statusline::MenuAction`): the shell translates, so a key the
/// picker does not bind is one the shell swallows on purpose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MenuAction {
    /// The previous choice, wrapping. Up, and Ctrl-K.
    Up,
    /// The next choice, wrapping. Down, and Ctrl-J.
    Down,
    /// Put the marked choice's command in the composer. Enter.
    Use,
    /// Close it. Escape.
    Close,
}

impl MenuAction {
    /// What a decoded action means here, and `None` for every keystroke the
    /// picker swallows (`routeActiveModalInput` and
    /// `routeCompactCommandMenuEscapeAction`, `app_input_runtime.zig:1366-1395`
    /// and `:2651-2689`): Left and Right change nothing on this one, unlike
    /// `/statusline`'s.
    pub(crate) fn of(action: Action) -> Option<Self> {
        match action {
            Action::Up | Action::ControlK => Some(Self::Up),
            Action::Down | Action::ControlJ => Some(Self::Down),
            Action::Submit => Some(Self::Use),
            Action::Escape => Some(Self::Close),
            _ => None,
        }
    }
}

/// One choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Choice {
    Add,
    /// The saved directory at this index.
    Remove(usize),
    Clear,
}

/// The picker: which choice is marked.
///
/// The directories are not held here. They are the session's, handed in to
/// paint and to act on, so a row can never name a directory the session does
/// not have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Menu {
    selected: usize,
}

impl Menu {
    /// A picker with `Add directory…` marked, which is where upstream's
    /// always opens (`State.open`, `workspace_menu.zig:14-16`).
    pub(crate) fn open() -> Self {
        Self { selected: 0 }
    }

    /// How many choices there are: add, each saved directory, and clear when
    /// there is a saved one (`actionCount`, `workspace_menu.zig:22-25`). Every
    /// directory xfx lists is saved, so it is also the number of choice rows.
    fn count(entries: &[Entry]) -> usize {
        1 + entries.len() + usize::from(!entries.is_empty())
    }

    /// The marked choice, with the mark kept inside the list the directories
    /// make now.
    fn choice(&self, entries: &[Entry]) -> Choice {
        let selected = self.selected % Self::count(entries);
        match selected {
            0 => Choice::Add,
            index if index <= entries.len() => Choice::Remove(index - 1),
            _ => Choice::Clear,
        }
    }

    /// Moves the mark one choice, wrapping at both ends (`move`,
    /// `workspace_menu.zig:27-38`).
    pub(crate) fn step(&mut self, down: bool, entries: &[Entry]) {
        let count = Self::count(entries);
        let selected = self.selected % count;
        self.selected = if down {
            (selected + 1) % count
        } else {
            (selected + count - 1) % count
        };
    }

    /// The command the marked choice writes into the composer
    /// (`prepareWorkspaceMenuCommand`): `/workspace add ` with the caret
    /// after its space, a directory's `remove` by the path it is saved as,
    /// or `clear`.
    pub(crate) fn command(&self, entries: &[Entry]) -> String {
        match self.choice(entries) {
            Choice::Add => "/workspace add ".to_string(),
            Choice::Remove(index) => {
                format!(
                    "/workspace remove {}",
                    entries[index].path.to_string_lossy()
                )
            }
            Choice::Clear => "/workspace clear".to_string(),
        }
    }

    /// How many rows the band gives it on a screen `terminal_rows` tall: the
    /// five pinned rows and one per choice, never more than the screen less
    /// three (`desiredRowCount`, `compact_command_menu_presentation.zig:28-35`,
    /// capped as `/statusline`'s is, `surface_frame.zig:586-592`). At least one.
    pub(crate) fn height(terminal_rows: u16, entries: &[Entry]) -> u16 {
        let wanted =
            u16::try_from(usize::from(PINNED_ROWS) + Self::count(entries)).unwrap_or(u16::MAX);
        wanted.min(terminal_rows.saturating_sub(3)).max(1)
    }

    /// The rows above the first choice row on a panel `visible` rows tall:
    /// all five pinned rows when there is room for them and a choice, else
    /// the title alone above at least one choice, else nothing
    /// (`composeWorkspaceRow`, `:1009-1030`).
    fn header_rows(visible: u16) -> u16 {
        if visible > PINNED_ROWS {
            PINNED_ROWS
        } else {
            u16::from(visible > 1)
        }
    }

    /// The first choice a panel `visible` rows tall shows, keeping the mark
    /// in its window (`composeWorkspaceChoiceRow`, `:1082-1100`).
    fn window_start(&self, visible: u16, entries: &[Entry]) -> usize {
        let rows = usize::from(visible.saturating_sub(Self::header_rows(visible))).max(1);
        let count = Self::count(entries);
        let selected = self.selected % count;
        selected
            .saturating_sub(rows - 1)
            .min(count.saturating_sub(rows))
    }

    /// Which row of a `visible`-row panel the marked choice is on.
    pub(crate) fn marked_row(&self, visible: u16, entries: &[Entry]) -> u16 {
        let selected = self.selected % Self::count(entries);
        let offset = selected - self.window_start(visible, entries);
        Self::header_rows(visible) + u16::try_from(offset).unwrap_or(0)
    }

    /// The picker's rows, top first, for a panel `visible` rows tall.
    pub(crate) fn rows(
        &self,
        cols: u16,
        visible: u16,
        primary: &Path,
        entries: &[Entry],
        palette: Palette,
    ) -> Vec<String> {
        let width = usize::from(cols);
        let header = Self::header_rows(visible);
        let mut rows = Vec::with_capacity(usize::from(visible));
        if header > 0 {
            rows.push(format!(
                "{}{}{}",
                palette.subtitle(),
                ellipsized(TITLE, width),
                palette.reset()
            ));
        }
        if header == PINNED_ROWS {
            let column = summary_column(width);
            let primary = terminal_safe(&primary.to_string_lossy());
            let count = format!("{} / {MAX_ADDITIONAL_DIRECTORIES}", entries.len());
            rows.push(String::new());
            rows.push(label_value_row("Primary", &primary, column, width, palette));
            rows.push(label_value_row(
                WIDEST_SUMMARY_LABEL,
                &count,
                column,
                width,
                palette,
            ));
            rows.push(String::new());
        }
        let selected = self.selected % Self::count(entries);
        let start = self.window_start(visible, entries);
        let info_column = info_column(entries, width);
        let choices = usize::from(visible.saturating_sub(header));
        for index in (start..Self::count(entries)).take(choices) {
            let marked = index == selected;
            let row = match index {
                0 => action_row(ADD.0, ADD.1, marked, info_column, width, palette),
                index if index <= entries.len() => {
                    let entry = &entries[index - 1];
                    action_row(
                        &terminal_safe(&entry.path.to_string_lossy()),
                        entry.status(),
                        marked,
                        info_column,
                        width,
                        palette,
                    )
                }
                _ => action_row(CLEAR.0, CLEAR.1, marked, info_column, width, palette),
            };
            rows.push(row);
        }
        rows
    }
}

/// Where the pinned rows' values start: the indent, the wider label and the
/// gap (`workspaceSummaryValueColumn`, `:1234-1238`).
fn summary_column(width: usize) -> usize {
    let indent = if width <= 2 { 0 } else { 2 };
    (indent + cells(WIDEST_SUMMARY_LABEL) + COLUMN_GAP).min(width)
}

/// Where the choices' descriptions start, or `None` on a screen too narrow
/// for them (`workspaceActionInfoColumn`, `:1208-1232`): after the widest
/// label and the gap, but never so far right that the widest description is
/// cut.
fn info_column(entries: &[Entry], width: usize) -> Option<usize> {
    let indent = if width <= 2 { 0 } else { 2 };
    let mut longest = cells(ADD.0).max(cells(CLEAR.0));
    let mut widest = cells(ADD.1).max(cells(CLEAR.1));
    for entry in entries {
        longest = longest.max(cells(&terminal_safe(&entry.path.to_string_lossy())));
        widest = widest.max(cells(entry.status()));
    }
    if width < indent + 8 + COLUMN_GAP + widest {
        return None;
    }
    Some((indent + longest + COLUMN_GAP).min(width - widest))
}

/// A pinned row: the label bold in the label grey, the value dim at `column`
/// (`composeLabelValueRow`, `:1254-1287`).
fn label_value_row(
    label: &str,
    value: &str,
    column: usize,
    width: usize,
    palette: Palette,
) -> String {
    if width == 0 {
        return String::new();
    }
    let mut row = String::from(palette.notice_heading());
    let prefix = &"  "[..width.min(2)];
    row.push_str(prefix);
    let used = prefix.len();
    let target = column.max(used + 2).min(width);
    row.push_str(&ellipsized(
        label,
        target.saturating_sub(used).saturating_sub(1),
    ));
    row.push_str(palette.reset());
    let used = cells(&row);
    if used >= width {
        return row;
    }
    let target = target.max(used + 1);
    if target >= width {
        return row;
    }
    pad_to(&mut row, target);
    row.push_str(palette.dim());
    row.push_str(&ellipsized(value, width - target));
    row.push_str(palette.reset());
    row
}

/// A choice: the mark or two blanks and the label, bright when marked and dim
/// otherwise, and the description dim at `info_column`
/// (`composeWorkspaceActionRow`, `:1160-1206`).
fn action_row(
    label: &str,
    info: &str,
    marked: bool,
    info_column: Option<usize>,
    width: usize,
    palette: Palette,
) -> String {
    let mut row = String::from(if marked {
        palette.notice_heading()
    } else {
        palette.dim()
    });
    let lead = if marked { MARK } else { "  " };
    row.push_str(&ellipsized(lead, width));
    let used = cells(&row);
    if used >= width {
        row.push_str(palette.reset());
        return row;
    }
    let Some(start) = info_column.filter(|start| *start > used + 2) else {
        row.push_str(&ellipsized(label, width - used));
        row.push_str(palette.reset());
        return row;
    };
    row.push_str(&ellipsized(label, start - used - 1));
    row.push_str(palette.reset());
    if cells(&row) >= start {
        return row;
    }
    pad_to(&mut row, start);
    row.push_str(palette.dim());
    row.push_str(&ellipsized(info, width - start));
    row.push_str(palette.reset());
    row
}

/// What the hint row says while the picker is up, on a screen `cols` wide: the
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
    use super::super::theme::{Depth, Mode};
    use super::*;
    use std::path::PathBuf;

    const PALETTE: Palette = Palette {
        mode: Mode::Dark,
        depth: Depth::Ansi256,
    };

    fn entry(path: &str, available: bool) -> Entry {
        Entry {
            path: PathBuf::from(path),
            available,
        }
    }

    /// A row with its colours taken out.
    fn plain(row: &str) -> String {
        let mut out = String::new();
        let mut chars = row.chars().peekable();
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

    #[test]
    fn the_empty_picker_is_upstreams_at_120_columns() {
        // `cap/fx-workspace-40x120-bare`, rows 10-16.
        let primary = Path::new("/private/tmp/claude-501/fxcap.C4kWG0/work");
        let menu = Menu::open();
        assert_eq!(Menu::height(40, &[]), 6);
        let rows = menu.rows(120, 6, primary, &[], PALETTE);
        assert_eq!(
            rows.iter().map(|row| plain(row)).collect::<Vec<_>>(),
            [
                "Workspace".to_string(),
                String::new(),
                format!("  Primary                   {}", primary.display()),
                "  Additional directories    0 / 16".to_string(),
                String::new(),
                "\u{276f} Add directory\u{2026}             Grant access to another directory"
                    .to_string(),
            ]
        );
        // Bold 255 title, the pinned labels in the bold label grey and their
        // values dim, the marked choice bright and its description dim.
        assert!(rows[0].starts_with(PALETTE.subtitle()), "{:?}", rows[0]);
        assert!(
            rows[2].starts_with(PALETTE.notice_heading()),
            "{:?}",
            rows[2]
        );
        assert!(rows[2].contains(PALETTE.dim()), "{:?}", rows[2]);
        assert!(
            rows[5].starts_with(PALETTE.notice_heading()),
            "{:?}",
            rows[5]
        );
        assert_eq!(
            key_help(120),
            "\u{2191}\u{2193} navigate     enter use     esc close"
        );
        assert_eq!(key_help(30), "\u{2191}\u{2193} move  enter use  esc");
        assert_eq!(key_help(10), "enter esc");
    }

    #[test]
    fn a_saved_directory_is_a_choice_between_add_and_clear() {
        // `cap/fx-workspace2-40x120-picker-one` and `-picker-down1`.
        let primary = Path::new("/private/tmp/claude-501/fxcap.6JMQe1/work");
        let entries = [entry("/private/tmp/claude-501/fxcap.6JMQe1/extra", true)];
        let mut menu = Menu::open();
        assert_eq!(Menu::height(40, &entries), 8);
        let rows = |menu: &Menu| -> Vec<String> {
            menu.rows(120, 8, primary, &entries, PALETTE)
                .iter()
                .map(|row| plain(row))
                .collect()
        };
        assert_eq!(
            rows(&menu)[3..],
            [
                "  Additional directories    1 / 16".to_string(),
                String::new(),
                format!(
                    "\u{276f} Add directory\u{2026}{}Grant access to another directory",
                    " ".repeat(32)
                ),
                format!(
                    "  /private/tmp/claude-501/fxcap.6JMQe1/extra{}Active \u{b7} Saved",
                    " ".repeat(4)
                ),
                format!(
                    "  Clear saved directories{}Remove every saved additional directory",
                    " ".repeat(23)
                ),
            ]
        );
        menu.step(true, &entries);
        assert!(rows(&menu)[6].starts_with("\u{276f} /private/tmp"));
        assert_eq!(
            menu.command(&entries),
            "/workspace remove /private/tmp/claude-501/fxcap.6JMQe1/extra"
        );
        assert_eq!(menu.marked_row(8, &entries), 6);
        menu.step(true, &entries);
        assert_eq!(menu.command(&entries), "/workspace clear");
        menu.step(true, &entries);
        assert_eq!(menu.command(&entries), "/workspace add ");
        menu.step(false, &entries);
        assert_eq!(menu.command(&entries), "/workspace clear");
    }

    #[test]
    fn with_nothing_saved_there_is_only_add_and_the_arrows_stay_on_it() {
        let mut menu = Menu::open();
        for down in [true, false, true] {
            menu.step(down, &[]);
            assert_eq!(menu.command(&[]), "/workspace add ");
        }
    }

    #[test]
    fn an_unavailable_directory_says_so() {
        let entries = [entry("/gone", false)];
        let rows = Menu::open().rows(120, 8, Path::new("/w"), &entries, PALETTE);
        assert!(
            plain(&rows[6]).ends_with("Unavailable \u{b7} Saved"),
            "{:?}",
            rows[6]
        );
    }

    #[test]
    fn a_short_panel_keeps_the_title_and_the_mark_in_view() {
        // Upstream's fallback below six rows: the title over the choices, or
        // the choices alone (`composeWorkspaceRow`, `:1017-1028`).
        let entries = [entry("/a", true), entry("/b", true)];
        let mut menu = Menu::open();
        menu.step(true, &entries);
        menu.step(true, &entries);
        menu.step(true, &entries);
        assert_eq!(menu.command(&entries), "/workspace clear");
        let rows: Vec<String> = menu
            .rows(80, 3, Path::new("/w"), &entries, PALETTE)
            .iter()
            .map(|row| plain(row))
            .collect();
        assert_eq!(rows[0], "Workspace");
        assert_eq!(rows.len(), 3);
        assert!(
            rows[2].starts_with("\u{276f} Clear saved directories"),
            "{rows:?}"
        );
        assert_eq!(menu.marked_row(3, &entries), 2);
        let one = menu.rows(80, 1, Path::new("/w"), &entries, PALETTE);
        assert_eq!(one.len(), 1);
        assert!(plain(&one[0]).starts_with("\u{276f} Clear"), "{one:?}");
        // The height never asks for more than the screen less three.
        assert_eq!(Menu::height(6, &entries), 3);
        assert_eq!(Menu::height(2, &entries), 1);
    }

    #[test]
    fn a_narrow_screen_drops_the_descriptions_and_keeps_the_labels() {
        let rows = Menu::open().rows(40, 6, Path::new("/w"), &[], PALETTE);
        assert_eq!(plain(&rows[5]), "\u{276f} Add directory\u{2026}");
    }

    #[test]
    fn a_directory_with_a_control_character_is_painted_inert() {
        let entries = [entry("/a\u{1b}[2Jb", true)];
        let rows = Menu::open().rows(120, 8, Path::new("/w"), &entries, PALETTE);
        assert!(rows[6].contains("/a\\u{001B}[2Jb"), "{:?}", rows[6]);
        assert!(!rows[6].contains("\u{1b}[2J"), "{:?}", rows[6]);
    }
}
