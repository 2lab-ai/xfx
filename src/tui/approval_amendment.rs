//! The sentence a user may attach to a permission decision, and the editor it
//! is typed in.
//!
//! A separate module from [`super::approval`] because the two answer different
//! questions and only one of them is about authority. `approval` owns what the
//! panel asks and what the three answers mean; this owns the **draft** -- a text
//! buffer with its own history, its own kill slot and its own paste assembler --
//! and nothing here can widen, narrow or re-key a grant. Feedback is context:
//! the call that ran is the call that was judged, whatever the sentence beside
//! it says (`vercel-labs/fx@580a0c5d src/core/permissions/tool_admission.zig:2039-2050`).
//!
//! # Two drafts, and one of the three choices has none
//!
//! Choice 0 (`Once`) and choice 2 (`Deny`) each carry a draft; choice 1
//! (`Always`) carries none. `Always` is the answer that buys the rest of the
//! session, and what it buys is the sentence the panel already prints
//! (`always_scope`); an amendment attached to it would read as a condition on a
//! grant that is keyed by tool and target and cannot carry one.
//!
//! The two are **independent**, down to their histories: an undo typed at the
//! deny side must not pull back a delta recorded at the allow side, and a user
//! who typed a reason for refusing and then allowed does not send the refusal
//! reason.
//!
//! # Why [`super::editor::Editor`] is not the whole contract
//!
//! `Editor::apply` returns `None` for `Undo`, `Redo`, `Yank`, `PasteStart` and
//! `PasteEnd` (`super::editor`'s own arm says so): those keys are the
//! *session's*, answered by `super::shell` out of an `EditHistory` and a
//! `Paste` rather than by the buffer. So "the draft edits like the composer, by
//! construction" would be false for exactly the five keys that need the most
//! care. [`Draft`] holds an `Editor` **and** an [`EditHistory`], and this module
//! answers those five itself.
//!
//! # Why the composer's paste assembler is the wrong tool here
//!
//! `super::paste::Paste` collapses anything past `COLLAPSE_ABOVE` into an
//! **entity summary** with a block id, and a feedback string that reached the
//! model as `[#3 pasted 40 lines]` would be a summary standing in for text the
//! user believes they sent. Feedback carries no entities, so [`DraftPaste`]
//! assembles the bytes itself -- reusing `super::paste::accepted`, because which
//! bytes of a bracketed paste are *content* is one fact about the terminal
//! rather than one fact per surface.

use crate::permission::ApprovalAnswer;

use super::edit_history::EditHistory;
use super::editor::Editor;
use super::input::Action;

/// The most bytes one amendment may carry.
///
/// Its own budget rather than the composer's eight mebibytes
/// (`super::paste::MAX_PASTE_BYTES`): what this holds is a sentence about a
/// decision, it is delivered as a user message after the tool result, and a
/// four-kibibyte cap is the difference between an amendment and a payload.
pub(crate) const MAX_FEEDBACK_BYTES: usize = 4096;

/// How many rows of one draft the panel ever paints.
///
/// The draft is a **viewport** onto its own text, windowed on the caret
/// ([`super::editor::window`]), so a long amendment scrolls inside its own rows
/// rather than growing the panel until the question is off the screen. Three,
/// which is enough to read a sentence back and small enough that both drafts
/// together cannot outweigh the disclosure they sit under.
pub(crate) const MAX_DRAFT_ROWS: usize = 3;

/// What the panel says when a paste was too large to take.
///
/// Said rather than swallowed, and said as a **refusal of the whole paste**:
/// the alternative -- keeping the first 4096 bytes of somebody's file -- is a
/// draft the user did not type and would have no reason to re-read.
pub(crate) const PASTE_REFUSED: &str =
    "that paste is larger than the 4 KiB an amendment carries; nothing was taken";

/// Which of the two drafts a choice owns, or nothing when it owns none.
///
/// Keyed on the **answer** rather than on the index, so that reordering
/// `super::approval`'s `CHOICES` cannot quietly give `Always` a draft: what is
/// ineligible is the answer that buys the session, not the middle row.
fn side(choice: usize) -> Option<usize> {
    match super::approval::choice(choice)? {
        ApprovalAnswer::Once => Some(0),
        ApprovalAnswer::Deny => Some(1),
        ApprovalAnswer::Always => None,
    }
}

/// One side's amendment: its text, its own undo history, and its own kill slot.
///
/// Per draft, not per panel: two drafts that shared a history would let an undo
/// typed at the deny side pull back a delta recorded on the allow side.
struct Draft {
    editor: Editor,
    history: EditHistory,
}

impl Default for Draft {
    fn default() -> Self {
        Self {
            editor: Editor::new(),
            history: EditHistory::new(),
        }
    }
}

impl Draft {
    /// Whether this side has anything to say.
    fn empty(&self) -> bool {
        self.editor.is_empty()
    }

    /// Puts `text` in at the caret and records the delta, if the budget admits
    /// it whole.
    ///
    /// **Whole or not at all**, which is the same rule the paste follows and
    /// for the same reason: half of a pasted line is text the user did not
    /// write, and a draft that silently swallowed the tail of a yank would be a
    /// sentence they cannot check by reading.
    fn insert(&mut self, text: &str) {
        if text.is_empty() || !fits(self.editor.text().len(), text.len()) {
            return;
        }
        if let Some(delta) = self.editor.insert(text) {
            self.history.record(delta);
        }
    }

    /// One editing key, on a draft `cols` cells wide.
    ///
    /// The five the buffer does not answer are answered here; everything else
    /// is `Editor::apply`'s, and its delta is recorded so that an undo has
    /// something to take back.
    fn apply(&mut self, action: Action, cols: u16) {
        match action {
            // The history's, not the buffer's. The two fields are split at the
            // call site, which is two disjoint borrows and what keeps the
            // transfer atomic (`super::edit_history::EditHistory::undo`).
            Action::Undo => {
                let Self { editor, history } = self;
                history.undo(|delta| editor.revert(delta));
            }
            Action::Redo => {
                let Self { editor, history } = self;
                history.redo(|delta| editor.replay(delta));
            }
            // Owned before the mutable borrow: `killed()` borrows the history
            // immutably and `insert` needs the editor mutably. The spans are
            // discarded, because a draft carries no entities -- there is
            // nothing in it a block could be a span *of*.
            Action::Yank => {
                let killed = self.history.killed().map(|(text, _)| text.to_owned());
                if let Some(text) = killed {
                    self.insert(&text);
                }
            }
            // Everything the buffer really answers, kills included: those are
            // `Editor::apply`'s own and they record normally, which is what
            // keeps `C-y` yanking the last thing that was really killed.
            other => {
                if let Some(delta) = self.editor.apply(other, cols) {
                    self.history.record(delta);
                }
            }
        }
    }

    /// The rows this draft would paint at `cols`, and the caret inside them.
    ///
    /// A window rather than the whole text ([`MAX_DRAFT_ROWS`]), placed on the
    /// caret's row by the same helper the composer's window uses -- so a draft
    /// longer than its rows shows the end the user is typing at rather than the
    /// beginning they have scrolled past.
    fn viewport(&self, cols: u16) -> (Vec<String>, (usize, u16)) {
        let painted = self.editor.rows(cols);
        let (row, column) = self.editor.point(cols);
        let window = super::editor::window(
            painted.len().max(1),
            row,
            u16::try_from(MAX_DRAFT_ROWS).unwrap_or(u16::MAX),
        );
        let rows: Vec<String> = if painted.is_empty() {
            // An open draft with nothing in it still owns a row: the caret has
            // to be somewhere, and a caret left on the choice above would say
            // the next keystroke answers the question.
            vec![String::new()]
        } else {
            painted[window.start.min(painted.len())..window.end.min(painted.len())].to_vec()
        };
        (rows, (row.saturating_sub(window.start), column))
    }
}

/// Whether `more` bytes fit a draft that already holds `held`.
fn fits(held: usize, more: usize) -> bool {
    held.saturating_add(more) <= MAX_FEEDBACK_BYTES
}

/// Bytes arriving between the bracketed-paste markers, for one draft.
///
/// Bounded at [`MAX_FEEDBACK_BYTES`], which is also the draft's own cap,
/// because a feedback string has one budget rather than two. Refused
/// **atomically**: an oversized paste inserts nothing and the panel says so.
#[derive(Default)]
struct DraftPaste {
    buffer: Vec<u8>,
    refused: bool,
}

impl DraftPaste {
    fn begin(&mut self) {
        self.buffer.clear();
        self.refused = false;
    }

    /// One content byte.
    ///
    /// Filtered by `super::paste::accepted`, which is reused rather than
    /// restated: which bytes of a bracketed paste are content is one fact
    /// (`paste_framing.zig:112-135`), and a second answer here would let a
    /// sequence the composer refuses into an amendment.
    fn byte(&mut self, byte: u8) {
        if !super::paste::accepted(byte) {
            return;
        }
        if self.buffer.len() >= MAX_FEEDBACK_BYTES {
            self.refused = true;
            return;
        }
        self.buffer.push(byte);
    }

    /// The finished text, or `None` when it was refused.
    ///
    /// `from_utf8_lossy` **before** the length check, because that is where the
    /// decoded size is first known: a non-UTF-8 byte becomes a three-byte
    /// replacement scalar, so the encoded bound is not the decoded one and a
    /// paste that fitted as bytes can overrun as text.
    fn finish(&mut self, held: usize) -> Option<String> {
        let bytes = std::mem::take(&mut self.buffer);
        if std::mem::take(&mut self.refused) {
            return None;
        }
        let text = String::from_utf8_lossy(&bytes).into_owned();
        fits(held, text.len()).then_some(text)
    }
}

/// One draft's rows as the panel will paint them, and where the caret is.
pub(crate) struct Block {
    /// Which of `super::approval`'s choices these rows belong under.
    pub choice: usize,
    /// The rows themselves, unindented: the surface owns its own indent.
    pub rows: Vec<String>,
    /// The caret's row inside [`Self::rows`] and the cells to its left, for the
    /// draft that has the keys. `None` on a draft that is merely being shown.
    pub caret: Option<(usize, u16)>,
}

/// The two drafts of one question, and which of them has the keys.
///
/// There is deliberately **no** slot here that outlives a question: `Drafts`
/// belongs to a `Panel` or an `ApprovalScreen`, both of which die with the
/// answer, so a sentence typed at one question is structurally incapable of
/// reaching the next one.
#[derive(Default)]
pub(crate) struct Drafts {
    sides: [Draft; 2],
    /// The choice whose draft has the keys, while one does.
    active: Option<usize>,
    paste: DraftPaste,
    /// What the last keystroke has to be reported to the user, once.
    notice: Option<&'static str>,
}

/// A manual `Debug`, because [`Editor`] has none and the band's slot is a
/// `Debug` enum (`super::shell::Slot`). What is printed is what a failure would
/// be about -- which side is open and what each holds -- rather than the editor
/// machinery behind them.
impl std::fmt::Debug for Drafts {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("Drafts")
            .field("active", &self.active)
            .field("allow", &self.sides[0].editor.text())
            .field("deny", &self.sides[1].editor.text())
            .finish()
    }
}

impl Drafts {
    /// Whether a draft has the keys.
    pub(crate) fn editing(&self) -> bool {
        self.active.is_some()
    }

    /// Whether `choice`'s draft has them.
    ///
    /// `#[cfg(test)]` for the reason `super::edit_history`'s `killed_text` is:
    /// production reads this through [`Self::blocks`], which is where the fact
    /// is needed, and a second public reader existing only for a test is a
    /// surface nothing maintains.
    #[cfg(test)]
    pub(crate) fn editing_at(&self, choice: usize) -> bool {
        self.active == Some(choice)
    }

    /// Opens `choice`'s draft, or reports that it has none.
    ///
    /// `None` is the answer for `Always`, and the caller's cue to do what Tab
    /// has always done: cycle to the next choice.
    pub(crate) fn enter(&mut self, choice: usize) -> Option<usize> {
        side(choice)?;
        self.active = Some(choice);
        Some(choice)
    }

    /// Ends editing and keeps **both** buffers.
    ///
    /// What arrow navigation does (`approval_decision.zig:214-225`: `moveChoice`
    /// updates `selected` and then calls `amendment.clearActive()`). Only a
    /// submit and the two refusals dispose of the text.
    pub(crate) fn leave(&mut self) {
        self.active = None;
    }

    /// Throws both drafts away, said and unsaid.
    ///
    /// Escape's and the interrupt's. A user bailing out of a panel did not
    /// choose to say the sentence that happened to be in the deny draft.
    pub(crate) fn discard(&mut self) {
        *self = Self::default();
    }

    /// The sentence belonging to `answer`, taken; every other draft dropped.
    ///
    /// Blank is absent: an empty or whitespace-only draft produces `None`, so
    /// no empty user message reaches the wire.
    pub(crate) fn submit(&mut self, answer: ApprovalAnswer) -> Option<String> {
        let taken = match answer {
            ApprovalAnswer::Once => Some(0),
            ApprovalAnswer::Deny => Some(1),
            ApprovalAnswer::Always => None,
        }
        .map(|side| self.sides[side].editor.text().to_string())
        .filter(|text| !text.trim().is_empty());
        self.discard();
        taken
    }

    /// What `choice`'s draft holds right now, for a reader that is not
    /// submitting.
    ///
    /// `#[cfg(test)]`: production takes a draft exactly once, at
    /// [`Self::submit`], and a getter that let some other path read a draft
    /// without consuming it is a second way for a sentence to travel.
    #[cfg(test)]
    pub(crate) fn text(&self, choice: usize) -> &str {
        match side(choice) {
            Some(side) => self.sides[side].editor.text(),
            None => "",
        }
    }

    /// One typed character, while a draft has the keys.
    ///
    /// Refused silently at the budget, exactly as the composer refuses one
    /// (`super::shell::Shell::type_character`): a keystroke that changed
    /// nothing says nothing.
    pub(crate) fn typed(&mut self, character: char) {
        let Some(draft) = self.open() else {
            return;
        };
        let mut encoded = [0u8; 4];
        let typed = character.encode_utf8(&mut encoded).to_string();
        draft.insert(&typed);
    }

    /// One editing key, while a draft has the keys.
    pub(crate) fn edit(&mut self, action: Action, cols: u16) {
        // Split so that the paste assembler and the open draft are two disjoint
        // borrows: the paste is not part of a draft until it is finished.
        let Self {
            sides,
            active,
            paste,
            notice,
        } = self;
        // No draft has the keys, so this key is not a draft's.
        let Some(side) = active.and_then(side) else {
            return;
        };
        let draft = &mut sides[side];
        match action {
            Action::PasteStart => paste.begin(),
            Action::PasteEnd => match paste.finish(draft.editor.text().len()) {
                Some(text) => {
                    draft.insert(&text);
                    *notice = None;
                }
                // Atomic: nothing was inserted, and the panel says so rather
                // than leaving the user to notice the absence.
                None => *notice = Some(PASTE_REFUSED),
            },
            other => draft.apply(other, cols),
        }
    }

    /// One raw byte of a bracketed paste, while a draft has the keys.
    ///
    /// Separate from the composer's byte route on purpose: that one feeds
    /// `super::paste::Paste` and would collapse a large paste into an entity.
    pub(crate) fn paste_byte(&mut self, byte: u8) {
        if self.active.is_none() {
            return;
        }
        self.paste.byte(byte);
    }

    /// What the panel owes the user about their last keystroke, taken once.
    pub(crate) fn take_notice(&mut self) -> Option<&'static str> {
        self.notice.take()
    }

    /// The rows both drafts would paint at `cols`, in choice order.
    ///
    /// A draft is shown while it has the keys **or** while it holds text: a
    /// sentence the user typed and then navigated away from still travels if
    /// they submit that choice, so hiding it would be sending something they
    /// cannot read back.
    pub(crate) fn blocks(&self, cols: u16) -> Vec<Block> {
        let mut blocks = Vec::new();
        for choice in 0..super::approval::CHOICE_COUNT {
            let Some(side) = side(choice) else {
                continue;
            };
            let draft = &self.sides[side];
            let open = self.active == Some(choice);
            if !open && draft.empty() {
                continue;
            }
            let (rows, caret) = draft.viewport(cols);
            blocks.push(Block {
                choice,
                rows,
                caret: open.then_some(caret),
            });
        }
        blocks
    }

    /// The draft that has the keys, mutably.
    fn open(&mut self) -> Option<&mut Draft> {
        let side = side(self.active?)?;
        Some(&mut self.sides[side])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The allow side, which is choice 0 (`super::approval`'s `CHOICES`).
    const ALLOW: usize = 0;
    /// The deny side, which is choice 2.
    const DENY: usize = 2;
    /// The width every case drives at, which is an eighty-column panel's inner
    /// width (`super::approval::draft_cols`).
    const COLS: u16 = 78;

    /// A `Drafts` with `text` typed into `choice`'s draft.
    fn typed_into(choice: usize, text: &str) -> Drafts {
        let mut drafts = Drafts::default();
        drafts.enter(choice).expect("an eligible choice");
        for character in text.chars() {
            drafts.typed(character);
        }
        drafts
    }

    /// Where the caret is inside the open draft: its row, and the cells left of
    /// it.
    fn caret(drafts: &Drafts) -> (usize, u16) {
        drafts
            .blocks(COLS)
            .into_iter()
            .find_map(|block| block.caret)
            .expect("a draft has the keys")
    }

    #[test]
    fn undo_redo_and_yank_work_inside_a_draft() {
        // **Asserted on real behavior**, never by comparing against
        // `Editor::apply`: that returns `None` for all three
        // (`super::super::editor`'s own arm), so a comparison would pass on a
        // draft that did nothing at all. What is checked is the text and the
        // caret after each key, which is what the user sees.
        let mut drafts = typed_into(ALLOW, "hello world");
        assert_eq!(drafts.text(ALLOW), "hello world");
        assert_eq!(caret(&drafts), (0, 11));
        assert!(
            drafts.editing_at(ALLOW),
            "the allow side does not have the keys"
        );
        assert!(!drafts.editing_at(DENY), "both sides have the keys at once");

        drafts.edit(Action::Home, COLS);
        assert_eq!(caret(&drafts), (0, 0), "Home did not reach the left edge");

        drafts.edit(Action::KillToEnd, COLS);
        assert_eq!(drafts.text(ALLOW), "", "the kill took nothing");
        assert_eq!(caret(&drafts), (0, 0));

        drafts.edit(Action::Undo, COLS);
        assert_eq!(
            drafts.text(ALLOW),
            "hello world",
            "an undo inside a draft did nothing, so the draft has no history"
        );

        drafts.edit(Action::Redo, COLS);
        assert_eq!(drafts.text(ALLOW), "", "a redo inside a draft did nothing");

        drafts.edit(Action::Yank, COLS);
        assert_eq!(
            drafts.text(ALLOW),
            "hello world",
            "the kill slot is the draft's own, and `C-y` did not read it"
        );
        assert_eq!(caret(&drafts), (0, 11), "the yank left the caret behind it");

        // And the histories are **per draft**: an undo typed at the deny side
        // cannot pull back a delta recorded at the allow side.
        drafts.leave();
        assert!(!drafts.editing(), "leaving kept the keys");
        drafts.enter(DENY).expect("an eligible choice");
        assert!(drafts.editing_at(DENY) && !drafts.editing_at(ALLOW));
        drafts.typed('n');
        drafts.edit(Action::Undo, COLS);
        assert_eq!(drafts.text(DENY), "", "the deny side undid its own edit");
        assert_eq!(
            drafts.text(ALLOW),
            "hello world",
            "an undo at one side reached into the other side's history"
        );
    }

    #[test]
    fn a_bracketed_paste_becomes_draft_text_and_never_an_entity() {
        // The composer collapses a large paste into a block and shows a summary
        // (`super::super::paste::Pasted::Collapsed`). A feedback string that
        // reached the model as `[Pasted text #1, 40 lines]` would be a summary
        // standing in for text the user believes they sent, so a draft assembles
        // its own bytes and mints nothing.
        let pasted = "the diff is right but rename the flag\nand keep the old name as an alias";
        let mut drafts = Drafts::default();
        drafts.enter(ALLOW).expect("an eligible choice");
        drafts.edit(Action::PasteStart, COLS);
        for byte in pasted.as_bytes() {
            drafts.paste_byte(*byte);
        }
        drafts.edit(Action::PasteEnd, COLS);

        assert_eq!(drafts.text(ALLOW), pasted, "the paste is not the draft");
        assert_eq!(
            drafts.blocks(COLS)[0].caret,
            Some((1, 33)),
            "the caret is not past the pasted text"
        );
        assert_eq!(
            drafts.sides[0].editor.entities().len(),
            0,
            "a draft minted a block, so the model would read a summary"
        );
        assert_eq!(
            drafts.take_notice(),
            None,
            "an accepted paste said something"
        );
        assert_eq!(
            drafts.submit(ApprovalAnswer::Once),
            Some(pasted.to_string()),
            "what travels is a summary rather than the text"
        );
    }

    #[test]
    fn an_oversized_paste_is_refused_atomically_with_a_visible_notice() {
        // Atomic, because the alternative -- the first 4096 bytes of somebody's
        // file, silently -- is a draft the user did not type and has no reason
        // to re-read.
        let mut drafts = typed_into(ALLOW, "keep this");
        drafts.edit(Action::PasteStart, COLS);
        for _ in 0..=MAX_FEEDBACK_BYTES {
            drafts.paste_byte(b'x');
        }
        drafts.edit(Action::PasteEnd, COLS);

        assert_eq!(
            drafts.text(ALLOW),
            "keep this",
            "the draft was truncated rather than left alone"
        );
        assert_eq!(
            drafts.take_notice(),
            Some(PASTE_REFUSED),
            "the refusal was silent"
        );
        assert_eq!(drafts.take_notice(), None, "the notice was said twice");

        // And the assembler is usable again: a refusal is about one paste.
        drafts.edit(Action::PasteStart, COLS);
        for byte in b" and this" {
            drafts.paste_byte(*byte);
        }
        drafts.edit(Action::PasteEnd, COLS);
        assert_eq!(
            drafts.text(ALLOW),
            "keep this and this",
            "a refusal poisoned the next paste"
        );
    }

    #[test]
    fn a_multibyte_paste_is_not_cut_mid_character() {
        // Two claims, and they are about two different bounds. The first: a
        // paste whose byte count crosses the cap in the middle of a multi-byte
        // scalar is refused whole, so no half-scalar reaches the draft. The
        // second: the check is on the **decoded** size, because
        // `from_utf8_lossy` turns one non-UTF-8 byte into a three-byte
        // replacement scalar and a paste that fitted as bytes can overrun as
        // text.
        let mut drafts = Drafts::default();
        drafts.enter(ALLOW).expect("an eligible choice");
        drafts.edit(Action::PasteStart, COLS);
        // Three-byte scalars, so the cap at 4096 falls inside one of them:
        // 4096 = 3 * 1365 + 1.
        for _ in 0..1366 {
            for byte in "가".as_bytes() {
                drafts.paste_byte(*byte);
            }
        }
        drafts.edit(Action::PasteEnd, COLS);
        assert_eq!(
            drafts.text(ALLOW),
            "",
            "an oversized paste reached the draft"
        );
        assert!(
            !drafts.text(ALLOW).contains('\u{fffd}'),
            "a scalar was cut in half and the remains were shown as a replacement"
        );
        assert_eq!(drafts.take_notice(), Some(PASTE_REFUSED));

        // The decoded bound. Every byte here is accepted and within the encoded
        // cap, and every one of them is invalid UTF-8 -- so the text is three
        // times the bytes and the paste must be refused on what it decodes to.
        let mut drafts = Drafts::default();
        drafts.enter(ALLOW).expect("an eligible choice");
        drafts.edit(Action::PasteStart, COLS);
        for _ in 0..2000 {
            drafts.paste_byte(0xff);
        }
        drafts.edit(Action::PasteEnd, COLS);
        assert_eq!(
            drafts.text(ALLOW),
            "",
            "2000 bytes decoded to 6000 and the draft took them anyway"
        );
        assert_eq!(drafts.take_notice(), Some(PASTE_REFUSED));
    }

    #[test]
    fn a_draft_is_bounded_in_rows_and_windows_on_the_caret() {
        // The panel's rows come out of the user's document, so a draft that
        // grew without bound would push the question off the screen to show the
        // sentence about it. What a long draft gets instead is a window on the
        // row the caret is on ([`MAX_DRAFT_ROWS`]).
        let mut drafts = typed_into(ALLOW, &"x".repeat(usize::from(COLS) * 6));
        let blocks = drafts.blocks(COLS);
        assert_eq!(blocks.len(), 1, "{blocks:?}", blocks = blocks.len());
        assert_eq!(
            blocks[0].rows.len(),
            MAX_DRAFT_ROWS,
            "a long draft took more rows than it is allowed"
        );
        let (row, _) = blocks[0].caret.expect("a draft has the keys");
        assert_eq!(
            row,
            MAX_DRAFT_ROWS - 1,
            "the window is not on the row being typed at"
        );

        // Home walks the caret to the first row, and the window follows it.
        drafts.edit(Action::Home, COLS);
        for _ in 0..6 {
            drafts.edit(Action::Left, COLS);
        }
        let blocks = drafts.blocks(COLS);
        assert_eq!(blocks[0].caret.expect("the keys").0, 0);
    }

    #[test]
    fn a_blank_draft_is_an_absent_one_and_the_other_side_never_travels() {
        // Blank is absent: an empty user message on the wire would spend a turn
        // saying nothing.
        let mut drafts = typed_into(ALLOW, "   \t ");
        assert_eq!(drafts.submit(ApprovalAnswer::Once), None);

        // And only the submitted side's sentence travels. A user who typed a
        // reason for refusing and then allowed does not send the refusal reason.
        let mut drafts = typed_into(DENY, "too destructive");
        drafts.leave();
        drafts.enter(ALLOW).expect("an eligible choice");
        drafts.typed('y');
        assert_eq!(drafts.submit(ApprovalAnswer::Once), Some("y".to_string()));

        let mut drafts = typed_into(DENY, "too destructive");
        assert_eq!(
            drafts.submit(ApprovalAnswer::Always),
            None,
            "`Always` carries no amendment, and it took one"
        );
    }

    #[test]
    fn the_middle_choice_has_no_draft_and_a_closed_panel_takes_no_keys() {
        // `Always` buys the rest of the session and its scope is keyed by tool
        // and target; an amendment attached to it would read as a condition on
        // a grant that cannot carry one.
        let mut drafts = Drafts::default();
        assert_eq!(drafts.enter(1), None, "`Always` opened a draft");
        assert!(!drafts.editing());

        // And with nothing open, the draft's own keys reach nothing: a typed
        // character, an editing key and a paste byte all change nothing.
        drafts.typed('x');
        drafts.edit(Action::Backspace, COLS);
        drafts.edit(Action::PasteStart, COLS);
        drafts.paste_byte(b'x');
        drafts.edit(Action::PasteEnd, COLS);
        assert_eq!(drafts.text(ALLOW), "");
        assert_eq!(drafts.text(DENY), "");
        assert!(drafts.blocks(COLS).is_empty());
    }
}
