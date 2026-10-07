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

use super::edit_history::{Delta, DeltaKind};
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
    /// The other end of a selection, while there is one: the selection is the
    /// text between this and the caret (`selection_anchor`,
    /// `editor_state.zig:33`, `:87-94`).
    ///
    /// Only Super-A makes one here ([`Self::select_all`]); every move, every
    /// edit and every whole-draft replacement ends it, as upstream's
    /// `setCursor` and its insertions do (`:121-139`, `:163-176`).
    anchor: Option<usize>,
}

impl Editor {
    pub(crate) fn new() -> Self {
        Self {
            text: String::new(),
            cursor: 0,
            sticky: None,
            entities: Entities::new(),
            anchor: None,
        }
    }

    /// The selected range, while there is one (`selectionRange`,
    /// `editor_state.zig:87-94`).
    pub(crate) fn selection(&self) -> Option<std::ops::Range<usize>> {
        let anchor = self.anchor?;
        (anchor != self.cursor).then(|| anchor.min(self.cursor)..anchor.max(self.cursor))
    }

    /// The selected text, as the draft holds it -- a collapsed paste's summary
    /// and all (`selectedText`, `editor_state.zig:96-99`).
    pub(crate) fn selected_text(&self) -> Option<&str> {
        self.selection().map(|range| &self.text[range])
    }

    /// Selects the whole draft, the caret at its end, and says whether that
    /// changed anything (`selectAll`, `editor_state.zig:140-146`). An empty
    /// draft has nothing to select and ends any selection instead.
    pub(crate) fn select_all(&mut self) -> bool {
        if self.text.is_empty() {
            return self.anchor.take().is_some();
        }
        let changed = self.anchor != Some(0) || self.cursor != self.text.len();
        self.anchor = Some(0);
        self.cursor = self.text.len();
        self.sticky = None;
        changed
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

    /// Whether the caret is past the last character.
    ///
    /// A yes-or-no rather than the offset, for [`Self::before_caret`]'s reason.
    /// Two readers in `super::shell`. The `/provider` picker: Right commits a
    /// row only from the end of the text, so Right keeps moving the caret while
    /// the argument is being edited (`app_input_runtime.zig:281-296`). And
    /// `/model`'s inline column (`super::model_column`): a space advances it,
    /// and typing keeps a later column, only from the end.
    pub(crate) fn caret_at_end(&self) -> bool {
        self.cursor == self.text.len()
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
    /// `None` is the byte budget: the composer keeps exactly the text it had,
    /// which is the difference between a paste that did not fit and a paste
    /// that half fit -- and, since a refusal records nothing, it is also what
    /// keeps a keystroke that could not happen from clearing the redo stack
    /// (`super::edit_history`).
    pub(crate) fn insert(&mut self, text: &str) -> Option<Delta> {
        self.insert_with_entities(text, &[])
    }

    /// The same insertion, with blocks inside the text at offsets relative to
    /// it: what a yank puts back (`super::shell::Shell::yank_killed`).
    ///
    /// The spans arrive relative because the text is going somewhere it has
    /// never been -- the caret, not the offsets it was killed from -- and this
    /// is the module that owns the arithmetic that moves them
    /// (`super::entity`).
    pub(crate) fn insert_with_entities(&mut self, text: &str, entities: &[Span]) -> Option<Delta> {
        if text.is_empty() {
            return None;
        }
        // What is selected is replaced by what is inserted, in the same edit
        // (`insertComposerSliceBounded` at `selection.start`,
        // `app_input_runtime.zig:1628-1660`) -- a block in it whole.
        let selected = self.selection().map(|range| {
            let held: Vec<Span> = self.entities.spans().to_vec();
            (range, held)
        });
        let freed = selected.as_ref().map_or(0, |(range, _)| range.len());
        if (self.text.len() - freed).saturating_add(text.len()) > MAX_COMPOSER_BYTES {
            return None;
        }
        let caret_before = self.cursor;
        let mut removed = String::new();
        let mut lost: Vec<Span> = Vec::new();
        if let Some((range, held)) = selected {
            let taken = self.entities.delete_touching(range);
            removed = self.text[taken.clone()].to_string();
            lost = held
                .into_iter()
                .filter(|span| span.start < taken.end && taken.start < span.end)
                .collect();
            self.text.replace_range(taken.clone(), "");
            self.cursor = taken.start;
        }
        self.anchor = None;
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
        let placed: Vec<Span> = entities
            .iter()
            .map(|span| Span {
                start: at.saturating_add(span.start),
                end: at.saturating_add(span.end),
                kind: span.kind.clone(),
            })
            .collect();
        for span in &placed {
            self.entities.register(span.clone());
        }
        Some(Delta::new(
            at,
            removed,
            text.to_string(),
            lost,
            placed,
            caret_before,
            self.cursor,
            DeltaKind::Ordinary,
        ))
    }

    /// Inserts a collapsed paste's summary at the caret and records the block
    /// it stands for, or refuses the pair whole.
    ///
    /// One call rather than an insert and a registration, because the span is
    /// *where the summary went*: a caller that did the two steps itself would
    /// be a caller doing arithmetic on the offset this module keeps as an
    /// invariant, and a failed insert would leave a block naming bytes that are
    /// not there.
    /// The span the caller still needs is read back from the delta
    /// ([`Delta::inserted_entities`]), which is the same fact with one producer
    /// instead of two: a paste is one recorded transaction, and the entity in it
    /// is the one that was really registered.
    pub(crate) fn insert_entity(&mut self, summary: &str, kind: EntityKind) -> Option<Delta> {
        // Where the summary goes: the caret, or the start of what it replaces
        // -- whose ends are carets, so never inside a block.
        let at = self.selection().map_or(self.cursor, |range| range.start);
        let span = Span {
            start: 0,
            end: summary.len(),
            kind,
        };
        let delta = self.insert_with_entities(summary, std::slice::from_ref(&span))?;
        debug_assert_eq!(delta.at(), at);
        Some(Delta::new(
            delta.at(),
            delta.removed().to_string(),
            delta.inserted().to_string(),
            delta.removed_entities().to_vec(),
            delta.inserted_entities().to_vec(),
            delta.caret_before(),
            delta.caret_after(),
            // One framed paste is one transaction, whatever it weighed and
            // however many reads of the terminal it arrived in.
            DeltaKind::Paste,
        ))
    }

    /// Puts a skill token in place of `range` -- the `$` word a mention menu
    /// was searching, or the whole search of `/skills`' -- and a space after
    /// it when `separator`, the caret after both, or refuses whole
    /// (`bindSkillToken`, `vercel-labs/fx@c1db9191
    /// src/core/input/registered_entities.zig:466-533`).
    ///
    /// The range goes as a selection would, a block it touches with it, so
    /// the token is one edit: one delta whose inserted entity makes it a
    /// history boundary ([`super::edit_history::EditHistory::record`]).
    pub(crate) fn bind(
        &mut self,
        range: std::ops::Range<usize>,
        summary: &str,
        kind: EntityKind,
        separator: bool,
    ) -> Option<Delta> {
        if range.start > range.end
            || range.end > self.text.len()
            || !self.text.is_char_boundary(range.start)
            || !self.text.is_char_boundary(range.end)
            || summary.is_empty()
        {
            return None;
        }
        let (anchor, cursor, sticky) = (self.anchor, self.cursor, self.sticky);
        if range.start < range.end {
            self.anchor = Some(range.start);
            self.cursor = range.end;
        } else {
            self.anchor = None;
            self.cursor = range.start;
        }
        let mut text = summary.to_string();
        if separator {
            text.push(' ');
        }
        let span = Span {
            start: 0,
            end: summary.len(),
            kind,
        };
        let delta = self.insert_with_entities(&text, std::slice::from_ref(&span));
        if delta.is_none() {
            self.anchor = anchor;
            self.cursor = cursor;
            self.sticky = sticky;
        }
        delta
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
        self.anchor = None;
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
    /// `Some` is the difference the keystroke made, for the history to hold
    /// (`super::edit_history`); `None` is a caret move, or an edit with nothing
    /// to do -- a `Delete` at the end of the text, a `Backspace` at the start.
    /// A `None` records nothing, which is what keeps a keystroke that changed
    /// nothing from clearing the redo stack.
    pub(crate) fn apply(&mut self, action: Action, cols: u16) -> Option<Delta> {
        // A selection first: a deletion takes it (`deleteSelection`,
        // `composer_deletion.zig:30-35`), a kill kills it
        // (`composer_kill_ring.zig:123-137`), and a left or right move
        // collapses it to that edge and goes no further
        // (`moveCharacterLeft`/`Right`, `moveWordLeft`/`Right`,
        // `horizontal_navigation.zig:53-141`). Everything else moves the caret
        // from where it is and ends it on the way ([`Self::move_to`]).
        if let Some(range) = self.selection() {
            match action {
                Action::Backspace
                | Action::Delete
                | Action::DeleteWordBackward
                | Action::DeleteWordRight => {
                    return self.delete(range.start, range.end, DeltaKind::Ordinary);
                }
                Action::DeleteWordLeft
                | Action::KillToEnd
                | Action::KillToStart
                | Action::ControlK => {
                    return self.delete(range.start, range.end, DeltaKind::Kill);
                }
                Action::Left | Action::WordLeft => {
                    self.move_to(range.start);
                    return None;
                }
                Action::Right | Action::WordRight => {
                    self.move_to(range.end);
                    return None;
                }
                _ => {}
            }
        }
        match action {
            Action::Left => {
                self.move_to(self.left());
                None
            }
            Action::Right => {
                self.move_to(self.right());
                None
            }
            // Upstream's word moves (`moveWordLeft`/`moveWordRight`,
            // `horizontal_navigation.zig:85-141`): by word characters, not by
            // whitespace -- so `foo.bar` is two words to a move and one to
            // `C-w` -- and over a block whole.
            Action::WordLeft => {
                self.move_to(self.outside(self.word_move_left(), Direction::Backward));
                None
            }
            Action::WordRight => {
                self.move_to(self.outside(self.word_move_right(), Direction::Forward));
                None
            }
            Action::Home => {
                self.move_to(self.line_start());
                None
            }
            Action::End => {
                self.move_to(self.line_end());
                None
            }
            // Shift on the vertical arrows is a question field's to bind
            // (`super::input::Action::SelectUp`); to an editor it is the arrow.
            Action::Up | Action::SelectUp => {
                self.move_by_row(Step::Up, cols);
                None
            }
            Action::Down | Action::SelectDown => {
                self.move_by_row(Step::Down, cols);
                None
            }
            // The two that are not kills: what a `Backspace` or a `Delete`
            // takes does **not** go to the kill ring, so `C-y` still yanks the
            // last thing that was really killed (`kill_ring.zig:86-116`).
            Action::Backspace => self.delete(self.left(), self.cursor, DeltaKind::Ordinary),
            Action::Delete => self.delete(self.cursor, self.right(), DeltaKind::Ordinary),
            Action::DeleteWordLeft => self.delete(
                self.outside(self.word_left(), Direction::Backward),
                self.cursor,
                DeltaKind::Kill,
            ),
            // Upstream's `delete_word_right` is a deletion through
            // `deletionState`, not the kill ring (`app_input_runtime.zig:475-485`),
            // so what it takes is not what `C-y` yanks.
            Action::DeleteWordRight => self.delete(
                self.cursor,
                self.word_right_delete_end(),
                DeltaKind::Ordinary,
            ),
            // Alt-Backspace: a deletion by word characters, not `C-w`'s kill
            // (`delete_word_left` through `deletionState`,
            // `app_input_runtime.zig:454-465`).
            Action::DeleteWordBackward => self.delete(
                self.word_left_delete_start(),
                self.cursor,
                DeltaKind::Ordinary,
            ),
            // `C-k` is the kill everywhere a menu is not up, and a menu is the
            // session's to know about (`super::shell`).
            Action::KillToEnd | Action::ControlK => {
                self.delete(self.cursor, self.line_end(), DeltaKind::Kill)
            }
            Action::KillToStart => self.delete(self.line_start(), self.cursor, DeltaKind::Kill),
            // `C-j` likewise: a newline, here.
            Action::InsertNewline | Action::ControlJ => self.insert("\n"),

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
            // The page keys are the session's too: how many rows a page is,
            // is the screen's, and while a menu is up a page moves the menu
            // rather than the caret -- so `super::shell` decides, and moves
            // the caret with [`Self::move_rows`] when it is the caret's.
            //
            // They are named rather than caught by a wildcard so that an action
            // added later has to be routed on purpose.
            //
            // The two recall keys are the session's for the same reason
            // `Submit` is: what they replace is the *whole* draft, and the
            // composer is what is being replaced rather than what decides to
            // replace it (`super::shell::Shell::recall`, through
            // [`Self::set_text`]).
            //
            // The three editing-history keys are the session's for the same
            // reason: an undo is a delta the *history* holds being put back
            // ([`Self::revert`]), and a yank is text the kill slot holds being
            // inserted -- neither is a keystroke the composer can answer out of
            // its own buffer (`super::shell::Shell::act`).
            Action::Submit
            | Action::ControlP
            | Action::HistoryNext
            | Action::Escape
            | Action::Cancel
            | Action::Eof
            | Action::Redraw
            | Action::Tab
            | Action::BackTab
            | Action::Undo
            | Action::Redo
            | Action::Yank
            | Action::PasteImage
            | Action::PasteStart
            | Action::PasteEnd
            | Action::ToggleTranscript
            | Action::PageUp
            | Action::PageDown
            | Action::SelectAll
            | Action::CopySelection
            | Action::CutSelection
            | Action::Ignore => None,
        }
    }

    /// Puts the draft back the way it was before `delta`.
    ///
    /// The offsets are the delta's own -- widened where the editor widened them
    /// -- so this is a `replace_range` on a boundary this module produced rather
    /// than one re-derived from a buffer that no longer says how its bytes got
    /// there. The blocks come back with the **ids they had**, because the
    /// summaries being restored say those numbers out loud (`entity.rs:212-216`
    /// would reject a duplicate, and there is none: the block was gone).
    pub(crate) fn revert(&mut self, delta: &Delta) {
        self.splice(
            delta.at(),
            delta.inserted().len(),
            delta.removed(),
            delta.removed_entities(),
        );
        self.cursor = delta.caret_before();
        self.sticky = None;
        self.anchor = None;
    }

    /// Does `delta` again, after a [`Self::revert`] took it back.
    pub(crate) fn replay(&mut self, delta: &Delta) {
        self.splice(
            delta.at(),
            delta.removed().len(),
            delta.inserted(),
            delta.inserted_entities(),
        );
        self.cursor = delta.caret_after();
        self.sticky = None;
        self.anchor = None;
    }

    /// Replaces `remove` bytes at `at` with `insert`, and puts `entities` back
    /// at the absolute offsets they name.
    ///
    /// The one place a history operation touches the buffer, so the two
    /// directions cannot disagree about what "putting it back" means.
    fn splice(&mut self, at: usize, remove: usize, insert: &str, entities: &[Span]) {
        if remove > 0 {
            let end = at.saturating_add(remove);
            // Widening cannot reach past this range: what a delta names is
            // either the run the editor already widened to, or the summary it
            // registered whole -- and an entity that merely *abuts* the range is
            // not an overlap (`super::entity::Entities::delete_touching`).
            let taken = self.entities.delete_touching(at..end);
            debug_assert_eq!(taken, at..end, "a history delta named a partial entity");
            self.text.replace_range(taken, "");
        }
        if !insert.is_empty() {
            self.text.insert_str(at, insert);
            self.entities.shift_after_insert(at, insert.len());
        }
        for span in entities {
            self.entities.register(span.clone());
        }
    }

    /// Lends the whole composer out -- its text, its blocks and its caret --
    /// and leaves it empty, for [`Self::give_back`] to restore verbatim.
    ///
    /// What `C-p` does to a draft before the model catalog borrows the
    /// composer for its search (`composer_stash.State.capture`,
    /// `app_input_runtime.zig:2356-2375`). Not [`Self::take`]: that one ends
    /// the blocks, and a stashed draft's blocks are still its own.
    pub(crate) fn lend(&mut self) -> Lent {
        let lent = Lent {
            text: std::mem::take(&mut self.text),
            entities: std::mem::take(&mut self.entities),
            caret: self.cursor,
        };
        self.cursor = 0;
        self.sticky = None;
        self.anchor = None;
        lent
    }

    /// Puts a lent draft back exactly as it was, whatever the composer held
    /// meanwhile (`restoreModelPickerDraft`, `app_input_runtime.zig:2377-2386`).
    pub(crate) fn give_back(&mut self, lent: Lent) {
        self.text = lent.text;
        self.entities = lent.entities;
        self.cursor = lent.caret.min(self.text.len());
        self.sticky = None;
        self.anchor = None;
    }

    /// Takes the composer's text, leaving it empty.
    pub(crate) fn take(&mut self) -> String {
        self.cursor = 0;
        self.sticky = None;
        self.anchor = None;
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

    /// The same rows with the selection marked: `open` before each run of
    /// selected text on a row and `close` after it (`appendLayoutUnit`,
    /// `input_presentation.zig:995-1021`, which wraps selected units in
    /// reverse video and a reset) -- and each image's badge a link to its file,
    /// as upstream's composer writes one (`writeImageBadgeClipped`,
    /// `input_presentation.zig:1079-1089`, `image_attachments.zig:1342-1354`).
    /// Exactly [`Self::rows`] with neither.
    ///
    /// Both are zero-width, so the rows wrap, and the caret is placed
    /// ([`Self::point`]), exactly as they are without them. A link is closed on
    /// the row it opened on and opened again on the next, as upstream balances
    /// a badge across narrow card rows (`user_message_card.zig:354-379`).
    #[cfg(test)]
    pub(crate) fn painted_rows(&self, cols: u16, open: &str, close: &str) -> Vec<String> {
        self.painted_rows_with(cols, (open, close), ("", ""))
    }

    /// [`Self::painted_rows`], and each skill token's name in `token.0`,
    /// closed by `token.1` -- upstream's bold bright name
    /// (`selected_completion_style`, `fx-skills-40x120-dollar-enter`:
    /// `┃ \e[1m\e[38;5;255mdemo\e[0m`). The close is a full reset, so a
    /// selection the token was inside is opened again after it.
    pub(crate) fn painted_rows_with(
        &self,
        cols: u16,
        (open, close): (&str, &str),
        token: (&str, &str),
    ) -> Vec<String> {
        let selected = self.selection();
        let links = self.badge_links();
        let tokens: Vec<std::ops::Range<usize>> = if token.0.is_empty() {
            Vec::new()
        } else {
            self.entities
                .spans()
                .iter()
                .filter(|span| span.skill().is_some())
                .map(|span| span.start..span.end)
                .collect()
        };
        if selected.is_none() && links.is_empty() && tokens.is_empty() {
            return self.rows(cols);
        }
        let link_at = |at: usize| {
            links
                .iter()
                .find(|(span, _)| span.contains(&at))
                .map(|(_, link)| link.as_str())
        };
        wrap::wrap(&self.text, cols.max(1))
            .into_iter()
            .map(|row| {
                let body = body(&self.text[row.start..row.end]);
                let mut out = String::with_capacity(body.len() + open.len() + close.len());
                let mut inside = false;
                let mut linked: Option<&str> = None;
                let mut named = false;
                for (offset, cluster) in body.grapheme_indices(true) {
                    let at = row.start + offset;
                    let in_token = tokens.iter().any(|range| range.contains(&at));
                    if !in_token && named {
                        out.push_str(token.1);
                        named = false;
                        // The close was a reset: a selection still running
                        // past the token is opened again.
                        if inside {
                            out.push_str(open);
                        }
                    }
                    let wanted = selected.as_ref().is_some_and(|range| range.contains(&at));
                    if wanted != inside {
                        out.push_str(if wanted { open } else { close });
                        inside = wanted;
                        // A selection that ended inside a token took the
                        // token's style with its reset: given back.
                        if !wanted && named {
                            out.push_str(token.0);
                        }
                    }
                    if in_token && !named {
                        out.push_str(token.0);
                        named = true;
                    }
                    let link = link_at(at);
                    if link != linked {
                        out.push_str(link.unwrap_or(super::hyperlink::CLOSE));
                        linked = link;
                    }
                    out.push_str(&wrap::expand_tabs(cluster));
                }
                if named {
                    out.push_str(token.1);
                } else if inside {
                    out.push_str(close);
                }
                if linked.is_some() {
                    out.push_str(super::hyperlink::CLOSE);
                }
                out
            })
            .collect()
    }

    /// Each image badge in the draft, and the link that opens around it: upstream's
    /// badge has no parameters, only its file (`writeImageBadge`,
    /// `image_attachments.zig:1336-1340`). A badge whose file has no target a
    /// link may carry ([`super::hyperlink::file_uri`]) is text.
    fn badge_links(&self) -> Vec<(std::ops::Range<usize>, String)> {
        self.entities
            .spans()
            .iter()
            .filter_map(|span| match &span.kind {
                EntityKind::Image { image, .. } => super::hyperlink::file_uri(&image.path)
                    .map(|uri| (span.start..span.end, super::hyperlink::open("", &uri))),
                EntityKind::Paste { .. } | EntityKind::Skill { .. } => None,
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
        self.anchor = None;
    }

    /// Removes `start..end`, and puts the caret where the text used to be.
    ///
    /// `None` is a keystroke with nothing to do -- `start >= end`, which is a
    /// `Delete` at the end of the text or a `Backspace` at the start (the same
    /// guard `kill_ring.zig:93` makes). It records no history, so a no-op
    /// keystroke cannot clear the redo stack.
    fn delete(&mut self, start: usize, end: usize, kind: DeltaKind) -> Option<Delta> {
        if start >= end {
            return None;
        }
        let caret_before = self.cursor;
        // The spans as the draft still holds them, so the ones the widening is
        // about to take can be handed to the history with the offsets and the
        // **ids** they had. An undo puts those numbers back.
        let held: Vec<Span> = self.entities.spans().to_vec();
        // **What it really has to lose.** A deletion that overlaps a block at
        // all takes the whole of it, so what is removed is the widened range
        // rather than the one that was asked for -- a backspace at a summary's
        // right edge removes the block instead of damaging its name
        // (`super::entity::Entities::delete_touching`).
        let taken = self.entities.delete_touching(start..end);
        let removed = self.text[taken.clone()].to_string();
        let lost: Vec<Span> = held
            .into_iter()
            .filter(|span| span.start < taken.end && taken.start < span.end)
            .collect();
        self.text.replace_range(taken.clone(), "");
        self.cursor = taken.start;
        self.sticky = None;
        self.anchor = None;
        Some(Delta::new(
            taken.start,
            removed,
            String::new(),
            lost,
            Vec::new(),
            caret_before,
            self.cursor,
            kind,
        ))
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
        self.anchor = None;
    }

    /// The caret to the start of the draft (`Step::Up`) or to its end
    /// (`Step::Down`): what an arrow does on the composer's first or last row
    /// before it is a history step (`draft_start` / `draft_end`,
    /// `input_completion_runtime.zig:265-284`, `horizontal_navigation.zig:29-30`).
    ///
    /// `false` when the caret is already there, which is the arrow that steps
    /// the history instead.
    pub(crate) fn move_to_draft_edge(&mut self, step: Step) -> bool {
        let edge = match step {
            Step::Up => 0,
            Step::Down => self.text.len(),
        };
        if self.cursor == edge {
            return false;
        }
        self.move_to(edge);
        true
    }

    /// `count` rows up or down at once, stopping at the first or the last row:
    /// a page move (`scanRowDelta`, `visual_layout.zig:598-622`).
    ///
    /// The same column rule as a single row's ([`Self::move_by_row`]), and the
    /// same refusal to land inside a block. `false` is a caret already on the
    /// row the page would reach -- the first row for a page up, the last for a
    /// page down -- and a page never turns into a history step there, which is
    /// the difference from the arrows.
    pub(crate) fn move_rows(&mut self, step: Step, count: usize, cols: u16) -> bool {
        let rows = wrap::wrap(&self.text, cols.max(1));
        let (row, column) = wrap::cursor_point(&self.text, &rows, self.cursor);
        let target = self.sticky.unwrap_or(column);
        self.sticky = Some(target);
        let wanted = match step {
            Step::Up => row.saturating_sub(count),
            Step::Down => row.saturating_add(count).min(rows.len().saturating_sub(1)),
        };
        if wanted == row || wanted >= rows.len() {
            return false;
        }
        let last = wanted + 1 == rows.len();
        let landed = self.offset_on(&rows[wanted], last, target);
        self.cursor = self.beside(landed);
        self.anchor = None;
        true
    }

    /// Where the composer's text puts `offset` on a screen `cols` wide: the
    /// row and the cells to the left of it there, as [`Self::point`] says for
    /// the caret.
    ///
    /// What the `@` file picker is placed by: its rows start under the `@`
    /// (`picker_start_col`, `input_presentation.zig:377-380`).
    pub(crate) fn point_of(&self, offset: usize, cols: u16) -> (usize, u16) {
        let rows = wrap::wrap(&self.text, cols.max(1));
        wrap::cursor_point(&self.text, &rows, offset.min(self.text.len()))
    }

    /// Where the caret is, as a byte offset into [`Self::text`].
    ///
    /// Read-only: the offset is this module's invariant, so a caller that
    /// wants it moved asks for an edit rather than doing arithmetic on it.
    /// The `@` file picker reads it to find the token the caret is in.
    pub(crate) fn caret(&self) -> usize {
        self.cursor
    }

    /// `start..end` replaced with `text`, the caret left at `caret` -- an
    /// offset into the text **after** the replacement -- as one edit, or a
    /// refusal that changes nothing.
    ///
    /// What accepting a file picker row is (`replaceRangeBounded`,
    /// `input_completion_runtime.zig:932-938`): the `@` token becomes the
    /// path, and the caret goes after the separator. **Refused** when the range
    /// touches a block -- a path is not something a summary can be part of --
    /// or when the result would pass the byte budget; and the caret is snapped
    /// to a grapheme boundary as every insertion's is.
    pub(crate) fn replace(
        &mut self,
        start: usize,
        end: usize,
        text: &str,
        caret: usize,
    ) -> Option<Delta> {
        if start > end
            || end > self.text.len()
            || !self.text.is_char_boundary(start)
            || !self.text.is_char_boundary(end)
            || (start == end && text.is_empty())
        {
            return None;
        }
        if self
            .entities
            .spans()
            .iter()
            .any(|span| span.start < end.max(start + 1) && start < span.end)
        {
            return None;
        }
        let grown = self.text.len() - (end - start) + text.len();
        if grown > MAX_COMPOSER_BYTES {
            return None;
        }
        let caret_before = self.cursor;
        let removed = self.text[start..end].to_string();
        self.splice(start, end - start, text, &[]);
        self.cursor = boundary_at_or_after(&self.text, caret.min(self.text.len()));
        self.sticky = None;
        self.anchor = None;
        Some(Delta::new(
            start,
            removed,
            text.to_string(),
            Vec::new(),
            Vec::new(),
            caret_before,
            self.cursor,
            DeltaKind::Ordinary,
        ))
    }

    /// A `\` right before the caret turned into a line break, as one edit --
    /// or `None` when the character before the caret is not one
    /// (`replaceBackslashBeforeCursorWithNewline`,
    /// `composer_line_continuation.zig:11-50`).
    ///
    /// What Return does in front of a trailing backslash: the line continues
    /// rather than being sent (`app_input_runtime.zig:1612`).
    pub(crate) fn continue_line(&mut self) -> Option<Delta> {
        let at = self.cursor.checked_sub(1)?;
        if self.text.as_bytes().get(at) != Some(&b'\\')
            || self.entities.inside(self.cursor).is_some()
        {
            return None;
        }
        self.replace(at, self.cursor, "\n", at + 1)
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

    /// Where a word move to the left lands (`moveWordLeft`,
    /// `horizontal_navigation.zig:85-114`): back over what is not a word
    /// character, then over the word characters before it -- and to the start
    /// of a block that ends where the walk is.
    fn word_move_left(&self) -> usize {
        let ends_block = |at: usize| self.entities.spans().iter().find(|span| span.end == at);
        let mut at = self.cursor;
        while at > 0 {
            if let Some(span) = ends_block(at) {
                return span.start;
            }
            let previous = before(&self.text, at);
            if is_word(&self.text[previous..at]) {
                break;
            }
            at = previous;
        }
        while at > 0 {
            if let Some(span) = ends_block(at) {
                return span.start;
            }
            let previous = before(&self.text, at);
            if !is_word(&self.text[previous..at]) {
                break;
            }
            at = previous;
        }
        at
    }

    /// Where Alt-Backspace starts deleting (`wordLeftRange`,
    /// `composer_deletion.zig:103-136`): a block the caret is at the end of is
    /// taken whole; otherwise back over what is not a word character, then
    /// over the word characters before it -- and a block met on the way is
    /// taken whole and ends the walk.
    fn word_left_delete_start(&self) -> usize {
        if let Some(start) = self.entities.step_over(self.cursor, Direction::Backward) {
            return start;
        }
        let ends_block = |at: usize| self.entities.spans().iter().find(|span| span.end == at);
        let mut at = self.cursor;
        while at > 0 {
            if let Some(span) = ends_block(at) {
                return span.start;
            }
            let previous = before(&self.text, at);
            if is_word(&self.text[previous..at]) {
                break;
            }
            at = previous;
        }
        while at > 0 {
            if let Some(span) = ends_block(at) {
                return span.start;
            }
            let previous = before(&self.text, at);
            if !is_word(&self.text[previous..at]) {
                break;
            }
            at = previous;
        }
        at
    }

    /// Where a word move to the right lands (`moveWordRight`,
    /// `horizontal_navigation.zig:116-141`): over what is not a word character,
    /// then to the end of the word after it -- the far side of a block met on
    /// the way, and short of one that starts where the word ends.
    fn word_move_right(&self) -> usize {
        let end = self.text.len();
        let starting = |at: usize| self.entities.spans().iter().find(|span| span.start == at);
        let mut at = self.cursor;
        while at < end {
            if let Some(span) = starting(at) {
                return span.end;
            }
            let next = after(&self.text, at);
            if is_word(&self.text[at..next]) {
                break;
            }
            at = next;
        }
        while at < end && starting(at).is_none() {
            let next = after(&self.text, at);
            if !is_word(&self.text[at..next]) {
                break;
            }
            at = next;
        }
        at
    }

    /// The start of the word to the left: the whitespace before the caret is
    /// stepped over, and then the run of non-whitespace before that
    /// (`whitespace_word_left`, `composer_kill_ring.zig:14-18`) -- what `C-w`
    /// and Alt-Backspace take.
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

    /// Where a forward word delete stops (`wordRightRange`,
    /// `composer_deletion.zig:138-164`): over the word characters the caret is
    /// in front of, then over what separates them from the next word -- and
    /// the far side of a block that starts at the caret, which is deleted
    /// whole. The run of separators stops at a block rather than taking it.
    ///
    /// Upstream's word characters (`isWordCharacter`,
    /// `text_boundaries.zig:71-77`): ASCII letters and digits, `_`, and
    /// everything from `U+00C0` up. Not the whitespace words the moves use --
    /// this key's own rule.
    fn word_right_delete_end(&self) -> usize {
        if let Some(end) = self.entities.step_over(self.cursor, Direction::Forward) {
            return end;
        }
        let end = self.text.len();
        let starts_block = |at: usize| self.entities.spans().iter().any(|span| span.start == at);
        let mut at = self.cursor;
        while at < end && !starts_block(at) && is_word(&self.text[at..after(&self.text, at)]) {
            at = after(&self.text, at);
        }
        while at < end && !starts_block(at) && !is_word(&self.text[at..after(&self.text, at)]) {
            at = after(&self.text, at);
        }
        at
    }
}

/// A draft lent out of the composer ([`Editor::lend`]): every byte, every
/// block under its own number, and where the caret was.
#[derive(Debug)]
pub(crate) struct Lent {
    text: String,
    entities: Entities,
    caret: usize,
}

/// Which way [`Editor::move_by_row`] is going.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Step {
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

/// Whether a grapheme cluster is whitespace, for the whitespace word `C-w`
/// takes.
fn is_space(cluster: &str) -> bool {
    cluster.chars().all(char::is_whitespace)
}

/// Whether a grapheme cluster is a word character for the forward word delete
/// (`isWordCharacter`, `text_boundaries.zig:71-77`), judged by its first
/// scalar as upstream judges a rune.
fn is_word(cluster: &str) -> bool {
    cluster
        .chars()
        .next()
        .is_some_and(|scalar| scalar == '_' || scalar.is_ascii_alphanumeric() || scalar >= '\u{c0}')
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
        assert!(editor.insert(text).is_some());
        editor
    }

    // -----------------------------------------------------------------------
    // the deltas an undo is built on
    // -----------------------------------------------------------------------

    #[test]
    fn undo_of_a_paste_restores_the_block_with_its_original_id() {
        let mut editor = Editor::new();
        let delta = editor
            .insert_entity(
                "[Pasted text #1, 12 lines]",
                EntityKind::Paste {
                    id: 1,
                    text: std::sync::Arc::from("a\nb\nc"),
                    lines: 12,
                },
            )
            .expect("the paste fits");
        assert_eq!(delta.kind(), DeltaKind::Paste);
        let id = delta.inserted_entities()[0].id();

        editor.revert(&delta);
        assert_eq!(editor.text(), "");
        assert!(
            editor.entities().is_empty(),
            "the block outlived the summary it stands for"
        );
        assert_eq!(caret(&editor), delta.caret_before());

        editor.replay(&delta);
        assert_eq!(editor.text(), "[Pasted text #1, 12 lines]");
        assert_eq!(
            editor.entities().spans()[0].id(),
            id,
            "the replayed block came back under a different number"
        );
        assert_eq!(editor.expanded(), "a\nb\nc");
        assert_eq!(caret(&editor), delta.caret_after());
    }

    #[test]
    fn a_delta_round_trips_over_an_entity_boundary() {
        // A backspace at a summary's right edge takes the **whole** block, so
        // the delta names the widened range (`Entities::delete_touching`) and
        // the undo has to put the block back with it -- text, span and id.
        let (mut editor, range) = with_block("before ", "a very long paste", " after");
        assert_eq!(editor.entities().len(), 1);
        let whole = editor.text().to_string();
        editor.apply(Action::Home, 80);
        editor.apply(Action::End, 80);
        // The caret is at the end of the line; walk it back over ` after` and
        // then take the block with one more Backspace.
        for _ in 0..6 {
            editor.apply(Action::Left, 80);
        }
        assert_eq!(caret(&editor), range.end);

        let delta = editor
            .apply(Action::Backspace, 80)
            .expect("a backspace over a block is an edit");
        assert_eq!(editor.text(), "before  after");
        assert!(editor.entities().is_empty());
        assert_eq!(
            delta.at(),
            range.start,
            "the delta named the un-widened range"
        );
        assert_eq!(delta.removed().len(), range.end - range.start);
        assert_eq!(delta.removed_entities().len(), 1);
        assert_eq!(delta.removed_entities()[0].id(), 1);

        editor.revert(&delta);
        assert_eq!(editor.text(), whole);
        assert_eq!(editor.entities().len(), 1, "the block did not come back");
        assert_eq!(editor.entities().spans()[0].range(), range);
        assert_eq!(editor.expanded(), "before a very long paste after");
        assert_eq!(caret(&editor), delta.caret_before());

        editor.replay(&delta);
        assert_eq!(editor.text(), "before  after");
        assert!(editor.entities().is_empty());
        assert_eq!(caret(&editor), delta.caret_after());
    }

    #[test]
    fn undo_then_redo_restores_text_and_caret_over_a_grapheme_cluster() {
        // The caret is an offset that must land on a cluster boundary, or
        // `wrap::cursor_point` reports a column inside a glyph. A revert that
        // restored the bytes and left the caret where the edit put it would be
        // exactly that defect.
        let family = "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}";
        let mut editor = editor(&format!("x{family}y"));
        editor.apply(Action::Left, 80);
        let before = caret(&editor);
        let delta = editor
            .apply(Action::Backspace, 80)
            .expect("a backspace over a family is an edit");
        assert_eq!(editor.text(), "xy");
        assert_eq!(
            delta.removed(),
            family,
            "the backspace took part of a cluster"
        );

        editor.revert(&delta);
        assert_eq!(editor.text(), format!("x{family}y"));
        assert_eq!(caret(&editor), before);
        editor.replay(&delta);
        assert_eq!(editor.text(), "xy");
        assert_eq!(caret(&editor), delta.caret_after());
    }

    #[test]
    fn the_three_kills_are_kills_and_the_two_deletes_are_not() {
        // Which keystroke loads the kill ring is decided here, once, because
        // `super::edit_history` reads the kind rather than the keystroke
        // (`kill_ring.zig:86-116` has no arm for a backspace).
        for (action, kind) in [
            (Action::KillToEnd, DeltaKind::Kill),
            (Action::KillToStart, DeltaKind::Kill),
            (Action::DeleteWordLeft, DeltaKind::Kill),
            (Action::Backspace, DeltaKind::Ordinary),
            (Action::Delete, DeltaKind::Ordinary),
        ] {
            let mut editor = editor("one two");
            editor.apply(Action::Left, 80);
            let delta = editor
                .apply(action, 80)
                .unwrap_or_else(|| panic!("{action:?} had nothing to do"));
            assert_eq!(delta.kind(), kind, "{action:?}");
        }
    }

    #[test]
    fn a_keystroke_with_nothing_to_do_records_nothing() {
        // The guard `kill_ring.zig:93` makes, in the terms this module answers
        // in: `None` rather than an empty delta, so a caller cannot record a
        // history entry for a keystroke that changed nothing.
        let mut editor = editor("ab");
        assert!(editor.apply(Action::Delete, 80).is_none(), "at the end");
        editor.apply(Action::Home, 80);
        assert!(
            editor.apply(Action::Backspace, 80).is_none(),
            "at the start"
        );
        assert!(editor.apply(Action::KillToStart, 80).is_none());
        editor.apply(Action::End, 80);
        assert!(editor.apply(Action::KillToEnd, 80).is_none());
        assert_eq!(editor.text(), "ab");
    }

    #[test]
    fn a_refused_insert_is_not_an_edit() {
        let mut editor = Editor::new();
        assert!(editor.insert(&"a".repeat(MAX_COMPOSER_BYTES)).is_some());
        assert!(
            editor.insert("b").is_none(),
            "the budget admitted a byte it had no room for"
        );
        assert!(
            editor.insert("").is_none(),
            "an empty insert is not a change to record"
        );
    }

    /// A composer holding `before`, one collapsed block, and `after`, with the
    /// run the block's summary occupies.
    fn with_block(before: &str, text: &str, after: &str) -> (Editor, std::ops::Range<usize>) {
        let mut editor = Editor::new();
        // An empty insert is not an edit and has no delta to hand back
        // (`Editor::insert`), so the two ends are only put in when they exist.
        if !before.is_empty() {
            assert!(editor.insert(before).is_some());
        }
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
        if !after.is_empty() {
            assert!(editor.insert(after).is_some());
        }
        (editor, span.inserted_entities()[0].range())
    }

    /// Where the caret is, in bytes, without handing the tests the offset the
    /// module keeps as an invariant.
    fn caret(editor: &Editor) -> usize {
        editor.before_caret().len()
    }

    #[test]
    fn a_word_move_goes_by_word_characters_and_over_a_block_whole() {
        // `horizontal_navigation.zig`'s own case (:190-216): `h\u{e9}llo.a_b`
        // is two words to a move, and a block is one stop.
        let (mut blocked, span) = with_block("h\u{e9}llo.a_b ", "y", "");
        blocked.apply(Action::Home, 80);
        blocked.apply(Action::WordRight, 80);
        assert_eq!(caret(&blocked), "h\u{e9}llo".len());
        blocked.apply(Action::WordRight, 80);
        assert_eq!(caret(&blocked), "h\u{e9}llo.a_b".len());
        blocked.apply(Action::WordRight, 80);
        assert_eq!(caret(&blocked), span.end);
        blocked.apply(Action::WordLeft, 80);
        assert_eq!(caret(&blocked), span.start);
        // `fx-composer-40x120-alt-f`'s shape: to the end of the word, not to
        // the start of the next one.
        let mut draft = editor("Yfoo bar");
        draft.apply(Action::Home, 80);
        draft.apply(Action::Right, 80);
        draft.apply(Action::WordRight, 80);
        assert_eq!(caret(&draft), "Yfoo".len());
    }

    #[test]
    fn a_forward_word_delete_takes_the_word_and_what_separates_it_from_the_next() {
        // `wordRightRange` (`composer_deletion.zig:138-164`): word characters
        // are letters, digits, `_` and everything from U+00C0 up.
        let mut words = editor("foo_bar, baz");
        words.apply(Action::Home, 80);
        let delta = words.apply(Action::DeleteWordRight, 80).expect("an edit");
        assert_eq!(words.text(), "baz");
        assert_eq!(delta.kind(), DeltaKind::Ordinary, "a deletion, not a kill");
        // In front of punctuation, the punctuation and the gap go.
        let mut dots = editor("a ... b");
        dots.apply(Action::Home, 80);
        dots.apply(Action::Right, 80);
        dots.apply(Action::DeleteWordRight, 80);
        assert_eq!(dots.text(), "ab");
        // Non-ASCII letters are word characters.
        let mut accented = editor("caf\u{e9} x");
        accented.apply(Action::Home, 80);
        accented.apply(Action::DeleteWordRight, 80);
        assert_eq!(accented.text(), "x");
        // At the end there is nothing to do, and nothing is recorded.
        let mut end = editor("x");
        assert!(end.apply(Action::DeleteWordRight, 80).is_none());
    }

    #[test]
    fn a_selection_is_painted_where_it_is_and_replaced_whole_with_its_blocks() {
        // `editor_state.zig:87-146`: select all, then an insertion replaces
        // the selection -- a block in it included -- as one edit.
        let (mut draft, _span) = with_block("a\tb ", "x\ny", " c");
        assert!(draft.select_all());
        assert!(!draft.select_all(), "selecting all twice changed something");
        let rows = draft.painted_rows(80, "<", ">");
        assert_eq!(rows.len(), 1);
        assert!(rows[0].starts_with("<a    b "), "{rows:?}");
        assert!(rows[0].ends_with(" c>"), "{rows:?}");
        let delta = draft.insert("z").expect("an edit");
        assert_eq!(draft.text(), "z");
        assert!(draft.entities().is_empty());
        draft.revert(&delta);
        assert_eq!(draft.entities().len(), 1, "the undo lost the block");
        assert_eq!(draft.selection(), None);
        // With nothing selected the painted rows are the rows.
        assert_eq!(draft.painted_rows(80, "<", ">"), draft.rows(80));
        // A selection on part of a wrapped draft is marked on each row it
        // reaches.
        let mut wrapped = editor("abcdefgh");
        wrapped.select_all();
        assert_eq!(wrapped.painted_rows(4, "<", ">"), ["<abcd>", "<efgh>"]);
    }

    /// An editor holding the badge of an image read from `path`, typed at
    /// the caret.
    fn attach(editor: &mut Editor, id: u32, path: &str) {
        let image = crate::image::Image::new(
            id,
            std::path::PathBuf::from(path),
            crate::image::MediaType::Png,
            std::sync::Arc::from(&b"png"[..]),
        );
        editor
            .insert_entity(
                &crate::image::badge(id),
                EntityKind::Image {
                    token: std::sync::Arc::from(crate::image::token(id)),
                    image: std::sync::Arc::new(image),
                },
            )
            .expect("room for a badge");
    }

    const LINK_CLOSE: &str = "\u{1b}]8;;\u{1b}\\";

    #[test]
    fn a_badge_is_a_link_to_its_file_and_costs_no_cell() {
        // `writeImageBadgeClipped` (`image_attachments.zig:1342-1354`): no
        // parameters, the path percent-encoded, the label inside.
        let mut draft = editor("see ");
        attach(&mut draft, 1, "/tmp/a b.png");
        assert!(draft.insert(" ok").is_some());
        let open = "\u{1b}]8;;file:///tmp/a%20b.png\u{1b}\\";
        assert_eq!(
            draft.painted_rows(80, "<", ">"),
            [format!("see {open}[Image 1]{LINK_CLOSE} ok")]
        );
        // The caret and the wrap are the plain rows': the link is zero-width.
        assert_eq!(draft.point(80), (0, 16));
        assert_eq!(draft.rows(80), ["see [Image 1] ok"]);
    }

    #[test]
    fn two_badges_of_one_file_are_one_link_and_another_file_switches_it() {
        let mut draft = Editor::new();
        attach(&mut draft, 1, "/tmp/a.png");
        attach(&mut draft, 2, "/tmp/a.png");
        attach(&mut draft, 3, "/tmp/b.png");
        let a = "\u{1b}]8;;file:///tmp/a.png\u{1b}\\";
        let b = "\u{1b}]8;;file:///tmp/b.png\u{1b}\\";
        assert_eq!(
            draft.painted_rows(80, "<", ">"),
            [format!("{a}[Image 1][Image 2]{b}[Image 3]{LINK_CLOSE}")]
        );
    }

    #[test]
    fn a_badge_wrapped_across_rows_is_closed_on_each_and_opened_on_the_next() {
        // `user_message_card.zig:354-379` balances a badge across narrow rows.
        let mut draft = Editor::new();
        attach(&mut draft, 1, "/tmp/a.png");
        let a = "\u{1b}]8;;file:///tmp/a.png\u{1b}\\";
        assert_eq!(
            draft.painted_rows(5, "<", ">"),
            [
                format!("{a}[Imag{LINK_CLOSE}"),
                format!("{a}e 1]{LINK_CLOSE}")
            ]
        );
    }

    #[test]
    fn a_selected_badge_is_marked_and_linked_and_both_end_on_the_row() {
        let mut draft = editor("x ");
        attach(&mut draft, 1, "/tmp/a.png");
        assert!(draft.select_all());
        let a = "\u{1b}]8;;file:///tmp/a.png\u{1b}\\";
        assert_eq!(
            draft.painted_rows(80, "<", ">"),
            [format!("<x {a}[Image 1]>{LINK_CLOSE}")]
        );
    }

    #[test]
    fn a_badge_with_no_absolute_file_is_text() {
        // A clipboard image is named by the copy the session keeps, which is
        // not a path from the root (`super::super::shell`'s
        // `settle_clipboard_reads`).
        let mut draft = Editor::new();
        attach(&mut draft, 1, "image-1-0123456789abcdef.bin");
        assert_eq!(draft.painted_rows(80, "<", ">"), ["[Image 1]"]);
    }

    #[test]
    fn alt_backspace_deletes_by_word_characters_and_a_block_whole() {
        // `wordLeftRange` (`composer_deletion.zig:103-136`).
        let mut words = editor("see foo.bar  ");
        let delta = words
            .apply(Action::DeleteWordBackward, 80)
            .expect("an edit");
        assert_eq!(words.text(), "see foo.");
        assert_eq!(delta.kind(), DeltaKind::Ordinary, "a deletion, not a kill");
        words.apply(Action::DeleteWordBackward, 80);
        assert_eq!(words.text(), "see ");
        assert!(Editor::new()
            .apply(Action::DeleteWordBackward, 80)
            .is_none());
        // A block the caret is after goes whole, and so does one the walk meets.
        let (mut blocked, _span) = with_block("ab ", "x\ny", "");
        blocked.apply(Action::DeleteWordBackward, 80);
        assert_eq!(blocked.text(), "ab ");
        assert!(blocked.entities().is_empty());
        let (mut met, _span) = with_block("ab ", "x\ny", "  ");
        met.apply(Action::DeleteWordBackward, 80);
        assert_eq!(met.text(), "ab ");
    }

    #[test]
    fn a_forward_word_delete_takes_a_block_whole_and_stops_in_front_of_one() {
        let (mut editor, span) = with_block("ab ", "x\ny", " cd");
        // From the block's left edge: the whole block.
        editor.apply(Action::Home, 80);
        for _ in 0..3 {
            editor.apply(Action::Right, 80);
        }
        assert_eq!(caret(&editor), span.start);
        editor.apply(Action::DeleteWordRight, 80);
        assert_eq!(editor.text(), "ab  cd");
        assert!(editor.entities().is_empty());
        // From in front of the gap before a block: the gap, not the block.
        let (mut editor, span) = with_block("ab ", "x\ny", " cd");
        editor.apply(Action::Home, 80);
        editor.apply(Action::Right, 80);
        editor.apply(Action::Right, 80);
        editor.apply(Action::DeleteWordRight, 80);
        assert_eq!(editor.entities().len(), 1);
        assert_eq!(
            editor.text().len(),
            "ab ".len() + span.len() + " cd".len() - 1
        );
    }

    #[test]
    fn a_page_move_goes_as_far_as_the_draft_and_no_further() {
        let mut draft = editor("one\ntwo\nthree\nfour");
        assert!(draft.move_rows(Step::Up, 2, 80));
        assert_eq!(draft.point(80), (1, 3));
        assert!(draft.move_rows(Step::Up, 9, 80), "a short page still moves");
        assert_eq!(draft.point(80).0, 0);
        assert!(
            !draft.move_rows(Step::Up, 9, 80),
            "on the first row a page is nothing"
        );
        // The column the run began at is the one it goes back to.
        assert!(draft.move_rows(Step::Down, 9, 80));
        assert_eq!(draft.point(80), (3, 4));
        assert!(!draft.move_rows(Step::Down, 1, 80));
    }

    #[test]
    fn a_replacement_is_one_edit_and_never_touches_a_block() {
        let mut draft = editor("see @gui");
        let delta = draft.replace(4, 8, "@docs/guide.md ", 19).expect("an edit");
        assert_eq!(draft.text(), "see @docs/guide.md ");
        assert_eq!(caret(&draft), 19);
        draft.revert(&delta);
        assert_eq!(draft.text(), "see @gui");
        assert_eq!(caret(&draft), 8);
        let (mut editor, span) = with_block("@", "x\ny", "");
        assert!(editor.replace(0, span.start + 1, "@a", 2).is_none());
        assert!(
            editor.replace(0, 1, "@a", 2).is_some(),
            "beside a block is fine"
        );
    }

    #[test]
    fn a_backslash_before_the_caret_becomes_the_line_break() {
        let mut draft = editor("line3\\");
        let delta = draft.continue_line().expect("a continuation");
        assert_eq!(draft.text(), "line3\n");
        assert_eq!(caret(&draft), 6);
        draft.revert(&delta);
        assert_eq!(draft.text(), "line3\\");
        // Not right before the caret: nothing.
        draft.apply(Action::Left, 80);
        assert!(draft.continue_line().is_none());
        assert!(Editor::new().continue_line().is_none());
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
        assert!(editor.insert("xxxxxxxxxxxx\n").is_some());
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
            .inserted_entities()[0]
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
        assert!(editor.insert("!").is_some());
        assert_eq!(editor.expanded(), "see y!");
        editor.apply(Action::Home, 80);
        assert!(editor.insert("? ").is_some());
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
        assert!(editor.insert(&crate::tui::paste::summary(1, 1)).is_some());
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
            assert!(editor
                .insert(&"x".repeat(per.saturating_sub(name.len())))
                .is_some());
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
            assert!(editor.insert("z").is_some());
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
            assert!(editor.insert("z").is_some());
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
        assert!(editor.insert(&"a".repeat(MAX_COMPOSER_BYTES)).is_some());
        assert!(
            editor.insert("b").is_none(),
            "the budget was exceeded silently"
        );
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
        assert!(editor.insert("ab").is_some());
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
        assert!(editor.insert("next").is_some());
        assert_eq!(editor.text(), "next", "the caret was left past the text");
    }

    #[test]
    fn a_refused_insert_changes_nothing_at_all() {
        let mut editor = editor("kept");
        assert!(editor.insert(&"a".repeat(MAX_COMPOSER_BYTES)).is_none());
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
        assert!(editor.insert("\u{200d}").is_some(), "the zero-width joiner");
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
        assert!(editor.insert("\u{301}").is_some());
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
        assert!(editor.insert("b").is_some());
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
            assert!(editor.insert(&"\n".repeat(rows - 1)).is_some());
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
