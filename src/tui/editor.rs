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
//! control the composer both holds and shows: [`wrap::TAB_WIDTH`] measures it
//! at four cells and the row this module hands the painter has it expanded to
//! that many spaces, so pasted indentation is sent whole *and* drawn, and the
//! caret sits on the glyph it belongs to rather than four columns from it.
//!
//! # What a block is
//!
//! A collapsed paste is an **entity**: a span of this buffer, held beside the
//! text and moved by every edit ([`super::entity`]). Every motion below steps
//! over one whole and every delete that overlaps one takes all of it, which is
//! what makes the caret's second invariant true -- it is never *inside* a unit
//! -- and what makes a summary a thing rather than the words it looks like.
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

use super::entity::{Direction, Entities, EntityKind, Span};
use super::input::Action;
use super::wrap::{self, Row};

/// The most text a composer may hold (`paste_framing.zig:16-35`).
pub(crate) const MAX_COMPOSER_BYTES: usize = 8 * 1024 * 1024;

/// The text being composed, and where the caret is in it.
pub(crate) struct Editor {
    text: String,
    /// A byte offset into [`text`](Self::text), always on a grapheme boundary.
    ///
    /// And never **inside** an entity ([`entities`](Self::entities)): every
    /// motion below steps over one whole, which is what makes a collapsed
    /// paste a unit rather than a word the caret can be lost in.
    cursor: usize,
    /// The column vertical motion is aiming for, in cells, while a run of it
    /// lasts. `None` the moment anything else moves the caret.
    sticky: Option<u16>,
    /// The collapsed pastes this text stands on, as runs of it.
    ///
    /// Held **here** rather than beside the composer because a span is a range
    /// of this buffer: every insertion and every deletion has to move it, and a
    /// set of spans kept anywhere else would be a set that had to be told about
    /// each of them by a caller that could forget (`super::entity`).
    entities: Entities,
}

impl Editor {
    pub(crate) fn new() -> Self {
        Self {
            text: String::new(),
            cursor: 0,
            sticky: None,
            entities: Entities::new(),
        }
    }

    /// The collapsed pastes the text stands on.
    pub(crate) fn entities(&self) -> &Entities {
        &self.entities
    }

    /// The text with every block put back where its summary stands -- what a
    /// submit sends.
    pub(crate) fn expanded(&self) -> String {
        self.entities.expand(&self.text)
    }

    /// How many bytes the draft is holding out of sight, behind its summaries.
    pub(crate) fn retained(&self) -> usize {
        self.entities.retained()
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
    /// offset is this module's invariant (always a grapheme boundary, never
    /// inside an entity, and `wrap::cursor_point` panics on one that is not
    /// even a `char` boundary), and a caller given the number would be a caller
    /// doing arithmetic on it.
    ///
    /// The **prospective** question this and its other half used to answer --
    /// "what draft would this keystroke produce" -- is gone with the name-based
    /// block model: an edit can no longer release megabytes by damaging a
    /// summary, so `super::shell` asks the budget once and about the text as it
    /// stands. What is left is the cases below, which read the caret through it
    /// rather than being handed the offset.
    #[cfg(test)]
    pub(crate) fn before_caret(&self) -> &str {
        &self.text[..self.cursor]
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
        let at = self.cursor;
        self.text.insert_str(at, text);
        // The blocks after the insertion are that many bytes further along, and
        // a block the insertion landed *inside* is a block whose summary is no
        // longer its summary -- which cannot happen from the keyboard and is
        // answered anyway (`super::entity::Entities::shift_after_insert`).
        self.entities.shift_after_insert(at, text.len());
        // **Not** simply `cursor + text.len()`. An insertion can *merge* what
        // was on either side of it into one cluster -- a ZWJ typed between two
        // emoji is the whole of that case -- and the offset the insertion ended
        // at is then inside a cluster the terminal draws as one glyph. Every
        // caller below, and `wrap::cursor_point` above them, is owed a
        // boundary, so the caret is snapped to the first one at or after where
        // the text went in: after the merged glyph rather than into it.
        self.cursor = boundary_at_or_after(&self.text, at + text.len());
        self.sticky = None;
        true
    }

    /// Inserts a collapsed paste's summary at the caret and records the block
    /// it stands for, or refuses the pair whole.
    ///
    /// One call rather than an insert and a registration, because the span is
    /// *where the summary went*: a caller that did the two steps itself would
    /// be a caller doing arithmetic on the offset this module keeps as an
    /// invariant, and a failed insert would leave a block naming bytes that are
    /// not there.
    pub(crate) fn insert_entity(&mut self, summary: &str, kind: EntityKind) -> Option<Span> {
        let at = self.cursor;
        if !self.insert(summary) {
            return None;
        }
        let span = Span {
            start: at,
            end: at.saturating_add(summary.len()),
            kind,
        };
        self.entities.register(span.clone());
        Some(span)
    }

    /// Replaces the whole draft, or refuses it whole.
    ///
    /// What a history recall needs and what [`Self::insert`] cannot give it: a
    /// recall is not an insertion at the caret, it is *this line instead of
    /// that one*, and a composer built out of a take plus an insert would be a
    /// composer that had been momentarily empty -- and would have thrown away
    /// the recalled line's blocks on the way through, since a `take` is what
    /// ends a draft's entities.
    ///
    /// The caret lands at the **end** of the new text, which is where every
    /// shell with a history puts it: a recalled line is one the user is about
    /// to add to or send, and a caret parked in front of it would make the
    /// first keystroke after a recall an insertion into the middle of somebody
    /// else's sentence. `text.len()` is a grapheme boundary because the end of
    /// a string always is, which is the obligation this module owes
    /// [`wrap::cursor_point`].
    ///
    /// `false` is the byte budget, and it means the composer kept exactly the
    /// text it had -- the same refusal [`Self::insert`] gives, for the same
    /// reason.
    pub(crate) fn set_text(&mut self, text: &str, entities: Entities) -> bool {
        if text.len() > MAX_COMPOSER_BYTES {
            return false;
        }
        self.text.clear();
        self.text.push_str(text);
        // The blocks arrive **with** the text, because they are runs of it: a
        // recall that kept the old ones would have spans measured against a
        // draft that is gone, and a recall that dropped them would put a
        // summary on the screen standing for nothing
        // (`super::shell::Shell::recall`).
        self.entities = entities;
        self.cursor = self.text.len();
        // The text under the caret is a different text, so a column remembered
        // from a run of vertical motion over the old one would aim the next
        // `Up` at a place nobody chose.
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
            Action::Left => self.move_to(self.left()),
            Action::Right => self.move_to(self.right()),
            Action::WordLeft => self.move_to(self.outside(self.word_left(), Direction::Backward)),
            Action::WordRight => self.move_to(self.outside(self.word_right(), Direction::Forward)),
            Action::Home => self.move_to(self.line_start()),
            Action::End => self.move_to(self.line_end()),
            Action::Up => self.move_by_row(Step::Up, cols),
            Action::Down => self.move_by_row(Step::Down, cols),
            Action::Backspace => self.delete(self.left(), self.cursor),
            Action::Delete => self.delete(self.cursor, self.right()),
            Action::DeleteWordLeft => {
                self.delete(
                    self.outside(self.word_left(), Direction::Backward),
                    self.cursor,
                );
            }
            Action::KillToEnd => self.delete(self.cursor, self.line_end()),
            Action::KillToStart => self.delete(self.line_start(), self.cursor),
            Action::InsertNewline => {
                self.insert("\n");
            }
            // Not the composer's: submitting, leaving and cancelling are the
            // session's (`super::shell`), a paste is Task 18's, and an
            // `Ignore` is a keystroke this session has no binding for.
            //
            // `Tab` is preserved as an [`Action`] rather than as text, and
            // `super::shell` is what dispatches it: to the approval panel while
            // a question owns the focus (`super::approval`), otherwise to the
            // inline slash completion menu while one is open
            // (`super::picker`). With neither up it lands here and does
            // nothing -- the composer still has no completion of its own to
            // drive. What never happens on any of those paths is a literal tab
            // reaching the composer's text: this arm inserts nothing, so a
            // draft cannot acquire a `\t` from a keystroke.
            //
            // They are named rather than caught by a wildcard so that an action
            // added later has to be routed on purpose.
            //
            // The two recall keys are the session's for the same reason
            // `Submit` is: what they replace is the *whole* draft, and the
            // composer is what is being replaced rather than what decides to
            // replace it (`super::shell::Shell::recall`, through
            // [`Self::set_text`]).
            Action::Submit
            | Action::HistoryPrevious
            | Action::HistoryNext
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
        // The blocks die with the draft they were pasted into. One that
        // outlived it would be a span into a buffer that has been emptied.
        self.entities.clear();
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
            .map(|row| {
                // Expanded here rather than at the painter, because the wrap
                // that produced this row already measured the tab at
                // `wrap::TAB_WIDTH` cells: a row handed over with the control
                // still in it would be a row the terminal indents by its own
                // tab stop, which is not the number the caret was placed from.
                wrap::expand_tabs(body(&self.text[row.start..row.end])).into_owned()
            })
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
        // **What it really has to lose.** A deletion that overlaps a block at
        // all takes the whole of it, so what is removed is the widened range
        // rather than the one that was asked for -- a backspace at a summary's
        // right edge removes the block instead of damaging its name
        // (`super::entity::Entities::delete_touching`).
        let taken = self.entities.delete_touching(start..end);
        self.text.replace_range(taken.clone(), "");
        self.cursor = taken.start;
        self.sticky = None;
    }

    /// Where a step back goes: the near side of the unit the caret is at the
    /// end of, or one grapheme.
    fn left(&self) -> usize {
        self.entities
            .step_over(self.cursor, Direction::Backward)
            .unwrap_or_else(|| before(&self.text, self.cursor))
    }

    /// Where a step forward goes: the far side of the unit the caret is at the
    /// start of, or one grapheme.
    fn right(&self) -> usize {
        self.entities
            .step_over(self.cursor, Direction::Forward)
            .unwrap_or_else(|| after(&self.text, self.cursor))
    }

    /// `at`, pushed out of any unit it is inside, the way the motion was going.
    ///
    /// The word moves need it and the two kills do not: a summary has spaces in
    /// it, so a word move measured on the text alone stops between the words of
    /// a name -- inside a unit -- while a line boundary never can, because a
    /// summary holds no line break.
    fn outside(&self, at: usize, direction: Direction) -> usize {
        match self.entities.inside(at) {
            Some(span) => match direction {
                Direction::Backward => span.start,
                Direction::Forward => span.end,
            },
            None => at,
        }
    }

    /// `at`, moved to the **nearer** edge of any unit it is inside.
    ///
    /// The vertical moves' answer, and it is nearest-edge rather than a refusal
    /// because a refusal is a caret that cannot get past a row with a summary
    /// on it: `Down` would stop moving. The column the run is aiming for is
    /// kept, so walking on through the block lands where the run wanted.
    fn beside(&self, at: usize) -> usize {
        match self.entities.inside(at) {
            Some(span) => {
                if at.saturating_sub(span.start) <= span.end.saturating_sub(at) {
                    span.start
                } else {
                    span.end
                }
            }
            None => at,
        }
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
        let landed = self.offset_on(&rows[wanted], last, target);
        self.cursor = self.beside(landed);
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
    use crate::tui::entity::{Entities, EntityKind, Span};

    fn editor(text: &str) -> Editor {
        let mut editor = Editor::new();
        assert!(editor.insert(text));
        editor
    }

    /// A composer holding `before`, one collapsed block, and `after`, with the
    /// run the block's summary occupies.
    fn with_block(before: &str, text: &str, after: &str) -> (Editor, std::ops::Range<usize>) {
        let mut editor = Editor::new();
        assert!(editor.insert(before));
        let lines = text.lines().count();
        let name = crate::tui::paste::summary(1, lines);
        let span = editor
            .insert_entity(
                &name,
                EntityKind::Paste {
                    id: 1,
                    text: std::sync::Arc::from(text),
                    lines,
                },
            )
            .expect("the block fits the composer");
        assert!(editor.insert(after));
        (editor, span.range())
    }

    /// Where the caret is, in bytes, without handing the tests the offset the
    /// module keeps as an invariant.
    fn caret(editor: &Editor) -> usize {
        editor.before_caret().len()
    }

    #[test]
    fn a_backspace_at_a_blocks_right_edge_removes_the_whole_block() {
        // The narrowing item 16 closes: Phase 1 edited the *name*, leaving a
        // damaged summary on the screen and a block nobody could send.
        let (mut editor, span) = with_block("see ", "y\ny", "");
        assert_eq!(caret(&editor), span.end);
        editor.apply(Action::Backspace, 80);
        assert_eq!(editor.text(), "see ", "the backspace edited the name");
        assert!(
            editor.entities().is_empty(),
            "the block outlived its summary"
        );
        assert_eq!(caret(&editor), span.start);
        assert_eq!(editor.expanded(), "see ");
    }

    #[test]
    fn a_delete_at_a_blocks_left_edge_removes_the_whole_block() {
        let (mut editor, span) = with_block("see ", "y", " ok");
        editor.apply(Action::Home, 80);
        for _ in 0..span.start {
            editor.apply(Action::Right, 80);
        }
        assert_eq!(caret(&editor), span.start);
        editor.apply(Action::Delete, 80);
        assert_eq!(editor.text(), "see  ok");
        assert!(editor.entities().is_empty());
    }

    #[test]
    fn a_word_delete_that_reaches_a_block_takes_all_of_it() {
        // A summary has spaces in it, so a word delete that stopped where the
        // words do would cut it in half.
        let (mut editor, _) = with_block("see ", "y", "");
        editor.apply(Action::DeleteWordLeft, 80);
        assert_eq!(editor.text(), "see ");
        assert!(editor.entities().is_empty());
    }

    #[test]
    fn the_kills_take_a_block_whole_or_not_at_all() {
        let (mut editor, _) = with_block("see ", "y", "");
        editor.apply(Action::KillToStart, 80);
        assert_eq!(editor.text(), "");
        assert!(editor.entities().is_empty());

        let (mut editor, span) = with_block("see ", "y", " ok");
        editor.apply(Action::Home, 80);
        for _ in 0..span.start {
            editor.apply(Action::Right, 80);
        }
        editor.apply(Action::KillToEnd, 80);
        assert_eq!(editor.text(), "see ");
        assert!(editor.entities().is_empty());
    }

    #[test]
    fn the_horizontal_moves_step_over_a_block_as_one_unit() {
        let (mut editor, span) = with_block("see ", "y", " ok");
        for _ in 0..3 {
            editor.apply(Action::Left, 80);
        }
        assert_eq!(
            caret(&editor),
            span.end,
            "the caret is not at the right edge"
        );
        editor.apply(Action::Left, 80);
        assert_eq!(
            caret(&editor),
            span.start,
            "a left step landed inside the name"
        );
        editor.apply(Action::Right, 80);
        assert_eq!(
            caret(&editor),
            span.end,
            "a right step landed inside the name"
        );
    }

    #[test]
    fn the_word_moves_step_over_a_block_as_one_unit() {
        let (mut editor, span) = with_block("see ", "y", " ok");
        editor.apply(Action::End, 80);
        editor.apply(Action::WordLeft, 80);
        // Over `ok`, which stops at the start of that word -- one byte past the
        // block's right edge, because the space between them is the word move's
        // own boundary rather than the unit's.
        assert_eq!(caret(&editor), span.end + 1);
        editor.apply(Action::WordLeft, 80);
        assert_eq!(
            caret(&editor),
            span.start,
            "a word move stopped between the words of a summary"
        );
        editor.apply(Action::WordRight, 80);
        assert_eq!(caret(&editor), span.end);
    }

    #[test]
    fn a_vertical_move_never_lands_inside_a_block() {
        // The one motion whose target is a *column* rather than an offset: the
        // row below can have a summary where the column is, and a caret there
        // would be a caret inside a unit.
        let mut editor = Editor::new();
        assert!(editor.insert("xxxxxxxxxxxx\n"));
        let name = crate::tui::paste::summary(1, 1);
        let span = editor
            .insert_entity(
                &name,
                EntityKind::Paste {
                    id: 1,
                    text: std::sync::Arc::from("y"),
                    lines: 1,
                },
            )
            .expect("the block fits")
            .range();
        editor.apply(Action::Home, 80);
        editor.apply(Action::Up, 80);
        for _ in 0..10 {
            editor.apply(Action::Right, 80);
        }
        assert_eq!(caret(&editor), 10);
        editor.apply(Action::Down, 80);
        let at = caret(&editor);
        assert!(
            at == span.start || at == span.end,
            "the caret landed inside the block, at {at} of {span:?}"
        );
        assert_eq!(at, span.start, "the caret was pushed to the far edge");
    }

    #[test]
    fn text_typed_beside_a_block_leaves_it_whole_and_expanding() {
        let (mut editor, span) = with_block("see ", "y", "");
        assert!(editor.insert("!"));
        assert_eq!(editor.expanded(), "see y!");
        editor.apply(Action::Home, 80);
        assert!(editor.insert("? "));
        assert_eq!(editor.expanded(), "? see y!");
        assert_eq!(editor.entities().len(), 1);
        assert_eq!(editor.entities().spans()[0].start, span.start + 2);
    }

    #[test]
    fn a_block_put_in_front_of_another_expands_in_the_order_they_are_read() {
        // The spans are kept in the order they appear in the draft, not in the
        // order they were registered: a paste at the caret can land in front of
        // a block that is already there, and an expansion that walked
        // registration order would splice the two the wrong way round.
        let (mut editor, _) = with_block("", "second", "");
        editor.apply(Action::Home, 80);
        let name = crate::tui::paste::summary(2, 1);
        editor
            .insert_entity(
                &name,
                EntityKind::Paste {
                    id: 2,
                    text: std::sync::Arc::from("first"),
                    lines: 1,
                },
            )
            .expect("the block fits");
        assert_eq!(editor.entities().len(), 2);
        assert_eq!(
            editor.expanded(),
            "firstsecond",
            "the blocks were expanded in the order they were pasted rather than \
             the order they are read in"
        );
    }

    #[test]
    fn the_words_of_a_summary_typed_by_hand_are_only_words() {
        // Identity is the span, not the text: a second copy of the name is
        // never expanded, however exactly it matches.
        let (mut editor, _) = with_block("", "y", "");
        assert!(editor.insert(&crate::tui::paste::summary(1, 1)));
        assert_eq!(
            editor.expanded(),
            format!("y{}", crate::tui::paste::summary(1, 1)),
            "typed words stood in for a block"
        );
    }

    #[test]
    fn taking_the_draft_takes_its_blocks_with_it() {
        let (mut editor, _) = with_block("see ", "y", "");
        assert_eq!(
            editor.take(),
            format!("see {}", crate::tui::paste::summary(1, 1))
        );
        assert!(editor.entities().is_empty(), "a block outlived its draft");
        assert_eq!(editor.expanded(), "");
    }

    #[test]
    fn a_recall_replaces_the_text_and_the_blocks_together() {
        let (mut editor, _) = with_block("see ", "y", "");
        let mut entities = Entities::new();
        let name = crate::tui::paste::summary(7, 1);
        entities.register(Span {
            start: 0,
            end: name.len(),
            kind: EntityKind::Paste {
                id: 7,
                text: std::sync::Arc::from("z"),
                lines: 1,
            },
        });
        assert!(editor.set_text(&name, entities));
        assert_eq!(editor.text(), name);
        assert_eq!(
            editor.expanded(),
            "z",
            "the recalled draft kept the old blocks"
        );
    }

    /// A composer holding `blocks` collapsed pastes inside a draft of about
    /// `bytes` bytes -- the shape the cost claims are about.
    fn loaded(bytes: usize, blocks: usize) -> Editor {
        let mut editor = Editor::new();
        let per = bytes / blocks;
        for index in 0..blocks {
            let id = u32::try_from(index + 1).expect("a test never mints that many");
            let name = crate::tui::paste::summary(id, 1);
            // The summary is part of the draft, so the filler is what is left
            // of this block's share of it -- a draft built past
            // `MAX_COMPOSER_BYTES` would be refused rather than measured.
            assert!(editor.insert(&"x".repeat(per.saturating_sub(name.len()))));
            editor
                .insert_entity(
                    &name,
                    EntityKind::Paste {
                        id,
                        text: std::sync::Arc::from("yyy"),
                        lines: 1,
                    },
                )
                .expect("the block fits");
        }
        editor
    }

    #[test]
    fn a_keystroke_reads_the_draft_no_times_however_many_blocks_it_holds() {
        // **The receipt the retained-block cap was removed on.** The old model
        // re-read the whole draft once per block on every keystroke, which is
        // why it needed a bound of 64; the spans cost integer arithmetic, and
        // the count that proves it is portable in a way a stopwatch is not.
        //
        // The two points are the ones the plan names: a megabyte with the old
        // cap's worth of blocks, and the composer's whole budget with fifteen
        // times as many.
        for (bytes, blocks) in [(1024 * 1024, 64), (8 * 1024 * 1024 - 4096, 1000)] {
            let mut editor = loaded(bytes, blocks);
            assert_eq!(editor.entities().len(), blocks);
            crate::tui::entity::scans::reset();
            assert!(editor.insert("z"));
            editor.apply(Action::Backspace, 80);
            editor.apply(Action::Left, 80);
            editor.apply(Action::Right, 80);
            assert_eq!(
                crate::tui::entity::scans::taken(),
                0,
                "a keystroke on a draft holding {blocks} blocks read it {} time(s)",
                crate::tui::entity::scans::taken()
            );
        }
    }

    #[test]
    fn a_submit_reads_the_draft_once_however_many_blocks_it_holds() {
        for (bytes, blocks) in [(1024 * 1024, 64), (8 * 1024 * 1024 - 4096, 1000)] {
            let editor = loaded(bytes, blocks);
            crate::tui::entity::scans::reset();
            let prompt = editor.expanded();
            assert_eq!(
                crate::tui::entity::scans::taken(),
                1,
                "{blocks} blocks cost {} reads of the draft",
                crate::tui::entity::scans::taken()
            );
            assert_eq!(prompt.matches("yyy").count(), blocks);
        }
    }

    /// The stopwatch beside the counter, and it is a **receipt** rather than a
    /// gate: it measures the machine it runs on, so it is the counter above
    /// that binds and this that is quoted. Ignored by default because a debug
    /// build measures the compiler rather than the code -- run it with
    /// `cargo test --release --lib tui::editor -- --ignored`.
    #[test]
    #[ignore = "timing receipt: release only, see the task report"]
    fn an_edit_and_a_submit_stay_inside_the_ceiling_on_this_machine() {
        const CEILING: std::time::Duration = std::time::Duration::from_millis(250);
        for (bytes, blocks) in [(1024 * 1024, 64), (8 * 1024 * 1024 - 4096, 1000)] {
            let mut editor = loaded(bytes, blocks);
            let started = std::time::Instant::now();
            assert!(editor.insert("z"));
            editor.apply(Action::Backspace, 80);
            let keystroke = started.elapsed();

            let started = std::time::Instant::now();
            let prompt = editor.expanded();
            let submit = started.elapsed();
            assert_eq!(prompt.matches("yyy").count(), blocks);

            println!("{bytes} bytes, {blocks} blocks: keystroke {keystroke:?}, submit {submit:?}");
            assert!(
                keystroke <= CEILING,
                "a keystroke on {bytes} bytes and {blocks} blocks took {keystroke:?}"
            );
            assert!(
                submit <= CEILING,
                "a submit of {bytes} bytes and {blocks} blocks took {submit:?}"
            );
        }
    }

    #[test]
    fn a_pasted_tab_is_the_same_run_of_cells_measured_and_painted() {
        // Phase 1 kept a tab in the text and drew nothing for it, so the caret
        // sat a column away from where the text was. One width, and the
        // painter uses the same one.
        let editor = editor("a\tb");
        let cells = usize::from(wrap::TAB_WIDTH);
        assert_eq!(editor.rows(80), vec![format!("a{}b", " ".repeat(cells))]);
        assert_eq!(
            editor.point(80),
            (0, 1 + wrap::TAB_WIDTH + 1),
            "the caret is not past the cells the tab paints"
        );
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
    fn set_text_replaces_the_draft_and_leaves_the_caret_at_its_end() {
        // What a history recall needs and what nothing else in this module
        // offers: the whole buffer at once, with the caret where the next
        // keystroke continues the line rather than in front of it.
        let mut editor = editor("what was being typed");
        editor.apply(Action::Home, 80);
        assert!(editor.set_text("the line that was recalled", Entities::new()));
        assert_eq!(editor.text(), "the line that was recalled");
        assert_eq!(
            editor.point(80),
            (0, 26),
            "the caret was left somewhere other than the end of the recalled line"
        );
    }

    #[test]
    fn set_text_forgets_the_column_a_vertical_run_was_aiming_for() {
        // A recall is not a step in a run of vertical motion: the text under
        // the caret is a different text, so a column remembered from the old
        // one would aim the next Up at a place nobody chose.
        let mut editor = editor("alphabet\nxy\nzulu-long");
        editor.apply(Action::Home, 80);
        editor.apply(Action::Right, 80);
        editor.apply(Action::Right, 80);
        editor.apply(Action::Right, 80);
        editor.apply(Action::Up, 80);
        assert_eq!(editor.point(80), (1, 2), "the short row clamped the column");
        // A replacement whose caret lands in a column the old run was **not**
        // aiming for, and whose first row is wide enough to tell the two apart:
        // with the preferred column still standing the `Up` below would aim at
        // 3, and with it forgotten it aims at the column the caret is really in.
        assert!(editor.set_text("abcdefghij\nxy", Entities::new()));
        assert_eq!(
            editor.point(80),
            (1, 2),
            "the caret is not at the end of the replaced draft"
        );
        editor.apply(Action::Up, 80);
        assert_eq!(
            editor.point(80),
            (0, 2),
            "the replaced draft kept the column the old run was aiming for"
        );
    }

    #[test]
    fn set_text_past_the_budget_refuses_and_changes_nothing() {
        // The same refusal `insert` gives, and for the same reason: a draft
        // that half arrived is not the line anybody recalled.
        let mut editor = editor("kept");
        editor.apply(Action::Home, 80);
        // Exactly at the cap is inside it, which is the boundary an off-by-one
        // would move: a draft of exactly `MAX_COMPOSER_BYTES` is one the
        // composer can hold, so it is one a recall can put back.
        assert!(editor.set_text(&"a".repeat(MAX_COMPOSER_BYTES), Entities::new()));
        assert_eq!(editor.text().len(), MAX_COMPOSER_BYTES);
        assert!(editor.set_text("kept", Entities::new()));
        editor.apply(Action::Home, 80);
        assert!(!editor.set_text(&"a".repeat(MAX_COMPOSER_BYTES + 1), Entities::new()));
        assert_eq!(
            editor.text(),
            "kept",
            "a refused replacement damaged the draft"
        );
        assert_eq!(
            editor.point(80),
            (0, 0),
            "a refused replacement moved the caret"
        );
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
