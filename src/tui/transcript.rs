//! The transcript: text in, rows the terminal's own document keeps.
//!
//! xfx does not own a transcript viewport. Everything above the band's divider
//! is the terminal's document, and a row this module hands over is written
//! there **once** -- scrolled in with a literal newline so that when it leaves
//! the top of the screen it is in the terminal's native scrollback, where the
//! user's wheel and the user's `less` can still reach it, and where it stays
//! after xfx exits. Phase 1 never rewrites one. The repainted viewport, and the
//! cell diff that makes it affordable, are Phase 2.
//!
//! That is what the state here is for. A stream of text does not arrive one
//! finished line at a time: a delta can be three characters that lengthen a row
//! already on the screen, or a hundred that wrap it onto four more. So the
//! module keeps the **unfinished line** -- the tail -- and how many rows of the
//! screen it currently occupies, and answers every push with an [`Append`]:
//! how many rows to scroll in, and the rows to write. A tail that grew without
//! wrapping scrolls nothing and is simply written again a little longer.
//!
//! Once a line is finished it is gone from here ([`Transcript::end_line`]).
//! There is nothing to remember about it, because nothing will ever repaint it.

use super::wrap;

/// What one push owes the terminal's document.
///
/// `scroll` is how many rows the screen must move up to make room; `rows` is
/// the whole of the unfinished line as it now stands, top first, to be written
/// on the `rows.len()` document rows immediately above the divider. The two are
/// different numbers on purpose: a push that only lengthened the last row
/// scrolls nothing and rewrites one row, and a push that wrapped it scrolls one
/// and rewrites both.
///
/// **`scroll` is a `usize`, and counting rows in a `u16` anywhere on this path
/// is a bug.** A row count is bounded by the *text*, not by the screen: one
/// 8 MiB composer submission (`editor::MAX_COMPOSER_BYTES`) wrapped on a narrow
/// terminal is well past 65535 rows. A count that saturated there would leave
/// `scroll` smaller than the rows it describes, and the renderer -- which
/// derives "already on the screen" from `rows.len() - scroll` -- would treat
/// the difference as settled and never paint it. That is silent, permanent
/// loss of the beginning of an answer, the same class as a batch scroll that
/// outruns the document area.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Append {
    pub(crate) scroll: usize,
    pub(crate) rows: Vec<String>,
}

impl Append {
    /// An append that asks the terminal for nothing.
    fn nothing() -> Self {
        Self {
            scroll: 0,
            rows: Vec::new(),
        }
    }
}

/// The unfinished line, and what the screen already shows of it.
pub(crate) struct Transcript {
    /// The screen's width, which is what the rows are wrapped to.
    ///
    /// Fixed for the session: this phase does not re-layout on `SIGWINCH`, and
    /// `docs/parity.md` says so. A phase that does has to re-wrap the tail and
    /// repaint it, which is the same work as Phase 2's viewport.
    cols: u16,
    /// The line that has not ended yet. Never holds a line break: the breaks
    /// are what [`Transcript::push`] splits on.
    tail: String,
    /// How many rows of the screen the tail occupies **now** -- that is, how
    /// many rows a previous append already wrote and the next one may write
    /// over. Not the same as the number of rows the tail's text wraps to: after
    /// [`Transcript::end_line`] the tail is empty and occupies nothing, and a
    /// wrap of an empty string is still one row.
    ///
    /// A `usize` for the reason [`Append::scroll`] is one: the tail's rows are
    /// bounded by the text, not by the screen.
    painted: usize,
    /// Whether the last non-empty push ended on a carriage return.
    ///
    /// A CRLF that arrives in two pieces would otherwise be two line breaks:
    /// the CR becomes one here, and the LF that opens the next push would be
    /// another. The flag makes the pair one break whichever read they were
    /// split across, which is the same promise [`normalize`] makes inside one.
    ///
    /// The promise it is really keeping is stronger and is what the tests
    /// state: **a push is invariant under chunking.** `push(a); push(b)` writes
    /// what `push(a + b)` writes, for every place a stream could be cut. That
    /// is why the flag is consulted against the *raw* next chunk and why an
    /// empty push does not disturb it.
    split_crlf: bool,
}

impl Transcript {
    pub(crate) fn new(cols: u16) -> Self {
        Self {
            cols,
            tail: String::new(),
            painted: 0,
            split_crlf: false,
        }
    }

    /// Adds `text` to the transcript and says what the document owes.
    ///
    /// A line break inside `text` finishes the line before it, exactly as
    /// [`end_line`](Self::end_line) does, and the part after it becomes the new
    /// tail; a push may therefore finish several lines at once, and the
    /// [`Append`] it returns covers all of them -- `rows` is every row from the
    /// first one this push changed down to the last one it wrote.
    pub(crate) fn push(&mut self, text: &str) -> Append {
        // A push with nothing in it is a chunk boundary and nothing else. It
        // must be **transparent**: clearing the carry here would turn the CR
        // that ended the last chunk into a break of its own, and the LF that
        // opens the chunk after this one into a second.
        if text.is_empty() {
            return Append::nothing();
        }
        let normalized = normalize(text);
        // Decided against the **raw** chunk, not the normalized one. Only a
        // chunk that really begins with an LF is the other half of the CR that
        // ended the last one; a chunk beginning with another CR is a break of
        // its own, and stripping the newline `normalize` just made of it would
        // swallow it. The invariant both halves of this serve:
        // `push(a); push(b)` writes what `push(a + b)` writes, wherever the
        // stream was cut.
        let carried = self.split_crlf && text.starts_with('\n');
        self.split_crlf = text.ends_with('\r');
        let mut body = normalized.as_str();
        if carried {
            body = body.strip_prefix('\n').unwrap_or(body);
        }
        if body.is_empty() {
            return Append::nothing();
        }

        let painted = self.painted;
        let mut rows = Vec::new();
        let mut segments = body.split('\n');
        // `split` yields the text itself when there is no break in it, so the
        // first segment always exists and always joins the tail.
        self.tail.push_str(segments.next().unwrap_or_default());
        for next in segments {
            // Everything before the break is a finished line. Its rows are
            // written once, here, and never again.
            rows.append(&mut self.tail_texts());
            self.tail.clear();
            self.tail.push_str(next);
        }
        let mut tail = self.tail_texts();
        self.painted = tail.len();
        rows.append(&mut tail);

        // The rows already on the screen are the first `painted` of these --
        // the old tail is a prefix of the text they came from -- so they are
        // rewritten where they are and everything past them is new.
        Append {
            scroll: rows.len().saturating_sub(painted),
            rows,
        }
    }

    /// Ends the current line, leaving it in the document.
    ///
    /// Usually this writes nothing: the line is already on the screen exactly
    /// as it stands, and all that changes is that this module stops holding it.
    /// The exception is a line with nothing on it -- two breaks in a row --
    /// which has no row of its own yet and gets one, because a blank line in an
    /// answer is a blank line on the screen.
    // Task 10's submit, and Task 12's end-of-turn, are the callers. The method
    // is here rather than folded into `push("\n")` because ending a line is
    // what a *caller* knows and a byte in a stream is not: a turn ends without
    // a trailing newline in the text.
    #[allow(dead_code)]
    pub(crate) fn end_line(&mut self) -> Append {
        // A CR that ended the last push has been answered by this break.
        self.split_crlf = false;
        let rows = self.tail_texts();
        let scroll = rows.len().saturating_sub(self.painted);
        self.tail.clear();
        self.painted = 0;
        if scroll == 0 {
            return Append::nothing();
        }
        Append { scroll, rows }
    }

    /// How many rows of the screen the unfinished line occupies.
    // Task 9's footer needs it to know how much of the content area is left.
    #[allow(dead_code)]
    pub(crate) fn tail_rows(&self) -> usize {
        self.painted
    }

    /// The tail, wrapped, as the strings an append writes.
    fn tail_texts(&self) -> Vec<String> {
        wrap::wrap(&self.tail, self.cols)
            .into_iter()
            .map(|row| self.tail[row.start..row.end].to_string())
            .collect()
    }
}

/// `text` with every line break spelled the one way the document accepts.
///
/// A CRLF becomes an LF and a bare CR becomes one too (`frame_scroll_plan.zig:8-12`).
/// The document only ever receives LFs because that is the only byte that
/// scrolls it: a bare CR would rewind the terminal's cursor to the first column
/// of a row this module believes it has already finished writing, and the next
/// row placed would overwrite it.
pub(crate) fn normalize(text: &str) -> String {
    if !text.contains('\r') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        if character != '\r' {
            out.push(character);
            continue;
        }
        // A CR and the LF that follows it are one break, not two.
        if characters.peek() == Some(&'\n') {
            characters.next();
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_fragment_takes_one_row_of_the_screen() {
        let mut transcript = Transcript::new(80);
        assert_eq!(
            transcript.push("answer"),
            Append {
                scroll: 1,
                rows: vec!["answer".to_string()]
            }
        );
    }

    #[test]
    fn a_fragment_that_fits_the_tail_repaints_it_without_scrolling() {
        let mut transcript = Transcript::new(80);
        transcript.push("ans");
        assert_eq!(
            transcript.push("wer"),
            Append {
                scroll: 0,
                rows: vec!["answer".to_string()]
            }
        );
    }

    #[test]
    fn a_fragment_that_wraps_the_tail_scrolls_by_exactly_the_rows_it_added() {
        let mut transcript = Transcript::new(4);
        transcript.push("abcd");
        assert_eq!(
            transcript.push("efgh"),
            Append {
                scroll: 1,
                rows: vec!["abcd".to_string(), "efgh".to_string()]
            }
        );
    }

    #[test]
    fn a_finished_line_is_left_in_the_document_and_the_next_one_starts_fresh() {
        let mut transcript = Transcript::new(80);
        transcript.push("first");
        transcript.end_line();
        assert_eq!(transcript.tail_rows(), 0);
        assert_eq!(
            transcript.push("second"),
            Append {
                scroll: 1,
                rows: vec!["second".to_string()]
            }
        );
    }

    #[test]
    fn carriage_returns_are_normalized_so_a_row_cannot_overwrite_itself() {
        // frame_scroll_plan.zig:8-12 -- the document only ever receives
        // CR-before-LF bytes, and a bare CR would rewind the terminal's cursor
        // over a row this module believes it has already written.
        assert_eq!(normalize("a\r\nb"), "a\nb");
        assert_eq!(normalize("a\rb"), "a\nb");
        assert_eq!(normalize("plain"), "plain");
    }

    #[test]
    fn a_line_that_is_already_on_the_screen_is_finished_without_a_write() {
        // The rows are the terminal's now. Rewriting them would cost a scroll
        // that pushes a blank row into the document.
        let mut transcript = Transcript::new(80);
        transcript.push("first");
        assert_eq!(transcript.end_line(), Append::nothing());
    }

    #[test]
    fn a_break_inside_a_push_finishes_the_line_before_it() {
        let mut transcript = Transcript::new(80);
        assert_eq!(
            transcript.push("first\nsecond"),
            Append {
                scroll: 2,
                rows: vec!["first".to_string(), "second".to_string()]
            }
        );
        assert_eq!(
            transcript.tail_rows(),
            1,
            "only the unfinished line is still this module's"
        );
        // and the finished one is not rewritten by what follows
        assert_eq!(
            transcript.push("!"),
            Append {
                scroll: 0,
                rows: vec!["second!".to_string()]
            }
        );
    }

    #[test]
    fn a_blank_line_between_two_answers_takes_a_row_of_its_own() {
        // Two breaks in a row. Without a row for the empty line between them
        // the paragraph break the model wrote disappears.
        let mut transcript = Transcript::new(80);
        assert_eq!(
            transcript.push("a\n\nb"),
            Append {
                scroll: 3,
                rows: vec!["a".to_string(), String::new(), "b".to_string()]
            }
        );
    }

    /// The document a sequence of pushes leaves behind, replayed exactly as
    /// `frame::render_append` applies an [`Append`]: scroll by `scroll`, then
    /// write `rows` onto the last `rows.len()` lines of what is there.
    fn document(cols: u16, chunks: &[&str]) -> Vec<String> {
        let mut transcript = Transcript::new(cols);
        let mut document: Vec<String> = Vec::new();
        for chunk in chunks {
            let append = transcript.push(chunk);
            let kept = (document.len() + append.scroll).saturating_sub(append.rows.len());
            document.truncate(kept);
            document.extend(append.rows);
        }
        document
    }

    #[test]
    fn a_push_is_invariant_under_chunking() {
        // The property the carry exists for. A stream is cut wherever the
        // socket cut it, so every split of the same bytes must leave the same
        // document -- and the CR cases are the ones where a naive carry gets it
        // wrong in *both* directions: swallowing a break that was really there,
        // or inventing one that was not.
        for stream in [
            "a\r\nb",
            "a\r\rb",
            "a\r\r\nb",
            "a\rb\r\nc\r",
            "\r\n",
            "\r",
            "one\r\ntwo\r\nthree",
            "no breaks at all",
        ] {
            let whole = document(80, &[stream]);
            for at in 0..=stream.len() {
                if !stream.is_char_boundary(at) {
                    continue;
                }
                let (head, tail) = stream.split_at(at);
                assert_eq!(
                    document(80, &[head, tail]),
                    whole,
                    "{stream:?} split at {at} left a different document"
                );
                // and a chunk boundary that carries no bytes changes nothing
                assert_eq!(
                    document(80, &[head, "", tail]),
                    whole,
                    "{stream:?} split at {at} around an empty push"
                );
            }
        }
    }

    #[test]
    fn a_chunk_that_opens_with_its_own_carriage_return_keeps_its_break() {
        // The carry is not "strip the next newline you see". `a\r` + `\rb` is
        // two breaks: the first chunk's CR and the second chunk's own. Reading
        // the *normalized* chunk cannot tell them apart, because `normalize`
        // has already turned both into LFs.
        assert_eq!(
            document(80, &["a\r", "\rb"]),
            vec!["a".to_string(), String::new(), "b".to_string()]
        );
        assert_eq!(document(80, &["a\r", "\rb"]), document(80, &["a\r\rb"]));
    }

    #[test]
    fn a_chunk_that_opens_with_a_crlf_of_its_own_is_one_break_not_two() {
        // `a\r` + `\r\nb`: the first chunk's CR is one break and the second
        // chunk's CRLF is one more -- not two, which is what dropping
        // `normalize`'s CRLF pairing would give, and not zero, which is what
        // stripping on the carry alone would give.
        assert_eq!(
            document(80, &["a\r", "\r\nb"]),
            vec!["a".to_string(), String::new(), "b".to_string()]
        );
        assert_eq!(document(80, &["a\r", "\r\nb"]), document(80, &["a\r\r\nb"]));
    }

    #[test]
    fn an_empty_push_between_the_halves_of_a_crlf_is_transparent() {
        // A delta that carried no text at all still arrives as a push. If it
        // cleared the carry, the LF that follows becomes a second break and the
        // answer grows a blank line the model never wrote.
        assert_eq!(
            document(80, &["a\r", "", "\nb"]),
            vec!["a".to_string(), "b".to_string()]
        );
        assert_eq!(document(80, &["a\r", "", "\nb"]), document(80, &["a\r\nb"]));
    }

    #[test]
    fn a_crlf_split_across_two_pushes_is_one_line_break() {
        // A stream is cut wherever the socket cut it. A CR that ends one push
        // and an LF that opens the next are the same break, and answering both
        // would put a blank row in the middle of an answer.
        let mut split = Transcript::new(80);
        assert_eq!(
            split.push("a\r"),
            Append {
                scroll: 2,
                rows: vec!["a".to_string(), String::new()]
            },
            "the line and the row its successor starts on"
        );
        assert_eq!(
            split.push("\nb"),
            Append {
                scroll: 0,
                rows: vec!["b".to_string()]
            },
            "the leading newline was answered a second time"
        );

        let mut whole = Transcript::new(80);
        assert_eq!(
            whole.push("a\r\nb"),
            Append {
                scroll: 2,
                rows: vec!["a".to_string(), "b".to_string()]
            }
        );
    }

    #[test]
    fn a_push_with_nothing_in_it_asks_the_terminal_for_nothing() {
        // Not a scroll of one blank row: an empty delta is a delta that said
        // nothing, and the wrap of an empty string is still one row.
        let mut transcript = Transcript::new(80);
        assert_eq!(transcript.push(""), Append::nothing());
        assert_eq!(transcript.tail_rows(), 0);
        transcript.push("text");
        assert_eq!(transcript.push(""), Append::nothing());
        assert_eq!(transcript.tail_rows(), 1);
    }

    #[test]
    fn no_row_of_an_append_carries_a_line_break() {
        // A row that still held one would move the terminal's cursor off the
        // row it was placed on, and every row written after it would land a row
        // too high (`frame::render_append`).
        let mut transcript = Transcript::new(12);
        for text in ["one\r\ntwo", "\rthree\n", "a very long line that wraps\n"] {
            for row in transcript.push(text).rows {
                assert!(
                    !row.contains(['\r', '\n']),
                    "a line break survived into a document row: {row:?}"
                );
            }
        }
    }

    #[test]
    fn a_wrapped_tail_scrolls_once_per_row_it_gained() {
        // The number that matters: scroll by fewer rows than were added and the
        // band paints over the transcript; by more and the document grows blank
        // rows nobody wrote.
        let mut transcript = Transcript::new(4);
        assert_eq!(transcript.push("abcdefghijkl").scroll, 3);
        assert_eq!(transcript.tail_rows(), 3);
        assert_eq!(transcript.push("mnop").scroll, 1);
    }

    #[test]
    fn a_line_with_more_rows_than_a_u16_is_counted_in_full() {
        // The count is a property of the text. `editor::MAX_COMPOSER_BYTES` is
        // 8 MiB, and a submission anywhere near it, echoed here and wrapped on
        // a narrow terminal, is past 65535 rows long before it is unusual.
        // Counting these in a `u16` saturates, and `Append::scroll` then
        // understates the rows it carries -- which the renderer reads as "the
        // difference was already painted" and never paints. This is the count
        // path, at the boundary and past it.
        let boundary = usize::from(u16::MAX);
        for rows in [boundary - 1, boundary, boundary + 1, boundary + 2] {
            let mut transcript = Transcript::new(1);
            let append = transcript.push(&"x".repeat(rows));
            assert_eq!(append.rows.len(), rows, "{rows} rows of text");
            assert_eq!(
                append.scroll, rows,
                "a fresh line of {rows} rows scrolled in fewer than it wrote"
            );
            assert_eq!(transcript.tail_rows(), rows);
            // and the next push is still measured against all of them, so the
            // saturation cannot reappear one delta later
            assert_eq!(
                transcript.push("y"),
                Append {
                    scroll: 1,
                    rows: {
                        let mut all = vec!["x".to_string(); rows];
                        all.push("y".to_string());
                        all
                    }
                }
            );
        }
    }

    #[test]
    fn the_rows_of_an_append_are_the_text_that_was_pushed() {
        // Every character in, every character out, in order -- across wrapping
        // and across breaks. A wrap that dropped a byte would be invisible in
        // the counts above.
        let mut transcript = Transcript::new(7);
        // The document the terminal ends up holding, replayed from the appends
        // exactly as `frame::render_append` applies them: scroll by `scroll`,
        // then write `rows` onto the last `rows.len()` rows of what is there.
        let mut document: Vec<String> = Vec::new();
        for text in ["alpha bra", "vo\ncharlie ", "delta\n", "echo"] {
            let append = transcript.push(text);
            let kept = (document.len() + append.scroll).saturating_sub(append.rows.len());
            document.truncate(kept);
            document.extend(append.rows);
        }
        assert_eq!(
            document,
            vec!["alpha ", "bravo", "charlie ", "delta", "echo"]
        );
        assert_eq!(
            document.join("").replace(' ', ""),
            "alphabravocharliedeltaecho",
            "a character was dropped or repeated between the wrap and the append"
        );
    }
}
