//! The freeform answer a question's `Other` slot opens: the text, its caret,
//! and the rows it is drawn on.
//!
//! Upstream's field, ported rather than approximated
//! (`vercel-labs/fx@c1db919`: `src/ui/footer/question_freeform_layout.zig`,
//! the freeform half of `src/core/agent/question_prompt.zig`, and
//! `writeSelectedFreeformOptionLines`, `src/ui/footer/question_ui.zig:323-399`).
//! Three things set it apart from the composer's editor, and each is why this
//! is not `super::editor::Editor`:
//!
//! * **It wraps by cell, not by word** (`prefixByWidth`,
//!   `question_freeform_layout.zig:60-70`): a row is as many whole graphemes as
//!   fit, and the next one starts mid-word. The editor wraps at word
//!   boundaries, which is a different set of rows.
//! * **The plain arrows walk its rows** and keep the column they were aiming
//!   for (`moveCursor`, `:117-156`), falling back to the next choice only at
//!   the first or last row -- so the caret is set to an offset the editor has
//!   no way to be told.
//! * **Its caret is a painted cell**: one grapheme drawn in reverse video, or
//!   a reversed space past the end of a row, and a row of its own when the last
//!   row is full or the text ends in a line break (`question_ui.zig:377-383`).
//!
//! Home and End are the start and the end of the whole answer, as upstream's
//! are (`question_prompt.zig:455-466`), not of the row.

use unicode_segmentation::UnicodeSegmentation;

use super::wrap::width;

/// One row of the field: where its text starts and ends in the answer, and
/// what follows it (`Line`, `question_freeform_layout.zig:9-15`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Line {
    start: usize,
    content_end: usize,
    next_start: usize,
    /// The row ended at a line break in the text rather than at the margin.
    hard_break: bool,
    /// The empty row past the end of the text, which exists only for a caret
    /// to stand on.
    synthetic: bool,
}

/// Which way a vertical move goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Direction {
    Up,
    Down,
}

/// One row of the field as it is drawn, split around the caret's cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FieldRow<'a> {
    /// The text in front of the caret's cell -- or the whole row, when the
    /// caret is not on it.
    pub before: &'a str,
    /// The caret's cell: `Some("")` is a reversed space past the text, `Some`
    /// of a grapheme is that grapheme reversed, and `None` is a row the caret
    /// is not on.
    pub caret: Option<&'a str>,
    /// The text after the caret's cell.
    pub after: &'a str,
}

/// The answer being typed, and where in it the caret is.
#[derive(Debug, Clone, Default)]
pub(crate) struct Freeform {
    text: String,
    /// A byte offset into [`Self::text`], always on a grapheme boundary.
    cursor: usize,
    /// The column a run of vertical moves is aiming for, so a walk through a
    /// short row comes back out at the column it went in at
    /// (`freeform_preferred_column`). Forgotten by every other key.
    preferred: Option<u16>,
}

impl Freeform {
    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// `typed` at the caret, and the caret after it
    /// (`insertFreeformSlice`, `question_prompt.zig:573-583`).
    pub(crate) fn insert(&mut self, typed: &str) {
        self.text.insert_str(self.cursor, typed);
        self.cursor += typed.len();
        self.preferred = None;
    }

    /// The grapheme before the caret, gone (`freeformBackspace`). `false` at
    /// the start, where there is nothing to take.
    pub(crate) fn backspace(&mut self) -> bool {
        self.preferred = None;
        if self.cursor == 0 {
            return false;
        }
        let start = previous_start(&self.text, self.cursor);
        self.text.replace_range(start..self.cursor, "");
        self.cursor = start;
        true
    }

    /// The grapheme after the caret, gone (`freeformDeleteNext`,
    /// `question_prompt.zig:514-521`). `false` at the end.
    pub(crate) fn delete(&mut self) -> bool {
        self.preferred = None;
        if self.cursor >= self.text.len() {
            return false;
        }
        let end = next_end(&self.text, self.cursor);
        self.text.replace_range(self.cursor..end, "");
        true
    }

    /// Back to the start of the word before the caret (`freeformCursorWordLeft`,
    /// `previousWordStart`, `text_boundaries.zig:34-47`).
    pub(crate) fn word_left(&mut self) {
        self.preferred = None;
        self.cursor = previous_word_start(&self.text, self.cursor);
    }

    /// On to the end of the word after it (`nextWordEnd`, `:49-58`).
    pub(crate) fn word_right(&mut self) {
        self.preferred = None;
        self.cursor = next_word_end(&self.text, self.cursor);
    }

    /// The whitespace-delimited token before the caret, on its own line, gone
    /// -- `C-w` (`freeformDeleteWhitespaceDelimitedWordLeft`,
    /// `question_prompt.zig:531-542`).
    pub(crate) fn delete_token_left(&mut self) -> bool {
        self.preferred = None;
        let line_start = self.text[..self.cursor].rfind('\n').map_or(0, |at| at + 1);
        let mut start = self.cursor;
        while start > line_start {
            let previous = previous_start(&self.text, start);
            if !is_space(&self.text[previous..start]) {
                break;
            }
            start = previous;
        }
        while start > line_start {
            let previous = previous_start(&self.text, start);
            if is_space(&self.text[previous..start]) {
                break;
            }
            start = previous;
        }
        self.remove(start, self.cursor)
    }

    /// From the caret to the start of its line, gone -- `C-u`
    /// (`freeformDeleteToLineStart`, `question_prompt.zig:552-558`).
    pub(crate) fn kill_to_start(&mut self) -> bool {
        self.preferred = None;
        let start = self.text[..self.cursor].rfind('\n').map_or(0, |at| at + 1);
        self.remove(start, self.cursor)
    }

    /// From the caret to the end of its line, gone, and the line break with it
    /// when the caret is already there -- `C-k` (`freeformDeleteToLineEnd`,
    /// `question_prompt.zig:560-571`).
    pub(crate) fn kill_to_end(&mut self) -> bool {
        self.preferred = None;
        let line_end = self.text[self.cursor..]
            .find('\n')
            .map_or(self.text.len(), |at| self.cursor + at);
        let end = if self.cursor == line_end && line_end < self.text.len() {
            line_end + 1
        } else {
            line_end
        };
        self.remove(self.cursor, end)
    }

    /// `start..end` gone and the caret where it was; `false` when empty.
    fn remove(&mut self, start: usize, end: usize) -> bool {
        if start >= end {
            return false;
        }
        self.text.replace_range(start..end, "");
        self.cursor = start;
        true
    }

    pub(crate) fn left(&mut self) {
        self.preferred = None;
        self.cursor = previous_start(&self.text, self.cursor);
    }

    pub(crate) fn right(&mut self) {
        self.preferred = None;
        self.cursor = next_end(&self.text, self.cursor);
    }

    pub(crate) fn home(&mut self) {
        self.preferred = None;
        self.cursor = 0;
    }

    pub(crate) fn end(&mut self) {
        self.preferred = None;
        self.cursor = self.text.len();
    }

    /// Empties the field and leaves the caret at its start
    /// (`clearFreeformDraft`, `question_prompt.zig:428-435`).
    pub(crate) fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
        self.preferred = None;
    }

    /// One row up or down in a field `content` cells wide, keeping the column
    /// a run of these was aiming for. `false` when there is no row that way --
    /// the first row going up, the last going down -- which is where upstream
    /// lets the key move the choice instead (`input_question_runtime.zig:155-170`).
    pub(crate) fn move_vertical(&mut self, content: u16, direction: Direction) -> bool {
        let content = usize::from(content.max(1));
        let (cursor, preferred, moved) =
            move_cursor(&self.text, self.cursor, content, direction, self.preferred);
        self.cursor = cursor;
        self.preferred = Some(preferred);
        moved
    }

    /// The field's rows in a field `content` cells wide, top first, split
    /// around the caret's cell (`writeSelectedFreeformOptionLines`).
    pub(crate) fn rows(&self, content: u16) -> Vec<FieldRow<'_>> {
        let text = self.text.as_str();
        let content = usize::from(content.max(1));
        if text.is_empty() {
            return vec![FieldRow {
                before: "",
                caret: Some(""),
                after: "",
            }];
        }
        let cursor = self.cursor.min(text.len());
        let mut rows = Vec::new();
        let mut start = 0;
        let mut last_width = 0;
        let mut trailing_hard_break = false;
        while start < text.len() {
            let line = next_line(text, start, content);
            let end = line.content_end;
            last_width = usize::from(width(&text[start..end]));
            trailing_hard_break = line.hard_break && line.next_start == text.len();
            if cursor >= start && cursor < end {
                let unit_end = next_end(text, cursor).min(end);
                rows.push(FieldRow {
                    before: &text[start..cursor],
                    caret: Some(&text[cursor..unit_end]),
                    after: &text[unit_end..end],
                });
            } else {
                let at_break = cursor == end && line.hard_break;
                let at_tail = line.next_start == text.len()
                    && !line.hard_break
                    && cursor == text.len()
                    && last_width < content;
                rows.push(FieldRow {
                    before: &text[start..end],
                    caret: (at_break || at_tail).then_some(""),
                    after: "",
                });
            }
            start = line.next_start;
        }
        if trailing_hard_break || (cursor == text.len() && last_width == content) {
            rows.push(FieldRow {
                before: "",
                caret: Some(""),
                after: "",
            });
        }
        rows
    }

    /// Which of [`Self::rows`] the caret is on, and the cells to its left
    /// there.
    pub(crate) fn caret(&self, content: u16) -> (usize, u16) {
        self.rows(content)
            .iter()
            .enumerate()
            .find(|(_, row)| row.caret.is_some())
            .map_or((0, 0), |(index, row)| (index, width(row.before)))
    }
}

/// The start of the grapheme that ends at `cursor`.
fn previous_start(text: &str, cursor: usize) -> usize {
    text[..cursor]
        .grapheme_indices(true)
        .next_back()
        .map_or(0, |(start, _)| start)
}

/// The end of the grapheme that starts at `cursor`.
fn next_end(text: &str, cursor: usize) -> usize {
    text[cursor..]
        .graphemes(true)
        .next()
        .map_or(cursor, |unit| cursor + unit.len())
}

/// Whether a grapheme is part of a word (`isWordCharacter`,
/// `text_boundaries.zig:71-77`): ASCII letters, digits and `_`, and every code
/// point from U+00C0 up.
fn is_word(unit: &str) -> bool {
    unit.chars().next().is_some_and(|character| {
        character == '_' || character.is_ascii_alphanumeric() || u32::from(character) >= 0xc0
    })
}

/// Whether a grapheme is white space, for `C-w`'s token.
fn is_space(unit: &str) -> bool {
    unit.chars().next().is_some_and(char::is_whitespace)
}

/// `previousWordStart`: back over what is not a word, then over the word.
fn previous_word_start(text: &str, cursor: usize) -> usize {
    let mut at = cursor;
    while at > 0 {
        let previous = previous_start(text, at);
        if is_word(&text[previous..at]) {
            break;
        }
        at = previous;
    }
    while at > 0 {
        let previous = previous_start(text, at);
        if !is_word(&text[previous..at]) {
            break;
        }
        at = previous;
    }
    at
}

/// `nextWordEnd`: on over what is not a word, then over the word.
fn next_word_end(text: &str, cursor: usize) -> usize {
    let mut at = cursor;
    while at < text.len() && !is_word(&text[at..next_end(text, at)]) {
        at = next_end(text, at);
    }
    while at < text.len() && is_word(&text[at..next_end(text, at)]) {
        at = next_end(text, at);
    }
    at
}

/// The cells of the grapheme that starts at `cursor`, and how long it is.
fn unit_at(text: &str, cursor: usize) -> (usize, usize) {
    match text[cursor..].graphemes(true).next() {
        Some(unit) => (unit.len(), usize::from(width(unit))),
        None => (0, 0),
    }
}

/// As much of `segment` as fits in `cells`, in whole graphemes
/// (`prefixByWidth`, `display_width.zig:161-173`).
fn prefix_by_width(segment: &str, cells: usize) -> &str {
    let mut used = 0usize;
    let mut end = 0usize;
    for unit in segment.graphemes(true) {
        let wide = usize::from(width(unit));
        if wide > cells.saturating_sub(used) {
            break;
        }
        used += wide;
        end += unit.len();
    }
    &segment[..end]
}

/// The row that starts at `start` (`nextLine`, `question_freeform_layout.zig:46-90`).
fn next_line(text: &str, start: usize, content: usize) -> Line {
    if start >= text.len() {
        return Line {
            start: text.len(),
            content_end: text.len(),
            next_start: text.len(),
            hard_break: false,
            synthetic: true,
        };
    }
    let segment_end = text[start..]
        .find('\n')
        .map_or(text.len(), |offset| start + offset);
    let segment = &text[start..segment_end];
    let prefix = prefix_by_width(segment, content);
    if prefix.len() < segment.len() {
        // A row cut at the margin: the next one starts at the cut. A grapheme
        // wider than the whole row is a row of its own rather than nothing.
        let end = if prefix.is_empty() {
            next_end(text, start).min(segment_end)
        } else {
            start + prefix.len()
        };
        return Line {
            start,
            content_end: end,
            next_start: end,
            hard_break: false,
            synthetic: false,
        };
    }
    let hard_break = segment_end < text.len();
    Line {
        start,
        content_end: segment_end,
        next_start: if hard_break {
            segment_end + 1
        } else {
            segment_end
        },
        hard_break,
        synthetic: false,
    }
}

/// The row after `line`, including the empty one past a full last row or a
/// final line break, or `None` at the end (`advanceLine`, `:158-173`).
fn advance_line(text: &str, line: Line, content: usize) -> Option<Line> {
    if line.synthetic {
        return None;
    }
    if line.next_start < text.len() {
        return Some(next_line(text, line.next_start, content));
    }
    if line.hard_break && line.next_start == text.len() {
        return Some(next_line(text, text.len(), content));
    }
    if !line.hard_break
        && line.content_end == text.len()
        && usize::from(width(&text[line.start..line.content_end])) == content
    {
        return Some(next_line(text, text.len(), content));
    }
    None
}

/// Whether the caret at `cursor` stands on `line` (`lineOwnsCursor`,
/// `:175-186`): a caret at a soft margin belongs to the row below it.
fn line_owns_cursor(text: &str, line: Line, cursor: usize, content: usize) -> bool {
    if line.synthetic {
        return cursor == line.start;
    }
    if cursor < line.start || cursor > line.content_end {
        return false;
    }
    if cursor < line.content_end || line.hard_break {
        return true;
    }
    advance_line(text, line, content).is_none()
}

/// The cells to the left of `cursor` on `line` (`cursorColumn`).
fn cursor_column(text: &str, line: Line, cursor: usize) -> usize {
    if line.synthetic {
        return 0;
    }
    usize::from(width(&text[line.start..cursor]))
}

/// The offset on `line` closest to `target` cells in without passing it
/// (`cursorAtColumn`, `:194-215`).
fn cursor_at_column(text: &str, line: Line, target: usize, content: usize) -> usize {
    if line.synthetic || target == 0 {
        return line.start;
    }
    let mut cursor = line.start;
    let mut column = 0usize;
    let mut last_owned = line.start;
    while cursor < line.content_end {
        let (len, cells) = unit_at(text, cursor);
        if len == 0 || column + cells > target {
            break;
        }
        cursor += len;
        column += cells;
        if line_owns_cursor(text, line, cursor, content) {
            last_owned = cursor;
        }
        if column >= target {
            break;
        }
    }
    last_owned
}

/// One row up or down from `cursor` (`moveCursor`, `:117-156`): the new
/// caret, the column it was aiming for, and whether it moved.
fn move_cursor(
    text: &str,
    cursor: usize,
    content: usize,
    direction: Direction,
    preferred: Option<u16>,
) -> (usize, u16, bool) {
    let cursor = cursor.min(text.len());
    let mut previous: Option<Line> = None;
    let mut line = next_line(text, 0, content);
    loop {
        if line_owns_cursor(text, line, cursor, content) {
            let column = preferred.map_or_else(|| cursor_column(text, line, cursor), usize::from);
            let target = match direction {
                Direction::Up => previous,
                Direction::Down => advance_line(text, line, content),
            };
            let kept = u16::try_from(column).unwrap_or(u16::MAX);
            return match target {
                Some(target) => (cursor_at_column(text, target, column, content), kept, true),
                None => (cursor, kept, false),
            };
        }
        let Some(next) = advance_line(text, line, content) else {
            break;
        };
        previous = Some(line);
        line = next;
    }
    (cursor, preferred.unwrap_or(0), false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(text: &str, cursor: usize) -> Freeform {
        Freeform {
            text: text.to_string(),
            cursor,
            preferred: None,
        }
    }

    fn texts(field: &Freeform, content: u16) -> Vec<String> {
        field
            .rows(content)
            .iter()
            .map(|row| {
                format!(
                    "{}{}{}",
                    row.before,
                    row.caret.map_or(String::new(), |unit| format!(
                        "[{}]",
                        if unit.is_empty() { " " } else { unit }
                    )),
                    row.after
                )
            })
            .collect()
    }

    #[test]
    fn an_empty_field_is_one_row_with_the_caret_on_it() {
        let empty = Freeform::default();
        assert_eq!(texts(&empty, 10), ["[ ]"]);
        assert_eq!(empty.caret(10), (0, 0));
    }

    #[test]
    fn the_field_wraps_by_cell_and_not_by_word() {
        // `prefixByWidth`: a row is as many whole graphemes as fit, and a word
        // that crosses the margin is cut there.
        let typed = field("abc defgh", 9);
        assert_eq!(texts(&typed, 5), ["abc d", "efgh[ ]"]);
        assert_eq!(typed.caret(5), (1, 4));
    }

    #[test]
    fn a_full_last_row_puts_the_caret_on_a_row_of_its_own() {
        // `question_ui.zig:377-383`: no room for the reversed space on the
        // full row, so it gets one.
        let full = field("abcde", 5);
        assert_eq!(texts(&full, 5), ["abcde", "[ ]"]);
        assert_eq!(full.caret(5), (1, 0));
        // And a final line break is a row as well, the caret on it.
        let broke = field("ab\n", 3);
        assert_eq!(texts(&broke, 5), ["ab", "[ ]"]);
    }

    #[test]
    fn the_caret_reverses_the_grapheme_it_stands_on() {
        let inside = field("ab네d", 2);
        assert_eq!(texts(&inside, 20), ["ab[네]d"]);
        assert_eq!(inside.caret(20), (0, 2));
        // At a line break, a reversed space after the row's text.
        let at_break = field("ab\ncd", 2);
        assert_eq!(texts(&at_break, 20), ["ab[ ]", "cd"]);
    }

    #[test]
    fn a_wide_grapheme_is_never_split_across_rows() {
        let wide = field("네네네", 9);
        assert_eq!(texts(&wide, 5), ["네네", "네[ ]"]);
        // A grapheme wider than the whole row is a row of its own.
        let narrow = field("네a", 4);
        assert_eq!(texts(&narrow, 1), ["네", "a", "[ ]"]);
    }

    #[test]
    fn vertical_moves_keep_their_column_across_short_rows() {
        // `question_freeform_layout.zig` "vertical movement preserves preferred
        // column across hard lines": from the end of `ghij` up twice and down
        // once.
        let mut walked = field("abcd\nef\nghij", 11);
        assert!(walked.move_vertical(20, Direction::Up));
        assert_eq!(walked.cursor, 7);
        assert!(walked.move_vertical(20, Direction::Up));
        assert_eq!(walked.cursor, 3);
        assert!(walked.move_vertical(20, Direction::Down));
        assert_eq!(walked.cursor, 7);
    }

    #[test]
    fn vertical_moves_follow_soft_wrapped_rows_and_stop_at_the_edges() {
        // "vertical movement follows soft wrapped rows".
        let mut wrapped = field("abcdefghijkl", 10);
        assert!(wrapped.move_vertical(4, Direction::Up));
        assert_eq!(wrapped.cursor, 6);
        assert!(wrapped.move_vertical(4, Direction::Up));
        assert_eq!(wrapped.cursor, 2);
        // "vertical movement is inert beyond the first and last row".
        assert!(!wrapped.move_vertical(4, Direction::Up));
        assert_eq!(wrapped.cursor, 2);
        let mut last = field("one\ntwo", 6);
        assert!(!last.move_vertical(20, Direction::Down));
        assert_eq!(last.cursor, 6);
        // A single row has nowhere to go either way, which is what lets the
        // arrows choose.
        let mut single = field("one", 3);
        assert!(!single.move_vertical(20, Direction::Up));
        assert!(!single.move_vertical(20, Direction::Down));
    }

    #[test]
    fn vertical_moves_land_on_grapheme_boundaries() {
        // "vertical movement stays within utf8 display-unit boundaries" and
        // "preserves a combining sequence boundary".
        let mut emoji = field("a🙂b\nxy", "a🙂b\nxy".len());
        emoji.move_vertical(20, Direction::Up);
        assert_eq!(emoji.cursor, 1);
        let mut combining = field("e\u{301}x\nab", "e\u{301}x\nab".len());
        combining.move_vertical(20, Direction::Up);
        assert_eq!(combining.cursor, 4);
    }

    #[test]
    fn word_moves_and_the_kill_keys_are_upstreams() {
        // `text_boundaries.zig` and `question_prompt.zig:469-571`.
        let mut typed = field("use the_db, please\nnext line", 18);
        typed.word_left();
        assert_eq!(typed.cursor, 12, "back to `please`");
        typed.word_left();
        assert_eq!(typed.cursor, 4, "back over `, ` to `the_db`");
        typed.word_right();
        assert_eq!(typed.cursor, 10, "on to the end of `the_db`");
        // C-w: the whitespace-delimited token before the caret.
        let mut token = field("use the_db, please", 11);
        assert!(token.delete_token_left());
        assert_eq!(token.text(), "use  please");
        assert_eq!(token.cursor, 4);
        // C-u and C-k stop at the line's own ends; C-k at a line's end takes
        // the break.
        let mut lines = field("one two\nthree", 4);
        assert!(lines.kill_to_start());
        assert_eq!(lines.text(), "two\nthree");
        assert!(lines.kill_to_end());
        assert_eq!(lines.text(), "\nthree");
        assert!(lines.kill_to_end());
        assert_eq!(lines.text(), "three");
        assert!(!field("", 0).kill_to_end());
    }

    #[test]
    fn editing_moves_by_grapheme_and_home_and_end_are_the_whole_answer() {
        let mut typed = Freeform::default();
        typed.insert("ab\ncd");
        typed.left();
        typed.backspace();
        assert_eq!(typed.text(), "ab\nd");
        typed.home();
        typed.right();
        typed.insert("네");
        assert_eq!(typed.text(), "a네b\nd");
        typed.backspace();
        assert_eq!(typed.text(), "ab\nd");
        typed.end();
        assert_eq!(typed.caret(20), (1, 1));
        assert!(!field("", 0).backspace());
        typed.clear();
        assert!(typed.is_empty());
        assert_eq!(typed.caret(20), (0, 0));
    }
}
