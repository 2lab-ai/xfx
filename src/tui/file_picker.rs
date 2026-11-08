//! The `@` file picker: a list of workspace paths under the composer, opened by
//! an `@` the caret is in, that puts the chosen path into the draft as text.
//!
//! Upstream's (`vercel-labs/fx@c1db9191`), in the band slot the slash popup
//! uses -- between the two rules under the composer -- with three differences
//! from that popup that are upstream's and are kept:
//!
//! * **The rows start under the `@`** (`picker_start_col`,
//!   `input_presentation.zig:377-380`; `composeFilePickerOptionRow`,
//!   `picker_presentation.zig:827-849`), two columns in for an `@` at the start
//!   of the draft and further for one after a word (capture
//!   `fx-composer-40x120-at-dir.txt`: `see @src/` puts them six in). An `@` on
//!   a row above the caret's puts them at the composer's own left edge.
//! * **The marked row is a grey bar**, not bold: upstream's file rows use
//!   `approval_button_inactive_style` where the slash popup uses
//!   `selected_completion_style` (`picker_presentation.zig:801-804`), and the
//!   letters the query matched are bold inside it
//!   (`appendStyledPathRange`, `:1027-1050`).
//! * **The hint row is left alone.** The slash popup puts its key help there;
//!   the file picker has no arm in upstream's hint switch
//!   (`paint_plan.zig:946-978`), so the statusline stays -- capture
//!   `fx-composer-40x120-at.txt`, `auto · model-a` under the list.
//!
//! The list is always as tall as the screen gives it, and a query that matched
//! fewer leaves the rest blank -- so the band does not jump while the user
//! types. That is fx v0.0.16's height (a5fe00d7; `filePickerReservedRows`,
//! `picker_presentation.zig:654-666@481104fd`): a list picker's six rows, or
//! fewer on a screen without them, grown to [`MOST_FILE_ROWS`] while five
//! transcript rows stay visible above the band ([`FilePicker::height`]). The
//! arrows scroll the rows the last frame that landed painted (99e62b8e;
//! [`FilePicker::step`]).
//!
//! # What opens it
//!
//! An `@` that starts a word -- at the start of the draft, or after a space or
//! one of `( [ { < ' " `` ` `` -- with the caret inside the path after it
//! ([`query_at`], a port of `file_picker_path.zig:66-188`). The path may be
//! quoted (`@"a b"`), and a backslash keeps the byte after it in the path.
//! Escape closes it until the caret leaves the `@` word, which is the slash
//! popup's rule (`super::picker::Dismissed`) applied to this trigger.
//!
//! # What choosing does
//!
//! Tab and Enter both put the marked path in place of what was typed after the
//! `@` (`applyFilePickerSelection`, `input_completion_runtime.zig:898-953`):
//! `@path` and a space for a file, so the next word can be typed and the picker
//! closes; `@dir/` and no space for a directory, so the picker goes on, listing
//! it. Enter does **not** submit -- a second Enter does
//! (`fx-composer-40x120-at-enter.txt`). With a query that matched nothing,
//! Enter submits the line as typed (`tests/e2e/tui-file-picker.test.ts:1142`);
//! while there is nothing to choose yet, it does nothing. The mention stays
//! text: what is sent is the line the user sees.

use std::ops::Range;

use super::file_index::{is_terminal_safe, Found, Kind, Lookup};
use super::picker::{cells, edge_start};
use super::theme::Palette;

/// A list picker's rows (`default_max_picker_rows`, `list_window.zig:3`): the
/// height the `@` list keeps on a screen too short to grow it, and the rows
/// its arrows scroll until a frame has painted it.
pub(crate) const MOST_ROWS: u16 = 6;

/// The most rows the `@` list grows to (`max_file_picker_rows`,
/// `picker_presentation.zig:654-656@481104fd`).
const MOST_FILE_ROWS: u16 = 10;

/// The transcript rows the grown list leaves above the band
/// (`minimum_transcript_rows`, `picker_presentation.zig:663@481104fd`).
const TRANSCRIPT_ROWS: u16 = 5;

/// The band's rows a list cannot have besides the composer's first
/// (`fixed_rows`, `picker_presentation.zig:648`, `:662@481104fd`).
const FIXED_ROWS: u16 = 5;

/// What a row too narrow for its path is cut with.
const ELLIPSIS: &str = "\u{2026}";

/// What the list says while it has no rows (`composePickerStatusRowWithProvider`,
/// `picker_presentation.zig:900-905`, and `app_render_runtime.zig:642-649`).
const INDEXING: &str = "indexing files...";
const NO_MATCHES: &str = "no matching files";
const FILES_UNAVAILABLE: &str = "Files unavailable. tab to retry; esc to dismiss.";
const DIRECTORY_UNAVAILABLE: &str = "Directory unavailable. tab to retry; esc to dismiss.";

// ---------------------------------------------------------------------------
// the `@` word
// ---------------------------------------------------------------------------

fn is_separator(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r')
}

fn is_start_boundary(byte: u8) -> bool {
    is_separator(byte) || matches!(byte, b'(' | b'[' | b'{' | b'<' | b'\'' | b'"' | b'`')
}

fn is_sentence_punctuation(byte: u8) -> bool {
    matches!(byte, b',' | b'.' | b';' | b':' | b'!' | b'?')
}

/// Where a word that may hold quotes ends (`token_end`,
/// `file_picker_path.zig:36-52`): the image path words of a prompt are cut by
/// it too ([`super::image_paths`]).
pub(crate) fn token_end(text: &[u8], start: usize) -> usize {
    let mut index = start;
    let mut quote: Option<u8> = None;
    while index < text.len() {
        let byte = text[index];
        if byte == b'\\' && index + 1 < text.len() {
            index += 2;
            continue;
        }
        match quote {
            Some(open) if byte == open => quote = None,
            Some(_) => {}
            None if byte == b'"' || byte == b'\'' => quote = Some(byte),
            None if is_separator(byte) => break,
            None => {}
        }
        index += 1;
    }
    index
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Status {
    Complete,
    Incomplete,
    Invalid,
}

/// One `@` word (`Token`, `file_picker_path.zig:58-67`).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Token {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) path_start: usize,
    pub(crate) path_end: usize,
    pub(crate) quote_end: Option<usize>,
    pub(crate) status: Status,
    pub(crate) quoted: bool,
    /// Whether every escape in a quoted path is `\\` or `\"`, the two the
    /// picker writes ([`encode`]): such a payload is decoded once and is the
    /// file's name, where any other spelling keeps the older rules of an
    /// image path ([`super::image_paths`]).
    pub(crate) canonical_escapes: bool,
}

/// The `@` word at `start`, if one starts there (`parse_at`,
/// `file_picker_path.zig:69-115`).
pub(crate) fn parse_at(text: &[u8], start: usize) -> Option<Token> {
    if start >= text.len() || text[start] != b'@' {
        return None;
    }
    if start > 0 && !is_start_boundary(text[start - 1]) {
        return None;
    }
    let quoted = text.get(start + 1) == Some(&b'"');
    let path_start = start + 1 + usize::from(quoted);
    if !quoted {
        let end = if text.get(path_start) == Some(&b'\'') {
            token_end(text, path_start)
        } else {
            // A quote inside a bare name is part of the name.
            let mut end = path_start;
            while end < text.len() && !is_separator(text[end]) {
                if text[end] == b'\\' && end + 1 < text.len() {
                    end += 1;
                }
                end += 1;
            }
            end.min(text.len())
        };
        return Some(Token {
            start,
            end,
            path_start,
            path_end: end,
            quote_end: None,
            status: if end == path_start {
                Status::Incomplete
            } else {
                Status::Complete
            },
            quoted: false,
            canonical_escapes: true,
        });
    }
    let mut index = path_start;
    let mut canonical_escapes = true;
    while index < text.len() {
        let byte = text[index];
        if matches!(byte, b'\n' | b'\r' | b'\t') {
            break;
        }
        if byte == b'\\' {
            if index + 1 == text.len() {
                index = text.len();
                break;
            }
            if matches!(text[index + 1], b'\n' | b'\r' | b'\t') {
                break;
            }
            canonical_escapes &= matches!(text[index + 1], b'\\' | b'"');
            index += 2;
            continue;
        }
        if byte == b'"' {
            let quote_end = index + 1;
            let end = token_end(text, quote_end);
            let valid = index > path_start
                && std::str::from_utf8(&text[path_start..index]).is_ok_and(is_terminal_safe)
                && text[quote_end..end]
                    .iter()
                    .all(|byte| is_sentence_punctuation(*byte));
            return Some(Token {
                start,
                end,
                path_start,
                path_end: index,
                quote_end: Some(quote_end),
                status: if valid {
                    Status::Complete
                } else {
                    Status::Invalid
                },
                quoted: true,
                canonical_escapes,
            });
        }
        index += 1;
    }
    Some(Token {
        start,
        end: index,
        path_start,
        path_end: index,
        quote_end: None,
        status: Status::Incomplete,
        quoted: true,
        canonical_escapes,
    })
}

/// Whether `cursor` is inside the path of an `@` word of `text`
/// (`contains_position`, `file_picker_path.zig:204-212`): a `$` typed there is
/// a character of the path, not a skill search.
pub(crate) fn contains_position(text: &[u8], cursor: usize) -> bool {
    let mut offset = 0usize;
    while offset < text.len() {
        let Some(token) = parse_at(text, offset) else {
            offset += 1;
            continue;
        };
        if cursor < token.path_start {
            return false;
        }
        let end = if token.quoted && token.status == Status::Complete {
            token.path_end
        } else {
            token.end
        };
        if cursor <= end {
            return true;
        }
        offset = token.end.max(offset + 1);
    }
    false
}

/// The `@` word the caret is in, and what has been typed of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Query {
    /// What is between the start of the path and the caret, as typed: a quoted
    /// path still has its escapes.
    pub(crate) raw: String,
    /// The `@`'s offset in the draft.
    pub(crate) at: usize,
    /// The end of what a choice replaces: the caret for a bare word, the
    /// closing quote's far side for a quoted one.
    pub(crate) replace_end: usize,
    pub(crate) quoted: bool,
}

impl Query {
    /// What the path is looked up as: a quoted path's escapes undone
    /// (`decode_into`, `file_picker_path.zig:135-151`), and `None` for one
    /// that cannot be.
    pub(crate) fn lookup(&self) -> Option<String> {
        if !self.quoted {
            return Some(self.raw.clone());
        }
        let mut out = String::with_capacity(self.raw.len());
        let mut chars = self.raw.chars();
        while let Some(scalar) = chars.next() {
            if scalar == '\\' {
                out.push(chars.next()?);
            } else {
                out.push(scalar);
            }
        }
        is_terminal_safe(&out).then_some(out)
    }
}

/// The `@` word `caret` is in, in `text` (`query_at`, `file_picker_path.zig:168-188`).
pub(crate) fn query_at(text: &str, caret: usize) -> Option<Query> {
    let bytes = text.as_bytes();
    if caret > bytes.len() {
        return None;
    }
    let mut offset = 0;
    while offset < bytes.len() {
        let Some(token) = parse_at(bytes, offset) else {
            offset += 1;
            continue;
        };
        offset = token.end.max(offset + 1);
        if caret < token.path_start {
            return None;
        }
        if caret > token.path_end {
            continue;
        }
        if token.status == Status::Invalid {
            return None;
        }
        let prefix = text.get(token.path_start..caret)?;
        if !is_terminal_safe(prefix) {
            return None;
        }
        if !token.quoted && prefix.bytes().any(is_separator) {
            return None;
        }
        return Some(Query {
            raw: prefix.to_string(),
            at: token.start,
            replace_end: token.quote_end.unwrap_or(caret),
            quoted: token.quoted,
        });
    }
    None
}

/// Whether a path has to be quoted to survive as one word
/// (`needs_quotes`, `file_picker_path.zig:4-11`).
fn needs_quotes(path: &str) -> bool {
    path.ends_with('.')
        || path.bytes().any(|byte| {
            !(byte >= 0x80
                || byte.is_ascii_alphanumeric()
                || matches!(byte, b'_' | b'-' | b'.' | b'/' | b'~'))
        })
}

/// The `@` text for `path` (`encode`, `file_picker_path.zig:206-233`): quoted
/// when asked or when it has to be, a directory with its `/` and its quote
/// left open so the browse goes on, and a workspace path that begins with `~`
/// made `./~...` so it is not read as the home directory.
pub(crate) fn encode(
    path: &str,
    quoted: bool,
    directory: bool,
    workspace_relative: bool,
) -> String {
    let quote = quoted || needs_quotes(path);
    let mut out = String::with_capacity(path.len() + 6);
    out.push('@');
    if quote {
        out.push('"');
    }
    if workspace_relative && path.starts_with('~') {
        out.push_str("./");
    }
    for scalar in path.chars() {
        if quote && (scalar == '\\' || scalar == '"') {
            out.push('\\');
        }
        out.push(scalar);
    }
    if directory {
        out.push('/');
    }
    if quote && !directory {
        out.push('"');
    }
    out
}

/// What accepting a row does to the draft.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Completion {
    /// The range of the draft replaced: from the `@` to [`Query::replace_end`].
    pub(crate) range: Range<usize>,
    pub(crate) text: String,
    /// Where the caret goes, as an offset into the draft after the edit.
    pub(crate) caret: usize,
    /// The offset of a space the completion added, so one typed straight after
    /// it can be taken as that space rather than a second one
    /// (`markFileCompletionSeparator`, `input_completion_runtime.zig:940-946`).
    pub(crate) separator: Option<usize>,
}

/// What choosing `found` for `query` does to `draft`
/// (`applyFilePickerSelection`, `input_completion_runtime.zig:898-953`).
///
/// A file gets a space after it unless the draft already has a separator
/// there, in which case the caret steps over that one; a directory gets
/// neither, so the next keystroke goes on with the path.
pub(crate) fn completion(draft: &str, query: &Query, found: &Found, indexed: bool) -> Completion {
    let directory = found.kind == Kind::Directory;
    let reuses = draft
        .as_bytes()
        .get(query.replace_end)
        .is_some_and(|byte| is_separator(*byte));
    let adds = !directory && !reuses;
    let mut text = encode(&found.path, query.quoted, directory, indexed);
    let separator = adds.then(|| query.at + text.len());
    if adds {
        text.push(' ');
    }
    let caret = query.at + text.len() + usize::from(!directory && reuses);
    Completion {
        range: query.at..query.replace_end,
        text,
        caret,
        separator,
    }
}

// ---------------------------------------------------------------------------
// the list
// ---------------------------------------------------------------------------

/// What the list has to show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Shown {
    /// Nothing yet: the index or the directory is still being read.
    Loading,
    /// Rows.
    Rows,
    /// A lookup that found nothing.
    Empty,
    /// Nothing could be read.
    Unavailable,
}

/// What Enter means to an open list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Enter {
    /// Put this row in the draft, and send nothing.
    Accept(Found),
    /// Nothing to choose from and nothing coming: the line goes as typed.
    Submit,
    /// Rows may still arrive, so nothing is sent and nothing is chosen.
    Wait,
}

/// The list for one `@` word.
#[derive(Debug)]
pub(crate) struct FilePicker {
    query: Query,
    /// The decoded query, or `None` for a quoted path that does not decode.
    lookup: Option<String>,
    rows: Vec<Found>,
    shown: Shown,
    selected: usize,
    /// The first row in view, kept as the last move left it
    /// (`list_window.zig:55-68@481104fd`). A frame paints the rows that keep
    /// the mark in view from here and does not write them back
    /// (`paint_plan.zig:936-937`; `acknowledge` keeps the start it was given,
    /// `file_completion_state.zig:293`), so a screen made shorter shows the
    /// mark and the next move scrolls from where the last one left this.
    window_start: usize,
}

impl FilePicker {
    /// A list for `query`, with nothing in it yet.
    pub(crate) fn open(query: Query) -> Self {
        let lookup = query.lookup();
        Self {
            query,
            shown: if lookup.is_some() {
                Shown::Loading
            } else {
                Shown::Unavailable
            },
            lookup,
            rows: Vec::new(),
            selected: 0,
            window_start: 0,
        }
    }

    /// The word this list is for.
    pub(crate) fn query(&self) -> &Query {
        &self.query
    }

    /// What it is looked up as, when it can be.
    pub(crate) fn lookup(&self) -> Option<&str> {
        self.lookup.as_deref()
    }

    /// Whether the query searches the workspace index rather than browsing a
    /// path the user typed.
    pub(crate) fn indexed(&self) -> bool {
        self.lookup
            .as_deref()
            .is_some_and(|lookup| !super::file_index::is_explicit(lookup))
    }

    /// What the list shows.
    pub(crate) fn shown(&self) -> Shown {
        self.shown
    }

    /// Takes a lookup's answer, and says whether the list changed.
    ///
    /// The marked row stays on the **same path** when the new rows still have
    /// it -- a refresh that landed while the user was choosing does not move
    /// the choice -- and goes back to the top otherwise.
    pub(crate) fn settle(&mut self, lookup: Lookup) -> bool {
        let (rows, shown) = match lookup {
            Lookup::Rows(rows) if rows.is_empty() => (rows, Shown::Empty),
            Lookup::Rows(rows) => (rows, Shown::Rows),
            Lookup::Loading => (Vec::new(), Shown::Loading),
            Lookup::Unavailable => (Vec::new(), Shown::Unavailable),
        };
        if rows == self.rows && shown == self.shown {
            return false;
        }
        let marked = self
            .rows
            .get(self.selected)
            .map(|found| (found.path.clone(), found.kind));
        self.selected = marked
            .and_then(|(path, kind)| {
                rows.iter()
                    .position(|found| found.path == path && found.kind == kind)
            })
            .unwrap_or(0);
        if self.selected == 0 {
            self.window_start = 0;
        }
        // The window stays where the last move left it: the frame paints the
        // rows that keep the mark in view, and the next move scrolls from
        // here (`acknowledge` keeps the start, `file_completion_state.zig:293`).
        self.rows = rows;
        self.shown = shown;
        true
    }

    /// Moves the mark `delta` rows (`advanceSelectionWithin`,
    /// `list_window.zig:44-53@481104fd`): past either end it goes to the other
    /// end, however far it was asked to go, and the window scrolls only as far
    /// as keeps the mark in view of `painted` rows -- the rows the last frame
    /// that landed painted the list in (`navigate`,
    /// `file_completion_state.zig:239-249`).
    pub(crate) fn step(&mut self, delta: i64, painted: u16) -> bool {
        let count = self.rows.len();
        if count == 0 {
            return false;
        }
        let count_i = i64::try_from(count).unwrap_or(i64::MAX);
        let current = i64::try_from(self.selected % count).unwrap_or(0);
        let next = current.saturating_add(delta);
        let next = if next < 0 {
            count_i - 1
        } else if next >= count_i {
            0
        } else {
            next
        };
        self.selected = usize::try_from(next).unwrap_or(0);
        self.window_start = edge_start(self.window_start, count, self.selected, painted);
        true
    }

    /// The marked row, when there is one.
    pub(crate) fn marked(&self) -> Option<&Found> {
        self.rows.get(self.selected)
    }

    /// What Enter does to it (`submitFilePickerOnEnter`,
    /// `input_completion_runtime.zig:955-973`).
    pub(crate) fn enter(&self) -> Enter {
        match (self.marked(), self.shown) {
            (Some(found), _) => Enter::Accept(found.clone()),
            (None, Shown::Empty) => Enter::Submit,
            (None, _) => Enter::Wait,
        }
    }

    /// The rows the band gives the list on a screen `terminal_rows` tall with a
    /// composer `input_extra` rows past its first and a waiting prompt's
    /// `banner_rows` (its rows and the blank under them) above it
    /// (`filePickerReservedRows`, `picker_presentation.zig:658-666@481104fd`).
    ///
    /// A list picker's height (`activeListPickerReservedRows`, `:647-652`):
    /// what the screen has past the band's fixed rows, at most six and never
    /// none -- grown to ten while five transcript rows stay visible.
    pub(crate) fn height(terminal_rows: u16, input_extra: u16, banner_rows: u16) -> u16 {
        let fixed = FIXED_ROWS
            .saturating_add(input_extra)
            .saturating_add(banner_rows);
        let available = terminal_rows.saturating_sub(fixed);
        let list = if available == 0 {
            1
        } else {
            available.min(MOST_ROWS)
        };
        let grown = MOST_FILE_ROWS.min(available.saturating_sub(TRANSCRIPT_ROWS));
        list.max(grown)
    }

    /// The list's rows, top first, exactly `height` of them, starting at the
    /// one-based column `start_col` on a screen `cols` wide.
    pub(crate) fn rows(
        &self,
        start_col: u16,
        cols: u16,
        height: u16,
        palette: Palette,
    ) -> Vec<String> {
        let start_col = start_col.clamp(1, cols.max(1));
        let indent = " ".repeat(usize::from(start_col - 1));
        let width = usize::from(cols.saturating_sub(start_col - 1));
        let mut rows = Vec::with_capacity(usize::from(height));
        if self.rows.is_empty() {
            let label = match self.shown {
                Shown::Loading => INDEXING,
                Shown::Unavailable if self.indexed() => FILES_UNAVAILABLE,
                Shown::Unavailable => DIRECTORY_UNAVAILABLE,
                Shown::Rows | Shown::Empty => NO_MATCHES,
            };
            rows.push(format!(
                "{indent}{}{}{}",
                palette.dim(),
                super::frame::clip(label, u16::try_from(width).unwrap_or(u16::MAX)),
                palette.reset()
            ));
        } else {
            let start = edge_start(self.window_start, self.rows.len(), self.selected, height);
            let end = (start + usize::from(height)).min(self.rows.len());
            let selected = palette.selected();
            for (index, found) in self.rows.iter().enumerate().take(end).skip(start) {
                let base = if index == self.selected {
                    selected.as_str()
                } else {
                    palette.dim()
                };
                let mut row = format!("{indent}{base}");
                label(&mut row, found, width, base);
                row.push_str(palette.reset());
                rows.push(row);
            }
        }
        rows.resize(usize::from(height), String::new());
        rows
    }
}

/// `found`'s path onto `row` in `width` cells (`appendFilePickerLabel`,
/// `picker_presentation.zig:920-957`).
///
/// Whole when it fits, a directory with its `/`. Otherwise the directories
/// give way first -- cut in the middle to a third of the width, at least three
/// cells and at most twelve -- and the name keeps the rest, cut toward its end;
/// a row narrower than eight cells shows the name alone.
fn label(row: &mut String, found: &Found, width: usize, base: &str) {
    let path = found.path.as_str();
    let slash = usize::from(found.kind == Kind::Directory);
    if cells(path) + slash <= width {
        styled(row, found, 0..path.len(), base);
        if slash == 1 {
            row.push('/');
        }
        return;
    }
    let base_start = path.rfind('/').map_or(0, |at| at + 1);
    let Some(dir_end) = path.rfind('/') else {
        basename_only(row, found, base_start, width, base);
        return;
    };
    if width < 8 {
        basename_only(row, found, base_start, width, base);
        return;
    }
    let directory = &path[..dir_end];
    let dir_budget = cells(directory).min((width / 3).clamp(3, 12));
    let name_budget = width.saturating_sub(dir_budget + 1 + slash);
    ellipsized(row, found, 0..dir_end, dir_budget, Placement::Middle, base);
    styled(row, found, dir_end..base_start, base);
    ellipsized(
        row,
        found,
        base_start..path.len(),
        name_budget,
        Placement::Prefix,
        base,
    );
    if slash == 1 {
        row.push('/');
    }
}

/// The name alone, in `width` cells (`appendBasenameProjection`, `:961-978`).
fn basename_only(row: &mut String, found: &Found, base_start: usize, width: usize, base: &str) {
    let end = found.path.len();
    if found.kind == Kind::Directory {
        if width == 0 {
            return;
        }
        if width > 1 {
            ellipsized(
                row,
                found,
                base_start..end,
                width - 1,
                Placement::Prefix,
                base,
            );
        }
        row.push('/');
        return;
    }
    ellipsized(row, found, base_start..end, width, Placement::Prefix, base);
}

/// Where a cut path keeps its cells.
#[derive(Debug, Clone, Copy)]
enum Placement {
    /// As much of the start as of the end.
    Middle,
    /// Three quarters of the start and a quarter of the end.
    Prefix,
}

/// `range` of the path in `budget` cells, its middle swapped for an ellipsis
/// when it does not fit (`appendStyledEllipsizedRange`, `:980-1011`).
fn ellipsized(
    row: &mut String,
    found: &Found,
    range: Range<usize>,
    budget: usize,
    placement: Placement,
    base: &str,
) {
    if budget == 0 || range.start >= range.end {
        return;
    }
    let source = &found.path[range.clone()];
    if cells(source) <= budget {
        styled(row, found, range, base);
        return;
    }
    if budget == 1 {
        row.push_str(ELLIPSIS);
        return;
    }
    let content = budget - 1;
    let (head, tail) = match placement {
        Placement::Middle => (content.div_ceil(2), content / 2),
        Placement::Prefix => (content - content / 4, content / 4),
    };
    let head_len = prefix_len(source, head);
    let tail_len = suffix_len(source, tail);
    styled(row, found, range.start..range.start + head_len, base);
    row.push_str(ELLIPSIS);
    styled(row, found, range.end - tail_len..range.end, base);
}

/// The bytes of the longest run of whole clusters at the start of `text` that
/// is at most `budget` cells.
fn prefix_len(text: &str, budget: usize) -> usize {
    use unicode_segmentation::UnicodeSegmentation;
    let mut used = 0;
    let mut len = 0;
    for cluster in text.graphemes(true) {
        let width = cells(cluster);
        if used + width > budget {
            break;
        }
        used += width;
        len += cluster.len();
    }
    len
}

/// The same from the end, never starting on a zero-width cluster
/// (`displaySafeSuffixByWidth`, `:1013-1025`).
fn suffix_len(text: &str, budget: usize) -> usize {
    use unicode_segmentation::UnicodeSegmentation;
    let mut used = 0;
    let mut len = 0;
    for cluster in text.graphemes(true).rev() {
        let width = cells(cluster);
        if used + width > budget {
            break;
        }
        used += width;
        len += cluster.len();
    }
    let suffix = &text[text.len() - len..];
    let leading_zero_width: usize = suffix
        .graphemes(true)
        .take_while(|cluster| cells(cluster) == 0)
        .map(str::len)
        .sum();
    if len == text.len() {
        return len;
    }
    len - leading_zero_width
}

/// `range` of the path, its matched letters bold and the row's own style put
/// back after each (`appendStyledPathRange`, `:1027-1050`).
fn styled(row: &mut String, found: &Found, range: Range<usize>, base: &str) {
    let path = found.path.as_str();
    let mut cursor = range.start;
    for span in &found.spans {
        if span.end <= range.start {
            continue;
        }
        if span.start >= range.end {
            break;
        }
        let start = span.start.max(range.start);
        let end = span.end.min(range.end);
        if cursor < start {
            row.push_str(&path[cursor..start]);
        }
        row.push_str("\u{1b}[1m");
        row.push_str(&path[start..end]);
        row.push_str("\u{1b}[0m");
        row.push_str(base);
        cursor = end;
    }
    if cursor < range.end {
        row.push_str(&path[cursor..range.end]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::tui::theme::Mode;

    const DARK: Palette = Palette { mode: Mode::Dark };

    fn query(text: &str) -> Option<Query> {
        query_at(text, text.len())
    }

    /// A row's text with its colours taken out.
    fn plain(row: &str) -> String {
        let mut out = String::new();
        let mut rest = row;
        while !rest.is_empty() {
            if let Some(len) = crate::tui::pacer::escape_at(rest) {
                rest = &rest[len..];
                continue;
            }
            let mut characters = rest.chars();
            out.extend(characters.next());
            rest = characters.as_str();
        }
        out
    }

    fn file(path: &str) -> Found {
        Found {
            path: path.to_string(),
            kind: Kind::File,
            spans: Vec::new(),
        }
    }

    fn directory(path: &str) -> Found {
        Found {
            path: path.to_string(),
            kind: Kind::Directory,
            spans: Vec::new(),
        }
    }

    #[test]
    fn an_at_that_starts_a_word_opens_the_picker() {
        let found = query("@").expect("a query");
        assert_eq!(
            (found.raw.as_str(), found.at, found.replace_end),
            ("", 0, 1)
        );
        let found = query("see @src/").expect("a query");
        assert_eq!((found.raw.as_str(), found.at), ("src/", 4));
        // `tests/e2e/tui-file-picker.test.ts:1168`: after a quote and after a
        // parenthesis.
        assert_eq!(
            query("Compare \"@src/main").expect("a query").raw,
            "src/main"
        );
        assert_eq!(query("Review (@src/main").expect("a query").raw, "src/main");
        // Not inside a word: an address is not a mention.
        assert!(query("me@example.com").is_none());
        assert!(query("x@").is_none());
    }

    #[test]
    fn the_caret_has_to_be_in_the_path() {
        let text = "a @foo b";
        assert_eq!(query_at(text, 3).expect("at the start of the path").raw, "");
        assert_eq!(query_at(text, 5).expect("in it").raw, "fo");
        assert_eq!(query_at(text, 6).expect("at its end").raw, "foo");
        assert!(query_at(text, 7).is_none(), "past the separator");
        assert!(query_at(text, 2).is_none(), "on the @ itself");
        // A later mention, after an earlier one.
        let text = "@one @two";
        assert_eq!(query_at(text, text.len()).expect("the second").raw, "two");
        assert_eq!(query_at(text, text.len()).expect("the second").at, 5);
    }

    #[test]
    fn a_quoted_path_holds_spaces_and_decodes_its_escapes() {
        let found = query("@\"my dir/a\\\"b").expect("a quoted query");
        assert!(found.quoted);
        assert_eq!(found.raw, "my dir/a\\\"b");
        assert_eq!(found.lookup().as_deref(), Some("my dir/a\"b"));
        // A closed quote: what a choice replaces runs to the closing quote.
        let text = "read @\"a b\" now";
        let inside = query_at(text, 8).expect("inside the quotes");
        assert_eq!(inside.replace_end, 11);
        // A closed quote followed by something that is not punctuation is not
        // a path at all (`file_picker_path.zig` test, :262).
        assert!(query_at("@\"photo.png\"other.png", 3).is_none());
    }

    #[test]
    fn a_bare_path_ends_at_a_separator_and_an_escaped_one_is_no_query() {
        // A backslash keeps the space in the word (`parse_at`), but a bare
        // completion replaces only what is before the caret, and a space in
        // that is not a path anybody typed (`query_at`,
        // `file_picker_path.zig:178-181`).
        let text = "@a\\ b";
        assert!(query_at(text, text.len()).is_none());
        assert_eq!(query_at(text, 2).expect("before the escape").raw, "a");
        assert!(query_at("@a b", 4).is_none());
    }

    #[test]
    fn a_path_is_encoded_as_one_word() {
        // `file_picker_path.zig` tests, :236-305.
        assert_eq!(encode("src/main.zig", false, false, true), "@src/main.zig");
        assert_eq!(encode("docs", false, true, true), "@docs/");
        assert_eq!(encode("./dir name", false, true, false), "@\"./dir name/");
        assert_eq!(encode("a b.txt", false, false, true), "@\"a b.txt\"");
        assert_eq!(encode("a\"b", false, false, true), "@\"a\\\"b\"");
        assert_eq!(encode("trailing.", false, false, true), "@\"trailing.\"");
        assert_eq!(encode("~notes", false, false, true), "@./~notes");
        assert_eq!(encode("~/notes", false, false, false), "@~/notes");
        assert_eq!(
            encode("caf\u{e9}/\u{6587}.txt", false, false, true),
            "@caf\u{e9}/\u{6587}.txt"
        );
    }

    #[test]
    fn choosing_a_file_adds_one_separator_and_a_directory_none() {
        // `fx-composer-40x120-at-enter.txt`: `@gui` + Enter.
        let draft = "@gui";
        let chosen = completion(
            draft,
            &query(draft).expect("a query"),
            &file("docs/guide.md"),
            true,
        );
        assert_eq!(chosen.range, 0..4);
        assert_eq!(chosen.text, "@docs/guide.md ");
        assert_eq!(chosen.caret, 15);
        assert_eq!(chosen.separator, Some(14));
        // A directory keeps the browse going.
        let chosen = completion(
            "see @sr",
            &query("see @sr").expect("a query"),
            &directory("src"),
            true,
        );
        assert_eq!(
            (chosen.text.as_str(), chosen.caret, chosen.separator),
            ("@src/", 9, None)
        );
        // `tests/e2e/tui-file-picker.test.ts:2038`: in the middle of a line the
        // separator already there is used, and the caret steps over it.
        let draft = "Read @first- suffix";
        let caret = "Read @first-".len();
        let found = query_at(draft, caret).expect("a query");
        let chosen = completion(draft, &found, &file("first-file.txt"), true);
        assert_eq!(chosen.text, "@first-file.txt");
        assert_eq!(chosen.caret, "Read @first-file.txt ".len());
        assert_eq!(chosen.separator, None);
    }

    fn picker(rows: &[Found]) -> FilePicker {
        let mut picker = FilePicker::open(query("@").expect("a query"));
        picker.settle(Lookup::Rows(rows.to_vec()));
        picker
    }

    fn ws2() -> Vec<Found> {
        vec![
            file("Cargo.toml"),
            file("README.md"),
            directory("docs"),
            file("docs/guide.md"),
            directory("src"),
            directory("src/deep"),
            file("src/deep/sched.rs"),
            file("src/lib.rs"),
            file("src/main.rs"),
        ]
    }

    #[test]
    fn the_list_is_six_rows_at_the_at_column_with_the_mark_a_grey_bar() {
        let rows = picker(&ws2()).rows(3, 120, 6, DARK);
        // `fx-composer-40x120-at.ansi`, rows 5-10, in upstream's wire order
        // (background, then foreground; tmux re-serializes them the other way).
        assert_eq!(
            rows,
            [
                "  \u{1b}[48;5;239m\u{1b}[38;5;255mCargo.toml\u{1b}[0m",
                "  \u{1b}[38;5;245mREADME.md\u{1b}[0m",
                "  \u{1b}[38;5;245mdocs/\u{1b}[0m",
                "  \u{1b}[38;5;245mdocs/guide.md\u{1b}[0m",
                "  \u{1b}[38;5;245msrc/\u{1b}[0m",
                "  \u{1b}[38;5;245msrc/deep/\u{1b}[0m",
            ]
        );
    }

    #[test]
    fn the_matched_letters_are_bold_inside_the_rows_own_style() {
        let mut found = file("src/deep/sched.rs");
        found.spans = std::iter::once(9..12).collect();
        let rows = picker(&[found]).rows(3, 120, 6, DARK);
        // `fx-composer-40x120-at-sch.ansi`, row 5, and five blank rows under it.
        assert_eq!(
            rows[0],
            "  \u{1b}[48;5;239m\u{1b}[38;5;255msrc/deep/\u{1b}[1msch\u{1b}[0m\u{1b}[48;5;239m\u{1b}[38;5;255med.rs\u{1b}[0m"
        );
        assert!(rows[1..].iter().all(String::is_empty));
        assert_eq!(rows.len(), 6);
    }

    #[test]
    fn a_move_wraps_and_the_window_follows_the_mark_to_its_edge() {
        let mut list = picker(&ws2());
        list.step(1, MOST_ROWS);
        assert_eq!(
            list.marked().map(|found| found.path.as_str()),
            Some("README.md")
        );
        list.step(-2, MOST_ROWS);
        assert_eq!(
            list.marked().map(|found| found.path.as_str()),
            Some("src/main.rs")
        );
        let rows = list.rows(3, 120, 6, DARK);
        // The window scrolled to keep the last row in view: the mark is on the
        // bottom row, three rows of it below the start of the list.
        assert!(
            rows[5].contains("\u{1b}[48;5;239m\u{1b}[38;5;255msrc/main.rs"),
            "{rows:?}"
        );
        assert!(rows[0].contains("docs/guide.md\u{1b}[0m"), "{rows:?}");
        // A page past the end goes to the other end, as one row past it does.
        list.step(6, MOST_ROWS);
        assert_eq!(
            list.marked().map(|found| found.path.as_str()),
            Some("Cargo.toml")
        );
    }

    #[test]
    fn a_refresh_keeps_the_mark_on_the_same_path() {
        let mut list = picker(&ws2());
        list.step(3, MOST_ROWS);
        assert_eq!(
            list.marked().map(|found| found.path.as_str()),
            Some("docs/guide.md")
        );
        let mut grown = ws2();
        grown.insert(0, file("AGENTS.md"));
        assert!(list.settle(Lookup::Rows(grown.clone())));
        assert_eq!(
            list.marked().map(|found| found.path.as_str()),
            Some("docs/guide.md")
        );
        assert!(
            !list.settle(Lookup::Rows(grown)),
            "the same rows are no change"
        );
    }

    #[test]
    fn an_empty_list_says_why_at_the_at_column() {
        let mut list = FilePicker::open(query("see @zz").expect("a query"));
        assert_eq!(
            list.rows(7, 120, 6, DARK)[0],
            "      \u{1b}[38;5;245mindexing files...\u{1b}[0m"
        );
        assert_eq!(
            list.enter(),
            Enter::Wait,
            "Enter waits for rows that are coming"
        );
        list.settle(Lookup::Rows(Vec::new()));
        assert_eq!(
            list.rows(7, 120, 6, DARK)[0],
            "      \u{1b}[38;5;245mno matching files\u{1b}[0m"
        );
        // `tests/e2e/tui-file-picker.test.ts:1142`: nothing matched, so Enter
        // sends the line as typed.
        assert_eq!(list.enter(), Enter::Submit);
        list.settle(Lookup::Unavailable);
        assert_eq!(
            list.rows(7, 120, 6, DARK)[0],
            "      \u{1b}[38;5;245mFiles unavailable. tab to retry; esc to dismiss.\u{1b}[0m"
        );
        let mut browsed = FilePicker::open(query("@nope/").expect("a query"));
        browsed.settle(Lookup::Unavailable);
        assert_eq!(
            browsed.rows(3, 40, 1, DARK),
            ["  \u{1b}[38;5;245mDirectory unavailable. tab to retry; e\u{1b}[0m"]
        );
    }

    #[test]
    fn a_long_path_gives_up_its_directories_first() {
        let long =
            file("deeply-nested-source-directory/alpha-component-01-with-a-very-long-name.zig");
        let rows = picker(&[long]).rows(3, 40, 1, DARK);
        // Width 38: the directory gets twelve cells, cut in the middle; the
        // name keeps twenty-five, cut toward its end.
        let text = plain(&rows[0]);
        assert_eq!(
            text,
            "  deeply\u{2026}ctory/alpha-component-01\u{2026}me.zig"
        );
        assert_eq!(cells(&text), 40);
        // Narrower than eight cells: the name alone.
        let rows = picker(&[file("a/bcdefghijkl")]).rows(1, 7, 1, DARK);
        assert_eq!(plain(&rows[0]), "bcdef\u{2026}l");
        let rows = picker(&[directory("a/bcdefghijkl")]).rows(1, 7, 1, DARK);
        assert_eq!(plain(&rows[0]), "bcde\u{2026}l/");
    }

    #[test]
    fn the_list_grows_to_ten_rows_while_five_transcript_rows_stay_visible() {
        // fx's own table (`file picker grows to ten rows while five transcript
        // rows stay visible`, `picker_presentation.zig:1588-1597@481104fd`).
        assert_eq!(FilePicker::height(40, 0, 0), 10);
        assert_eq!(FilePicker::height(24, 0, 0), 10);
        assert_eq!(FilePicker::height(18, 0, 0), 8);
        // Short terminals keep the list picker's height.
        assert_eq!(FilePicker::height(16, 0, 0), 6);
        assert_eq!(FilePicker::height(12, 0, 0), 6);
        assert_eq!(FilePicker::height(6, 0, 0), 1);
        assert_eq!(FilePicker::height(20, 2, 4), 6);
    }

    #[test]
    fn below_six_rows_the_list_takes_what_the_screen_has_and_never_none() {
        // `activeListPickerReservedRows`, `picker_presentation.zig:647-652`:
        // the floor the grown height never goes under.
        assert_eq!(FilePicker::height(9, 0, 0), 4);
        assert_eq!(FilePicker::height(9, 2, 0), 2);
        assert_eq!(FilePicker::height(11, 0, 2), 4);
        assert_eq!(FilePicker::height(5, 0, 0), 1, "never none");
        assert_eq!(FilePicker::height(0, 0, 0), 1, "never none");
        // Every row the composer or a waiting prompt takes is one the list
        // gives up, once it has grown.
        assert_eq!(FilePicker::height(24, 3, 0), 10);
        assert_eq!(FilePicker::height(24, 4, 0), 10);
        assert_eq!(FilePicker::height(24, 5, 0), 9);
        assert_eq!(FilePicker::height(24, 0, 5), 9);
        assert_eq!(FilePicker::height(u16::MAX, u16::MAX, u16::MAX), 1);
    }

    /// Thirty files, `./00.txt` to `./29.txt`, as fx's own case has them.
    fn thirty() -> FilePicker {
        let rows: Vec<Found> = (0..30).map(|n| file(&format!("./{n:02}.txt"))).collect();
        picker(&rows)
    }

    fn mark_and_window(list: &FilePicker) -> (usize, usize) {
        (list.selected, list.window_start)
    }

    #[test]
    fn the_arrows_scroll_within_the_rows_the_last_frame_painted() {
        // fx's `file picker navigation scrolls within the rows the last frame
        // painted` (`file_completion_state.zig:319-350@481104fd`).
        let mut list = thirty();
        for _ in 0..7 {
            list.step(1, 6);
        }
        assert_eq!(list.window_start, 2);
        // Moving back up keeps the window until the selection reaches its top.
        for _ in 0..5 {
            list.step(-1, 6);
        }
        assert_eq!(list.window_start, 2);
        list.step(-1, 6);
        assert_eq!(list.window_start, 1);
        // A frame painted ten.
        for _ in 0..8 {
            list.step(1, 10);
        }
        assert_eq!(mark_and_window(&list), (9, 1));
    }

    #[test]
    fn the_window_scrolls_only_past_the_rows_painted() {
        // `selection scrolls only past the picker's own row count`
        // (`list_window.zig:70-85@481104fd`).
        let mut list = thirty();
        for _ in 0..9 {
            list.step(1, 10);
        }
        assert_eq!(mark_and_window(&list), (9, 0));
        list.step(1, 10);
        assert_eq!(mark_and_window(&list), (10, 1));
        // Other pickers keep six rows.
        let mut list = thirty();
        for _ in 0..6 {
            list.step(1, MOST_ROWS);
        }
        assert_eq!(list.window_start, 1);
    }

    #[test]
    fn a_frame_paints_the_rows_that_keep_the_mark_in_view_and_keeps_the_window() {
        // `paint_plan.zig:936-937`: the painted window is re-fitted from the
        // window the last move left; the window itself is not written back
        // (`acknowledge`, `file_completion_state.zig:293`). Captures
        // `p5-input-at-resize`, 40x120 then 16x120.
        let mut list = thirty();
        for _ in 0..12 {
            list.step(1, 10);
        }
        assert_eq!(mark_and_window(&list), (12, 3));
        let rows = list.rows(1, 40, 10, DARK);
        assert_eq!(plain(&rows[0]), "./03.txt");
        assert_eq!(plain(&rows[9]), "./12.txt");
        assert!(rows[9].contains("\u{1b}[48;5;239m"), "{rows:?}");
        // Six rows now: the frame shows 07-12, and the window stays at 3.
        let rows = list.rows(1, 40, 6, DARK);
        assert_eq!(rows.len(), 6);
        assert_eq!(plain(&rows[0]), "./07.txt");
        assert_eq!(plain(&rows[5]), "./12.txt");
        assert_eq!(list.window_start, 3);
        // So one Up scrolls six rows from 3: the window moves to 6.
        list.step(-1, 6);
        assert_eq!(mark_and_window(&list), (11, 6));
        let rows = list.rows(1, 40, 6, DARK);
        assert_eq!(plain(&rows[0]), "./06.txt");
        assert_eq!(plain(&rows[5]), "./11.txt");
    }

    #[test]
    fn a_refresh_leaves_the_window_where_the_last_move_left_it() {
        // `acknowledge` writes back the window the frame was given
        // (`file_completion_state.zig:293`): rows that land while the user is
        // choosing re-fit what is painted, not where the next move starts.
        let mut list = thirty();
        for _ in 0..12 {
            list.step(1, 10);
        }
        let mut fewer: Vec<Found> = (0..30).map(|n| file(&format!("./{n:02}.txt"))).collect();
        fewer.drain(0..10);
        assert!(list.settle(Lookup::Rows(fewer)));
        assert_eq!(
            list.marked().map(|found| found.path.as_str()),
            Some("./12.txt")
        );
        assert_eq!(mark_and_window(&list), (2, 3));
        let rows = list.rows(1, 40, 10, DARK);
        assert_eq!(plain(&rows[0]), "./12.txt");
        list.step(1, 10);
        assert_eq!(mark_and_window(&list), (3, 3));
    }
}
