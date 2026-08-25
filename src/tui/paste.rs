//! What arrives between the bracketed-paste markers, and what becomes of it.
//!
//! This is a **correctness** module rather than a feature one. Without a frame
//! every byte of a pasted stack trace is a keystroke: each embedded newline
//! submits the composer, so one paste becomes four prompts the user never
//! typed and cannot take back; a `0x03` in the text cancels the running turn;
//! and an `ESC [ A` is obeyed as an arrow key. The mode set enables `?2004h`
//! (`super::term`) and the decoder already turns what follows into
//! [`Input::PasteByte`](super::input::Input::PasteByte) between
//! [`Action::PasteStart`](super::input::Action::PasteStart) and
//! `Action::PasteEnd` (`super::input`); this module is the state that sits
//! behind them.
//!
//! Four rules, and each one is there because of a specific way a paste can hurt
//! a session:
//!
//! * **A filter, not a decoder.** [`accepted`] keeps CR, LF, Tab and every
//!   printing byte and drops the rest (`paste_framing.zig:112-135`). The bytes
//!   it drops are the ones a terminal or a transcript would *obey* -- an ESC
//!   above all -- and dropping them here is what makes "a paste is content"
//!   true of the composer's buffer as well as of the input path. A `0x03`
//!   between the markers never reaches [`super::gesture`], because the decoder
//!   never offered it as a key; and it never reaches the model either, because
//!   this filter drops it.
//! * **A budget.** [`MAX_PASTE_BYTES`] bounds what one paste may hold. The
//!   bytes past it are not buffered -- an unbounded buffer fed by a terminal is
//!   a memory hole with a keyboard on it -- and the paste is **refused whole**
//!   rather than half-taken: [`Paste::refused`] is what the shell asks, and a
//!   refused paste registers no block, so there is no summary that expands into
//!   a truncated half of what the user pasted.
//! * **A collapse.** More than [`COLLAPSE_ABOVE`] codepoints becomes
//!   `[Pasted text #N, M lines]` in the composer (`pasted_blocks.zig:7`) and
//!   the text is kept here under that summary. Two reasons, and the second is
//!   the load-bearing one: 1800 codepoints painted into a band is a band that
//!   has eaten the screen, and every later keystroke re-wraps whatever the
//!   composer holds -- a megabyte in the composer makes every subsequent
//!   keypress cost milliseconds.
//! * **An expansion.** [`Paste::expand`] puts the text back at submit time, so
//!   what the model receives is the paste rather than a description of it
//!   (`pasted_blocks.zig:53-63`). The summary is what the *screen* holds; it is
//!   never what is sent. "The paste" is precisely the text this module made of
//!   it -- filtered, decoded as UTF-8 with the bytes that are not UTF-8
//!   replaced, and with its line breaks normalized -- and not the bytes the
//!   terminal sent; those three are the only things that happen to it, and each
//!   one is above.
//!
//! # What a line break is
//!
//! A terminal sends CR for Return, so a real multi-line paste arrives as bare
//! carriage returns rather than as newlines. They are normalized here, by
//! [`super::transcript::normalize`] and not by a second copy of the same rule,
//! because a bare CR left in the composer would be a line break to the person
//! who pasted it, no break at all to [`super::wrap`], and a cursor rewind to
//! the terminal -- three answers to one question.
//!
//! # What this is deliberately not
//!
//! The summary is text in the composer, not an atomic entity: backspacing into
//! one edits the characters of the summary rather than removing the block,
//! there is no span shifting as the text around it is edited, no renumbering,
//! and no undo boundary. That is Phase 2 (plan item 16), and the trade is
//! stated rather than hidden: a Phase-1 user who backspaces into a summary gets
//! an edit that looks slightly wrong, and a Phase-1 user with no framing at all
//! gets four prompts they never sent.

use super::transcript;

/// The most codepoints a paste may put in the composer before it is shown as a
/// summary instead (`pasted_blocks.zig:7`).
pub(crate) const COLLAPSE_ABOVE: usize = 1000;

/// The most bytes one paste may hold.
///
/// The same number as the composer's own budget
/// ([`super::editor::MAX_COMPOSER_BYTES`], `paste_framing.zig:16-35`) and for
/// the same reason: a paste is the only way text arrives faster than a person
/// types, so it is the only thing either budget is really about.
pub(crate) const MAX_PASTE_BYTES: usize = 8 * 1024 * 1024;

/// The paste being framed, and the blocks the composer's summaries name.
#[derive(Debug, Default)]
pub(crate) struct Paste {
    /// The bytes of the paste that is arriving, filtered as they arrive.
    ///
    /// Bytes rather than a `String`: a paste is whatever the terminal sends,
    /// and [`accepted`] passes every byte above `0x7f` without knowing whether
    /// the sequence they form is a scalar. It becomes text once, at
    /// [`Paste::finish`], where an incomplete scalar can be replaced rather
    /// than dropping the rest of the paste with it.
    buffer: Vec<u8>,
    /// Whether the paste that is arriving has already overrun the budget.
    ///
    /// Set the moment a byte does not fit and cleared by the next
    /// [`Paste::begin`], so the shell can ask it about the paste it has just
    /// finished.
    refused: bool,
    /// The collapsed blocks a submitted line may still name, oldest first.
    ///
    /// Emptied when the composer is (`super::shell::Shell::take_draft`): a
    /// summary that outlived the draft it was in would expand a *later* prompt
    /// that happened to contain the same words.
    blocks: Vec<Block>,
    /// How many bytes the composer already held when this paste began.
    ///
    /// Captured at [`Paste::begin`] and good for the whole frame, because every
    /// byte between the markers comes here: nothing can edit the composer while
    /// a paste is arriving, so the draft this paste is landing in cannot change
    /// under it.
    draft: usize,
    /// How many bytes of text those blocks are holding.
    ///
    /// **The budget's real subject.** What a draft *shows* for a collapsed
    /// paste is 25 bytes, so a composer with two short lines in it can be
    /// holding two whole pastes -- and a budget that measured only the paste
    /// arriving would bound one of an unbounded number of them. Kept as a
    /// running sum rather than re-derived, so the check is a comparison rather
    /// than a walk of every block on every byte.
    retained: usize,
    /// The id the last collapsed block took.
    ///
    /// Never reset, so two pastes in one session are `#1` and `#2` even when
    /// the first was submitted and forgotten in between. Renumbering is
    /// Phase 2's, and a session that renumbered under the user would move the
    /// name of a block they can see.
    next: u32,
}

/// One collapsed paste: what the composer shows, and what it stands for.
#[derive(Debug)]
struct Block {
    summary: String,
    text: String,
    /// Which copy of [`summary`](Self::summary) in the draft is *this* block's
    /// placeholder, counted from zero.
    ///
    /// **The whole of placeholder identity.** A summary is ordinary text on a
    /// screen the user can read and retype, so a draft can hold several copies
    /// of one -- and a block that stood for all of them would send its paste
    /// once per copy, while a block that stood for the first would be claimed
    /// by words that were in the draft before anything was pasted. So the copy
    /// is recorded at the moment the paste puts it there
    /// (`super::shell::Shell::pasted`), which is the one moment at which it is
    /// known exactly.
    occurrence: usize,
}

/// What one finished paste is.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Pasted {
    /// Small enough to be text in the composer, exactly as pasted.
    Inline(String),
    /// Too large for that: the composer gets the summary, and the text is kept
    /// here until it is submitted or the draft is thrown away.
    Collapsed { summary: String, id: u32 },
}

impl Paste {
    /// A session that has already used `next` numbers.
    ///
    /// Only the tests build one: the id space is four billion pastes deep, and
    /// the case worth proving -- what two blocks that ran out of numbers do --
    /// is not reachable by pasting four billion times.
    #[cfg(test)]
    fn with_next(next: u32) -> Self {
        Self {
            next,
            ..Self::default()
        }
    }

    /// A paste is starting: whatever the last one left is gone.
    ///
    /// Cleared here rather than at [`Self::finish`] because a paste can be
    /// abandoned -- a session that put an approval panel up mid-paste swallows
    /// the end marker with everything else the panel does not bind -- and the
    /// next `begin` is the one moment at which the buffer is certainly stale.
    pub(crate) fn begin(&mut self, draft: usize) {
        self.buffer.clear();
        self.refused = false;
        self.draft = draft;
    }

    /// Whether `more` bytes may join a draft of `draft` bytes.
    ///
    /// **One budget, and it is the prompt's rather than the screen's.** A
    /// collapsed block shows 25 bytes and stands for as much as the whole cap,
    /// so a draft bounded on its own and a set of blocks bounded on their own
    /// are two ceilings that add up to twice the number either of them names --
    /// and what leaves this session is their sum. `draft + retained` is an
    /// upper bound on what [`Self::expand`] can produce, so bounding it bounds
    /// the prompt.
    ///
    /// Conservative by the length of the summaries -- the draft holds a name
    /// where the prompt holds the text, and this counts both -- which is some
    /// 25 bytes per block against a budget of eight million, and errs towards
    /// refusing a paste rather than sending one that is too big.
    pub(crate) fn admits(&self, draft: usize, more: usize) -> bool {
        draft.saturating_add(self.retained).saturating_add(more) <= MAX_PASTE_BYTES
    }

    /// One byte from between the markers.
    pub(crate) fn byte(&mut self, byte: u8) {
        if !accepted(byte) {
            return;
        }
        // **Not buffered and then trimmed.** The budget is about memory as much
        // as about the prompt: a terminal can hand this function bytes for as
        // long as it likes, and a buffer that grew first would already have
        // paid for the paste it is about to refuse.
        if !self.admits(self.draft, self.buffer.len().saturating_add(1)) {
            self.refused = true;
            return;
        }
        self.buffer.push(byte);
    }

    /// The paste is over: what the composer should be given.
    ///
    /// A refused paste still comes back as a `Collapsed` -- it is one, by every
    /// measure this type has -- but nothing is registered under its id, so the
    /// summary expands to itself. The caller asks [`Self::refused`] and puts
    /// nothing in the composer at all; the two together are what "refused
    /// rather than truncated" means.
    pub(crate) fn finish(&mut self) -> Pasted {
        let bytes = std::mem::take(&mut self.buffer);
        // Lossy rather than strict: a paste cut off mid-scalar by the budget,
        // or a terminal sending bytes in an encoding this session does not
        // read, must not lose the whole paste to one bad byte.
        let text = transcript::normalize(&String::from_utf8_lossy(&bytes));
        // **Measured again, now that it is text.** The bytes were counted as
        // they arrived, but `from_utf8_lossy` turns every byte that is not
        // UTF-8 into a three-byte replacement scalar -- so a paste of bytes in
        // some other encoding is three times the size once it is decoded, and
        // what a block holds and a prompt carries is the decoded form. Checked
        // here rather than by refusing malformed input, which would throw a
        // whole log file away for one stray byte the user cannot see: the
        // contract is a size, and this is where the size is known.
        //
        // **Before** the inline branch, because inline text goes into the draft
        // and is exactly as much of the prompt as a block is.
        if !self.admits(self.draft, text.len()) {
            self.refused = true;
        }
        if !self.refused && text.chars().count() <= COLLAPSE_ABOVE {
            return Pasted::Inline(text);
        }
        // Counted in **lines of text**, so a block ending in a newline is not
        // reported as having one more line than a reader can see.
        let lines = text.lines().count();
        // The number is spent only if the block is kept, so a refused paste
        // does not make the next one `#2`.
        let id = self.next.saturating_add(1);
        let summary = summary(id, lines);
        if !self.refused {
            self.next = id;
            self.retained = self.retained.saturating_add(text.len());
            self.blocks.push(Block {
                summary: summary.clone(),
                text,
                // Corrected by [`Self::placed`] the moment the composer takes
                // it. Zero until then, which is what it is for every draft that
                // did not already contain those words.
                occurrence: 0,
            });
        }
        Pasted::Collapsed { summary, id }
    }

    /// The summary [`Self::finish`] just handed back really went into the
    /// draft, as the `occurrence`-th copy of itself.
    ///
    /// Called by the composer's side, which is the only side that knows: it has
    /// the draft and the caret, so it can count the copies of those words that
    /// were already in front of the insertion. Anything after the insertion is
    /// a later copy and is not this block's.
    pub(crate) fn placed(&mut self, occurrence: usize) {
        if let Some(block) = self.blocks.last_mut() {
            block.occurrence = occurrence;
        }
    }

    /// Whether the paste that just finished overran the budget.
    ///
    /// Valid from [`Self::byte`] until the next [`Self::begin`], which is
    /// exactly the window in which the shell asks it.
    pub(crate) fn refused(&self) -> bool {
        self.refused
    }

    /// `submitted` with each block's **own** placeholder replaced by the text
    /// it stands for.
    ///
    /// Two properties, and both are about the fact that a summary is text a
    /// user can also write:
    ///
    /// * **Each block expands at most once**, at the copy of its name that the
    ///   paste itself put there ([`Block::occurrence`]). Replacing every
    ///   occurrence would send one paste as many times as its name appears --
    ///   and a draft gets a second copy of a name by nothing more exotic than
    ///   the user typing it, or pasting it back off the screen.
    /// * **What is put in is never looked at again.** Every position is found
    ///   in `submitted` before anything is spliced, so a block whose *text*
    ///   contains another block's name -- a pasted session transcript is the
    ///   ordinary way that happens -- cannot be expanded a second time.
    pub(crate) fn expand(&self, submitted: &str) -> String {
        if self.blocks.is_empty() {
            return submitted.to_string();
        }
        let mut found: Vec<(usize, &Block)> = self
            .blocks
            .iter()
            .filter_map(|block| {
                placeholder_at(submitted, &block.summary, block.occurrence).map(|at| (at, block))
            })
            .collect();
        found.sort_by_key(|(at, _)| *at);

        let mut out = String::with_capacity(submitted.len());
        let mut cut = 0usize;
        for (at, block) in found {
            // **Load-bearing, not decoration.** Two blocks usually have
            // different names, but `next` saturates: a session that pasted four
            // billion times gives every block after that the same id, and then
            // one placeholder is claimed by two blocks. The first one takes it;
            // without this the second would splice a backwards byte range and
            // panic.
            if at < cut {
                continue;
            }
            out.push_str(&submitted[cut..at]);
            out.push_str(&block.text);
            cut = at + block.summary.len();
        }
        out.push_str(&submitted[cut..]);
        out
    }

    /// The composer has been emptied, so the blocks its summaries named are
    /// dead.
    ///
    /// Not tidiness. A block that outlived its draft would be expanded into a
    /// **later** prompt that happened to contain the same summary text -- which
    /// a user can type by hand -- and it would hold the whole paste for the
    /// rest of the session.
    pub(crate) fn forget(&mut self) {
        self.blocks.clear();
        self.retained = 0;
    }
}

/// Whether a byte between the markers is content.
///
/// CR, LF, Tab and every printing byte, and nothing else
/// (`paste_framing.zig:112-135`). `0x7f` is out with the C0 bytes: it is a
/// delete, not a character.
///
/// Everything from `0x80` up passes without inspection, because at this point
/// there is no scalar to inspect -- the bytes of one UTF-8 character arrive one
/// at a time like any other, and judging them singly would cut every non-ASCII
/// paste into pieces.
pub(crate) fn accepted(byte: u8) -> bool {
    matches!(byte, b'\t' | b'\n' | b'\r' | 0x20..=0x7e | 0x80..=0xff)
}

/// Where the `occurrence`-th copy of `needle` begins in `text`, if there is
/// one.
///
/// Copies are counted the way a reader counts them -- left to right, and never
/// overlapping -- because the number was taken by counting the same way.
fn placeholder_at(text: &str, needle: &str, occurrence: usize) -> Option<usize> {
    text.match_indices(needle).nth(occurrence).map(|(at, _)| at)
}

/// What the composer shows in place of a collapsed block.
pub(crate) fn summary(id: u32, lines: usize) -> String {
    format!("[Pasted text #{id}, {lines} lines]")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paste(text: &str) -> Pasted {
        let mut paste = Paste::default();
        paste.begin(0);
        for byte in text.as_bytes() {
            paste.byte(*byte);
        }
        paste.finish()
    }

    #[test]
    fn the_filter_keeps_text_and_drops_control_bytes() {
        // paste_framing.zig:112-135: CR, LF, Tab and printables, nothing else.
        for byte in [b'\r', b'\n', b'\t', b'a', 0xc7] {
            assert!(accepted(byte), "{byte:#04x} was dropped");
        }
        for byte in [0x00, 0x03, 0x07, 0x1b, 0x7f] {
            assert!(!accepted(byte), "{byte:#04x} was kept");
        }
    }

    #[test]
    fn a_short_paste_lands_in_the_composer_verbatim() {
        assert_eq!(
            paste("line one\nline two"),
            Pasted::Inline("line one\nline two".to_string())
        );
    }

    #[test]
    fn control_bytes_inside_a_paste_are_content_that_never_became_keys() {
        // A `0x03` in pasted text must not cancel a turn, and an ESC must not
        // be decoded as a key: both are dropped by the filter, and neither
        // reaches the decoder because the frame bypasses it entirely.
        assert_eq!(paste("a\x03b\x1b[Ac"), Pasted::Inline("ab[Ac".to_string()));
    }

    #[test]
    fn a_paste_over_a_thousand_codepoints_collapses_and_round_trips() {
        let big = "x".repeat(1200) + "\nsecond line";
        let mut state = Paste::default();
        state.begin(0);
        for byte in big.as_bytes() {
            state.byte(*byte);
        }
        let pasted = state.finish();
        assert_eq!(
            pasted,
            Pasted::Collapsed {
                summary: "[Pasted text #1, 2 lines]".into(),
                id: 1
            }
        );
        // pasted_blocks.zig:53-63 -- what is submitted is what was pasted.
        assert_eq!(
            state.expand("see [Pasted text #1, 2 lines] please"),
            format!("see {big} please")
        );
    }

    #[test]
    fn the_collapse_begins_one_codepoint_past_the_threshold() {
        // The boundary itself, from both sides: a threshold read one off shows
        // a summary for a draft the user could have read, or 1001 codepoints in
        // a band.
        let inline = paste(&"x".repeat(COLLAPSE_ABOVE));
        assert_eq!(inline, Pasted::Inline("x".repeat(COLLAPSE_ABOVE)));
        assert!(matches!(
            paste(&"x".repeat(COLLAPSE_ABOVE + 1)),
            Pasted::Collapsed { .. }
        ));
    }

    #[test]
    fn the_budget_is_the_last_byte_that_fits_and_the_first_that_does_not() {
        // The boundary of the bound, so that "8 MiB" is the number rather than
        // approximately the number -- on the way in, again once the bytes are
        // text, and across the draft rather than across one paste.
        let mut state = Paste::default();
        state.begin(0);
        for _ in 0..MAX_PASTE_BYTES {
            state.byte(b'x');
        }
        assert!(
            !state.refused(),
            "a paste of exactly the budget was refused as it arrived"
        );
        let kept = state.finish();
        assert!(
            !state.refused(),
            "a paste of exactly the budget was refused once it was decoded"
        );
        assert!(matches!(kept, Pasted::Collapsed { .. }), "{kept:?}");

        // The budget is spent now, so the next paste's first byte is already
        // one too many.
        state.begin(0);
        state.byte(b'x');
        assert!(
            state.refused(),
            "a byte past the budget the draft is already holding was taken"
        );
    }

    #[test]
    fn a_paste_larger_than_the_budget_is_refused_rather_than_truncated() {
        let mut state = Paste::default();
        state.begin(0);
        for _ in 0..(MAX_PASTE_BYTES + 10) {
            state.byte(b'x');
        }
        match state.finish() {
            Pasted::Collapsed { .. } => {}
            other => panic!("{other:?}"),
        }
        assert!(state.expand("[Pasted text #1, 1 lines]").len() <= MAX_PASTE_BYTES);
    }

    #[test]
    fn a_pasted_carriage_return_is_the_line_break_it_looks_like() {
        // What a terminal really sends for Return inside a paste. Left alone it
        // would be a line break to the person who pasted it, no break at all to
        // `super::super::wrap`, and a cursor rewind to the terminal.
        assert_eq!(
            paste("one\r\ntwo\rthree"),
            Pasted::Inline("one\ntwo\nthree".to_string())
        );
    }

    #[test]
    fn a_refused_paste_registers_nothing_a_summary_could_expand_into() {
        // The other half of "refused rather than truncated". The bound in the
        // case above is satisfied by a block holding exactly the budget, which
        // is the truncation this asserts did not happen: the summary names no
        // block at all, so nothing can be sent in place of what did not fit.
        let mut state = Paste::default();
        state.begin(0);
        for _ in 0..(MAX_PASTE_BYTES + 10) {
            state.byte(b'x');
        }
        let pasted = state.finish();
        assert!(state.refused(), "a paste past the budget was taken whole");
        let Pasted::Collapsed { summary, .. } = pasted else {
            panic!("{pasted:?}");
        };
        assert_eq!(state.expand(&summary), summary);
    }

    #[test]
    fn the_budget_is_everything_the_draft_is_holding_rather_than_one_paste() {
        // Every collapsed block hides behind a 25-byte summary, so a draft that
        // looks like two short lines can be holding two whole pastes. A budget
        // that measured only the paste arriving would bound nothing at all.
        let half = MAX_PASTE_BYTES / 2 + 1;
        let mut state = Paste::default();
        state.begin(0);
        for _ in 0..half {
            state.byte(b'x');
        }
        let Pasted::Collapsed { summary: first, .. } = state.finish() else {
            panic!("half the budget did not collapse");
        };
        assert!(!state.refused(), "the first paste was already refused");

        state.begin(0);
        for _ in 0..half {
            state.byte(b'x');
        }
        let second = state.finish();
        assert!(
            state.refused(),
            "a second paste of half the budget fit alongside the first"
        );
        let Pasted::Collapsed {
            summary: second, ..
        } = second
        else {
            panic!("{second:?}");
        };
        assert_eq!(state.expand(&second), second, "the refused paste was kept");
        assert_ne!(
            state.expand(&first),
            first,
            "the paste that did fit was thrown away with the one that did not"
        );
    }

    #[test]
    fn a_paste_that_grows_when_it_is_decoded_is_measured_after_it_grew() {
        // A byte that is not UTF-8 becomes a three-byte replacement scalar, so
        // the text that is kept and sent can be three times the bytes that
        // arrived. The budget is about the text.
        let raw = MAX_PASTE_BYTES / 2;
        let mut state = Paste::default();
        state.begin(0);
        for _ in 0..raw {
            state.byte(0xff);
        }
        assert!(
            !state.refused(),
            "the raw bytes were over the budget on their own, so this case \
             proves nothing about the decoded ones"
        );

        let pasted = state.finish();
        assert!(
            state.refused(),
            "a paste that trebled in size when it was decoded was kept whole"
        );
        let Pasted::Collapsed { summary, .. } = pasted else {
            panic!("{pasted:?}");
        };
        assert_eq!(state.expand(&summary), summary);
    }

    #[test]
    fn a_paste_that_only_grows_past_the_budget_once_decoded_counts_the_draft_too() {
        // The two checks are not one check written twice. This paste's *bytes*
        // fit beside the draft and its *text* does not, which arrival cannot
        // know -- so the check that runs once it is text has to weigh the same
        // draft the first one did.
        let draft = 4_000_000;
        let mut state = Paste::default();
        state.begin(draft);
        for _ in 0..2_000_000 {
            state.byte(0xff);
        }
        assert!(
            !state.refused(),
            "the bytes did not fit beside the draft, so this case proves \
             nothing about the text they became"
        );

        let pasted = state.finish();
        assert!(
            state.refused(),
            "a paste that trebled as it was decoded was kept beside a 4 MB draft"
        );
        let Pasted::Collapsed { summary, .. } = pasted else {
            panic!("{pasted:?}");
        };
        assert_eq!(
            state.expand(&summary),
            summary,
            "the refused paste was kept"
        );
    }

    #[test]
    fn a_second_copy_of_a_summary_is_words_rather_than_a_second_block() {
        // A summary is ordinary text, so a draft can hold two of them -- typed,
        // or pasted from the screen. Exactly one of them is the placeholder,
        // and a block that expanded into both would send the paste twice.
        let block = "y".repeat(1200);
        let mut state = Paste::default();
        state.begin(0);
        for byte in block.as_bytes() {
            state.byte(*byte);
        }
        let Pasted::Collapsed { summary, .. } = state.finish() else {
            panic!("1200 codepoints did not collapse");
        };

        assert_eq!(
            state.expand(&format!("{summary} and {summary}")),
            format!("{block} and {summary}")
        );
    }

    #[test]
    fn only_the_copy_the_paste_put_there_stands_for_the_block() {
        // The draft already said those words once when the paste landed after
        // them, so the placeholder is the *second* copy -- and the third, typed
        // later, is words like the first.
        let block = "y".repeat(1200);
        let mut state = Paste::default();
        state.begin(0);
        for byte in block.as_bytes() {
            state.byte(*byte);
        }
        let Pasted::Collapsed { summary, .. } = state.finish() else {
            panic!("1200 codepoints did not collapse");
        };
        state.placed(1);

        assert_eq!(
            state.expand(&format!("{summary} {summary} {summary}")),
            format!("{summary} {block} {summary}")
        );
    }

    #[test]
    fn a_refused_paste_does_not_spend_the_number_the_next_one_uses() {
        // The user never saw `#1`, so a session that answered their first
        // successful paste with `#2` would be counting something they cannot
        // see.
        let mut state = Paste::default();
        state.begin(0);
        for _ in 0..(MAX_PASTE_BYTES / 2) {
            state.byte(0xff);
        }
        assert!(matches!(state.finish(), Pasted::Collapsed { .. }));
        assert!(
            state.refused(),
            "the paste was kept, so nothing was refused"
        );

        state.begin(0);
        for byte in "y".repeat(1200).as_bytes() {
            state.byte(*byte);
        }
        assert_eq!(
            state.finish(),
            Pasted::Collapsed {
                summary: "[Pasted text #1, 1 lines]".into(),
                id: 1
            }
        );
    }

    #[test]
    fn a_draft_that_was_sent_gives_its_budget_back() {
        // The budget belongs to the **draft**, not to the session. A user who
        // pastes half of it, sends that prompt and pastes again would otherwise
        // be refused by bytes that are already on their way to the model.
        let half = MAX_PASTE_BYTES / 2 + 1;
        let mut state = Paste::default();
        for round in 1..=2 {
            state.begin(0);
            for _ in 0..half {
                state.byte(b'x');
            }
            let pasted = state.finish();
            assert!(
                !state.refused(),
                "the paste in draft {round} was refused by a draft that had \
                 already been sent"
            );
            assert!(matches!(pasted, Pasted::Collapsed { .. }), "{pasted:?}");
            // What the shell does when the submitted composer is emptied.
            state.forget();
        }
    }

    #[test]
    fn two_summaries_can_never_name_the_same_run_of_a_draft() {
        // Half of why `expand` splices safely: a summary carries its own id
        // and contains exactly one `[`, so one can be found inside another
        // only when the two are the same string. The other half is that two
        // blocks CAN hold the same string once `next` saturates -- which is
        // the case below, and the reason the overlap guard is load-bearing.
        for first in [1u32, 2, 11, 100, u32::MAX] {
            for second in [1u32, 2, 11, 100, u32::MAX] {
                for lines in [1usize, 2, 10, 1000] {
                    let one = summary(first, lines);
                    let two = summary(second, lines);
                    assert_eq!(
                        one.matches('[').count(),
                        1,
                        "a summary that opened twice could start inside another"
                    );
                    assert_eq!(
                        one.contains(&two),
                        first == second,
                        "{one:?} and {two:?} can overlap in a draft"
                    );
                }
            }
        }
    }

    /// Pastes `text` and gives back the name the composer would show for it.
    fn collapse(state: &mut Paste, text: &str) -> String {
        state.begin(0);
        for byte in text.as_bytes() {
            state.byte(*byte);
        }
        match state.finish() {
            Pasted::Collapsed { summary, .. } => summary,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_paste_is_measured_against_the_draft_it_is_landing_in() {
        // What the block puts on the screen is 25 bytes, so the draft's own
        // size is the other half of the number -- and two ceilings of 8 MiB
        // each are one prompt of 16.
        let room = 1_000_000;
        let mut state = Paste::default();
        state.begin(MAX_PASTE_BYTES - room);
        for _ in 0..=room {
            state.byte(b'x');
        }
        assert!(
            state.refused(),
            "a paste was measured without the draft it is landing in"
        );

        let pasted = state.finish();
        let Pasted::Collapsed { summary, .. } = pasted else {
            panic!("{pasted:?}");
        };
        assert_eq!(
            state.expand(&summary),
            summary,
            "the refused paste was kept"
        );
    }

    #[test]
    fn a_collapsed_paste_the_budget_admits_always_fits_the_composer() {
        // Why `super::super::shell::Shell::pasted` has no arm for a composer
        // that refuses the summary. A block is collapsed only past
        // `COLLAPSE_ABOVE` codepoints, so its text is at least that many
        // bytes; the budget admits it only if the draft plus that text is
        // inside `MAX_PASTE_BYTES`; and the composer's own cap is the same
        // number. So the room the draft has left over is never smaller than
        // the codepoints the collapse needed -- and a name is far shorter than
        // that. Both halves are asserted, because either one moving alone
        // brings the missing arm back.
        // A `const` block, so this half is not a test that could be deleted
        // but a build that stops.
        const {
            assert!(
                super::super::editor::MAX_COMPOSER_BYTES >= MAX_PASTE_BYTES,
                "the composer now caps a draft the paste budget would admit, \
                 so a summary can be refused and `Shell::pasted` owes that \
                 case an arm"
            );
        }
        let widest = summary(u32::MAX, usize::MAX).len();
        assert!(
            widest <= COLLAPSE_ABOVE,
            "a summary ({widest} bytes) can now be longer than the text that \
             earned it ({COLLAPSE_ABOVE}), so the room argument no longer holds"
        );
    }

    #[test]
    fn two_blocks_that_ran_out_of_numbers_do_not_expand_each_other() {
        // `next` saturates, so a session that pasted four billion times gives
        // two blocks the same name -- and then one placeholder in the draft is
        // claimed by both. The splice guard is what makes that a wrong answer
        // instead of a panic: the first block takes it and the second finds it
        // taken.
        let mut state = Paste::with_next(u32::MAX - 1);
        let first = "y".repeat(1200);
        let second = "z".repeat(1200);
        let one = collapse(&mut state, &first);
        let two = collapse(&mut state, &second);
        assert_eq!(
            one, two,
            "the numbers did not run out, so this case proves nothing"
        );

        let expanded = state.expand(&one);
        assert_eq!(expanded, first, "the placeholder was taken twice");
        assert!(
            !expanded.contains(&second),
            "both blocks expanded into one placeholder"
        );
    }

    #[test]
    fn a_block_the_composer_no_longer_holds_expands_into_nothing_later() {
        // A summary is ordinary text in a composer, so a user can type one --
        // and a block that outlived the draft it was pasted into would turn
        // that typing into the whole of a paste they sent a turn ago.
        let mut state = Paste::default();
        state.begin(0);
        for byte in "y".repeat(1200).as_bytes() {
            state.byte(*byte);
        }
        let Pasted::Collapsed { summary, .. } = state.finish() else {
            panic!("1200 codepoints did not collapse");
        };
        assert_ne!(
            state.expand(&summary),
            summary,
            "the block was never registered, so this case proves nothing"
        );

        state.forget();
        assert_eq!(
            state.expand(&summary),
            summary,
            "a block the composer no longer holds was still expanded"
        );
    }

    #[test]
    fn two_pastes_are_two_blocks_and_each_expands_where_it_stands() {
        // Ids that did not move would make the second paste answer to the
        // first one's name, and every summary in a draft would expand to the
        // same text.
        let mut state = Paste::default();
        let first = "y".repeat(1200);
        let second = "z".repeat(1100);
        state.begin(0);
        for byte in first.as_bytes() {
            state.byte(*byte);
        }
        let one = state.finish();
        state.begin(0);
        for byte in second.as_bytes() {
            state.byte(*byte);
        }
        let two = state.finish();

        assert_eq!(
            one,
            Pasted::Collapsed {
                summary: "[Pasted text #1, 1 lines]".into(),
                id: 1
            }
        );
        assert_eq!(
            two,
            Pasted::Collapsed {
                summary: "[Pasted text #2, 1 lines]".into(),
                id: 2
            }
        );
        assert_eq!(
            state.expand("[Pasted text #2, 1 lines] then [Pasted text #1, 1 lines]"),
            format!("{second} then {first}")
        );
    }
}
