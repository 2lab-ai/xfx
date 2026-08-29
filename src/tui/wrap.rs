//! The one soft-wrap: text in, rows out, measured in cells.
//!
//! Two surfaces need the same answer to "where does this text break" -- the
//! transcript, which turns finished text into document rows, and the composer,
//! which turns typed text into band rows and has to put a caret on one of them.
//! Wrapping them separately is how the caret ends up a row away from the
//! character it is supposed to sit on, so there is one function and both call
//! it.
//!
//! A [`Row`] is a **range of the text**, not a copy of it. The caller already
//! owns the string; handing back byte offsets is what lets the composer map a
//! cursor into a row ([`cursor_point`]) without a second pass over anything.
//!
//! Three rules, and each of them is a decision rather than an accident:
//!
//! * **A word moves whole.** A word that does not fit what is left of a row
//!   starts the next one (`visual_layout.zig:278-282`); a word longer than a
//!   whole row is the exception, and breaks at the grapheme that would cross
//!   the margin, because the alternative is a row with nothing on it.
//! * **Spaces hang.** The run of spaces that ends a row is *measured* -- so a
//!   [`Row`]'s `width` may be wider than `cols` -- and left on that row rather
//!   than carried down, because the painter clips them
//!   (`visual_layout.zig:146-148`) and a continuation row that began with a
//!   column of blanks would be the visible cost of a character nobody can see.
//! * **Cells, not bytes and not `char`s.** A terminal paints cells, so a wide
//!   glyph costs two and never straddles the margin. Control characters cost
//!   none: they are the escape bytes and line breaks that live *in* the text
//!   and paint nothing. The **tab** is the exception, and item 16 is why: a
//!   paste is the one way a tab reaches the composer, and a character measured
//!   at no cells but written into the text puts the caret a column away from
//!   the glyph it is supposed to sit on. It measures [`TAB_WIDTH`] here and is
//!   painted as that many spaces ([`expand_tabs`], `super::frame::row_text`),
//!   so measure and paint are one number in two places rather than two answers.

use std::borrow::Cow;

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// How many cells a tab paints.
///
/// **A fixed run, not a tab stop.** Every measurement here is of one cluster at
/// a time ([`width`]), so a width that depended on the column the tab happens
/// to be in could not be expressed -- and a wrap that answered "it depends"
/// would disagree with the painter on any row it broke. Four rather than eight
/// because the composer is a band a few rows tall inside a gutter: one
/// character taking a tenth of an eighty-column row is indentation that has
/// eaten the line it indents.
pub(crate) const TAB_WIDTH: u16 = 4;

/// `text` with every tab replaced by the cells it measures.
///
/// The painter's half of [`TAB_WIDTH`]: the composer hands its rows over
/// already expanded, so what reaches the terminal is spaces the wrap has
/// already counted rather than a control the terminal would obey with a tab
/// stop of its own.
///
/// Borrowed when there is no tab, which is every row of nearly every session:
/// the allocation belongs to the pasted indentation that needs it.
pub(crate) fn expand_tabs(text: &str) -> Cow<'_, str> {
    if !text.contains('\t') {
        return Cow::Borrowed(text);
    }
    Cow::Owned(text.replace('\t', &" ".repeat(usize::from(TAB_WIDTH))))
}

/// One visual row: the half-open byte range of the text it shows, and the
/// cells it takes.
///
/// `width` is what the text really measures, which is not always what fits: a
/// row that ends in hanging spaces is wider than the screen on purpose, and the
/// painter is what cuts it (`super::frame::clip`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Row {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) width: u16,
}

/// How many cells `text` paints.
///
/// Control characters count as none. `unicode_width` gives a lone control
/// character a cell of its own -- reasonable for a string that will be printed
/// with its controls visible, wrong for one a terminal will *obey* -- so the
/// clusters made only of them are dropped here rather than measured.
///
/// **An escape sequence is free whole**, not just its `ESC`. `\x1b[31m` is one
/// instruction the terminal executes and four characters it draws nothing for,
/// and measuring the `[31m` as text is how a row with a colour in it loses four
/// columns of the answer to something invisible.
///
/// **Every** sequence, not only the ones a row is allowed to carry, and that is
/// the load-bearing half. Whether a sequence may reach the terminal is
/// [`super::frame::row_text`]'s question and it is asked *after* this one; if
/// this measured a rejected `\x1b[2J` as three printing characters, a narrow
/// row could break in the middle of it -- and then the removal, which only
/// knows how to take a **whole** sequence out, would find half of one on each
/// row and leave the printable tail of it on the screen. One tokenizer,
/// [`super::pacer::escape_at`], answers "how many bytes travel together" for
/// this, for [`super::frame::clip`], and for the removal; the allowlist decides
/// only what is kept.
pub(crate) fn width(text: &str) -> u16 {
    let cells: usize = painting(text)
        .map(|painted| cluster_cells(painted.cluster))
        .sum();
    u16::try_from(cells).unwrap_or(u16::MAX)
}

/// One cluster that paints something, and the bytes it travels with.
///
/// `start` is where the escape sequences *in front of* the cluster begin, and
/// that is the whole reason this is a struct rather than a pair: a break taken
/// at `start` moves a colour down with the word it colours, and a break taken
/// after them would leave the colour on the row above and cut the answer's
/// attributes off the text they belong to. For a sequence that will be removed
/// rather than kept it matters more than cosmetically -- a break inside one is
/// a fragment the removal cannot recognize.
#[derive(Debug, Clone, Copy)]
struct Painted<'a> {
    start: usize,
    end: usize,
    cluster: &'a str,
}

/// The clusters of `text` that paint something, with the escape sequences
/// stepped over.
///
/// A sequence is not a cluster to be measured, wrapped after, or broken inside:
/// it belongs to the text around it. Its bytes stay in the row, because a
/// [`Row`] is a range and the ranges tile the text -- a sequence between two
/// clusters is inside whichever row those clusters put it in.
fn painting(text: &str) -> impl Iterator<Item = Painted<'_>> {
    let mut cursor = 0usize;
    std::iter::from_fn(move || {
        let start = cursor;
        while cursor < text.len() {
            let rest = &text[cursor..];
            if let Some(len) = super::pacer::escape_at(rest) {
                cursor += len;
                continue;
            }
            let cluster = rest.graphemes(true).next().unwrap_or(rest);
            cursor += cluster.len();
            return Some(Painted {
                start,
                end: cursor,
                cluster,
            });
        }
        None
    })
}

/// The rows `text` occupies on a screen `cols` wide, in order, tiling the whole
/// of it.
///
/// Every byte of `text` is in exactly one row, the first row starts at 0, the
/// last ends at `text.len()`, and a row's `end` is the next row's `start` --
/// including the line break that ended a row, which belongs to the row it ended
/// rather than to the one it began. There is always at least one row: a text
/// that ends in a newline has an empty last row, which is where the caret goes
/// after the newline is typed.
pub(crate) fn wrap(text: &str, cols: u16) -> Vec<Row> {
    // A zero-column screen is not one the layout will ever produce
    // (`layout::MIN_COLS`), and a zero budget here would be an empty row per
    // grapheme forever.
    let cols = cols.max(1);
    let mut rows = Vec::new();
    // Where the row being built starts, and what it has taken so far.
    let mut start = 0usize;
    let mut used = 0u16;
    // Where the last word began, when a space has been seen on this row since
    // the row began. That is the point a break moves the word from.
    let mut word: Option<usize> = None;
    let mut after_space = false;

    for painted in painting(text) {
        let Painted {
            start: index,
            end,
            cluster,
        } = painted;
        if is_line_break(cluster) {
            // A hard break always breaks, and takes its own bytes with it --
            // and `end` rather than `index + cluster.len()`, because a colour
            // in front of the break travels with it and a row that ended
            // between the two would end inside an escape sequence.
            rows.push(Row {
                start,
                end,
                width: used,
            });
            start = end;
            used = 0;
            word = None;
            after_space = false;
            continue;
        }
        let cells = cluster_width(cluster);
        if cluster == " " {
            // Hanging: a space past the margin is measured and stays where it
            // is, so the row it ends can be wider than the screen.
            used = used.saturating_add(cells);
            after_space = true;
            continue;
        }
        if after_space {
            word = Some(index);
            after_space = false;
        }
        // `index > start` is what stops a single grapheme wider than the whole
        // row from pushing an empty row and trying again forever.
        if used.saturating_add(cells) > cols && index > start {
            let split = match word {
                // The word began on this row, so the whole of it moves down.
                Some(at) if at > start => at,
                // Either no word boundary on this row at all, or the row *is*
                // the middle of one word: break at the grapheme that would
                // cross the margin.
                _ => index,
            };
            rows.push(Row {
                start,
                end: split,
                width: width(&text[start..split]),
            });
            start = split;
            // Whatever of the word had already been counted onto the row that
            // just ended is now the beginning of this one.
            used = width(&text[split..index]);
            word = None;
        }
        used = used.saturating_add(cells);
    }
    rows.push(Row {
        start,
        end: text.len(),
        width: used,
    });
    rows
}

/// Which row a byte offset is on, and how many cells are to the left of it
/// there.
///
/// A cursor sitting exactly on a soft-wrap boundary belongs to the **following**
/// row (`visual_layout.zig:432-437`): the character it is about to type lands
/// there, so that is where the caret has to be. A cursor at the very end of the
/// text is on the last row, which is the one case where an offset equal to a
/// row's `end` stays on that row.
///
/// `text` is a parameter, and the plan's signature (`rows` and `cursor` only)
/// is not implementable without it: a column is a count of **cells**, and no
/// number of byte offsets can tell you how many cells the bytes between them
/// paint. See `a_cursor_after_a_wide_glyph_is_a_column_of_cells_rather_than_of_bytes`.
///
/// # The caller's obligation
///
/// **`cursor` must be a grapheme-cluster boundary of `text`.** Two things rest
/// on it, and they fail differently:
///
/// * An offset that is not even a `char` boundary **panics** the slice. This
///   function does not clamp to one, deliberately: a caret silently moved to a
///   neighbouring byte is a caret that lies about where the next character will
///   land, and Task 9's editor already moves by grapheme
///   (`unicode_segmentation`) precisely so it cannot produce one.
/// * An offset that is a `char` boundary *inside* a cluster -- between an `e`
///   and the combining accent that composes with it, or between two scalars of
///   a ZWJ emoji -- does not panic and is not meaningful either: it reports a
///   column in the middle of a cell the terminal draws as one glyph.
///
/// So this is Task 9 and Task 10's invariant to keep, not this function's to
/// repair. `a_composed_grapheme_is_one_column_however_many_bytes_it_took` pins
/// what the boundaries themselves must report.
// Task 10's editor is the first caller -- `Up`/`Down` and the caret both ask
// this question -- and it is here rather than there because it is the inverse
// of `wrap`, and an inverse that lives in another module drifts from it.
#[allow(dead_code)]
pub(crate) fn cursor_point(text: &str, rows: &[Row], cursor: usize) -> (usize, u16) {
    for (index, row) in rows.iter().enumerate() {
        let last = index + 1 == rows.len();
        if cursor >= row.end && !last {
            continue;
        }
        if cursor >= row.end {
            // The end of the last row: the row's own measurement, which is
            // what a hanging run of spaces makes different from `cols`.
            return (index, row.width);
        }
        return (index, width(&text[row.start..cursor.max(row.start)]));
    }
    (0, 0)
}

/// Whether a grapheme cluster is a hard line break.
///
/// `"\r\n"` is **one** cluster, which is why this is a function rather than a
/// comparison against `'\n'`: a CRLF matched as two would end a row on the CR
/// and open an empty one for the LF.
///
/// A **lone** carriage return is not one. What a bare CR means is a policy, and
/// it has exactly one owner -- [`super::transcript::normalize`], which turns it
/// into a newline before any of this text reaches here -- so restating it here
/// would be a second answer to the same question with nothing keeping the two
/// in step. Reaching this function it is an ordinary control character,
/// measured at no cells and stripped by the painter.
fn is_line_break(cluster: &str) -> bool {
    cluster == "\n" || cluster == "\r\n"
}

/// One cluster's cells, as [`width`] counts them.
fn cluster_width(cluster: &str) -> u16 {
    u16::try_from(cluster_cells(cluster)).unwrap_or(u16::MAX)
}

/// One cluster's cells: none when it paints nothing.
///
/// The tab is the one control with a width, because it is the one control a
/// draft may hold and the painter draws it ([`TAB_WIDTH`]).
fn cluster_cells(cluster: &str) -> usize {
    if cluster == "\t" {
        return usize::from(TAB_WIDTH);
    }
    if cluster.chars().all(char::is_control) {
        return 0;
    }
    cluster.width()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts<'a>(text: &'a str, rows: &[Row]) -> Vec<&'a str> {
        rows.iter().map(|row| &text[row.start..row.end]).collect()
    }

    #[test]
    fn a_word_that_does_not_fit_the_rest_of_a_row_wraps_whole() {
        // visual_layout.zig:278-282
        let text = "alpha bravo";
        assert_eq!(texts(text, &wrap(text, 8)), vec!["alpha ", "bravo"]);
    }

    #[test]
    fn spaces_hang_past_the_right_margin_instead_of_wrapping() {
        // visual_layout.zig:146-148: the painter clips them, so a continuation
        // row starts at the word rather than at a column of blanks.
        let text = "alpha     bravo";
        let rows = wrap(text, 6);
        assert_eq!(texts(text, &rows), vec!["alpha     ", "bravo"]);
        assert_eq!(
            rows[0].width, 10,
            "a hanging space is measured, then clipped"
        );
    }

    #[test]
    fn a_word_wider_than_a_row_splits_per_grapheme() {
        let text = "abcdefghij";
        assert_eq!(texts(text, &wrap(text, 4)), vec!["abcd", "efgh", "ij"]);
    }

    #[test]
    fn a_hard_newline_always_breaks() {
        let text = "a\nb";
        assert_eq!(texts(text, &wrap(text, 80)), vec!["a\n", "b"]);
    }

    #[test]
    fn a_wide_glyph_costs_two_cells_and_never_straddles_the_margin() {
        let text = "\u{c548}\u{b155}\u{d558}\u{c138}\u{c694}";
        let rows = wrap(text, 5);
        assert_eq!(
            texts(text, &rows),
            vec!["\u{c548}\u{b155}", "\u{d558}\u{c138}", "\u{c694}"]
        );
    }

    #[test]
    fn a_cursor_at_a_soft_wrap_boundary_belongs_to_the_following_row() {
        // visual_layout.zig:432-437
        let text = "alpha bravo";
        let rows = wrap(text, 8);
        assert_eq!(cursor_point(text, &rows, 6), (1, 0));
        assert_eq!(cursor_point(text, &rows, 11), (1, 5));
    }

    #[test]
    fn a_cursor_after_a_wide_glyph_is_a_column_of_cells_rather_than_of_bytes() {
        // Why `cursor_point` takes the text. Each of these glyphs is three
        // bytes and two cells, so a column derived from `cursor - row.start`
        // -- the only column the plan's `(rows, cursor)` signature can compute
        // -- puts the caret at column 6 on a row that is 4 cells wide, three
        // columns past the character it is supposed to sit on.
        let text = "\u{c548}\u{b155}";
        let rows = wrap(text, 80);
        assert_eq!(cursor_point(text, &rows, text.len()), (0, 4));
        assert_eq!(cursor_point(text, &rows, 3), (0, 2));
    }

    #[test]
    fn a_composed_grapheme_is_one_column_however_many_bytes_it_took() {
        // A cluster is one cell's worth of caret movement whatever it cost in
        // bytes: `e` + a combining acute is three bytes and one column, and a
        // ZWJ family emoji is eighteen bytes and two. A caret stepped in bytes,
        // or in `char`s, lands somewhere the terminal drew nothing.
        let acute = "e\u{301}";
        let family = "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}";
        let text = format!("{acute}{family}!");
        let rows = wrap(&text, 80);
        assert_eq!(rows.len(), 1, "one row: {rows:?}");

        assert_eq!(cursor_point(&text, &rows, 0), (0, 0));
        assert_eq!(
            cursor_point(&text, &rows, acute.len()),
            (0, 1),
            "the accent is composed onto the letter, not a column of its own"
        );
        assert_eq!(
            cursor_point(&text, &rows, acute.len() + family.len()),
            (0, 3),
            "the family emoji is one cluster and two cells"
        );
        assert_eq!(cursor_point(&text, &rows, text.len()), (0, 4));
    }

    #[test]
    fn a_cursor_before_a_hard_newline_stays_on_the_row_that_break_ended() {
        // The break's own bytes belong to the row it ended, so an offset
        // *inside* the break is still that row -- and the offset after it is
        // the first column of the next one, which is where the caret goes the
        // instant the newline is typed.
        let text = "ab\ncd";
        let rows = wrap(text, 80);
        assert_eq!(cursor_point(text, &rows, 2), (0, 2));
        assert_eq!(cursor_point(text, &rows, 3), (1, 0));
        assert_eq!(cursor_point(text, &rows, 5), (1, 2));
    }

    #[test]
    fn a_text_that_ends_in_a_newline_has_an_empty_row_after_it() {
        // Otherwise the caret after a typed newline is reported on the row the
        // newline ended, which is the row above the one it is really on.
        let text = "line\n";
        let rows = wrap(text, 80);
        assert_eq!(texts(text, &rows), vec!["line\n", ""]);
        assert_eq!(cursor_point(text, &rows, 5), (1, 0));
    }

    #[test]
    fn an_empty_text_is_one_empty_row_rather_than_no_rows() {
        let rows = wrap("", 80);
        assert_eq!(
            rows,
            vec![Row {
                start: 0,
                end: 0,
                width: 0
            }]
        );
        assert_eq!(cursor_point("", &rows, 0), (0, 0));
    }

    #[test]
    fn the_rows_tile_the_text_with_no_gap_and_no_overlap() {
        // The property every caller rests on: a row is a *range*, so a break
        // that dropped or repeated a byte would show up as text the transcript
        // never printed or printed twice, and no single example above would
        // catch it.
        for text in [
            "",
            "a",
            "alpha bravo charlie delta",
            "alpha     bravo",
            "abcdefghij",
            "a\nb\n",
            "\u{c548}\u{b155}\u{d558}\u{c138}\u{c694} ok",
            "   leading",
            "trailing   ",
        ] {
            for cols in [1u16, 2, 4, 6, 8, 80] {
                let rows = wrap(text, cols);
                assert!(!rows.is_empty(), "{text:?} at {cols} produced no rows");
                assert_eq!(rows[0].start, 0, "{text:?} at {cols}");
                assert_eq!(
                    rows.last().expect("a row").end,
                    text.len(),
                    "{text:?} at {cols} lost the end of the text"
                );
                for pair in rows.windows(2) {
                    assert_eq!(
                        pair[0].end, pair[1].start,
                        "{text:?} at {cols} has a gap or an overlap between rows"
                    );
                    assert!(
                        pair[0].start < pair[0].end,
                        "{text:?} at {cols} produced an empty row in the middle"
                    );
                }
            }
        }
    }

    #[test]
    fn a_glyph_wider_than_the_whole_row_takes_the_row_rather_than_looping() {
        // Two cells will not fit one column, and a break that fired anyway
        // would push an empty row and meet the same glyph again.
        let text = "\u{c548}\u{b155}";
        let rows = wrap(text, 1);
        assert_eq!(texts(text, &rows), vec!["\u{c548}", "\u{b155}"]);
        assert_eq!(rows[0].width, 2, "the glyph is measured, then clipped");
    }

    #[test]
    fn control_characters_cost_no_cells() {
        // They are obeyed by the terminal rather than painted, and
        // `unicode_width` gives a lone control character a column of its own.
        assert_eq!(width("a\nb"), 2);
        assert_eq!(width("\r\n"), 0);
        assert_eq!(width(""), 0);
        assert_eq!(width("\u{c548}"), 2);
    }

    #[test]
    fn a_tab_measures_the_cells_the_painter_writes_for_it() {
        // Item 16: a paste is the one way a tab reaches a draft, and Phase 1
        // measured it at nothing and painted nothing -- which is consistent
        // until you ask where the caret goes, because the *text* is still there
        // and every offset after it is a column short. One number, and the
        // painter's expansion is the same one.
        // The number is spelled out as well as read from the constant, for the
        // reason `history`'s cap is: a test that took the width from the thing
        // enforcing it would pass for any width, including one that puts a
        // tenth of an eighty-column row under one character.
        assert_eq!(TAB_WIDTH, 4);
        assert_eq!(width("\t"), TAB_WIDTH);
        assert_eq!(width("a\tb"), TAB_WIDTH + 2);
        let painted = format!("a{}b", " ".repeat(usize::from(TAB_WIDTH)));
        assert_eq!(expand_tabs("a\tb"), painted);
        assert_eq!(
            width(&painted),
            width("a\tb"),
            "the painted row is a different number of cells from the measured one"
        );
        // Borrowed when there is nothing to expand, which is every row of
        // nearly every session.
        assert!(matches!(
            expand_tabs("plain"),
            std::borrow::Cow::Borrowed(_)
        ));
        // And the cells are cells the break is taken on: four of them do not
        // fit beside `ab` in a four-column row, so the tab takes a row of its
        // own -- which a control measured at nothing never could.
        let text = "ab\tcd";
        let rows = wrap(text, 4);
        assert_eq!(texts(text, &rows), vec!["ab", "\t", "cd"]);
    }

    #[test]
    fn an_escape_sequence_is_free_whole_rather_than_only_its_escape_byte() {
        // What Task 13 changed, and why. Counting only the `ESC` as a control
        // left `[31m` measured as four printing characters, so a row with two
        // colours in it lost eight columns of text to instructions that paint
        // nothing -- and `frame::clip`, which counted the `ESC` as a column of
        // its own, then cut the row somewhere else again. One sequence, one
        // answer, no cells.
        assert_eq!(width("\u{1b}[31m"), 0);
        assert_eq!(width("\u{1b}[1;38;5;200mred\u{1b}[0m"), 3);
        // **Every** sequence, not only the ones a row may keep. Measuring a
        // rejected one as text is how a narrow row comes to break inside it,
        // and a removal that only knows how to take a whole sequence out then
        // leaves the printable half of one on the screen.
        assert_eq!(width("\u{1b}[2J"), 0, "the erase was measured as text");
        assert_eq!(width("a\u{1b}[?1049hb"), 2);
        assert_eq!(width("a\u{1b}]0;title\u{7}b"), 2);
        // A sequence that has not finished is still not text: the bytes that
        // have arrived travel together and paint nothing.
        assert_eq!(width("ab\u{1b}[3"), 2);
    }

    #[test]
    fn a_colour_never_takes_the_break_that_belongs_to_the_text_around_it() {
        // A row is broken where the *text* crosses the margin. A sequence that
        // took a break point with it would put the colour on one row and the
        // word it colours on the next, and -- because a break is where a row's
        // bytes end -- could cut the sequence itself in half.
        let text = "alpha \u{1b}[31mbravo";
        assert_eq!(
            texts(text, &wrap(text, 8)),
            vec!["alpha ", "\u{1b}[31mbravo"]
        );
        let split = "abcd\u{1b}[31mefgh";
        assert_eq!(
            texts(split, &wrap(split, 4)),
            vec!["abcd", "\u{1b}[31mefgh"],
            "a row ended inside an escape sequence"
        );
    }
}
