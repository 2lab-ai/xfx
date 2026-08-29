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
//! * **A budget.** [`MAX_PASTE_BYTES`] bounds what one paste may hold, and
//!   [`fits`] bounds the *prompt* a draft would send. The bytes past the first
//!   are not buffered -- an unbounded buffer fed by a terminal is a memory hole
//!   with a keyboard on it -- and the paste is **refused whole** rather than
//!   half-taken: a [`Pasted::Refused`] puts nothing in the composer, so there
//!   is no summary standing for a truncated half of what the user pasted.
//! * **A collapse.** More than [`COLLAPSE_ABOVE`] codepoints becomes
//!   `[Pasted text #N, M lines]` in the composer (`pasted_blocks.zig:7`) and
//!   the text travels with it as an entity. Two reasons, and the second is the
//!   load-bearing one: 1800 codepoints painted into a band is a band that has
//!   eaten the screen, and every later keystroke re-wraps whatever the composer
//!   holds -- a megabyte in the composer makes every subsequent keypress cost
//!   milliseconds.
//! * **An expansion.** The block is put back at submit time, so what the model
//!   receives is the paste rather than a description of it
//!   (`pasted_blocks.zig:53-63`). The summary is what the *screen* holds; it is
//!   never what is sent. "The paste" is precisely the text this module made of
//!   it -- filtered, decoded as UTF-8 with the bytes that are not UTF-8
//!   replaced, and with its line breaks normalized -- and not the bytes the
//!   terminal sent; those three are the only things that happen to it, and each
//!   one is above.
//!
//! # Where the block lives
//!
//! **Not here.** A finished paste hands its text to the composer, which holds
//! it as a span of the draft (`super::entity`, `super::editor::Editor`): the
//! block *is* those bytes of that text, so it moves when they move and dies
//! when they are deleted. What this module keeps between pastes is one number
//! -- the last id it minted -- because that is the only thing about a paste
//! that outlives the draft it landed in.
//!
//! Until item 16 the block was a **name**: the text was kept here and found
//! again by searching the draft for the summary. That is what needed an
//! arbitration between copies of one name, a cap on how many blocks a draft
//! could hold (the search ran once per block per keystroke), and a rule for
//! what a damaged name released. The span replaced all three
//! (`super::entity`).
//!
//! # What a line break is
//!
//! A terminal sends CR for Return, so a real multi-line paste arrives as bare
//! carriage returns rather than as newlines. They are normalized here, by
//! [`super::transcript::normalize`] and not by a second copy of the same rule,
//! because a bare CR left in the composer would be a line break to the person
//! who pasted it, no break at all to [`super::wrap`], and a cursor rewind to
//! the terminal -- three answers to one question.

use std::sync::Arc;

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

/// Whether a draft of `draft` bytes holding `retained` bytes behind its
/// summaries may take `more` bytes.
///
/// **One budget, and it is the prompt's rather than the screen's.** A collapsed
/// block shows 25 bytes and stands for as much as the whole cap, so a draft
/// bounded on its own and a set of blocks bounded on their own are two ceilings
/// that add up to twice the number either of them names -- and what leaves this
/// session is their sum.
///
/// Conservative by the length of the summaries -- the draft holds a name where
/// the prompt holds the text, and this counts both -- which is some 25 bytes
/// per block against a budget of eight million, and errs towards refusing a
/// paste rather than sending one that is too big.
///
/// A free function rather than a method, because none of the three numbers is
/// this module's any more: the draft and its blocks are the composer's
/// (`super::editor::Editor::retained`), and what is left is the arithmetic.
pub(crate) fn fits(draft: usize, retained: usize, more: usize) -> bool {
    draft.saturating_add(retained).saturating_add(more) <= MAX_PASTE_BYTES
}

/// The paste being framed, and the numbers this session has spent.
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
    /// Why the paste that is arriving has already been refused, if it has.
    ///
    /// Set the moment a byte does not fit and cleared by the next
    /// [`Paste::begin`], because a paste can be abandoned mid-frame.
    refused: Option<Refusal>,
    /// The id the last collapsed block took.
    ///
    /// **The session's one allocator.** A paste spends the next number here and
    /// a recall spends them through [`Self::ids`]
    /// (`super::entity::Entities::renumber_recalled`), so no two live blocks
    /// can answer to one name. Never reset and never wrapped: two pastes in one
    /// session are `#1` and `#2` even when the first was submitted and
    /// forgotten in between, and at the end of the space a paste is refused
    /// rather than given a number that is already in a draft.
    next: u32,
}

/// Why a paste put nothing in the composer.
///
/// Two, and they are told apart because the hint row says which: a paste that
/// vanished without a word looks exactly like a terminal that never sent it,
/// and a paste refused for the wrong stated reason is worse than one refused
/// for none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// It would take the prompt past [`MAX_PASTE_BYTES`].
    Oversized,
    /// This session has minted every paste number there is.
    Unnumbered,
}

/// What one finished paste is.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Pasted {
    /// Small enough to be text in the composer, exactly as pasted.
    Inline(String),
    /// Too large for that: the composer gets the summary, and the text travels
    /// with it as the block that summary stands for.
    Collapsed {
        summary: String,
        id: u32,
        /// `Arc<str>` from here on, because this text is about to be in the
        /// composer and in every history entry that remembers the line.
        text: Arc<str>,
        lines: usize,
    },
    /// Nothing goes into the composer, and the hint row says why.
    Refused(Refusal),
}

impl Paste {
    /// A session that has already used `next` numbers.
    ///
    /// Only the tests build one: the id space is four billion pastes deep, and
    /// the case worth proving -- what a paste with no number left does -- is
    /// not reachable by pasting four billion times.
    #[cfg(test)]
    pub(crate) fn with_next(next: u32) -> Self {
        Self {
            next,
            ..Self::default()
        }
    }

    /// The number allocator, for the one other thing that mints paste numbers.
    ///
    /// A recall renumbers the blocks an entry carries
    /// (`super::entity::Entities::renumber_recalled`) and it has to take its
    /// numbers from **this** counter: a recall with an allocator of its own
    /// would hand a live draft two blocks with one name.
    pub(crate) fn ids(&mut self) -> &mut u32 {
        &mut self.next
    }

    /// A paste is starting: whatever the last one left is gone.
    ///
    /// Cleared here rather than at [`Self::finish`] because a paste can be
    /// abandoned -- a session that put an approval panel up mid-paste swallows
    /// the end marker with everything else the panel does not bind -- and the
    /// next `begin` is the one moment at which the buffer is certainly stale.
    pub(crate) fn begin(&mut self) {
        self.buffer.clear();
        self.refused = None;
    }

    /// One byte from between the markers.
    pub(crate) fn byte(&mut self, byte: u8) {
        if !accepted(byte) {
            return;
        }
        // **A bound on this buffer, and nothing else.** A terminal can hand
        // this function bytes for as long as it likes, so the buffer needs a
        // ceiling of its own -- but whether the paste is *admitted* is not a
        // question that can be answered here. It depends on what the paste
        // becomes when it is decoded, which is not known until it is whole
        // ([`Self::finish`]): a byte that is not UTF-8 trebles on its way to
        // being text.
        //
        // What that costs is transient: the draft, its blocks and this buffer
        // can each be at the cap at once while a paste is arriving, so peak
        // memory during one is a small multiple of it rather than the cap.
        if self.buffer.len() >= MAX_PASTE_BYTES {
            self.refused = Some(Refusal::Oversized);
            return;
        }
        self.buffer.push(byte);
    }

    /// The paste is over: what the composer should be given.
    ///
    /// Judged against the draft it is landing in -- `draft` bytes on the screen
    /// and `retained` bytes behind its summaries -- because a paste and the
    /// blocks a draft already holds go into one prompt ([`fits`]). A refusal
    /// puts nothing in the composer and spends no number, so the next paste is
    /// still `#N`.
    pub(crate) fn finish(&mut self, draft: usize, retained: usize) -> Pasted {
        let bytes = std::mem::take(&mut self.buffer);
        // Lossy rather than strict: a paste in an encoding this session does
        // not read must not be lost whole to one bad byte.
        let text = transcript::normalize(&String::from_utf8_lossy(&bytes));
        if let Some(refusal) = self.refused {
            return Pasted::Refused(refusal);
        }

        // The *decoded* size is what both budgets are about, and this is where
        // it is first known: `from_utf8_lossy` turns each byte that is not
        // UTF-8 into a three-byte replacement scalar, so a paste can treble on
        // its way to being text.
        if text.chars().count() <= COLLAPSE_ABOVE {
            // Inline: the text itself goes into the draft and no block is kept,
            // so it is charged once rather than as text plus a name.
            if fits(draft, retained, text.len()) {
                return Pasted::Inline(text);
            }
            self.refused = Some(Refusal::Oversized);
            return Pasted::Refused(Refusal::Oversized);
        }

        // Checked, and the refusal is visible. At the end of the space the
        // alternatives are a number that wraps -- two live blocks called `#1`,
        // and a recall that expands the wrong paste -- or a number that
        // saturates, which is the same collision spelled differently.
        let Some(id) = self.next.checked_add(1) else {
            self.refused = Some(Refusal::Unnumbered);
            return Pasted::Refused(Refusal::Unnumbered);
        };
        // Counted in **lines of text**, so a block ending in a newline is not
        // reported as having one more line than a reader can see.
        let lines = text.lines().count();
        let summary = summary(id, lines);
        // Collapsed: the draft gets the *name* and the text goes behind it, so
        // the prompt is the draft plus what its blocks hold -- this paste's own
        // text included, and its name is in the draft half of that sum.
        if !fits(draft.saturating_add(summary.len()), retained, text.len()) {
            self.refused = Some(Refusal::Oversized);
            return Pasted::Refused(Refusal::Oversized);
        }
        // The number is spent only now, so a refused paste does not make the
        // next one `#2`.
        self.next = id;
        Pasted::Collapsed {
            summary,
            id,
            text: Arc::from(text),
            lines,
        }
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

/// What the composer shows in place of a collapsed block.
pub(crate) fn summary(id: u32, lines: usize) -> String {
    format!("[Pasted text #{id}, {lines} lines]")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One whole paste, into an empty composer.
    fn paste(text: &str) -> Pasted {
        let mut paste = Paste::default();
        paste.begin();
        for byte in text.as_bytes() {
            paste.byte(*byte);
        }
        paste.finish(0, 0)
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
    fn a_paste_over_a_thousand_codepoints_collapses_and_carries_its_text() {
        let big = "x".repeat(1200) + "\nsecond line";
        let Pasted::Collapsed {
            summary,
            id,
            text,
            lines,
        } = paste(&big)
        else {
            panic!("a paste past the threshold did not collapse");
        };
        assert_eq!(summary, "[Pasted text #1, 2 lines]");
        assert_eq!(id, 1);
        assert_eq!(lines, 2);
        // pasted_blocks.zig:53-63 -- what is submitted is what was pasted, and
        // the text travels with the block rather than being kept here to be
        // found again by name.
        assert_eq!(&*text, big.as_str());
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
        // approximately the number. A collapsed paste is charged its text *and*
        // the name that stands in for it in the draft -- some 25 bytes against
        // eight million, and the conservatism runs towards refusing a paste
        // rather than sending a prompt that is too big.
        let name = summary(1, 1).len();
        let mut state = Paste::default();
        state.begin();
        for _ in 0..(MAX_PASTE_BYTES - name) {
            state.byte(b'x');
        }
        let Pasted::Collapsed {
            summary: held,
            text,
            ..
        } = state.finish(0, 0)
        else {
            panic!("a paste of exactly the budget, name and all, was refused");
        };

        // The budget is spent: the draft shows that name and holds the whole of
        // it behind it, so the next paste has nowhere to go.
        state.begin();
        state.byte(b'x');
        assert_eq!(
            state.finish(held.len(), text.len()),
            Pasted::Refused(Refusal::Oversized),
            "a paste past the budget the draft is already holding was taken"
        );

        // And the **first byte that does not fit**, into an empty composer: one
        // more than the largest block a draft can hold. It is refused on
        // account of the name that would stand in for it, which is the half of
        // the sum a budget measured on the text alone would miss.
        let mut state = Paste::default();
        state.begin();
        for _ in 0..(MAX_PASTE_BYTES - name + 1) {
            state.byte(b'x');
        }
        assert_eq!(
            state.finish(0, 0),
            Pasted::Refused(Refusal::Oversized),
            "a paste one byte past what its own summary leaves room for was taken"
        );
    }

    #[test]
    fn a_paste_larger_than_the_budget_is_refused_rather_than_truncated() {
        let mut state = Paste::default();
        state.begin();
        for _ in 0..(MAX_PASTE_BYTES + 10) {
            state.byte(b'x');
        }
        assert_eq!(
            state.finish(0, 0),
            Pasted::Refused(Refusal::Oversized),
            "the buffer took more than its own ceiling, which is the one bound \
             that has to hold before a paste is whole"
        );
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
    fn a_refused_paste_carries_no_text_a_summary_could_stand_for() {
        // The other half of "refused rather than truncated": a refusal is not a
        // collapsed block with a shorter text, it is nothing at all, so no
        // summary can reach the composer to stand in front of half a paste.
        let mut state = Paste::default();
        state.begin();
        for _ in 0..(MAX_PASTE_BYTES + 10) {
            state.byte(b'x');
        }
        assert!(matches!(state.finish(0, 0), Pasted::Refused(_)));
        assert!(!matches!(state.finish(0, 0), Pasted::Collapsed { .. }));
    }

    #[test]
    fn the_budget_is_everything_the_draft_is_holding_rather_than_one_paste() {
        // Every collapsed block hides behind a 25-byte summary, so a draft that
        // looks like two short lines can be holding two whole pastes. A budget
        // that measured only the paste arriving would bound nothing at all.
        let half = MAX_PASTE_BYTES / 2 + 1;
        let mut state = Paste::default();
        state.begin();
        for _ in 0..half {
            state.byte(b'x');
        }
        let Pasted::Collapsed {
            summary: first,
            text: held,
            ..
        } = state.finish(0, 0)
        else {
            panic!("half the budget did not collapse");
        };

        state.begin();
        for _ in 0..half {
            state.byte(b'x');
        }
        assert_eq!(
            state.finish(first.len(), held.len()),
            Pasted::Refused(Refusal::Oversized),
            "two halves of the budget were admitted as one prompt"
        );
    }

    #[test]
    fn a_paste_that_grows_when_it_is_decoded_is_measured_after_it_grew() {
        // A byte that is not UTF-8 becomes a three-byte replacement scalar, so
        // the text that is kept and sent can be three times the bytes that
        // arrived. The budget is about the text.
        let raw = MAX_PASTE_BYTES / 2;
        let mut state = Paste::default();
        state.begin();
        for _ in 0..raw {
            state.byte(0xff);
        }
        assert_eq!(
            state.finish(0, 0),
            Pasted::Refused(Refusal::Oversized),
            "a paste that trebled in size when it was decoded was kept whole"
        );
    }

    #[test]
    fn a_paste_that_only_grows_past_the_budget_once_decoded_counts_the_draft_too() {
        // The bytes are well inside the buffer's own ceiling; the *text* they
        // become is three times as long and does not fit beside the draft. Only
        // the finished paste can be asked.
        let mut state = Paste::default();
        state.begin();
        for _ in 0..2_000_000 {
            state.byte(0xff);
        }
        assert_eq!(
            state.finish(4_000_000, 0),
            Pasted::Refused(Refusal::Oversized),
            "a paste that trebled as it was decoded was kept beside a 4 MB draft"
        );
    }

    #[test]
    fn a_refused_paste_does_not_spend_the_number_the_next_one_uses() {
        // A number is spent when a block is kept. A refusal that took one would
        // leave a gap a user can see -- `#1` then `#3` -- and the gap would say
        // that something was pasted when nothing was.
        let mut state = Paste::default();
        state.begin();
        for _ in 0..(MAX_PASTE_BYTES + 10) {
            state.byte(b'x');
        }
        assert!(matches!(state.finish(0, 0), Pasted::Refused(_)));

        state.begin();
        for _ in 0..(COLLAPSE_ABOVE + 1) {
            state.byte(b'x');
        }
        let Pasted::Collapsed { id, .. } = state.finish(0, 0) else {
            panic!("the paste after a refusal was refused too");
        };
        assert_eq!(id, 1, "the refused paste spent a number");
    }

    #[test]
    fn a_paste_with_no_number_left_is_refused_rather_than_given_a_used_one() {
        // The end of the id space. Wrapping would put two live blocks under one
        // name and a recall would expand the wrong one; saturating is the same
        // collision spelled differently.
        let mut state = Paste::with_next(u32::MAX);
        state.begin();
        for _ in 0..(COLLAPSE_ABOVE + 1) {
            state.byte(b'x');
        }
        assert_eq!(
            state.finish(0, 0),
            Pasted::Refused(Refusal::Unnumbered),
            "a paste was given a number this session had already minted"
        );
        assert_eq!(*state.ids(), u32::MAX, "the allocator wrapped");
    }

    #[test]
    fn a_collapsed_paste_the_budget_admits_always_fits_the_composer() {
        // Arithmetic rather than optimism, and the claim the composer's own
        // refusal-free insertion rests on: a block is only collapsed past
        // `COLLAPSE_ABOVE` codepoints, so the text it hides is longer than the
        // name that replaces it, and a paste the prompt budget admits leaves
        // room for that name in a composer whose cap is the same number.
        let mut state = Paste::default();
        state.begin();
        for _ in 0..(COLLAPSE_ABOVE + 1) {
            state.byte(b'x');
        }
        let Pasted::Collapsed { summary, text, .. } = state.finish(0, 0) else {
            panic!("the paste did not collapse");
        };
        assert!(
            summary.len() < text.len(),
            "a summary is longer than the block it stands for"
        );
        assert!(fits(summary.len(), text.len(), 0));
    }

    #[test]
    fn a_paste_is_measured_against_the_draft_it_is_landing_in() {
        // The draft is half the budget and so is the paste: together they are
        // past it, and the paste is the one that is refused.
        let half = MAX_PASTE_BYTES / 2;
        let mut state = Paste::default();
        state.begin();
        for _ in 0..(half + 1) {
            state.byte(b'x');
        }
        assert_eq!(state.finish(half, 0), Pasted::Refused(Refusal::Oversized));

        // And the same paste into an empty composer is taken.
        state.begin();
        for _ in 0..(half + 1) {
            state.byte(b'x');
        }
        assert!(matches!(state.finish(0, 0), Pasted::Collapsed { .. }));
    }

    #[test]
    fn an_abandoned_paste_leaves_nothing_for_the_next_one() {
        // A question can arrive mid-paste and swallow the end marker with
        // everything else it does not bind, so the next `begin` is the one
        // moment at which the buffer is certainly stale.
        let mut state = Paste::default();
        state.begin();
        for byte in b"abandoned" {
            state.byte(*byte);
        }
        state.begin();
        for byte in b"fresh" {
            state.byte(*byte);
        }
        assert_eq!(state.finish(0, 0), Pasted::Inline("fresh".to_string()));
    }
}
