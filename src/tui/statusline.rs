//! `/statusline`: which of the footer's three optional segments are shown, the
//! compact menu that toggles them, and the facts the workspace segment is made
//! of.
//!
//! Upstream's command (`vercel-labs/fx@c1db9191`, `commands.zig:494`): a bare
//! `/statusline` opens a three-row picker in place of the composer, and
//! `/statusline context|session|workspace` flips one item and says so in two
//! notices. Every change is persisted at once under
//! `statusLine.{context,session,workspace}` in the profile
//! (`crate::provider::profile::write_preference`), and every item is off until
//! the profile turns it on (`crate::config::StatusLine`).
//!
//! # Two paths, and they differ on purpose
//!
//! The argument form **announces** -- `* statusline: saved to user settings
//! (scope=user)` and `* statusline: context: on` -- and the picker is
//! **silent**: Enter and the arrows flip the marked row in place and print
//! nothing unless the write failed (`applyStatuslineItem(.announce)` against
//! `(.silent)`, `app_commands.zig:3905-3929`, and the e2e pin
//! `tui-slash-menu.test.ts:1816`). The picker's own row is the answer there;
//! a notice per keystroke would be the transcript repeating the band.
//!
//! # What the picker looks like
//!
//! `StatuslineMenu` (`settings_catalog.zig:136-168`) as
//! `compact_command_menu_presentation.zig:53-90` paints it, between the band's
//! two rules: a bold `Status line` title, a blank row, then one row per item --
//! two columns of indent, the label (bold when marked, dim otherwise), and at a
//! fixed column the two values `off  on` with the one in force bold. The marked
//! row is brightness alone, there is no caret glyph, and the descriptions
//! upstream carries for each item are not drawn. The composer row is not shown
//! while it is up: the picker is a modal of its own rather than a view of the
//! draft (`surface_frame.zig:384-393`, `input_visible` false).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::config::StatusLine;
use crate::config::StatusLineItem as Item;

use super::input::Action;
use super::picker::{cells, ellipsized, pad_to};
use super::theme::Palette;

/// The picker's title row.
const TITLE: &str = "Status line";

/// The title, the blank row under it, and one row per item.
const MENU_ROWS: u16 = 2 + Item::ALL.len() as u16;

/// The title and the blank row under it (`settings_row_offset`).
const TITLE_ROWS: u16 = 2;

/// The columns between the widest label and the values (`column_gap`).
const COLUMN_GAP: usize = 4;

/// The indent every item row starts with.
const INDENT: &str = "  ";

/// The values each item cycles through, in the order they are drawn.
const VALUES: [&str; 2] = ["off", "on"];

/// What the hint row says while the picker is up, widest first
/// (`composeCompactCommandMenuHintRow`, `input_presentation.zig:772-776`).
const KEY_HELP: [&str; 3] = [
    "\u{2191}\u{2193} navigate     \u{2190}\u{2192} change     esc close",
    "\u{2191}\u{2193} move  \u{2190}\u{2192} change  esc",
    "\u{2190}\u{2192} esc",
];

/// The longest session title the footer shows, in cells (`render.zig:207`).
const MOST_TITLE_CELLS: u16 = 32;

/// The most of `.git` or `HEAD` that is read (`statusline_identity.zig:5`).
const MOST_GIT_METADATA_BYTES: u64 = 4096;

/// Every item, in upstream's order.
const ITEMS: [Item; 3] = Item::ALL;

/// The picker's label for `item`.
fn label(item: Item) -> &'static str {
    match item {
        Item::Context => "Context",
        Item::Session => "Session",
        Item::Workspace => "Workspace",
    }
}

/// `on` or `off`, as the picker spells the value in force.
fn on_off(on: bool) -> &'static str {
    VALUES[usize::from(on)]
}

/// What one keystroke means to the picker.
///
/// The picker's own vocabulary, for the reason [`super::picker::PickerAction`]
/// has one: the shell translates, so a key the picker does not bind is one the
/// shell swallows deliberately rather than one that falls through by accident.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MenuAction {
    /// The previous row, wrapping. Up, and Ctrl-K.
    Up,
    /// The next row, wrapping. Down, and Ctrl-J.
    Down,
    /// Flip the marked row. Enter, Left and Right: with two values each,
    /// cycling either way is a flip (`cycleChange`, `settings_catalog.zig:332-341`).
    Flip,
    /// Close it. Escape.
    Close,
}

impl MenuAction {
    /// What a decoded action means here, and `None` for every keystroke the
    /// picker swallows without meaning anything by it.
    ///
    /// Ctrl-J and Ctrl-K are upstream's too (`app_input_runtime.zig:1366-1395`,
    /// bytes 10 and 11), and arrive here as the actions those bytes decode to
    /// on the composer.
    pub(crate) fn of(action: Action) -> Option<Self> {
        match action {
            Action::Up | Action::KillToEnd => Some(Self::Up),
            Action::Down | Action::InsertNewline => Some(Self::Down),
            Action::Submit | Action::Left | Action::Right => Some(Self::Flip),
            Action::Escape => Some(Self::Close),
            _ => None,
        }
    }
}

/// The picker: which row is marked.
///
/// The values are not held here. They are the session's
/// ([`crate::config::StatusLine`]), handed in to paint, so a row can never show
/// a value the footer is not using.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Menu {
    selected: usize,
}

impl Menu {
    /// A picker with its first row marked, which is where upstream's always
    /// opens (`StatuslineMenu.open`).
    pub(crate) fn open() -> Self {
        Self { selected: 0 }
    }

    /// The marked item.
    pub(crate) fn selected(&self) -> Item {
        ITEMS[self.selected]
    }

    /// Moves the mark one row, wrapping at both ends (`moveCompactSelection`,
    /// `settings_catalog.zig:179-187`).
    pub(crate) fn step(&mut self, down: bool) {
        let count = ITEMS.len();
        self.selected = if down {
            (self.selected + 1) % count
        } else {
            (self.selected + count - 1) % count
        };
    }

    /// How many rows the band gives it on a screen `terminal_rows` tall:
    /// the title, a blank and three items, and never more than the screen less
    /// three (`surface_frame.zig:586-592`). At least one.
    pub(crate) fn height(terminal_rows: u16) -> u16 {
        MENU_ROWS.min(terminal_rows.saturating_sub(3)).max(1)
    }

    /// The picker's rows, top first, for a panel `visible` rows tall
    /// (`composeSettingsRow`, `compact_command_menu_presentation.zig:53-90`).
    ///
    /// One row is the marked item alone; two are the title and the marked
    /// item; more are the title, the blank row and as many items as fit, in a
    /// window that keeps the mark in view.
    pub(crate) fn rows(
        &self,
        cols: u16,
        visible: u16,
        line: StatusLine,
        palette: Palette,
    ) -> Vec<String> {
        let first = self.first_item_row(visible);
        let mut rows = Vec::with_capacity(usize::from(visible));
        if first > 0 {
            rows.push(format!(
                "{}{}{}",
                palette.subtitle(),
                ellipsized(TITLE, usize::from(cols)),
                palette.reset()
            ));
        }
        if first > 1 {
            rows.push(String::new());
        }
        let (start, end) = self.window(visible);
        for (index, item) in ITEMS.into_iter().enumerate().take(end).skip(start) {
            rows.push(item_row(item, index == self.selected, line, cols, palette));
        }
        rows
    }

    /// Which row of a `visible`-row panel the marked item is on.
    pub(crate) fn marked_row(&self, visible: u16) -> u16 {
        let (start, _) = self.window(visible);
        // At most three items above it, so the narrowing cannot fail.
        self.first_item_row(visible) + u16::try_from(self.selected - start).unwrap_or(0)
    }

    /// The panel row the first item is painted on.
    fn first_item_row(&self, visible: u16) -> u16 {
        match visible {
            0 | 1 => 0,
            2 => 1,
            _ => TITLE_ROWS,
        }
    }

    /// The items a `visible`-row panel shows, as a half-open range.
    fn window(&self, visible: u16) -> (usize, usize) {
        let room =
            usize::from(visible.saturating_sub(self.first_item_row(visible))).clamp(1, ITEMS.len());
        let start = self
            .selected
            .saturating_sub(room - 1)
            .min(ITEMS.len() - room);
        (start, start + room)
    }
}

/// One item's row (`composeChoiceRow`, `compact_command_menu_presentation.zig:1289-1320`).
fn item_row(item: Item, marked: bool, line: StatusLine, cols: u16, palette: Palette) -> String {
    let width = usize::from(cols);
    let indent = if width <= 2 { "" } else { INDENT };
    let column = value_column(cols);
    let mut row = String::from(indent);
    row.push_str(if marked {
        palette.subtitle()
    } else {
        palette.dim()
    });
    row.push_str(&ellipsized(
        label(item),
        column.saturating_sub(indent.len()).saturating_sub(2),
    ));
    row.push_str(palette.reset());
    pad_to(&mut row, column);
    let on = line.get(item);
    for (index, value) in VALUES.into_iter().enumerate() {
        let used = cells(&row);
        if used >= width {
            break;
        }
        if index > 0 {
            row.push_str(&" ".repeat(2.min(width - used)));
        }
        let current = on_off(on) == value;
        row.push_str(if current {
            palette.subtitle()
        } else {
            palette.dim()
        });
        row.push_str(&ellipsized(value, width.saturating_sub(cells(&row))));
        row.push_str(palette.reset());
    }
    row
}

/// The column the values start at: the indent, the widest label and the gap
/// (`statuslineValueColumn`, `compact_command_menu_presentation.zig:1322-1330`).
fn value_column(cols: u16) -> usize {
    let indent = if cols <= 2 { 0 } else { INDENT.len() };
    let widest = ITEMS
        .into_iter()
        .map(|item| cells(label(item)))
        .max()
        .unwrap_or(0);
    (indent + widest + COLUMN_GAP).min(usize::from(cols))
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

/// The session title as the footer shows it: its control characters dropped
/// and at most [`MOST_TITLE_CELLS`] cells of what is left, cut without an
/// ellipsis (`prefixByWidth`, `render.zig:399-401`).
///
/// Dropped rather than escaped, as upstream's cached title drops them
/// (`appendTitleWithoutControlBytes`, `app_session_runtime.zig:3580-3590`):
/// the title is the first line of a prompt, and a colour sequence in it would
/// otherwise be the one escape the painter lets through.
pub(crate) fn session_label(title: &str) -> String {
    let inert: String = title.chars().filter(|c| !c.is_control()).collect();
    super::frame::clip(inert.trim(), MOST_TITLE_CELLS).to_string()
}

/// The workspace segment for a left side with `room` cells left: the path,
/// then ` (<branch>)` when there is one (`composeWorkspaceIdentity`,
/// `render.zig:312-358`).
///
/// The path gives up its **head** and the branch its **tail**, each behind an
/// ellipsis, because the end of a path and the start of a branch are the parts
/// that say which one it is. The branch gets at most half the room, never
/// fewer than four cells, and never so many the path has none; below seven
/// cells there is no branch at all, because fragments of both say less than
/// the end of the path alone.
pub(crate) fn workspace_segment(label: &str, branch: Option<&str>, room: usize) -> Option<String> {
    if label.is_empty() || room == 0 {
        return None;
    }
    if let Some(branch) = branch.filter(|branch| !branch.is_empty()) {
        if room >= 7 {
            let branch_room = cells(branch).min((room - 4).min(4.max(room / 2)));
            let path_room = room - 3 - branch_room;
            return Some(format!(
                "{} ({})",
                tail(label, path_room),
                head(branch, branch_room)
            ));
        }
    }
    Some(tail(label, room))
}

/// The end of `text` in at most `room` cells, behind an ellipsis when it was
/// cut (`clippedWorkspaceSuffix`).
fn tail(text: &str, room: usize) -> String {
    if cells(text) <= room {
        return text.to_string();
    }
    if room <= 1 {
        return "\u{2026}".repeat(room);
    }
    let start = text
        .char_indices()
        .map(|(index, _)| index)
        .find(|&index| cells(&text[index..]) < room)
        .unwrap_or(text.len());
    format!("\u{2026}{}", &text[start..])
}

/// The start of `text` in at most `room` cells, an ellipsis after it when it
/// was cut (`clippedBranchPrefix`).
fn head(text: &str, room: usize) -> String {
    if cells(text) <= room {
        return text.to_string();
    }
    if room <= 1 {
        return "\u{2026}".repeat(room);
    }
    let keep = u16::try_from(room - 1).unwrap_or(u16::MAX);
    format!("{}\u{2026}", super::frame::clip(text, keep))
}

/// `raw` with nothing in it a terminal would act on: every control character
/// and every invisible formatting code point spelled out instead
/// (`encodeTerminalSafe`, `text_utils.zig:560-626`).
///
/// Spelled out rather than dropped, as upstream does for the two values this
/// is used on -- a directory's path and a branch name -- because those are
/// names the user has to be able to recognise, and a name with a character
/// silently missing is a different name.
fn terminal_safe(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for character in raw.chars() {
        let point = u32::from(character);
        if point < 0x20 || point == 0x7f {
            out.push_str(&format!("\\x{point:02x}"));
        } else if (0x80..=0x9f).contains(&point)
            || (0x200b..=0x200f).contains(&point)
            || (0x2028..=0x202e).contains(&point)
            || (0x2060..=0x206f).contains(&point)
            || point == 0xfeff
        {
            out.push_str(&format!("\\u{{{point:04x}}}"));
        } else {
            out.push(character);
        }
    }
    out
}

/// The workspace as the footer names it: its path, and the Git branch its
/// `HEAD` was on when it was last read (`statusline_identity.zig`).
///
/// **Read when it is asked for, never per frame.** Where `HEAD` lives is
/// resolved once, here; the branch is read again by [`Self::refresh`], which
/// the shell calls at launch, when the segment is turned on, at the end of
/// every turn and on a resize -- the moments a branch can have moved under a
/// session that is showing it. Upstream stats `HEAD` on every render instead;
/// a band painted from `&self` has nowhere to keep the answer, and one small
/// read at those moments is what the segment is worth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Workspace {
    label: String,
    head: Option<PathBuf>,
    branch: Option<String>,
}

impl Workspace {
    /// The workspace at `root`, its branch not read yet.
    pub(crate) fn new(root: &Path) -> Self {
        Self {
            label: terminal_safe(&root.to_string_lossy()),
            head: resolve_head(root),
            branch: None,
        }
    }

    /// Reads the branch again.
    pub(crate) fn refresh(&mut self) {
        self.branch = self.head.as_deref().and_then(read_branch);
    }

    /// The path, terminal-safe.
    pub(crate) fn label(&self) -> &str {
        &self.label
    }

    /// The branch as of the last [`Self::refresh`].
    pub(crate) fn branch(&self) -> Option<&str> {
        self.branch.as_deref()
    }
}

/// Where a `.git` entry says what it is.
enum Found {
    /// There is no `.git` here; look in the parent.
    Missing,
    /// There is one and it is not a repository this can read; stop.
    Invalid,
    /// `HEAD` is here.
    Head(PathBuf),
}

/// The `HEAD` file of the repository `root` is inside, if any
/// (`resolveHeadPath`, `statusline_identity.zig:196-252`).
///
/// The nearest `.git` going up wins. A directory is a repository's own; a
/// **file** is a linked worktree's or a submodule's, and its `gitdir:` line
/// names the directory whose `HEAD` is this checkout's -- so a worktree shows
/// its own branch rather than the main checkout's. Anything else in the way
/// -- a symlink, a `.git` file without that line -- ends the search: a parent
/// repository is not the one the user is in.
fn resolve_head(root: &Path) -> Option<PathBuf> {
    let mut candidate = Some(root);
    while let Some(dir) = candidate {
        match head_at(dir) {
            Found::Head(head) => return Some(head),
            Found::Invalid => return None,
            Found::Missing => candidate = dir.parent(),
        }
    }
    None
}

fn head_at(dir: &Path) -> Found {
    let dot_git = dir.join(".git");
    let metadata = match fs::symlink_metadata(&dot_git) {
        Ok(metadata) => metadata,
        Err(err)
            if matches!(
                err.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
            ) =>
        {
            return Found::Missing
        }
        Err(_) => return Found::Invalid,
    };
    if metadata.is_dir() {
        return Found::Head(dot_git.join("HEAD"));
    }
    if !metadata.is_file() {
        return Found::Invalid;
    }
    let Some(text) = read_small(&dot_git) else {
        return Found::Invalid;
    };
    let Some(git_dir) = text
        .trim()
        .strip_prefix("gitdir:")
        .map(str::trim)
        .filter(|git_dir| !git_dir.is_empty())
    else {
        return Found::Invalid;
    };
    Found::Head(dir.join(git_dir).join("HEAD"))
}

/// What `HEAD` says the checkout is on, terminal-safe (`parseHead`,
/// `statusline_identity.zig:263-286`): the branch a `ref: refs/heads/…` names,
/// any other ref as written, or `detached:` and twelve hex digits of a commit.
fn read_branch(head: &Path) -> Option<String> {
    let text = read_small(head)?;
    let text = text.trim();
    if let Some(reference) = text.strip_prefix("ref:") {
        let reference = reference.trim();
        let branch = reference.strip_prefix("refs/heads/").unwrap_or(reference);
        return (!branch.is_empty()).then(|| terminal_safe(branch));
    }
    if text.len() < 7 || !text.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    Some(format!("detached:{}", &text[..text.len().min(12)]))
}

/// A file of at most [`MOST_GIT_METADATA_BYTES`], as text.
fn read_small(path: &Path) -> Option<String> {
    let metadata = fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > MOST_GIT_METADATA_BYTES {
        return None;
    }
    fs::read_to_string(path).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::super::theme::{Depth, Mode};

    const PALETTE: Palette = Palette {
        mode: Mode::Dark,
        depth: Depth::Ansi256,
    };
    const BRIGHT: &str = "\u{1b}[1;38;5;255m";
    const DIM: &str = "\u{1b}[38;5;245m";
    const RESET: &str = "\u{1b}[0m";

    fn line(context: bool, session: bool, workspace: bool) -> StatusLine {
        StatusLine {
            context,
            session,
            workspace,
        }
    }

    #[test]
    fn the_argument_is_one_of_three_words_exactly() {
        assert_eq!(Item::parse("context"), Some(Item::Context));
        assert_eq!(Item::parse("session"), Some(Item::Session));
        assert_eq!(Item::parse("workspace"), Some(Item::Workspace));
        // Spaces and tabs around it are not part of it (`std.mem.trim(" \t")`).
        assert_eq!(Item::parse(" \tcontext\t "), Some(Item::Context));
        // Case-sensitive, and one word: everything else is the usage error.
        for refused in [
            "",
            "Context",
            "CONTEXT",
            "bogus",
            "context please",
            "sandbox",
            "ctx",
        ] {
            assert_eq!(Item::parse(refused), None, "{refused:?}");
        }
    }

    #[test]
    fn each_item_reads_and_writes_its_own_bool_and_its_own_key() {
        for item in ITEMS {
            let mut state = StatusLine::default();
            assert!(!state.get(item));
            state.set(item, true);
            assert!(state.get(item));
            // Only its own.
            let others = ITEMS.iter().filter(|other| **other != item);
            assert!(
                others.into_iter().all(|other| !state.get(*other)),
                "{item:?}"
            );
            assert_eq!(item.key_path(), ["statusLine", item.name()]);
        }
        assert_eq!(on_off(true), "on");
        assert_eq!(on_off(false), "off");
    }

    #[test]
    fn the_keys_the_picker_binds_and_nothing_else() {
        assert_eq!(MenuAction::of(Action::Up), Some(MenuAction::Up));
        assert_eq!(MenuAction::of(Action::KillToEnd), Some(MenuAction::Up));
        assert_eq!(MenuAction::of(Action::Down), Some(MenuAction::Down));
        assert_eq!(
            MenuAction::of(Action::InsertNewline),
            Some(MenuAction::Down)
        );
        for flip in [Action::Submit, Action::Left, Action::Right] {
            assert_eq!(MenuAction::of(flip), Some(MenuAction::Flip), "{flip:?}");
        }
        assert_eq!(MenuAction::of(Action::Escape), Some(MenuAction::Close));
        for other in [
            Action::Tab,
            Action::Backspace,
            Action::Cancel,
            Action::Eof,
            Action::Home,
            Action::PasteStart,
        ] {
            assert_eq!(MenuAction::of(other), None, "{other:?}");
        }
    }

    #[test]
    fn the_mark_wraps_at_both_ends_and_starts_on_the_first_row() {
        let mut menu = Menu::open();
        assert_eq!(menu.selected(), Item::Context);
        menu.step(true);
        assert_eq!(menu.selected(), Item::Session);
        menu.step(true);
        menu.step(true);
        assert_eq!(menu.selected(), Item::Context, "down from the last row");
        menu.step(false);
        assert_eq!(menu.selected(), Item::Workspace, "up from the first row");
    }

    #[test]
    fn the_picker_is_upstreams_rows_byte_for_byte() {
        // `cap/fx-statusline-40x120-bare.ansi`, rows 4-8, in this palette's
        // spelling of the same attributes (tmux re-serializes both).
        let menu = Menu::open();
        let rows = menu.rows(120, Menu::height(40), StatusLine::default(), PALETTE);
        assert_eq!(
            rows,
            vec![
                format!("{BRIGHT}Status line{RESET}"),
                String::new(),
                format!("  {BRIGHT}Context{RESET}      {BRIGHT}off{RESET}  {DIM}on{RESET}"),
                format!("  {DIM}Session{RESET}      {BRIGHT}off{RESET}  {DIM}on{RESET}"),
                format!("  {DIM}Workspace{RESET}    {BRIGHT}off{RESET}  {DIM}on{RESET}"),
            ]
        );
        // `…-enter`: Session marked and on.
        let mut menu = Menu::open();
        menu.step(true);
        let rows = menu.rows(120, 5, line(false, true, false), PALETTE);
        assert_eq!(
            rows[3],
            format!("  {BRIGHT}Session{RESET}      {DIM}off{RESET}  {BRIGHT}on{RESET}")
        );
        assert_eq!(
            rows[2],
            format!("  {DIM}Context{RESET}      {BRIGHT}off{RESET}  {DIM}on{RESET}")
        );
    }

    #[test]
    fn a_short_screen_shows_the_marked_row_and_as_much_around_it_as_fits() {
        assert_eq!(Menu::height(40), 5);
        assert_eq!(Menu::height(7), 4);
        assert_eq!(Menu::height(5), 2);
        assert_eq!(Menu::height(3), 1);
        let mut menu = Menu::open();
        menu.step(false);
        let plain = |rows: Vec<String>| -> Vec<String> {
            rows.iter()
                .map(|row| {
                    row.replace(BRIGHT, "")
                        .replace(DIM, "")
                        .replace(RESET, "")
                        .trim_end()
                        .to_string()
                })
                .collect()
        };
        let state = StatusLine::default();
        // Four rows: the title, the blank and the two items that end on the mark.
        assert_eq!(
            plain(menu.rows(120, 4, state, PALETTE)),
            [
                "Status line",
                "",
                "  Session      off  on",
                "  Workspace    off  on"
            ]
        );
        assert_eq!(menu.marked_row(4), 3);
        // Two: the title and the marked item. One: the marked item alone.
        assert_eq!(
            plain(menu.rows(120, 2, state, PALETTE)),
            ["Status line", "  Workspace    off  on"]
        );
        assert_eq!(menu.marked_row(2), 1);
        assert_eq!(
            plain(menu.rows(120, 1, state, PALETTE)),
            ["  Workspace    off  on"]
        );
        assert_eq!(menu.marked_row(1), 0);
        assert_eq!(Menu::open().marked_row(5), 2);
    }

    #[test]
    fn a_narrow_screen_cuts_the_values_rather_than_overflowing() {
        let menu = Menu::open();
        for cols in [1u16, 2, 10, 15, 17, 20] {
            for row in menu.rows(cols, 5, StatusLine::default(), PALETTE) {
                assert!(cells(&row) <= usize::from(cols), "{cols}: {row:?}");
            }
        }
    }

    #[test]
    fn the_key_help_gives_way_as_upstreams_does() {
        assert_eq!(
            key_help(120),
            "\u{2191}\u{2193} navigate     \u{2190}\u{2192} change     esc close"
        );
        assert_eq!(
            key_help(30),
            "\u{2191}\u{2193} move  \u{2190}\u{2192} change  esc"
        );
        assert_eq!(key_help(10), "\u{2190}\u{2192} esc");
        assert_eq!(key_help(2), "\u{2190}\u{2192} esc");
    }

    #[test]
    fn a_session_title_loses_its_controls_and_stops_at_thirty_two_cells() {
        assert_eq!(session_label("say the marker"), "say the marker");
        assert_eq!(session_label("red \u{1b}[31mtext"), "red [31mtext");
        let long = "x".repeat(40);
        assert_eq!(session_label(&long), "x".repeat(32));
    }

    #[test]
    fn the_workspace_segment_keeps_the_end_of_the_path_and_the_start_of_the_branch() {
        // `cap/fx-statusline-40x120-workspace`: a path and no repository.
        assert_eq!(
            workspace_segment("/private/tmp/fxcap.X/work", None, 100).as_deref(),
            Some("/private/tmp/fxcap.X/work")
        );
        assert_eq!(
            workspace_segment("/a/repo", Some("main"), 100).as_deref(),
            Some("/a/repo (main)")
        );
        // Too little room: the path loses its head behind an ellipsis.
        assert_eq!(
            workspace_segment("/a/long/path/status-root", None, 10).as_deref(),
            Some("\u{2026}atus-root")
        );
        // The branch takes at most half and at least four, and the path the
        // rest: 20 cells is 10 for the branch, 7 for the path, 3 for ` ()`.
        let both = workspace_segment(
            "/a/long/path/status-root",
            Some("feature/very-long-branch"),
            20,
        )
        .expect("a segment");
        assert_eq!(both, "\u{2026}s-root (feature/v\u{2026})");
        assert_eq!(cells(&both), 20);
        // Below seven cells, the path alone.
        assert_eq!(
            workspace_segment("/a/b/root", Some("main"), 6).as_deref(),
            Some("\u{2026}/root")
        );
        assert_eq!(workspace_segment("/a", Some("main"), 0), None);
        assert_eq!(workspace_segment("", Some("main"), 40), None);
        for room in 1..40 {
            let segment = workspace_segment("/x/y/z/workspace", Some("a-branch"), room)
                .expect("some room is a segment");
            assert!(cells(&segment) <= room, "{room}: {segment:?}");
        }
    }

    #[test]
    fn a_name_a_terminal_would_obey_is_spelled_out() {
        assert_eq!(terminal_safe("plain/path"), "plain/path");
        assert_eq!(terminal_safe("a\u{1b}[31mb"), "a\\x1b[31mb");
        assert_eq!(terminal_safe("tab\there"), "tab\\x09here");
        assert_eq!(terminal_safe("del\u{7f}"), "del\\x7f");
        assert_eq!(terminal_safe("c1\u{9b}"), "c1\\u{009b}");
        assert_eq!(terminal_safe("rtl\u{202e}x"), "rtl\\u{202e}x");
        assert_eq!(terminal_safe("bom\u{feff}"), "bom\\u{feff}");
        assert_eq!(terminal_safe("日本"), "日本");
    }

    #[test]
    fn the_branch_is_read_from_the_nearest_head_and_follows_a_linked_worktree() {
        let root = tempfile::tempdir().expect("a root");
        let repository = root.path().join("repository");
        let nested = repository.join("packages").join("status-root");
        fs::create_dir_all(repository.join(".git")).expect(".git");
        fs::create_dir_all(&nested).expect("nested");
        let head = repository.join(".git").join("HEAD");
        fs::write(&head, "ref: refs/heads/initial-branch\n").expect("HEAD");

        // From a directory inside it, up to the repository's own `.git`
        // (`tui-startup.test.ts:175-240`).
        let mut workspace = Workspace::new(&nested);
        assert_eq!(workspace.branch(), None, "not read until asked");
        workspace.refresh();
        assert_eq!(workspace.branch(), Some("initial-branch"));
        assert_eq!(workspace.label(), nested.to_string_lossy());

        // Read again when asked, and a detached HEAD is twelve hex digits.
        fs::write(&head, "0123456789abcdef0123456789abcdef01234567\n").expect("HEAD");
        workspace.refresh();
        assert_eq!(workspace.branch(), Some("detached:0123456789ab"));
        // A ref outside `refs/heads/` is said as written; nonsense is nothing.
        fs::write(&head, "ref: refs/remotes/origin/main\n").expect("HEAD");
        workspace.refresh();
        assert_eq!(workspace.branch(), Some("refs/remotes/origin/main"));
        for nonsense in ["", "abc", "not-a-sha-at-all", "ref:   "] {
            fs::write(&head, nonsense).expect("HEAD");
            workspace.refresh();
            assert_eq!(workspace.branch(), None, "{nonsense:?}");
        }

        // A linked worktree: `.git` is a file naming the worktree's own git
        // directory, relative or absolute, and its HEAD is the branch.
        let linked = root.path().join("linked");
        let worktree_git = repository.join(".git").join("worktrees").join("linked");
        fs::create_dir_all(&worktree_git).expect("worktree git dir");
        fs::create_dir_all(&linked).expect("linked checkout");
        fs::write(worktree_git.join("HEAD"), "ref: refs/heads/feat/linked\n").expect("HEAD");
        fs::write(
            linked.join(".git"),
            format!("gitdir: {}\n", worktree_git.display()),
        )
        .expect(".git file");
        let mut workspace = Workspace::new(&linked);
        workspace.refresh();
        assert_eq!(workspace.branch(), Some("feat/linked"));
        fs::write(
            linked.join(".git"),
            "gitdir: ../repository/.git/worktrees/linked\n",
        )
        .expect(".git file");
        let mut workspace = Workspace::new(&linked);
        workspace.refresh();
        assert_eq!(workspace.branch(), Some("feat/linked"));

        // A `.git` file that names nothing ends the search with no branch,
        // rather than borrowing a parent repository's.
        fs::write(linked.join(".git"), "not a gitdir line\n").expect(".git file");
        let mut workspace = Workspace::new(&linked);
        workspace.refresh();
        assert_eq!(workspace.branch(), None);
    }

    #[test]
    fn a_workspace_outside_any_repository_has_a_path_and_no_branch() {
        let root = tempfile::tempdir().expect("a root");
        let mut workspace = Workspace::new(root.path());
        workspace.refresh();
        // The temporary directory's parents are not a repository on any
        // machine this runs on; if one were, the claim is still that the
        // label is the path.
        assert_eq!(workspace.label(), root.path().to_string_lossy());
    }
}
