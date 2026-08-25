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
//!   what the model receives is what was pasted (`pasted_blocks.zig:53-63`).
//!   The summary is what the *screen* holds; it is never what is sent.
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
    /// A paste is starting: whatever the last one left is gone.
    ///
    /// Cleared here rather than at [`Self::finish`] because a paste can be
    /// abandoned -- a session that put an approval panel up mid-paste swallows
    /// the end marker with everything else the panel does not bind -- and the
    /// next `begin` is the one moment at which the buffer is certainly stale.
    pub(crate) fn begin(&mut self) {
        self.buffer.clear();
        self.refused = false;
    }

    /// One byte from between the markers.
    pub(crate) fn byte(&mut self, byte: u8) {
        if !accepted(byte) {
            return;
        }
        // **Not buffered and then trimmed.** The budget is about memory as much
        // as about the composer: a terminal can hand this function bytes for as
        // long as it likes, and a buffer that grew first would already have
        // paid for the paste it is about to refuse.
        if self.buffer.len() >= MAX_PASTE_BYTES {
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
        if !self.refused && text.chars().count() <= COLLAPSE_ABOVE {
            return Pasted::Inline(text);
        }
        // Counted in **lines of text**, so a block ending in a newline is not
        // reported as having one more line than a reader can see.
        let lines = text.lines().count();
        self.next = self.next.saturating_add(1);
        let id = self.next;
        let summary = summary(id, lines);
        if !self.refused {
            self.blocks.push(Block {
                summary: summary.clone(),
                text,
            });
        }
        Pasted::Collapsed { summary, id }
    }

    /// Whether the paste that just finished overran the budget.
    ///
    /// Valid from [`Self::byte`] until the next [`Self::begin`], which is
    /// exactly the window in which the shell asks it.
    pub(crate) fn refused(&self) -> bool {
        self.refused
    }

    /// `submitted` with every summary in it replaced by the text it stands for.
    ///
    /// Scanned once, left to right, rather than by replacing each block's
    /// summary in turn: a block's *text* can contain another block's summary --
    /// a user who pastes a session transcript is the ordinary way that happens
    /// -- and a second pass over already-expanded text would expand it again.
    pub(crate) fn expand(&self, submitted: &str) -> String {
        if self.blocks.is_empty() {
            return submitted.to_string();
        }
        let mut out = String::with_capacity(submitted.len());
        let mut rest = submitted;
        while let Some((at, block)) = self.first_summary_in(rest) {
            out.push_str(&rest[..at]);
            out.push_str(&block.text);
            rest = &rest[at + block.summary.len()..];
        }
        out.push_str(rest);
        out
    }

    /// The earliest summary in `text`, and the block it names.
    fn first_summary_in<'a>(&'a self, text: &str) -> Option<(usize, &'a Block)> {
        self.blocks
            .iter()
            .filter_map(|block| text.find(&block.summary).map(|at| (at, block)))
            .min_by_key(|(at, _)| *at)
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

    fn paste(text: &str) -> Pasted {
        let mut paste = Paste::default();
        paste.begin();
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
        state.begin();
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
        // approximately the number.
        let mut state = Paste::default();
        state.begin();
        for _ in 0..MAX_PASTE_BYTES {
            state.byte(b'x');
        }
        assert!(
            !state.refused(),
            "a paste of exactly the budget was refused"
        );
        state.byte(b'x');
        assert!(
            state.refused(),
            "a paste one byte past the budget was taken"
        );
    }

    #[test]
    fn a_paste_larger_than_the_budget_is_refused_rather_than_truncated() {
        let mut state = Paste::default();
        state.begin();
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
        state.begin();
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
    fn a_block_the_composer_no_longer_holds_expands_into_nothing_later() {
        // A summary is ordinary text in a composer, so a user can type one --
        // and a block that outlived the draft it was pasted into would turn
        // that typing into the whole of a paste they sent a turn ago.
        let mut state = Paste::default();
        state.begin();
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
        state.begin();
        for byte in first.as_bytes() {
            state.byte(*byte);
        }
        let one = state.finish();
        state.begin();
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
