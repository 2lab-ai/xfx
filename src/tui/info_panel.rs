//! A read-only panel in the band: a title over labelled rows, the screen fx's
//! `/status` and bare `/permissions` open (`vercel-labs/fx@481104fd
//! src/core/input/info_panel.zig`, painted by
//! `src/ui/footer/info_panel_presentation.zig`).
//!
//! Upstream counts it among its compact command menus, beside `/statusline`,
//! `/usage` and `/workspace` (`activeCompactCommandMenu`,
//! `app_input_runtime.zig:1840-1846`): while it is up it takes the composer's
//! rows, it binds the arrows, Escape and Enter, and it swallows every other
//! key. It writes nothing to the document, the session log or the settings.
//!
//! # What fills it
//!
//! Whoever opens it hands over its rows -- a label and a value, or a list item
//! under the labelled row before it ([`Row`]) -- and the facts they were built
//! from ([`Facts`]), and the panel owns them from then on. Opening starts at the
//! top; a rebuild keeps the scroll ([`Panel::open`], [`Panel::replace`];
//! `State.open`/`replace`, `info_panel.zig:76-86`). Whether a rebuild is owed is
//! the opener's question, asked of [`Panel::facts`] on each tick
//! (`collectInfoPanelFacts`, `app_commands.zig:541-552`).
//!
//! # Every text is made terminal-safe at the door
//!
//! A label and a value are written with every control character spelled as
//! its code point ([`terminal_safe`]), as upstream's rows are (`safeText`,
//! `output_contracts.zig:575-578`, `encodeTerminalSafe`). It is done **here**,
//! in [`Row`]'s constructors, rather than by each caller: a row built from a
//! workspace path, a model name or a rule's pattern cannot carry an escape --
//! or a line break that would tear the panel's layout -- onto the screen,
//! whoever built it. Spelling an already-spelled text again changes nothing,
//! so a caller that also encodes is harmless.
//!
//! # Layout
//!
//! The rows it asks for are its two header rows and every wrapped line, and
//! the rows it is given are those, at most the screen less the band's five
//! fixed rows and five rows of document (`footerRows`,
//! `info_panel_presentation.zig:30-40`; `inlinePickerRowBudgetCapped`,
//! `picker_presentation.zig:793-803`). The header is the title and a blank row
//! while they leave two rows for lines, the title alone while they leave one,
//! and nothing in a panel one row tall (`:131-136`). Values start in one column
//! ([`value_column`]) and wrap under it ([`next_break`]); a list item is a
//! bullet, its wrapped lines are indented to the bullet's text, and it gets two
//! columns fewer (`:96`, `:151-155`).

use crate::config::PermissionMode;
use crate::output::{ShellSession, StatusSnapshot};
use crate::permission::session_rules::Rule as SavedRule;
use crate::permission::RuleSet;
use crate::workspace::access::terminal_safe;

use super::input::Action;
use super::permissions::Granted;
use super::picker::{cells, ellipsized, pad_to};
use super::theme::Palette;

/// The columns before a label (`indent`, `info_panel_presentation.zig:19`).
const INDENT: usize = 2;

/// The columns between the longest label and the values, while the screen has
/// them (`column_gap`, `:20`).
const COLUMN_GAP: usize = 4;

/// The narrowest value column a screen gets before the gap shrinks
/// (`min_value_width`, `:22`).
const MIN_VALUE_WIDTH: usize = 24;

/// The title and the blank row under it (`header_rows`, `:24`).
const HEADER_ROWS: u16 = 2;

/// What starts a list item (`bullet`, `:26`).
const BULLET: &str = "\u{2022} ";

/// The columns [`BULLET`] takes, and the indent of an item's wrapped lines.
const BULLET_WIDTH: usize = 2;

/// What an item's wrapped line starts with, in place of the bullet.
const ITEM_CONTINUATION: &str = "  ";

/// The band's rows that are not the panel's -- the two rules, the hint row and
/// the two the composer's block keeps -- and the rows of document a panel
/// leaves in view (`inlinePickerRowBudgetCapped`'s `fixed_rows` with no
/// composer extra and no banner, and `minimum_transcript_rows`,
/// `picker_presentation.zig:799-800`).
const FIXED_ROWS: u16 = 5;
const MINIMUM_TRANSCRIPT_ROWS: u16 = 5;

/// The hint row while the lines do not all fit, widest first
/// (`composeCompactCommandMenuHintRow`, `input_presentation.zig:785-789`).
const SCROLL_HELP: [&str; 3] = [
    "\u{2191}\u{2193} scroll     esc close",
    "\u{2191}\u{2193} scroll  esc",
    "\u{2191}\u{2193} esc",
];

/// The hint row while they do (`:789-793`).
const CLOSE_HELP: [&str; 3] = ["esc close", "esc close", "esc"];

/// Which panel it is, and so its title (`Kind`, `info_panel.zig:11-21`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Status,
    Permissions,
}

impl Kind {
    /// The panel's title row.
    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::Status => "Status",
            Self::Permissions => "Permissions",
        }
    }
}

/// What a panel's rows depend on that can change with no keystroke while it is
/// open -- a running turn that saves the session, ends, or gains a session
/// grant from an approval -- so the rows are rebuilt when these differ from
/// the ones they were built from (`Facts`, `info_panel.zig:23-39`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Facts {
    Status {
        turns: usize,
        /// `None` when the shell has no session.
        saved: Option<bool>,
        grants: usize,
        /// The session's id as the row shows it, `None` with none. Not one of
        /// upstream's facts, whose session id is fixed at launch; xfx's is the
        /// next one's until a prompt opens it, and a `/new` while the panel is
        /// up moves it with nothing else changed.
        id: Option<String>,
    },
    /// A bare `/permissions`'s: the mode, the grants and the configured rules
    /// in force (`permissionsFacts`, `session_commands.zig:378-384@481104fd`),
    /// and two facts upstream's has not.
    ///
    /// The live authority's reset count: upstream's engine clears its grants
    /// in the reset itself; xfx's grants are the runtime's last publication,
    /// which a reset puts out of force the moment it is taken
    /// (`super::worker`'s `WorkHandle::grants`), so the count is what tells an
    /// open panel to say so before the runtime publishes again -- with the
    /// mode and the grant count unchanged too.
    ///
    /// How the session's saved-rule store stands
    /// ([`super::permissions::StoreStamp`]): the rules held before the first
    /// save, or the open session's file. Upstream reads its store only when
    /// another fact moves; xfx's open store is a file another hand can change
    /// or break, so the panel looks at it on each tick and reads it again
    /// when it moved.
    Permissions {
        mode: PermissionMode,
        grants: usize,
        rules: usize,
        resets: u64,
        store: super::permissions::StoreStamp,
    },
}

impl Facts {
    /// What a `/status` panel built from `snapshot` depends on: the turns, the
    /// session's being saved -- `None` with no session to save -- and the
    /// grants (`statusFacts`, `vercel-labs/fx@481104fd
    /// src/core/session/session_commands.zig:342-351`), and the session's id.
    pub(crate) fn of_status(snapshot: &StatusSnapshot) -> Self {
        let (saved, id) = match &snapshot.session {
            Some(ShellSession::Open { id, saved }) => (Some(*saved), Some(id.clone())),
            Some(ShellSession::Unavailable) | None => (None, None),
        };
        Self::Status {
            turns: usize::try_from(snapshot.history_turns).unwrap_or(usize::MAX),
            saved,
            grants: usize::try_from(snapshot.session_permission_grants).unwrap_or(usize::MAX),
            id,
        }
    }

    /// Which panel facts of this shape fill.
    pub(crate) fn kind(&self) -> Kind {
        match self {
            Self::Status { .. } => Kind::Status,
            Self::Permissions { .. } => Kind::Permissions,
        }
    }
}

/// One row the opener hands over: a label and its value, or an item of the
/// list the labelled row above it counts (`PanelRow`,
/// `output_contracts.zig:978-985`).
///
/// Built only through [`Row::new`] and [`Row::item`], which are where both
/// texts are made terminal-safe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Row {
    label: String,
    value: String,
    item: bool,
}

impl Row {
    /// A labelled row.
    pub(crate) fn new(label: &str, value: &str) -> Self {
        Self {
            label: terminal_safe(label),
            value: terminal_safe(value),
            item: false,
        }
    }

    /// An item of the list the labelled row above it counts: no label, a
    /// bullet before its value. `/status` has none; bare `/permissions` lists
    /// its rules, grants and saved rules with it ([`permissions_rows`]).
    pub(crate) fn item(value: &str) -> Self {
        Self {
            label: String::new(),
            value: terminal_safe(value),
            item: true,
        }
    }
}

/// What one keystroke means to the panel; `None` from [`Self::of`] is a key it
/// swallows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MenuAction {
    /// Scroll up a line. Up, and Ctrl-K.
    Up,
    /// Scroll down a line. Down, and Ctrl-J.
    Down,
    /// Close it. Escape, and Enter: the panel only shows, so Enter closes it
    /// as Escape does (`submitCompactCommandMenuSelection`,
    /// `app_input_runtime.zig:2121-2126`).
    Close,
}

impl MenuAction {
    /// What a decoded action means here (`moveCompactCommandMenu`,
    /// `:2173-2189`, from the arrows at `:2259-2265` and the bytes `10` and
    /// `11` at `:1205-1212`; `cancelCompactCommandMenu`, `:2816-2824`). Tab,
    /// Shift-Tab, Page Up and Down, Ctrl-C and every other key are swallowed
    /// (`:1187-1215`); a paste is not a key, and the shell routes it past the
    /// panel to the composer behind it.
    pub(crate) fn of(action: Action) -> Option<Self> {
        match action {
            Action::Up | Action::ControlK => Some(Self::Up),
            Action::Down | Action::ControlJ => Some(Self::Down),
            Action::Escape | Action::Submit => Some(Self::Close),
            _ => None,
        }
    }
}

/// The panel: its rows, how far they are scrolled, and the facts they were
/// built from (`Snapshot` and `State`, `info_panel.zig:52-112`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Panel {
    rows: Vec<Row>,
    /// The first line shown when they do not all fit.
    scroll: usize,
    facts: Facts,
}

/// The `/status` panel's rows from `snapshot`, with a `Settings` row in place
/// of the credential's when `settings_error` says why the profile settings did
/// not read ([`StatusSnapshot::panel_rows`]): made terminal-safe here, as every
/// row is ([`Row::new`]).
pub(crate) fn status_rows(snapshot: &StatusSnapshot, settings_error: Option<&str>) -> Vec<Row> {
    snapshot
        .panel_rows(settings_error)
        .iter()
        .map(|(label, value)| Row::new(label, value))
        .collect()
}

/// What a list's labelled row says: how many, or `none` (`countText`,
/// `output_contracts.zig:580-583@481104fd`).
fn count_text(count: usize) -> String {
    if count == 0 {
        "none".to_string()
    } else {
        count.to_string()
    }
}

/// The three rows a bare `/permissions` panel ends with: how each part of the
/// authority is changed (`PermissionsSnapshot.panelRows`,
/// `output_contracts.zig:1056-1058@481104fd`).
const CHANGE_MODE: &str = "/permissions ask, auto, full-access, or reset";
const REMEMBER: &str = "/permissions remember allow|deny <tool-name> <arguments-json>";
const REVOKE: &str = "/permissions revoke <rule-id>";

/// One item of a bare `/permissions` list, from text a file or a model put
/// there -- a configured rule's pattern, a grant's command or path, a saved
/// rule's action.
///
/// It names what is in force, so it is spelled as the approval band spells
/// what an "always" grants (`super::approval_view::named`): every control,
/// every character that reorders a line and every default-ignorable code
/// point named, so what the person reads as granted is the text that is,
/// laid out in the cells xfx counts. Upstream spells only its control
/// characters here (`safeText`); for printable text the two are the same.
/// [`Row::item`] then makes it terminal-safe, as it makes every row.
fn listed(text: &str) -> Row {
    Row::item(&super::approval_view::named(text))
}

/// A bare `/permissions` panel's rows, in upstream's order
/// (`PermissionsSnapshot.panelRows`, `output_contracts.zig:1036-1060@481104fd`):
/// the mode, the configured rules with their count -- `<action> <permission>
/// → <pattern>` each, in the table's order -- the conversation's grants --
/// `<tool> → <target>` -- and the saved session's rules -- `<id> <decision>
/// <action>` -- then how to change each.
///
/// `mode`, `rules` and `grants` are the authority in force, the caller's to
/// read (`super::shell`'s `Shell::permissions_panel`). Upstream shows `Saved
/// rules` only in a host that keeps saved sessions (`savedPermissionRules`,
/// `session_commands.zig:388-400`); the TUI always does, so the row is always
/// here, `none` before a session is open.
pub(crate) fn permissions_rows(
    mode: PermissionMode,
    rules: &RuleSet,
    grants: &[Granted],
    saved: &[SavedRule],
) -> Vec<Row> {
    let mut rows = vec![
        Row::new("Mode", mode.display_label()),
        Row::new("Configured rules", &count_text(rules.rules().len())),
    ];
    rows.extend(rules.rules().iter().map(|rule| {
        listed(&format!(
            "{} {} \u{2192} {}",
            rule.action.label(),
            rule.permission,
            rule.pattern
        ))
    }));
    rows.push(Row::new("Session grants", &count_text(grants.len())));
    rows.extend(
        grants
            .iter()
            .map(|grant| listed(&format!("{} \u{2192} {}", grant.tool, grant.target))),
    );
    rows.push(Row::new("Saved rules", &count_text(saved.len())));
    rows.extend(saved.iter().map(|rule| {
        listed(&format!(
            "{} {} {}",
            rule.id,
            rule.decision.label(),
            rule.display
        ))
    }));
    rows.push(Row::new("Change mode", CHANGE_MODE));
    rows.push(Row::new("Remember", REMEMBER));
    rows.push(Row::new("Revoke", REVOKE));
    rows
}

impl Panel {
    /// The `/status` panel at its top, from `snapshot` (`openStatusSnapshot`,
    /// `session_commands.zig:283-296@481104fd`).
    pub(crate) fn status(snapshot: &StatusSnapshot, settings_error: Option<&str>) -> Self {
        Self::open(
            status_rows(snapshot, settings_error),
            Facts::of_status(snapshot),
        )
    }

    /// A panel at its top (`State.open`, `info_panel.zig:76-79`).
    pub(crate) fn open(rows: Vec<Row>, facts: Facts) -> Self {
        Self {
            rows,
            scroll: 0,
            facts,
        }
    }

    /// New rows for the same panel, the scroll kept (`State.replace`,
    /// `:81-86`). Composing clamps a scroll the new rows are too few for.
    pub(crate) fn replace(&mut self, rows: Vec<Row>, facts: Facts) {
        self.rows = rows;
        self.facts = facts;
    }

    /// What the rows were built from.
    pub(crate) fn facts(&self) -> &Facts {
        &self.facts
    }

    /// The title row's text.
    pub(crate) fn title(&self) -> &'static str {
        self.facts.kind().title()
    }

    /// The rows the band gives it on a screen `terminal_rows` x `cols`: its
    /// header and every line, at most what leaves the band's fixed rows and
    /// five rows of document, and never none (`footerRows`,
    /// `info_panel_presentation.zig:38-40`).
    pub(crate) fn height(&self, terminal_rows: u16, cols: u16) -> u16 {
        let wanted = usize::from(HEADER_ROWS) + self.line_count(cols);
        let wanted = u16::try_from(wanted).unwrap_or(u16::MAX);
        let room = terminal_rows
            .saturating_sub(FIXED_ROWS)
            .saturating_sub(MINIMUM_TRANSCRIPT_ROWS)
            .max(1);
        wanted.min(room)
    }

    /// Whether its lines do not all fit in a panel `visible` rows tall, so the
    /// arrows scroll them (`overflows`, `:47-51`).
    pub(crate) fn overflows(&self, visible: u16, cols: u16) -> bool {
        self.line_count(cols) > visible_lines(visible)
    }

    /// One line up or down on a panel `visible` rows tall, clamped at both
    /// ends with no wrap; whether it moved (`scrollBy`, `info_panel.zig:101-111`).
    pub(crate) fn scroll_by(&mut self, down: bool, visible: u16, cols: u16) -> bool {
        let total = self.line_count(cols);
        let last_start = total.saturating_sub(visible_lines(visible).max(1));
        let start = self.scroll.min(last_start);
        let next = if down {
            start.saturating_add(1).min(last_start)
        } else {
            start.saturating_sub(1)
        };
        if next == self.scroll {
            return false;
        }
        self.scroll = next;
        true
    }

    /// What the hint row says under a panel `visible` rows tall on a screen
    /// `cols` wide: the widest of the three variants that fits, the narrowest
    /// when none does (`input_presentation.zig:785-793`, `:821-827`).
    pub(crate) fn key_help(&self, visible: u16, cols: u16) -> &'static str {
        let variants = if self.overflows(visible, cols) {
            &SCROLL_HELP
        } else {
            &CLOSE_HELP
        };
        variants
            .iter()
            .copied()
            .find(|candidate| cells(candidate) <= usize::from(cols))
            .unwrap_or(variants[variants.len() - 1])
    }

    /// The panel's rows, top first, for a panel `visible` rows tall on a
    /// screen `cols` wide (`composeRow`, `info_panel_presentation.zig:61-105`):
    /// the header, then the lines from the scroll -- clamped here too, so a
    /// scroll past the last full window shows the last full window.
    pub(crate) fn rows(&self, cols: u16, visible: u16, palette: Palette) -> Vec<String> {
        let width = usize::from(cols);
        let mut out = Vec::with_capacity(usize::from(visible));
        if width == 0 {
            return out;
        }
        let header = header_rows(visible);
        if header > 0 {
            out.push(format!(
                "{}{}{}",
                palette.subtitle(),
                ellipsized(self.title(), width),
                palette.reset()
            ));
        }
        if header > 1 {
            out.push(String::new());
        }
        let lines = lines(&self.rows, width);
        let shown = visible_lines(visible);
        let start = self.scroll.min(lines.len().saturating_sub(shown));
        let column = value_column(&self.rows, width);
        for line in lines.iter().skip(start).take(shown) {
            out.push(compose_line(line, column, width, palette));
        }
        out
    }

    /// How many lines its rows make on a screen `cols` wide (`lineCount`,
    /// `:53-59`).
    fn line_count(&self, cols: u16) -> usize {
        lines(&self.rows, usize::from(cols)).len()
    }
}

/// One line of the panel: the label it shows (its row's, on the row's first
/// line only), what leads the value, and the part of the value on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Line<'a> {
    label: &'a str,
    lead: &'static str,
    text: &'a str,
}

/// Every line `rows` make on a screen `width` wide, each value wrapped under
/// its column (`composeRow`'s walk and `wrappedLineCount`,
/// `info_panel_presentation.zig:88-103`, `:157-166`). A row with no value is
/// still one line.
fn lines(rows: &[Row], width: usize) -> Vec<Line<'_>> {
    let value_width = value_width(rows, width);
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let text_width = text_width(row, value_width);
        let mut rest = row.value.as_str();
        let mut first = true;
        while first || !rest.is_empty() {
            let cut = next_break(rest, text_width);
            let lead = match (row.item, first) {
                (false, _) => "",
                (true, true) => BULLET,
                (true, false) => ITEM_CONTINUATION,
            };
            out.push(Line {
                label: if first { &row.label } else { "" },
                lead,
                text: &rest[..cut.end],
            });
            rest = &rest[cut.next..];
            first = false;
        }
    }
    out
}

/// One line painted (`composeLine`, `info_panel_presentation.zig:107-129`):
/// the indent and the label bold in the label grey, cut one column short of
/// the value column; then, from the value column, the lead and the text dim.
/// Nothing past the label when the screen ends at or before the value column.
fn compose_line(line: &Line<'_>, column: usize, width: usize, palette: Palette) -> String {
    let mut row = String::from(palette.notice_heading());
    row.push_str(&"  "[..width.min(INDENT)]);
    row.push_str(&ellipsized(
        line.label,
        column.saturating_sub(INDENT).saturating_sub(1),
    ));
    row.push_str(palette.reset());
    let used = cells(&row);
    if used >= width || column >= width {
        return row;
    }
    pad_to(&mut row, column);
    row.push_str(palette.dim());
    let room = width - column;
    row.push_str(&ellipsized(line.lead, room));
    row.push_str(&ellipsized(
        line.text,
        room.saturating_sub(cells(line.lead)),
    ));
    row.push_str(palette.reset());
    row
}

/// The rows of a panel `visible` rows tall that are its header: the title
/// and its blank row while they leave room for two lines, the title alone
/// while they leave one, and none in a panel of one row (`headerRowsFor`,
/// `info_panel_presentation.zig:131-136`).
fn header_rows(visible: u16) -> u16 {
    if visible > HEADER_ROWS + 1 {
        HEADER_ROWS
    } else {
        u16::from(visible > 1)
    }
}

/// The lines a panel `visible` rows tall shows (`visibleLineCount`, `:42-45`).
fn visible_lines(visible: u16) -> usize {
    usize::from(visible.saturating_sub(header_rows(visible)))
}

/// Where the values start on a screen `width` wide: the indent, the longest
/// label and the gap when that leaves the values [`MIN_VALUE_WIDTH`] columns,
/// else a gap of two, never past the screen (`valueColumn`, `:138-145`).
fn value_column(rows: &[Row], width: usize) -> usize {
    let longest = rows.iter().map(|row| cells(&row.label)).max().unwrap_or(0);
    let roomy = INDENT + longest + COLUMN_GAP;
    if roomy + MIN_VALUE_WIDTH <= width {
        return roomy;
    }
    (INDENT + longest + 2).min(width)
}

/// The columns from the value column to the screen's edge, at least one
/// (`valueWidth`, `:147-149`).
fn value_width(rows: &[Row], width: usize) -> usize {
    width.saturating_sub(value_column(rows, width)).max(1)
}

/// The columns `row`'s text may take on each of its lines: the value's, less
/// a list item's bullet, at least one (`textWidth`, `:151-155`).
fn text_width(row: &Row, value_width: usize) -> usize {
    if row.item {
        value_width.saturating_sub(BULLET_WIDTH).max(1)
    } else {
        value_width
    }
}

/// Where a line of `text` ends, and where the next one starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Break {
    end: usize,
    next: usize,
}

/// Where the first line of `text` ends when it may take `width` columns
/// (`nextBreak`, `info_panel_presentation.zig:175-202`): all of it when it
/// fits; else at the last space that fits -- not one that starts the text --
/// with the space dropped; else just after the last `/` that fits, so a path
/// breaks between its folders; else where the columns run out, a space there
/// dropped. A line always takes at least one character, so a character wider
/// than the column cannot stall the wrap. Measured a character at a time, as
/// upstream measures a rune at a time.
fn next_break(text: &str, width: usize) -> Break {
    if cells(text) <= width {
        return Break {
            end: text.len(),
            next: text.len(),
        };
    }
    let mut col = 0;
    let mut index = 0;
    let mut last_space = None;
    let mut last_slash = None;
    let mut buffer = [0u8; 4];
    for (at, character) in text.char_indices() {
        let rune = cells(character.encode_utf8(&mut buffer));
        if col + rune > width {
            break;
        }
        if character == ' ' {
            last_space = Some(at);
        }
        if character == '/' {
            last_slash = Some(at + 1);
        }
        col += rune;
        index = at + character.len_utf8();
    }
    if index == 0 {
        let first = text.chars().next().map_or(0, char::len_utf8);
        return Break {
            end: first,
            next: first,
        };
    }
    if let Some(space) = last_space.filter(|&space| space > 0) {
        return Break {
            end: space,
            next: space + 1,
        };
    }
    if let Some(slash) = last_slash {
        return Break {
            end: slash,
            next: slash,
        };
    }
    let next = if text.as_bytes().get(index) == Some(&b' ') {
        index + 1
    } else {
        index
    };
    Break { end: index, next }
}

#[cfg(test)]
mod tests {
    use super::super::theme::{Depth, Mode};
    use super::*;

    const PALETTE: Palette = Palette {
        mode: Mode::Dark,
        depth: Depth::Ansi256,
    };

    /// `row` with the palette taken off, and the blank cells a terminal would
    /// not keep after its last character.
    fn unpainted(row: &str) -> String {
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
        out.trim_end().to_string()
    }

    /// The text of `rows` with the palette taken off.
    fn plain(rows: &[String]) -> Vec<String> {
        rows.iter().map(|row| unpainted(row)).collect()
    }

    fn status() -> Facts {
        Facts::Status {
            turns: 0,
            saved: Some(false),
            grants: 0,
            id: Some("VnpQQs_uueY8".to_string()),
        }
    }

    /// The rows `cap/panels/new/fx-p5-panels-status-short-*` show, in fx's
    /// order and words (`StatusSnapshot.panelRows`, `output_contracts.zig:731-776`):
    /// fixture data for the layout, not a claim about xfx's own facts, which
    /// are `/status`'s to map. The workspace is a path the length of the
    /// capture's, with its last fitting `/` in the same column.
    fn fx_status_rows(workspace: &str) -> Vec<Row> {
        vec![
            Row::new("Session", "VnpQQs_uueY8 \u{b7} not saved yet"),
            Row::new("Model", "fake/model-a"),
            Row::new("Auth", "AI_GATEWAY_API_KEY"),
            Row::new("Permissions", "auto"),
            Row::new("Workspace", workspace),
            Row::new("Turns", "0"),
            Row::new("Session grants", "0"),
            Row::new("Step limit", "unlimited"),
            Row::new("Ultrafast", "not requested"),
            Row::new("Updates", "stable"),
            Row::new("Build", "stable \u{b7} 481104fdbc24"),
        ]
    }

    /// Forty columns, broken after column 24 at 50 columns -- the shape of the
    /// capture's `/private/tmp/claude-501/fxcap.A18AGX/work`.
    const WORKSPACE: &str = "/scratch/workdir/claude/fxcap.A18AGX/work";

    fn brk(text: &str, width: usize) -> (&str, &str) {
        let cut = next_break(text, width);
        (&text[..cut.end], &text[cut.next..])
    }

    #[test]
    fn next_break_takes_all_of_a_text_that_fits() {
        assert_eq!(brk("fake/model-a", 12), ("fake/model-a", ""));
        assert_eq!(brk("", 3), ("", ""));
    }

    #[test]
    fn next_break_breaks_at_the_last_space_that_fits_and_drops_it() {
        // `Auth help`'s sentence at the noauth capture's width, in miniature.
        assert_eq!(brk("fx needs access", 10), ("fx needs", "access"));
        // The last space, not the first.
        assert_eq!(brk("a b c d", 5), ("a b", "c d"));
    }

    #[test]
    fn next_break_ignores_a_space_that_starts_the_text() {
        // `if (space > 0)`: a leading space is no break, so the line is cut
        // where the columns run out instead of being empty.
        assert_eq!(brk(" abcdef", 4), (" abc", "def"));
    }

    #[test]
    fn next_break_breaks_a_path_just_after_its_last_fitting_slash() {
        // `cap/panels/new/fx-p5-panels-status-short-24x50-open`, rows 10-11:
        // the `/` stays at the end of the first line.
        assert_eq!(
            brk("/private/tmp/claude-501/fxcap.A18AGX/work", 30),
            ("/private/tmp/claude-501/", "fxcap.A18AGX/work")
        );
        // A space that fits wins over a slash that fits.
        assert_eq!(brk("a/b c/def", 7), ("a/b", "c/def"));
    }

    #[test]
    fn next_break_cuts_where_the_columns_run_out_and_drops_a_space_there() {
        assert_eq!(brk("abcdefgh", 3), ("abc", "defgh"));
        // The character that did not fit is a space: the next line skips it.
        assert_eq!(brk("abc defgh", 3), ("abc", "defgh"));
    }

    #[test]
    fn next_break_always_takes_one_character() {
        // A wide character in a one-column budget, and a zero budget.
        assert_eq!(brk("\u{4e2d}\u{6587}", 1), ("\u{4e2d}", "\u{6587}"));
        assert_eq!(brk("ab", 0), ("a", "b"));
        // Wide characters count two columns each.
        assert_eq!(
            brk("\u{4e2d}\u{6587}\u{5b57}", 4),
            ("\u{4e2d}\u{6587}", "\u{5b57}")
        );
    }

    #[test]
    fn the_value_column_is_a_four_column_gap_while_the_values_keep_24_columns() {
        let rows = fx_status_rows(WORKSPACE);
        // `Session grants` is the longest label: 2 + 14 + 4 = 20.
        assert_eq!(value_column(&rows, 120), 20);
        // 20 + 24 = 44 still fits, as at the 24x50 capture.
        assert_eq!(value_column(&rows, 50), 20);
        assert_eq!(value_column(&rows, 44), 20);
    }

    #[test]
    fn the_value_column_falls_back_to_a_two_column_gap_and_never_past_the_screen() {
        let rows = fx_status_rows(WORKSPACE);
        // One column short of 44: 2 + 14 + 2.
        assert_eq!(value_column(&rows, 43), 18);
        assert_eq!(value_column(&rows, 20), 18);
        assert_eq!(value_column(&rows, 10), 10);
        assert_eq!(value_width(&rows, 10), 1);
        // And the line painted there: the label cut a column short of the
        // value column, the value from it.
        let panel = Panel::open(rows, status());
        let shown = plain(&panel.rows(43, 13, PALETTE));
        assert_eq!(
            shown[2],
            format!("  Session{}VnpQQs_uueY8 \u{b7} not saved", " ".repeat(9))
        );
        assert_eq!(shown[3], "                  yet");
    }

    #[test]
    fn a_label_too_long_for_its_column_is_ellipsized_one_column_short_of_it() {
        let panel = Panel::open(
            vec![
                Row::new("A very long label indeed", "v"),
                Row::new("Short", "w"),
            ],
            status(),
        );
        // Longest 24: 2 + 24 + 2 = 28 > 20, so the column is the screen's edge
        // and nothing past the label is painted.
        let shown = plain(&panel.rows(20, 4, PALETTE));
        assert_eq!(shown[2], "  A very long labe\u{2026}");
    }

    #[test]
    fn the_header_collapses_to_the_title_and_then_to_nothing() {
        let panel = Panel::open(fx_status_rows(WORKSPACE), status());
        let at = |visible| plain(&panel.rows(120, visible, PALETTE));
        let session = format!(
            "  Session{}VnpQQs_uueY8 \u{b7} not saved yet",
            " ".repeat(11)
        );
        let model = format!("  Model{}fake/model-a", " ".repeat(13));
        // One row: no header at all, the first line.
        assert_eq!(at(1), std::slice::from_ref(&session));
        // Two rows: the title alone, then a line.
        assert_eq!(at(2), ["Status".to_string(), session.clone()]);
        // Three: still the title alone -- the blank row needs two lines under it.
        assert_eq!(
            at(3),
            ["Status".to_string(), session.clone(), model.clone()]
        );
        // Four: the title and its blank row, then two lines.
        assert_eq!(at(4), ["Status".to_string(), String::new(), session, model]);
        assert_eq!(header_rows(5), 2);
        assert_eq!(visible_lines(1), 1);
        assert_eq!(visible_lines(2), 1);
        assert_eq!(visible_lines(3), 2);
        assert_eq!(visible_lines(4), 2);
    }

    #[test]
    fn the_panel_asks_for_its_lines_and_is_given_the_screen_less_ten_rows() {
        let panel = Panel::open(fx_status_rows(WORKSPACE), status());
        // 14 rows: 14 - 10 = 4 of the 13 it asks for (`…-14x120-open`).
        assert_eq!(panel.height(14, 120), 4);
        // 24x50: the workspace wraps, so 2 + 12 = 14 rows, all given.
        assert_eq!(panel.height(24, 50), 14);
        // 40x120: 2 + 11.
        assert_eq!(panel.height(40, 120), 13);
        // Never none.
        assert_eq!(panel.height(6, 120), 1);
        assert_eq!(panel.height(0, 120), 1);
    }

    #[test]
    fn the_scroll_clamps_at_both_ends_without_wrapping() {
        let mut panel = Panel::open(fx_status_rows(WORKSPACE), status());
        // Up at the top moves nothing.
        assert!(!panel.scroll_by(false, 4, 120));
        assert_eq!(panel.scroll, 0);
        // Eleven lines, two shown: the last window starts at 9.
        for _ in 0..9 {
            assert!(panel.scroll_by(true, 4, 120));
        }
        assert_eq!(panel.scroll, 9);
        for _ in 0..5 {
            assert!(!panel.scroll_by(true, 4, 120), "it wrapped");
        }
        assert_eq!(panel.scroll, 9);
        assert!(panel.scroll_by(false, 4, 120));
        assert_eq!(panel.scroll, 8);
        // A panel whose lines all fit does not scroll at all.
        let mut short = Panel::open(fx_status_rows(WORKSPACE), status());
        assert!(!short.scroll_by(true, 13, 120));
        assert_eq!(short.scroll, 0);
    }

    #[test]
    fn a_scroll_past_the_last_window_is_shown_as_the_last_window() {
        // A rebuild with fewer rows, or a taller screen, keeps a scroll the
        // rows no longer reach; composing clamps it (`:83`) and the next
        // arrow steps from the clamped start (`info_panel.zig:105-107`).
        let mut panel = Panel::open(fx_status_rows(WORKSPACE), status());
        panel.scroll = 9;
        panel.replace(fx_status_rows(WORKSPACE)[..4].to_vec(), status());
        assert_eq!(panel.scroll, 9, "replace kept the scroll");
        let shown = plain(&panel.rows(120, 4, PALETTE));
        assert!(shown[2].starts_with("  Auth"), "{shown:?}");
        assert!(shown[3].starts_with("  Permissions"), "{shown:?}");
        assert!(panel.scroll_by(false, 4, 120));
        assert_eq!(panel.scroll, 1);
    }

    #[test]
    fn opening_starts_at_the_top_and_names_the_panel_by_its_facts() {
        let panel = Panel::open(fx_status_rows(WORKSPACE), status());
        assert_eq!(panel.scroll, 0);
        assert_eq!(panel.title(), "Status");
        let permissions = Facts::Permissions {
            mode: PermissionMode::Ask,
            grants: 0,
            rules: 0,
            resets: 0,
            store: super::super::permissions::StoreStamp::default(),
        };
        let panel = Panel::open(Vec::new(), permissions.clone());
        assert_eq!(panel.title(), "Permissions");
        assert_eq!(panel.facts(), &permissions);
    }

    #[test]
    fn the_short_status_capture_is_drawn_at_each_scroll() {
        // `cap/panels/new/fx-p5-panels-status-short-14x120-{open,down1,
        // down-end,up1,ctrl-k}`, rows 4-7, and the hint row 9.
        let mut panel = Panel::open(fx_status_rows(WORKSPACE), status());
        let window = |panel: &Panel| plain(&panel.rows(120, 4, PALETTE))[2..].to_vec();
        let line =
            |label: &str, value: &str| format!("  {label}{}{value}", " ".repeat(18 - label.len()));
        assert_eq!(
            window(&panel),
            [
                line("Session", "VnpQQs_uueY8 \u{b7} not saved yet"),
                line("Model", "fake/model-a")
            ]
        );
        assert_eq!(
            panel.key_help(4, 120),
            "\u{2191}\u{2193} scroll     esc close"
        );
        panel.scroll_by(true, 4, 120);
        assert_eq!(
            window(&panel),
            [
                line("Model", "fake/model-a"),
                line("Auth", "AI_GATEWAY_API_KEY")
            ]
        );
        for _ in 0..14 {
            panel.scroll_by(true, 4, 120);
        }
        assert_eq!(
            window(&panel),
            [
                line("Updates", "stable"),
                line("Build", "stable \u{b7} 481104fdbc24")
            ]
        );
        panel.scroll_by(false, 4, 120);
        assert_eq!(
            window(&panel),
            [
                line("Ultrafast", "not requested"),
                line("Updates", "stable")
            ]
        );
        panel.scroll_by(false, 4, 120);
        assert_eq!(
            window(&panel),
            [
                line("Step limit", "unlimited"),
                line("Ultrafast", "not requested")
            ]
        );
    }

    #[test]
    fn the_short_status_capture_fits_whole_at_24_by_50_with_the_path_wrapped() {
        // `cap/panels/new/fx-p5-panels-status-short-24x50-open`, rows 4-17
        // and the hint row 19: everything fits, the workspace breaks after
        // its last fitting `/`, under the value column, and the hint offers
        // no arrows.
        let panel = Panel::open(fx_status_rows(WORKSPACE), status());
        let visible = panel.height(24, 50);
        let shown = plain(&panel.rows(50, visible, PALETTE));
        assert_eq!(shown.len(), 14);
        assert_eq!(shown[0], "Status");
        assert_eq!(shown[1], "");
        assert_eq!(
            shown[6],
            format!("  Workspace{}/scratch/workdir/claude/", " ".repeat(9))
        );
        assert_eq!(shown[7], format!("  {}fxcap.A18AGX/work", " ".repeat(18)));
        assert_eq!(
            shown[13],
            format!("  Build{}stable \u{b7} 481104fdbc24", " ".repeat(13))
        );
        assert!(!panel.overflows(visible, 50));
        assert_eq!(panel.key_help(visible, 50), "esc close");
    }

    #[test]
    fn the_rows_carry_upstreams_styles() {
        // `….ansi`: the title bold 255, a label (its indent too) bold 252, a
        // value 245, each closed by a reset.
        let panel = Panel::open(vec![Row::new("Model", "fake/model-a")], status());
        let rows = panel.rows(120, 3, PALETTE);
        assert_eq!(
            rows[0],
            format!("{}Status{}", PALETTE.subtitle(), PALETTE.reset())
        );
        assert_eq!(
            rows[1],
            format!(
                "{}  Model{}    {}fake/model-a{}",
                PALETTE.notice_heading(),
                PALETTE.reset(),
                PALETTE.dim(),
                PALETTE.reset()
            )
        );
    }

    #[test]
    fn list_items_are_bullets_with_their_wrapped_lines_under_the_bullets_text() {
        // `cap/panels/new/fx-p5-panels-permissions-40x120-rules`: the
        // labelled row counts, the items follow it as bullets.
        let panel = Panel::open(
            vec![
                Row::new("Configured rules", "1"),
                Row::item("allow bash \u{2192} git *"),
                Row::item("deny write_file abcdefghij"),
            ],
            status(),
        );
        // Longest 16: roomy 22 needs 46; at 40 the column is 2 + 16 + 2 = 20,
        // 20 value columns, an item's text 18.
        let shown = plain(&panel.rows(40, 6, PALETTE));
        assert_eq!(shown[2], "  Configured rules  1");
        assert_eq!(
            shown[3],
            format!("  {}\u{2022} allow bash \u{2192} git *", " ".repeat(18))
        );
        assert_eq!(
            shown[4],
            format!("  {}\u{2022} deny write_file", " ".repeat(18))
        );
        assert_eq!(shown[5], format!("  {}  abcdefghij", " ".repeat(18)));
    }

    /// A labelled line of a bare `/permissions` panel at 120 columns, and an
    /// item of it (`cap/panels/new/fx-p5-panels-permissions-40x120-grants`).
    fn permissions_line(label: &str, value: &str) -> String {
        format!("  {label}{}{value}", " ".repeat(20 - label.len()))
    }

    fn permissions_item(text: &str) -> String {
        format!("{}\u{2022} {text}", " ".repeat(22))
    }

    fn permissions_facts() -> Facts {
        Facts::Permissions {
            mode: PermissionMode::Ask,
            grants: 1,
            rules: 1,
            resets: 0,
            store: super::super::permissions::StoreStamp::default(),
        }
    }

    fn saved_rule(id: u64, display: &str) -> SavedRule {
        SavedRule {
            id,
            key: crate::permission::session_rules::RuleKey {
                kind: crate::permission::session_rules::KeyKind::FileMutation,
                canonical: b"k".to_vec(),
            },
            display: display.to_string(),
            decision: crate::permission::session_rules::Decision::Deny,
            generation: 1,
        }
    }

    #[test]
    fn the_permissions_panel_is_upstreams_rows_in_upstreams_order() {
        // `cap/panels/new/fx-p5-panels-permissions-40x120-grants`, rows 4-14,
        // with xfx's own grant: the mode, each list counted with its items
        // under it, and the three ways to change them.
        let rules = RuleSet::new(vec![crate::permission::ConfiguredRule::new(
            "bash",
            "git *",
            crate::permission::RuleAction::Allow,
        )]);
        let grants = [Granted {
            tool: "terminal".to_string(),
            target: "echo hi in /w".to_string(),
        }];
        let saved = [saved_rule(
            1,
            "write_file b.txt preimage=absent after=a1fce4363854ff88",
        )];
        let panel = Panel::open(
            permissions_rows(PermissionMode::Ask, &rules, &grants, &saved),
            permissions_facts(),
        );
        assert_eq!(panel.title(), "Permissions");
        assert_eq!(panel.height(40, 120), 12);
        assert!(!panel.overflows(12, 120));
        let shown = plain(&panel.rows(120, 12, PALETTE));
        assert_eq!(
            shown,
            [
                "Permissions".to_string(),
                String::new(),
                permissions_line("Mode", "ask"),
                permissions_line("Configured rules", "1"),
                permissions_item("allow bash \u{2192} git *"),
                permissions_line("Session grants", "1"),
                permissions_item("terminal \u{2192} echo hi in /w"),
                permissions_line("Saved rules", "1"),
                permissions_item("1 deny write_file b.txt preimage=absent after=a1fce4363854ff88"),
                permissions_line(
                    "Change mode",
                    "/permissions ask, auto, full-access, or reset"
                ),
                permissions_line(
                    "Remember",
                    "/permissions remember allow|deny <tool-name> <arguments-json>"
                ),
                permissions_line("Revoke", "/permissions revoke <rule-id>"),
            ]
        );
        assert_eq!(panel.key_help(12, 120), "esc close");
    }

    #[test]
    fn an_empty_list_says_none_and_lists_nothing() {
        // `cap/panels/new/fx-p5-panels-permissions-40x120-bare`; full access
        // in upstream's words (`permissionModeDisplayLabel`).
        let rows = permissions_rows(PermissionMode::Yolo, &RuleSet::default(), &[], &[]);
        let panel = Panel::open(rows, permissions_facts());
        let shown = plain(&panel.rows(120, 9, PALETTE));
        assert_eq!(
            shown[2..6],
            [
                permissions_line("Mode", "full access"),
                permissions_line("Configured rules", "none"),
                permissions_line("Session grants", "none"),
                permissions_line("Saved rules", "none"),
            ]
        );
    }

    #[test]
    fn a_permissions_item_names_what_a_model_or_a_file_put_in_it() {
        // A grant's command is a model's text, a pattern a file's, a saved
        // rule's action the store's: every control, line break, reordering
        // and default-ignorable character is named, as the approval band
        // names what an "always" grants (`approval_view::named`), so the
        // listing is the text in force, on one line, in the cells xfx counts.
        let rules = RuleSet::new(vec![crate::permission::ConfiguredRule::new(
            "bash",
            "git\u{200b} *",
            crate::permission::RuleAction::Deny,
        )]);
        let grants = [Granted {
            tool: "terminal".to_string(),
            target: "printf '\u{1b}[2J'\necho \u{202e}fdsa in /w".to_string(),
        }];
        let saved = [saved_rule(7, "terminal.exec \u{2066}x\u{2069}")];
        let rows = permissions_rows(PermissionMode::Ask, &rules, &grants, &saved);
        let items: Vec<&str> = rows
            .iter()
            .filter(|row| row.item)
            .map(|row| row.value.as_str())
            .collect();
        assert_eq!(
            items,
            [
                "deny bash \u{2192} git\\u{200B} *",
                "terminal \u{2192} printf '\\u{001B}[2J'\\necho \\u{202E}fdsa in /w",
                "7 deny terminal.exec \\u{2066}x\\u{2069}",
            ]
        );
        for row in &rows {
            assert!(
                !row.value.chars().any(|c| c.is_control()
                    || matches!(c, '\u{200b}' | '\u{202e}' | '\u{2066}' | '\u{2069}')),
                "{row:?}"
            );
        }
    }

    #[test]
    fn a_permissions_item_names_the_joiners_and_selectors_an_emoji_carries() {
        // Review P-permissions-b item 3: what is listed as in force is spelled
        // as the approval band spells what an "always" grants
        // (`approval_view::named`), with no emoji exception. `tamed` -- the
        // band's spelling for a diff or a question -- keeps an RGI sequence's
        // joiners and a `U+FE0F` after a heart as they are; here each is named,
        // and the bell is named by either.
        let grants = [Granted {
            tool: "terminal".to_string(),
            target: "echo \u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467} \u{2764}\u{fe0f}\u{7} in /w"
                .to_string(),
        }];
        let rows = permissions_rows(PermissionMode::Ask, &RuleSet::default(), &grants, &[]);
        let item = rows.iter().find(|row| row.item).expect("the grant's item");
        assert_eq!(
            item.value,
            "terminal \u{2192} echo \u{1f468}\\u{200D}\u{1f469}\\u{200D}\u{1f467} \u{2764}\\u{FE0F}\\u{0007} in /w"
        );
    }

    #[test]
    fn every_label_and_value_is_made_terminal_safe() {
        // A value that would clear the screen and a label that would set the
        // title: spelled as code points, so nothing reaches the terminal to
        // obey and the line breaks no layout.
        let row = Row::new("Work\u{1b}]0;x\u{7}", "/w/\u{1b}[2J\nnext");
        assert_eq!(row.label, "Work\\u{001B}]0;x\\u{0007}");
        assert_eq!(row.value, "/w/\\u{001B}[2J\\u{000A}next");
        let item = Row::item("a\rb");
        assert_eq!(item.value, "a\\u{000D}b");
        // Spelling twice is spelling once.
        assert_eq!(Row::new(&row.label, &row.value), row);
        let panel = Panel::open(vec![row], status());
        for painted in panel.rows(120, 3, PALETTE) {
            assert!(
                !unpainted(&painted).chars().any(char::is_control),
                "{painted:?}"
            );
        }
    }

    #[test]
    fn the_hint_says_scroll_only_while_the_lines_overflow_and_narrows_with_the_screen() {
        let panel = Panel::open(fx_status_rows(WORKSPACE), status());
        assert!(panel.overflows(4, 120));
        assert_eq!(
            panel.key_help(4, 23),
            "\u{2191}\u{2193} scroll     esc close"
        );
        assert_eq!(panel.key_help(4, 22), "\u{2191}\u{2193} scroll  esc");
        assert_eq!(panel.key_help(4, 14), "\u{2191}\u{2193} scroll  esc");
        assert_eq!(panel.key_help(4, 13), "\u{2191}\u{2193} esc");
        assert_eq!(panel.key_help(4, 2), "\u{2191}\u{2193} esc");
        assert!(!panel.overflows(13, 120));
        assert_eq!(panel.key_help(13, 120), "esc close");
        // Whether it overflows is asked at the width in question: the same
        // rows wrap onto more lines on a narrower screen.
        assert!(panel.overflows(13, 40));
        // At nine columns `ask` is one column a line (the value column is the
        // eighth), three lines, which five rows hold.
        let short = Panel::open(vec![Row::new("Mode", "ask")], status());
        assert!(!short.overflows(5, 9));
        assert_eq!(short.key_help(5, 9), "esc close");
        assert_eq!(short.key_help(5, 8), "esc");
    }

    #[test]
    fn the_keys_it_binds_are_the_arrows_ctrl_j_and_k_escape_and_enter() {
        for (action, meant) in [
            (Action::Up, Some(MenuAction::Up)),
            (Action::ControlK, Some(MenuAction::Up)),
            (Action::Down, Some(MenuAction::Down)),
            (Action::ControlJ, Some(MenuAction::Down)),
            (Action::Escape, Some(MenuAction::Close)),
            (Action::Submit, Some(MenuAction::Close)),
            (Action::Tab, None),
            (Action::BackTab, None),
            (Action::PageDown, None),
            (Action::PageUp, None),
            (Action::Cancel, None),
            (Action::ControlP, None),
            (Action::HistoryNext, None),
        ] {
            assert_eq!(MenuAction::of(action), meant, "{action:?}");
        }
    }
}
