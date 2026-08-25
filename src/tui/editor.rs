//! The composer: one flat buffer, a byte cursor, and a preferred column.
//!
//! Upstream's editor is a `String` plus an offset into it (`editor_state.zig:30-33`) --
//! no line array, no rope -- and this is the same shape for the same reason:
//! every question the band asks of the composer is a question about *text*, and
//! a structure that stored lines would answer "which row is the caret on" from
//! its own bookkeeping rather than from the wrap the screen actually shows.
//!
//! So there is one buffer, and three rules that turn it into something a person
//! can edit:
//!
//! * **The cursor is a byte offset that is always a grapheme boundary.** Motion
//!   is by cluster ([`unicode_segmentation`]), never by byte and never by
//!   `char`, because a combining accent and a ZWJ family are one thing to the
//!   person typing and to the terminal drawing them
//!   (`text_boundaries.zig` tests `:184-199`). It is also an obligation this
//!   module owes [`wrap::cursor_point`], which **panics** on an offset that is
//!   not even a `char` boundary and reports a column inside a glyph for one
//!   that splits a cluster: the caret is only honest if the offset is.
//! * **Lines and rows are different things, and both are needed.** A *line* is
//!   what the newlines in the text delimit, and it is what `Home`, `End` and
//!   the two kills work on -- `C-a` on a wrapped paragraph goes to the start of
//!   the paragraph, which is what every line editor a terminal user has ever
//!   used does. A *row* is what [`wrap::wrap`] produces for the width the band
//!   has, and it is what `Up` and `Down` work on, because those are movements
//!   on the screen and the screen is rows.
//! * **Vertical motion remembers the column it wanted.** Walking down through a
//!   short row and out the other side lands where the caret started rather than
//!   where the short row ended (`vertical_navigation.zig:32-56`); any
//!   horizontal motion, and any edit, forgets it.
//!
//! # What the composer does not accept
//!
//! Nothing here filters the text, because nothing that reaches it is unfiltered:
//! [`super::input::Decoder`] never emits a control scalar as
//! [`super::input::Input::Text`], and the bytes of a bracketed paste arrive as
//! `PasteByte` and reach this module only through [`super::paste`], whose
//! filter keeps CR, LF, Tab and the printing bytes and drops everything else --
//! CR normalized to LF on the way, so a pasted line break is the same character
//! `C-j` inserts. The control characters a composer can therefore hold are line
//! breaks and tabs, and both are text rather than bytes the terminal would
//! obey. That is what makes submitting a composer into the transcript safe: an
//! `ESC [ 2 J` cannot be in it to be written back. A pasted **tab** is the one
//! character the composer holds and does not show: the wrap measures a control
//! at no cells and the painter drops it (`super::frame::row_text`), so pasted
//! indentation is sent whole and drawn as nothing. Rendering it is Phase 2's,
//! with the rest of the block model.
//!
//! # The two caps
//!
//! [`MAX_COMPOSER_BYTES`] is upstream's (`paste_framing.zig:16-35`) and it
//! **refuses** rather than truncates: half of a pasted line is not what anyone
//! meant to send, and an insert that cannot happen whole does not happen. The
//! other cap is on rows rather than bytes and belongs to the band --
//! [`super::layout::input_row_limit`] -- so a composer taller than that scrolls
//! inside the rows it has ([`window`]) instead of eating the transcript.

use unicode_segmentation::{GraphemeCursor, UnicodeSegmentation};

use super::input::Action;
use super::wrap::{self, Row};

/// The most text a composer may hold (`paste_framing.zig:16-35`).
pub(crate) const MAX_COMPOSER_BYTES: usize = 8 * 1024 * 1024;

/// The text being composed, and where the caret is in it.
pub(crate) struct Editor {
    text: String,
    /// A byte offset into [`text`](Self::text), always on a grapheme boundary.
    cursor: usize,
    /// The column vertical motion is aiming for, in cells, while a run of it
    /// lasts. `None` the moment anything else moves the caret.
    sticky: Option<u16>,
}

impl Editor {
    pub(crate) fn new() -> Self {
        Self {
            text: String::new(),
            cursor: 0,
            sticky: None,
        }
    }

    /// The text as it stands.
    // Task 11's worker is its first production reader: what is submitted is
    // sent, and `take` is what a submit uses. Until then this is how the tests
    // below read the buffer they are editing.
    #[allow(dead_code)]
    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// The text in front of the caret -- everything an insertion would land
    /// after.
    ///
    /// Handed out as text rather than as the cursor offset on purpose: the
    /// offset is this module's invariant (always a grapheme boundary,
    /// `wrap::cursor_point` panics on one that is not), and a caller given the
    /// number would be a caller doing arithmetic on it.
    pub(crate) fn before_caret(&self) -> &str {
        &self.text[..self.cursor]
    }

    /// The text behind the caret -- everything an insertion would land before.
    ///
    /// The other half of [`Self::before_caret`], and handed out for the same
    /// reason: together they are what a caller needs to ask a question about
    /// the draft an edit *would* produce, without being given the offset to do
    /// arithmetic on.
    pub(crate) fn after_caret(&self) -> &str {
        &self.text[self.cursor..]
    }

    /// Inserts `text` at the caret, or refuses it whole.
    ///
    /// `false` is the byte budget: the composer keeps exactly the text it had,
    /// which is the difference between a paste that did not fit and a paste
    /// that half fit.
    pub(crate) fn insert(&mut self, text: &str) -> bool {
        if self.text.len().saturating_add(text.len()) > MAX_COMPOSER_BYTES {
            return false;
        }
        self.text.insert_str(self.cursor, text);
        // **Not** simply `cursor + text.len()`. An insertion can *merge* what
        // was on either side of it into one cluster -- a ZWJ typed between two
        // emoji is the whole of that case -- and the offset the insertion ended
        // at is then inside a cluster the terminal draws as one glyph. Every
        // caller below, and `wrap::cursor_point` above them, is owed a
        // boundary, so the caret is snapped to the first one at or after where
        // the text went in: after the merged glyph rather than into it.
        self.cursor = boundary_at_or_after(&self.text, self.cursor + text.len());
        self.sticky = None;
        true
    }

    /// Applies one editing action, on a composer `cols` cells wide.
    ///
    /// `cols` is the width the *text* has, which is not the width of the
    /// screen: the band renders the composer in a gutter and passes what is
    /// left. It matters only to the two vertical moves, because they are the
    /// only actions whose answer depends on where the text wraps.
    pub(crate) fn apply(&mut self, action: Action, cols: u16) {
        match action {
            Action::Left => self.move_to(before(&self.text, self.cursor)),
            Action::Right => self.move_to(after(&self.text, self.cursor)),
            Action::WordLeft => self.move_to(self.word_left()),
            Action::WordRight => self.move_to(self.word_right()),
            Action::Home => self.move_to(self.line_start()),
            Action::End => self.move_to(self.line_end()),
            Action::Up => self.move_by_row(Step::Up, cols),
            Action::Down => self.move_by_row(Step::Down, cols),
            Action::Backspace => self.delete(before(&self.text, self.cursor), self.cursor),
            Action::Delete => self.delete(self.cursor, after(&self.text, self.cursor)),
            Action::DeleteWordLeft => self.delete(self.word_left(), self.cursor),
            Action::KillToEnd => self.delete(self.cursor, self.line_end()),
            Action::KillToStart => self.delete(self.line_start(), self.cursor),
            Action::InsertNewline => {
                self.insert("\n");
            }
            // Not the composer's: submitting, leaving and cancelling are the
            // session's (`super::shell`), a paste is Task 18's, `Tab` belongs
            // to the approval panel (`super::approval`) and the composer has no
            // completion for it to drive, and an `Ignore` is a keystroke this
            // session has no binding for. They are named rather than caught by
            // a wildcard so that an action added later has to be routed on
            // purpose.
            Action::Submit
            | Action::Escape
            | Action::Cancel
            | Action::Eof
            | Action::Redraw
            | Action::Tab
            | Action::PasteStart
            | Action::PasteEnd
            | Action::Ignore => {}
        }
    }

    /// Takes the composer's text, leaving it empty.
    pub(crate) fn take(&mut self) -> String {
        self.cursor = 0;
        self.sticky = None;
        std::mem::take(&mut self.text)
    }

    /// The composer's rows, top first, for a text width of `cols`.
    ///
    /// A row's own line break is not in it: it is a break in the text rather
    /// than a character on the screen, and a row handed to the painter with a
    /// newline in it would move the cursor off the row it was placed on
    /// (`super::frame::row_text` strips it, and a row that arrives already
    /// stripped is one the two modules cannot disagree about).
    pub(crate) fn rows(&self, cols: u16) -> Vec<String> {
        wrap::wrap(&self.text, cols.max(1))
            .into_iter()
            .map(|row| body(&self.text[row.start..row.end]).to_string())
            .collect()
    }

    /// Where the caret is: the row of [`rows`](Self::rows) it is on, and the
    /// cells to the left of it there.
    ///
    /// The row is a `usize` and is **never narrowed here**, for the reason
    /// `super::transcript`'s row counts are not: how many rows a composer has is
    /// a property of the text and of the width, not of the screen, and 65536
    /// newlines is 64 KiB of an 8 MiB budget. A row saturated at `u16::MAX`
    /// would send [`window`] chasing row 65535 while the caret was somewhere
    /// else entirely -- the window would stop following the cursor, which is the
    /// one thing it exists to do. The narrowing belongs where a *terminal
    /// coordinate* is made, clamped to the rows the band really has
    /// (`super::shell::Shell::cursor`).
    pub(crate) fn point(&self, cols: u16) -> (usize, u16) {
        let rows = wrap::wrap(&self.text, cols.max(1));
        wrap::cursor_point(&self.text, &rows, self.cursor)
    }

    /// Moves the caret, and forgets the column vertical motion was aiming for.
    fn move_to(&mut self, offset: usize) {
        self.cursor = offset;
        self.sticky = None;
    }

    /// Removes `start..end`, and puts the caret where the text used to be.
    fn delete(&mut self, start: usize, end: usize) {
        if start >= end {
            return;
        }
        self.text.replace_range(start..end, "");
        self.cursor = start;
        self.sticky = None;
    }

    /// One row up or down, at the column this run of vertical motion wants.
    ///
    /// The preferred column is recorded even when the caret cannot move: a
    /// `Up` on the first row is not a movement, but it is part of the run, and
    /// forgetting the column there would make the next `Down` aim at wherever
    /// the caret happened to be.
    fn move_by_row(&mut self, step: Step, cols: u16) {
        let rows = wrap::wrap(&self.text, cols.max(1));
        let (row, column) = wrap::cursor_point(&self.text, &rows, self.cursor);
        let target = self.sticky.unwrap_or(column);
        self.sticky = Some(target);
        let wanted = match step {
            Step::Up => row.checked_sub(1),
            Step::Down => row.checked_add(1),
        };
        let Some(wanted) = wanted.filter(|wanted| *wanted < rows.len()) else {
            return;
        };
        let last = wanted + 1 == rows.len();
        self.cursor = self.offset_on(&rows[wanted], last, target);
    }

    /// The offset on `row` at `target` cells from its left edge, or as close to
    /// it as the row reaches.
    fn offset_on(&self, row: &Row, last: bool, target: u16) -> usize {
        let text = &self.text[row.start..row.end];
        let body = body(text);
        // The far end of the row, which is not always the end of its bytes. A
        // row a *soft wrap* ended has no column past its last cluster: the
        // offset the next cluster begins at is the following row's first
        // column (`wrap::cursor_point`), so a caret placed there would be a row
        // below the one that was asked for.
        let limit = if body.len() == text.len() && !last {
            row.end - body.graphemes(true).next_back().map_or(0, str::len)
        } else {
            row.start + body.len()
        };
        let mut used = 0u16;
        for (index, cluster) in body.grapheme_indices(true) {
            if used >= target {
                return (row.start + index).min(limit);
            }
            used = used.saturating_add(wrap::width(cluster));
        }
        limit
    }

    /// The start of the line the caret is on: the byte after the break above
    /// it, or the start of the text.
    fn line_start(&self) -> usize {
        self.text[..self.cursor]
            .rfind('\n')
            .map_or(0, |break_at| break_at + 1)
    }

    /// The end of that line: the break below it, or the end of the text. The
    /// break itself is not part of the line, so `C-k` on a full line leaves the
    /// line empty rather than joining it to the next one.
    fn line_end(&self) -> usize {
        self.text[self.cursor..]
            .find('\n')
            .map_or(self.text.len(), |break_at| self.cursor + break_at)
    }

    /// The start of the word to the left: the whitespace before the caret is
    /// stepped over, and then the run of non-whitespace before that
    /// (`whitespace_word_left`, `composer_kill_ring.zig:14-18`).
    fn word_left(&self) -> usize {
        let mut at = self.cursor;
        while at > 0 && is_space(&self.text[before(&self.text, at)..at]) {
            at = before(&self.text, at);
        }
        while at > 0 && !is_space(&self.text[before(&self.text, at)..at]) {
            at = before(&self.text, at);
        }
        at
    }

    /// The mirror of [`word_left`](Self::word_left): over the word the caret is
    /// in, and then over the whitespace after it.
    fn word_right(&self) -> usize {
        let end = self.text.len();
        let mut at = self.cursor;
        while at < end && !is_space(&self.text[at..after(&self.text, at)]) {
            at = after(&self.text, at);
        }
        while at < end && is_space(&self.text[at..after(&self.text, at)]) {
            at = after(&self.text, at);
        }
        at
    }
}

/// Which way [`Editor::move_by_row`] is going.
#[derive(Debug, Clone, Copy)]
enum Step {
    Up,
    Down,
}

/// The rows of a `rows`-row composer that a band `limit` rows tall shows, given
/// that the caret is on `point_row` (`visual_layout.zig:699`
/// `visibleWindow(cursor_row, total, limit)`).
///
/// The window is derived from the caret rather than remembered, which is the
/// whole of why it is a function: a remembered window is a second piece of
/// state that can disagree with the text, and the one thing the composer must
/// never do is hide the row the user is typing on. A composer that fits shows
/// all of itself; one that does not is scrolled just far enough that the
/// caret's row is the last one visible.
pub(crate) fn window(rows: usize, point_row: usize, limit: u16) -> std::ops::Range<usize> {
    // A band with no composer row is not one `layout::solve` produces, and a
    // zero-length window would show nothing at all.
    let limit = usize::from(limit.max(1));
    if rows <= limit {
        return 0..rows;
    }
    // `rows - limit` is the furthest the window can start and still be full,
    // and it cannot underflow: `rows > limit` here.
    let start = point_row.saturating_sub(limit - 1).min(rows - limit);
    start..start + limit
}

/// The grapheme boundary at `at`, or the first one after it.
///
/// `unicode_segmentation`'s cursor rather than a walk over a neighbourhood of
/// the text, because a boundary is not a local fact: a run of regional-
/// indicator scalars decides its boundaries by its own parity, so a window
/// chosen for speed is a window that can answer wrongly. The chunk handed over
/// is the **whole** string, starting at 0, so the cursor is never asked for
/// context it does not have -- which is why the error arms are unreachable, and
/// why their answer is the end of the text, always a boundary, rather than the
/// offset that was asked about.
fn boundary_at_or_after(text: &str, at: usize) -> usize {
    if GraphemeCursor::new(at, text.len(), true).is_boundary(text, 0) == Ok(true) {
        return at;
    }
    GraphemeCursor::new(at, text.len(), true)
        .next_boundary(text, 0)
        .ok()
        .flatten()
        .unwrap_or(text.len())
}

/// One row's text without the line break that ended it.
fn body(row: &str) -> &str {
    row.strip_suffix('\n')
        .map_or(row, |body| body.strip_suffix('\r').unwrap_or(body))
}

/// Whether a grapheme cluster is whitespace, for the word moves.
fn is_space(cluster: &str) -> bool {
    cluster.chars().all(char::is_whitespace)
}

/// The grapheme boundary before `at`, or `at` when it is the start of the text.
fn before(text: &str, at: usize) -> usize {
    text[..at]
        .graphemes(true)
        .next_back()
        .map_or(at, |cluster| at - cluster.len())
}

/// The grapheme boundary after `at`, or `at` when it is the end of the text.
fn after(text: &str, at: usize) -> usize {
    text[at..]
        .graphemes(true)
        .next()
        .map_or(at, |cluster| at + cluster.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor(text: &str) -> Editor {
        let mut editor = Editor::new();
        assert!(editor.insert(text));
        editor
    }

    #[test]
    fn typing_and_backspace_work_on_the_flat_buffer() {
        let mut editor = editor("hello");
        editor.apply(Action::Backspace, 80);
        assert_eq!(editor.text(), "hell");
        editor.apply(Action::Home, 80);
        editor.apply(Action::Delete, 80);
        assert_eq!(editor.text(), "ell");
    }

    #[test]
    fn a_zwj_family_moves_and_deletes_as_one_unit() {
        // text_boundaries.zig tests :184-199
        let family = "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}";
        let mut editor = editor(&format!("a{family}b"));
        editor.apply(Action::Left, 80); // past 'b'
        editor.apply(Action::Left, 80); // past the whole family
        assert_eq!(editor.point(80).1, 1, "the family was entered mid-cluster");
        editor.apply(Action::Delete, 80);
        assert_eq!(editor.text(), "ab", "a partial cluster survived the delete");
    }

    #[test]
    fn a_combining_accent_stays_with_its_base() {
        let mut editor = editor("e\u{301}x");
        editor.apply(Action::Home, 80);
        editor.apply(Action::Right, 80);
        editor.apply(Action::Backspace, 80);
        assert_eq!(editor.text(), "x");
    }

    #[test]
    fn word_moves_stop_at_word_boundaries() {
        let mut editor = editor("alpha bravo");
        editor.apply(Action::WordLeft, 80);
        assert_eq!(editor.point(80).1, 6);
        editor.apply(Action::WordLeft, 80);
        assert_eq!(editor.point(80).1, 0);
    }

    #[test]
    fn vertical_motion_keeps_a_sticky_preferred_column() {
        // vertical_navigation.zig:32-56
        let mut editor = editor("alphabet\nxy\nzulu-long");
        editor.apply(Action::Home, 80); // start of the last row
        editor.apply(Action::Right, 80);
        editor.apply(Action::Right, 80);
        editor.apply(Action::Right, 80); // column 3
        editor.apply(Action::Up, 80); // "xy" is shorter: clamp to 2
        assert_eq!(editor.point(80), (1, 2));
        editor.apply(Action::Up, 80); // and the preferred column returns
        assert_eq!(editor.point(80), (0, 3));
    }

    #[test]
    fn the_byte_budget_refuses_rather_than_truncates() {
        let mut editor = Editor::new();
        assert!(editor.insert(&"a".repeat(MAX_COMPOSER_BYTES)));
        assert!(!editor.insert("b"), "the budget was exceeded silently");
        assert_eq!(editor.text().len(), MAX_COMPOSER_BYTES);
    }

    #[test]
    fn the_visible_window_follows_the_cursor_within_the_growth_cap() {
        // input_presentation.zig:201-220: the composer takes at most
        // content_bottom/2 + 1 rows, and scrolls inside that window.
        assert_eq!(window(3, 0, 11), 0..3);
        assert_eq!(window(20, 0, 11), 0..11);
        assert_eq!(window(20, 15, 11), 5..16);
        assert_eq!(window(20, 19, 11), 9..20);
    }

    // -----------------------------------------------------------------------
    // beyond the plan's rows
    // -----------------------------------------------------------------------

    #[test]
    fn a_horizontal_move_forgets_the_column_a_vertical_run_was_aiming_for() {
        // The other half of the sticky column, and the one an implementation
        // that simply never cleared it would pass the test above without.
        let mut editor = editor("alphabet\nxy\nzulu-long");
        editor.apply(Action::Home, 80);
        editor.apply(Action::Right, 80);
        editor.apply(Action::Right, 80);
        editor.apply(Action::Right, 80);
        editor.apply(Action::Up, 80);
        assert_eq!(editor.point(80), (1, 2), "the short row clamped the column");
        editor.apply(Action::Left, 80);
        editor.apply(Action::Up, 80);
        assert_eq!(
            editor.point(80),
            (0, 1),
            "a horizontal move left the run's preferred column standing"
        );
    }

    #[test]
    fn vertical_motion_is_by_row_rather_than_by_line() {
        // The composer is soft-wrapped, so `Up` from the middle of a wrapped
        // paragraph is one *row* up and not the top of the paragraph.
        let mut editor = editor("alpha bravo delta");
        // One line, three rows: the words wrap whole and the spaces hang.
        assert_eq!(editor.rows(6), vec!["alpha ", "bravo ", "delta"]);
        assert_eq!(editor.point(6), (2, 5));
        editor.apply(Action::Up, 6);
        assert_eq!(editor.point(6), (1, 5), "the row above, not the line above");
        editor.apply(Action::Up, 6);
        assert_eq!(editor.point(6), (0, 5));
    }

    #[test]
    fn a_caret_never_lands_on_the_far_side_of_a_soft_wrap() {
        // The offset a soft-wrapped row ends at belongs to the row *below* it
        // (`wrap::cursor_point`), so a vertical move that clamped to it would
        // land on the row the user was moving away from.
        let mut editor = editor("abcdefghijkl");
        assert_eq!(editor.rows(4), vec!["abcd", "efgh", "ijkl"]);
        assert_eq!(editor.point(4), (2, 4), "the caret is past the last row");
        editor.apply(Action::Up, 4);
        assert_eq!(
            editor.point(4),
            (1, 3),
            "the caret was pushed onto the row below the one it moved to"
        );
    }

    #[test]
    fn home_and_end_are_the_line_the_newlines_delimit() {
        // `C-a` on a wrapped paragraph goes to the start of the paragraph, the
        // way it does in every line editor a terminal user has met; the rows
        // are the screen's business and `Up`/`Down`'s.
        let mut editor = editor("alpha bravo\nsecond");
        editor.apply(Action::Home, 6);
        assert_eq!(editor.point(6), (2, 0), "the second line's first row");
        editor.apply(Action::Left, 6);
        editor.apply(Action::Home, 6);
        assert_eq!(editor.point(6), (0, 0), "the first line, not the row above");
        editor.apply(Action::End, 6);
        assert_eq!(&editor.text()[..editor.cursor], "alpha bravo");
    }

    #[test]
    fn the_kills_take_the_line_to_each_side_of_the_caret() {
        let mut editor = editor("alpha\nbravo charlie");
        editor.apply(Action::WordLeft, 80);
        editor.apply(Action::KillToEnd, 80);
        assert_eq!(editor.text(), "alpha\nbravo ");
        editor.apply(Action::KillToStart, 80);
        assert_eq!(editor.text(), "alpha\n");
    }

    #[test]
    fn a_word_delete_takes_the_whitespace_with_the_word() {
        let mut editor = editor("alpha bravo   ");
        editor.apply(Action::DeleteWordLeft, 80);
        assert_eq!(editor.text(), "alpha ");
        editor.apply(Action::DeleteWordLeft, 80);
        assert_eq!(editor.text(), "");
    }

    #[test]
    fn a_newline_is_a_break_in_the_text_and_a_row_of_its_own() {
        let mut editor = editor("ab");
        editor.apply(Action::InsertNewline, 80);
        assert_eq!(editor.text(), "ab\n");
        assert_eq!(editor.rows(80), vec!["ab", ""]);
        assert_eq!(editor.point(80), (1, 0));
    }

    #[test]
    fn an_edit_at_the_ends_of_the_text_is_a_no_op_rather_than_a_panic() {
        let mut editor = Editor::new();
        for action in [
            Action::Backspace,
            Action::Delete,
            Action::Left,
            Action::Right,
            Action::Up,
            Action::Down,
            Action::Home,
            Action::End,
            Action::WordLeft,
            Action::WordRight,
            Action::KillToEnd,
            Action::KillToStart,
            Action::DeleteWordLeft,
        ] {
            editor.apply(action, 80);
            assert!(editor.is_empty(), "{action:?} invented text");
        }
        assert!(editor.insert("ab"));
        editor.apply(Action::Delete, 80);
        editor.apply(Action::Right, 80);
        assert_eq!(editor.text(), "ab", "a delete at the end removed a byte");
    }

    #[test]
    fn taking_the_text_leaves_an_empty_composer_with_its_caret_at_the_start() {
        let mut editor = editor("submitted");
        assert_eq!(editor.take(), "submitted");
        assert!(editor.is_empty());
        assert_eq!(editor.point(80), (0, 0));
        assert!(editor.insert("next"));
        assert_eq!(editor.text(), "next", "the caret was left past the text");
    }

    #[test]
    fn a_refused_insert_changes_nothing_at_all() {
        let mut editor = editor("kept");
        assert!(!editor.insert(&"a".repeat(MAX_COMPOSER_BYTES)));
        assert_eq!(editor.text(), "kept");
        assert_eq!(editor.point(80), (0, 4), "the caret moved for a refusal");
    }

    #[test]
    fn a_wide_glyph_is_one_move_and_two_columns() {
        let mut editor = editor("\u{d55c}\u{ae00}");
        editor.apply(Action::Left, 80);
        assert_eq!(editor.point(80), (0, 2));
        editor.apply(Action::Backspace, 80);
        assert_eq!(editor.text(), "\u{ae00}");
    }

    #[test]
    fn an_insertion_that_merges_two_clusters_leaves_the_caret_outside_the_merged_one() {
        // A ZWJ typed between two emoji is one keystroke that makes one glyph
        // out of two. The caret cannot stay where the bytes ended -- that is
        // *inside* the new cluster -- or the next delete takes a piece of a
        // glyph and `wrap::cursor_point` reports a column in the middle of a
        // cell.
        let mut editor = editor("\u{1f468}\u{1f469}");
        editor.apply(Action::Left, 80);
        assert!(editor.insert("\u{200d}"), "the zero-width joiner");
        assert_eq!(editor.text(), "\u{1f468}\u{200d}\u{1f469}");
        assert_eq!(
            editor.rows(80).len(),
            1,
            "the two emoji did not merge, so this proves nothing"
        );
        assert_eq!(
            editor.point(80),
            (0, 2),
            "the caret is inside the merged cluster"
        );
        editor.apply(Action::Backspace, 80);
        assert!(
            editor.is_empty(),
            "a delete took part of a cluster: {:?}",
            editor.text()
        );
    }

    #[test]
    fn a_combining_mark_typed_after_its_base_joins_it_rather_than_standing_alone() {
        let mut editor = editor("ex");
        editor.apply(Action::Left, 80);
        assert!(editor.insert("\u{301}"));
        assert_eq!(editor.text(), "e\u{301}x");
        assert_eq!(
            editor.point(80),
            (0, 1),
            "the accent took a column of its own"
        );
        editor.apply(Action::Backspace, 80);
        assert_eq!(editor.text(), "x", "the base was left without its accent");
    }

    #[test]
    fn an_ordinary_insertion_still_puts_the_caret_after_what_was_typed() {
        // The other side of the snap: text that merges with nothing must not
        // move the caret past anything.
        let mut editor = editor("ac");
        editor.apply(Action::Left, 80);
        assert!(editor.insert("b"));
        assert_eq!(editor.text(), "abc");
        assert_eq!(editor.point(80), (0, 2));
        editor.apply(Action::Backspace, 80);
        assert_eq!(editor.text(), "ac");
    }

    #[test]
    fn a_composer_with_more_rows_than_a_u16_still_shows_the_row_the_caret_is_on() {
        // 65536 newlines is 64 KiB of an 8 MiB budget, so this is reachable
        // long before the byte cap -- and a row count saturated at `u16::MAX`
        // would leave the window following row 65535 while the caret sat on
        // row 65536, which is the window's one job.
        for rows in [65_535usize, 65_536, 65_537] {
            let mut editor = Editor::new();
            assert!(editor.insert(&"\n".repeat(rows - 1)));
            assert_eq!(editor.rows(80).len(), rows);
            let (row, column) = editor.point(80);
            assert_eq!((row, column), (rows - 1, 0), "{rows} rows");

            let shown = window(editor.rows(80).len(), row, 11);
            assert!(shown.contains(&row), "{rows} rows: {shown:?}");
            assert_eq!(shown, rows - 11..rows, "{rows} rows");

            // And the window still follows it after it moves.
            editor.apply(Action::Up, 80);
            let (row, _) = editor.point(80);
            assert_eq!(row, rows - 2, "{rows} rows: the caret did not move up");
            assert!(window(rows, row, 11).contains(&row), "{rows} rows");
        }
    }

    #[test]
    fn the_window_keeps_the_caret_visible_wherever_it_is() {
        // The property the four vectors above are examples of, and the one the
        // band rests on: the row the caret is on is always in the window, and
        // the window is always as full as the text allows.
        for rows in 1usize..40 {
            for limit in 1u16..12 {
                for point in 0..rows {
                    let window = window(rows, point, limit);
                    assert!(
                        window.contains(&point),
                        "{rows} rows, limit {limit}, caret on {point}: {window:?}"
                    );
                    assert!(window.end <= rows, "{window:?} runs past {rows} rows");
                    assert_eq!(
                        window.len(),
                        rows.min(usize::from(limit)),
                        "{rows} rows, limit {limit}: the window was not full"
                    );
                }
            }
        }
    }
}
