//! The band writer: one buffer, one `write_all`, one flush per frame.
//!
//! Everything the TUI puts on the screen goes through here, and that is the
//! point rather than tidiness. The band shares the screen with the terminal's
//! own document, so "what is on those rows is what this module last wrote" is
//! the only thing that makes the band's state knowable at all -- and it stops
//! being true the moment a second writer, or a second `write(2)` inside one
//! frame, can interleave with it.
//!
//! A frame is wrapped in synchronized output (`?2026h` ... `?2026l`), so a
//! terminal that supports it presents the whole band at once instead of
//! painting it row by row, and the cursor is hidden across the paint for the
//! terminals that do not. Every row is placed with `CUP` and clipped to the
//! screen's width: autowrap is off (`?7l` is in the mode set), so a row that
//! ran past the last column would be truncated by the terminal anyway, and
//! clipping here is what keeps the byte count honest.
//!
//! **The whole band is repainted, every frame.** The shadow grid and the cell
//! diff are Phase 2; `docs/parity.md` says so, and says what it costs.

use std::borrow::Cow;
use std::io::{self, Write};

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::layout::Geometry;

/// Begins a frame: synchronized output on, cursor hidden.
const BEGIN_FRAME: &str = "\x1b[?2026h\x1b[?25l";

/// Ends one: cursor shown, synchronized output off.
const END_FRAME: &str = "\x1b[?2026l\x1b[?25h";

/// Erase from the cursor to the end of the screen.
const ERASE_BELOW: &str = "\x1b[J";

/// Erase from the cursor to the end of the row it is on.
const ERASE_LINE: &str = "\x1b[K";

/// The band, and the buffer it is built in.
pub(crate) struct Band {
    /// Kept across frames so building one allocates nothing after the first.
    buffer: Vec<u8>,
    /// The band's top row as of the last frame this band began writing, or
    /// `None` while it has written none.
    ///
    /// It is what the exit clears from ([`super::term::shutdown`]), and the
    /// distinction it carries is load-bearing: a session that drew no band has
    /// no row to clear from, and clearing from the screen's first row instead
    /// would erase output the shell wrote before xfx ran.
    painted: Option<u16>,
}

impl Band {
    pub(crate) fn new() -> Self {
        Self {
            buffer: Vec::new(),
            painted: None,
        }
    }

    /// The band's top row, if this band has painted one.
    pub(crate) fn painted_top(&self) -> Option<u16> {
        self.painted
    }

    /// Builds the bytes of one frame.
    ///
    /// Pure with respect to the terminal -- nothing is written -- so a frame's
    /// geometry is assertable without one, which is the only way the band's row
    /// numbers get tested at all. The buffer it is built in is reused; the copy
    /// handed back is the caller's, and [`commit`](Self::commit) writes that
    /// copy in a single call.
    ///
    /// `rows` are the band's rows, top first, starting at the divider. `cursor`
    /// is `(row, cells)`: the terminal's own one-based row, and the number of
    /// cells to the **left** of the caret on it -- a count, which is what the
    /// composer measures, converted to a one-based column here and nowhere
    /// else.
    pub(crate) fn render(
        &mut self,
        rows: &[String],
        geometry: &Geometry,
        cursor: (u16, u16),
    ) -> Vec<u8> {
        self.buffer.clear();
        self.buffer.extend_from_slice(BEGIN_FRAME.as_bytes());
        // The band's own rows and nothing above them: the erase starts at the
        // divider, so the document keeps every row it has.
        cup(&mut self.buffer, geometry.divider, 1);
        self.buffer.extend_from_slice(ERASE_BELOW.as_bytes());
        for (offset, row) in rows.iter().enumerate() {
            let Ok(offset) = u16::try_from(offset) else {
                break;
            };
            let line = geometry.divider.saturating_add(offset);
            if line > geometry.hint {
                // More rows than the band owns. The extra ones are dropped
                // rather than written onto the row below the screen's last,
                // which a terminal would answer by scrolling the document.
                break;
            }
            cup(&mut self.buffer, line, 1);
            self.buffer
                .extend_from_slice(row_text(row, geometry.cols).as_bytes());
        }
        cup(&mut self.buffer, cursor.0, cursor.1.saturating_add(1));
        self.buffer.extend_from_slice(END_FRAME.as_bytes());
        self.buffer.clone()
    }

    /// [`render`](Self::render) plus exactly one `write_all` and one flush.
    ///
    /// The screen is a parameter rather than `io::stdout()` for the same reason
    /// `term::shutdown_with`'s is: "the session gives up on a screen that
    /// refuses every frame" is a claim about a screen that can be made to
    /// refuse, and a function that reached for the process's own standard
    /// output could only be tested by breaking it.
    pub(crate) fn commit(
        &mut self,
        out: &mut impl Write,
        rows: &[String],
        geometry: &Geometry,
        cursor: (u16, u16),
    ) -> io::Result<()> {
        // Published before the write rather than after it: a frame whose write
        // failed halfway still put bytes on those rows, and the exit has to
        // clear from the top of what was written rather than from nothing.
        self.painted = Some(geometry.divider);
        let frame = self.render(rows, geometry, cursor);
        out.write_all(&frame)?;
        out.flush()
    }

    /// Builds the bytes that put completed rows into the terminal's own
    /// document.
    ///
    /// The screen is scrolled with literal newlines from the bottom row
    /// ([`scroll_one`]) and the rows are placed with `CUP` and an erase
    /// ([`place`]); a row's own carriage returns and linefeeds are removed
    /// rather than written, because either would move the cursor out of the row
    /// it was just placed on.
    ///
    /// `scroll` and `rows` are **different numbers**, and that difference is
    /// what makes a streamed answer paintable at all. `rows` is the whole of
    /// the transcript's unfinished line as it now stands; `scroll` is how many
    /// of its rows are new. A delta that only lengthened the last row scrolls
    /// nothing and rewrites one row; a delta that wrapped it scrolls one and
    /// rewrites both. Scrolling by `rows.len()` either way would push a blank
    /// row into the document for every delta of a stream.
    ///
    /// **Every row is painted on the screen before anything scrolls it off**,
    /// and that is why the new rows go out one at a time rather than as one
    /// burst of linefeeds followed by one burst of placements. A terminal's
    /// scrollback is fed by what *leaves the top of the screen*: a row that was
    /// never on the screen was never in the document, and a batch that scrolled
    /// further than the document area is tall would push blank rows into
    /// scrollback and then paint only the surviving suffix -- losing the
    /// beginning of a long answer permanently, since Phase 1 never repaints a
    /// transcript. So: repaint the rows already on the screen where they are,
    /// then, for each new row, scroll by one and place it on the bottom
    /// document row. The cost is one `CUP` and one linefeed per *new* row, and
    /// a streamed answer adds at most one row per delta.
    ///
    /// **Row counts are `usize` here and are never narrowed.** How many rows an
    /// append carries is a property of the text, not of the screen: one 8 MiB
    /// composer submission (`editor::MAX_COMPOSER_BYTES`) wrapped on a narrow
    /// terminal is well past 65535 rows. A count that saturated at `u16::MAX`
    /// would make `rows.len() - scroll` -- the number this function treats as
    /// *already painted, and therefore already in the document* -- too large by
    /// exactly the amount the count lost, and those rows would never be
    /// painted at all. Same silent, permanent loss as a batch scroll that
    /// outruns the document area, one order of magnitude further out.
    fn render_append(&mut self, scroll: usize, rows: &[String], geometry: &Geometry) -> Vec<u8> {
        self.buffer.clear();
        if scroll == 0 && rows.is_empty() {
            return self.buffer.clone();
        }
        // Everything above the divider is the document; the band's own rows
        // belong to `render`, and nothing here may write at or below it.
        let area = geometry.divider.saturating_sub(1);
        // The rows a previous append already put on the screen, and the ones
        // this append adds. `scroll` past the end of `rows` is not something
        // `Transcript` produces -- an append's rows are its whole tail -- but a
        // scroll is still a scroll, so it is honoured as blank rows rather than
        // silently dropped.
        let fresh = scroll.min(rows.len());
        let settled = rows.len() - fresh;
        for _ in 0..scroll - fresh {
            scroll_one(&mut self.buffer, geometry);
        }
        // The settled rows, repainted where they already are. Only as many of
        // them as the screen still holds: the rest left the top of it, with
        // their text on them, when an earlier append scrolled them there.
        //
        // `shown` is the one count that becomes a `u16`, and it is a *row
        // number* rather than a row total: it is clamped to the document area
        // before the conversion, so the conversion cannot fail, and its
        // fallback is the clamp itself rather than a return that would drop the
        // whole append.
        let shown = u16::try_from(settled.min(usize::from(area))).unwrap_or(area);
        let first = geometry.divider.saturating_sub(shown);
        for (offset, row) in rows[settled - usize::from(shown)..settled]
            .iter()
            .enumerate()
        {
            // A row number too, bounded by `shown` a line above it.
            let offset = u16::try_from(offset).unwrap_or(shown);
            place(
                &mut self.buffer,
                first.saturating_add(offset),
                row,
                geometry,
            );
        }
        // Each new row: one scroll, and the row painted on the row the scroll
        // freed -- so it is on the screen, and stays there until a later
        // append carries it off the top and into the terminal's own scrollback.
        for row in &rows[settled..] {
            scroll_one(&mut self.buffer, geometry);
            place(
                &mut self.buffer,
                geometry.divider.saturating_sub(1),
                row,
                geometry,
            );
        }
        self.buffer.clone()
    }

    /// [`render_append`](Self::render_append) plus exactly one write and one
    /// flush, for the same reason [`commit`](Self::commit) is one of each.
    pub(crate) fn append_document(
        &mut self,
        out: &mut impl Write,
        scroll: usize,
        rows: &[String],
        geometry: &Geometry,
    ) -> io::Result<()> {
        let appended = self.render_append(scroll, rows, geometry);
        if appended.is_empty() {
            return Ok(());
        }
        out.write_all(&appended)?;
        out.flush()
    }
}

/// Scrolls the screen by one row.
///
/// The cursor goes to the **bottom** row first, because a linefeed scrolls a
/// terminal only from the bottom margin -- from anywhere else it merely walks
/// the cursor down, and the row that was supposed to enter native scrollback
/// would still be on the screen (`frame_scroll_plan.zig:8-12`,
/// `terminal_diff.zig:1348-1397`).
fn scroll_one(buffer: &mut Vec<u8>, geometry: &Geometry) {
    cup(buffer, geometry.rows, 1);
    buffer.push(b'\n');
}

/// Places one row of the document: `CUP`, the clipped text, and `EL`.
///
/// The erase is not tidiness, and it is what makes a document row knowable at
/// all. A scroll brings the band's own rows up into the document area, so the
/// row this text lands on is as likely to hold the divider's rule or the
/// composer's prompt as it is to be blank; and a re-wrap can make a row
/// *shorter* than the one it replaces, when a word moves down. Without the
/// erase, either leaves characters behind on a row nothing will ever repaint --
/// they are the terminal's document now.
fn place(buffer: &mut Vec<u8>, line: u16, row: &str, geometry: &Geometry) {
    cup(buffer, line, 1);
    buffer.extend_from_slice(row_text(row, geometry.cols).as_bytes());
    buffer.extend_from_slice(ERASE_LINE.as_bytes());
}

/// `CUP`: place the cursor at a one-based row and column.
fn cup(buffer: &mut Vec<u8>, row: u16, column: u16) {
    // Writing into a `Vec` cannot fail, and there is nothing this function
    // could do about it if it could.
    let _ = write!(buffer, "\x1b[{row};{column}H");
}

/// One row's text: clipped to the screen, with nothing left in it that moves
/// the cursor off the row it was placed on.
fn row_text(row: &str, cols: u16) -> Cow<'_, str> {
    if row.contains(['\r', '\n']) {
        return Cow::Owned(clip(&row.replace(['\r', '\n'], ""), cols).to_string());
    }
    Cow::Borrowed(clip(row, cols))
}

/// As much of `row` as fits in `cols` cells, cut between grapheme clusters.
///
/// Measured in cells rather than in bytes or in `char`s, because the terminal
/// paints cells: a wide character that straddled the last column would be drawn
/// in a column the layout believes is empty.
fn clip(row: &str, cols: u16) -> &str {
    let budget = usize::from(cols);
    let mut used = 0usize;
    let mut end = 0usize;
    for cluster in row.graphemes(true) {
        let width = cluster.width();
        if used + width > budget {
            break;
        }
        used += width;
        end += cluster.len();
    }
    &row[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geometry() -> Geometry {
        crate::tui::layout::solve(24, 80, 1).expect("a band")
    }

    fn band_rows() -> Vec<String> {
        vec!["--".to_string(), "> ".to_string(), "hint".to_string()]
    }

    /// A terminal, to the extent a document append can move one.
    ///
    /// The exact-byte tests above say what goes on the wire; this says what the
    /// wire *does* to a screen, which is the claim that matters for scrollback
    /// and the one a byte string cannot make: "the row reached the terminal's
    /// document" is a fact about rows that left the top of the screen, not
    /// about escape sequences. It is the same instrument as
    /// `probe::tests::Screen` -- the launch push's model -- with text on the
    /// rows instead of marks, because that is what an append writes; Task 19's
    /// QA emulator is where the two become one.
    ///
    /// Four rules, and it refuses everything else loudly, so an append that
    /// grows a fifth cannot be silently unmodelled:
    ///
    /// * `CUP(row, 1)` places the cursor at the start of a row.
    /// * a linefeed on the bottom row scrolls, and the top row leaves for
    ///   native scrollback; anywhere else it walks the cursor down.
    /// * `EL` erases from the cursor to the end of its row.
    /// * printable text overwrites from the cursor rightwards.
    struct Screen {
        rows: u16,
        divider: u16,
        lines: Vec<String>,
        cursor_row: u16,
        cursor_column: usize,
        scrolled_off: Vec<String>,
    }

    impl Screen {
        /// A screen whose band has been painted and whose document is empty --
        /// the state every append after the first frame really meets.
        ///
        /// The band matters: a scroll carries those rows up into the document
        /// area, so a screen that started blank would let an append that never
        /// erases pass.
        fn under_a_painted_band(geometry: &Geometry) -> Self {
            let mut lines = vec![String::new(); usize::from(geometry.rows)];
            for line in &mut lines[usize::from(geometry.divider) - 1..] {
                *line = "\u{2500}".repeat(usize::from(geometry.cols));
            }
            Self {
                rows: geometry.rows,
                divider: geometry.divider,
                lines,
                cursor_row: 1,
                cursor_column: 0,
                scrolled_off: Vec::new(),
            }
        }

        fn feed(&mut self, bytes: &[u8]) {
            let mut rest = std::str::from_utf8(bytes).expect("an append is text and escapes");
            while !rest.is_empty() {
                if let Some(tail) = rest.strip_prefix('\n') {
                    self.linefeed();
                    rest = tail;
                } else if let Some(tail) = rest.strip_prefix(ERASE_LINE) {
                    self.erase_to_end_of_row();
                    rest = tail;
                } else if let Some((row, tail)) = parse_cup(rest) {
                    self.cursor_row = row.clamp(1, self.rows);
                    self.cursor_column = 0;
                    rest = tail;
                } else if rest.starts_with('\u{1b}') {
                    panic!("the append wrote {rest:?}, which this screen does not model");
                } else {
                    let end = rest.find('\u{1b}').unwrap_or(rest.len());
                    let end = rest[..end].find('\n').unwrap_or(end);
                    let (text, tail) = rest.split_at(end);
                    self.print(text);
                    rest = tail;
                }
            }
        }

        fn print(&mut self, text: &str) {
            assert!(
                self.cursor_row < self.divider,
                "the append wrote {text:?} on row {} -- the band's own rows are \
                 `render`'s, and everything at or below the divider ({}) is one",
                self.cursor_row,
                self.divider
            );
            let line = &mut self.lines[usize::from(self.cursor_row) - 1];
            let mut cells: Vec<char> = line.chars().collect();
            for (offset, character) in text.chars().enumerate() {
                let at = self.cursor_column + offset;
                if at < cells.len() {
                    cells[at] = character;
                } else {
                    cells.push(character);
                }
            }
            *line = cells.into_iter().collect();
            self.cursor_column += text.chars().count();
        }

        fn erase_to_end_of_row(&mut self) {
            let line = &mut self.lines[usize::from(self.cursor_row) - 1];
            *line = line.chars().take(self.cursor_column).collect();
        }

        fn linefeed(&mut self) {
            if self.cursor_row < self.rows {
                self.cursor_row += 1;
                return;
            }
            self.scrolled_off.push(self.lines.remove(0));
            self.lines.push(String::new());
        }

        /// The document rows still on the screen, top first.
        fn visible_document(&self) -> Vec<String> {
            self.lines[..usize::from(self.divider) - 1].to_vec()
        }

        /// Everything the document holds: what has scrolled into native
        /// scrollback, then what is still on the screen -- with the blank rows
        /// above the answer dropped, since a document that has never been
        /// filled starts empty.
        fn document(&self) -> Vec<String> {
            let mut rows = self.scrolled_off.clone();
            rows.extend(self.visible_document());
            let first = rows.iter().position(|row| !row.is_empty()).unwrap_or(0);
            rows.split_off(first)
        }
    }

    /// `CSI <row> ; 1 H`, and nothing else with an escape in it.
    fn parse_cup(text: &str) -> Option<(u16, &str)> {
        let rest = text.strip_prefix("\u{1b}[")?;
        let end = rest.find('H')?;
        let (parameters, tail) = rest.split_at(end);
        let (row, column) = parameters.split_once(';')?;
        assert_eq!(
            column, "1",
            "an append left the cursor off the first column"
        );
        Some((row.parse().ok()?, &tail[1..]))
    }

    #[test]
    fn every_frame_is_wrapped_in_synchronized_output_and_hides_the_cursor() {
        let mut band = Band::new();
        let bytes = band.render(&band_rows(), &geometry(), (23, 3));
        let text = String::from_utf8(bytes).expect("utf-8");
        assert!(text.starts_with("\u{1b}[?2026h\u{1b}[?25l"), "{text:?}");
        assert!(text.ends_with("\u{1b}[?2026l\u{1b}[?25h"), "{text:?}");
    }

    #[test]
    fn a_frame_positions_every_row_of_the_band_and_clears_to_the_end_of_it() {
        let mut band = Band::new();
        let bytes = band.render(&band_rows(), &geometry(), (23, 3));
        let text = String::from_utf8(bytes).expect("utf-8");
        // The band's top row is the divider (22), and the paint clears from
        // there down: autowrap is off, so every row is placed with CUP.
        assert!(text.contains("\u{1b}[22;1H"), "{text:?}");
        assert!(text.contains("\u{1b}[23;1H"), "{text:?}");
        assert!(text.contains("\u{1b}[24;1H"), "{text:?}");
        assert!(
            text.contains("\u{1b}[J"),
            "the band was not cleared: {text:?}"
        );
        // and the cursor ends where the composer says it is
        assert!(text.contains("\u{1b}[23;4H"), "{text:?}");
    }

    #[test]
    fn a_document_append_scrolls_the_screen_so_the_row_enters_native_scrollback() {
        let mut band = Band::new();
        let bytes = band.render_append(1, &["answered".to_string()], &geometry());
        let text = String::from_utf8(bytes).expect("utf-8");
        // CUP to the last row, then a literal newline: the terminal really
        // scrolls, so the row that leaves the top is in its own scrollback
        // (`frame_scroll_plan.zig:8-12`, `terminal_diff.zig:1348-1397`).
        assert!(text.contains("\u{1b}[24;1H\n"), "{text:?}");
        assert!(text.contains("answered"));
        assert!(
            !text.contains('\r'),
            "CR before LF was not normalized away: {text:?}"
        );
    }

    #[test]
    fn the_frame_is_exactly_these_bytes_in_exactly_this_order() {
        // The three assertions above are each satisfied by a frame with the
        // right pieces in the wrong order. This one is not: it spells the whole
        // frame out, so a paint that cleared *after* it drew, or placed the
        // caret before the rows, fails here.
        let mut band = Band::new();
        let bytes = band.render(&band_rows(), &geometry(), (23, 2));
        assert_eq!(
            String::from_utf8(bytes).expect("utf-8"),
            "\u{1b}[?2026h\u{1b}[?25l\
             \u{1b}[22;1H\u{1b}[J\
             \u{1b}[22;1H--\
             \u{1b}[23;1H> \
             \u{1b}[24;1Hhint\
             \u{1b}[23;3H\
             \u{1b}[?2026l\u{1b}[?25h"
        );
    }

    #[test]
    fn the_erase_starts_at_the_band_and_never_above_it() {
        // The document is the terminal's, and `ED` from anywhere above the
        // divider would take rows the session never wrote. Proven by position
        // rather than by presence: the erase must follow the divider's own CUP
        // and nothing else.
        let mut band = Band::new();
        let bytes = band.render(&band_rows(), &geometry(), (23, 2));
        let text = String::from_utf8(bytes).expect("utf-8");
        let erase = text.find(ERASE_BELOW).expect("the erase");
        assert_eq!(
            &text[..erase],
            format!("{BEGIN_FRAME}\u{1b}[22;1H"),
            "the erase was not the first thing written after the divider's CUP"
        );
    }

    #[test]
    fn a_row_wider_than_the_screen_is_cut_at_the_last_column() {
        let mut band = Band::new();
        let geometry = crate::tui::layout::solve(24, 20, 1).expect("a narrow band");
        let bytes = band.render(
            &[
                "-".repeat(40),
                format!("> {}", "x".repeat(40)),
                String::new(),
            ],
            &geometry,
            (23, 2),
        );
        let text = String::from_utf8(bytes).expect("utf-8");
        assert!(
            text.contains(&format!("\u{1b}[22;1H{}\u{1b}[23;1H", "-".repeat(20))),
            "the divider ran past the last column: {text:?}"
        );
        assert!(
            !text.contains(&"x".repeat(19)),
            "the composer row was not clipped: {text:?}"
        );
    }

    #[test]
    fn a_wide_character_that_would_straddle_the_last_column_is_dropped_whole() {
        // Two cells per glyph and an odd budget: the clip has to leave the last
        // column empty rather than paint half a character into it.
        let mut band = Band::new();
        let geometry = crate::tui::layout::solve(24, 21, 1).expect("a band");
        let bytes = band.render(&["\u{d55c}".repeat(20)], &geometry, (23, 2));
        let text = String::from_utf8(bytes).expect("utf-8");
        assert!(
            text.contains(&format!("\u{1b}[22;1H{}\u{1b}[23;", "\u{d55c}".repeat(10))),
            "the clip cut a wide character in half: {text:?}"
        );
    }

    #[test]
    fn a_row_carrying_a_line_break_is_placed_rather_than_allowed_to_move_the_cursor() {
        // A transcript row that still had a CRLF in it would scroll the screen
        // from the middle of a frame, and every row placed after it would land
        // one row too high.
        let mut band = Band::new();
        let bytes = band.render_append(
            2,
            &["first\r\nsecond".to_string(), "third\n".to_string()],
            &geometry(),
        );
        let text = String::from_utf8(bytes).expect("utf-8");
        assert!(!text.contains('\r'), "a carriage return survived: {text:?}");
        assert_eq!(
            text.matches('\n').count(),
            2,
            "the only newlines in an append are the ones that scroll: {text:?}"
        );
        assert_eq!(
            text,
            "\u{1b}[24;1H\n\u{1b}[21;1Hfirstsecond\u{1b}[K\
             \u{1b}[24;1H\n\u{1b}[21;1Hthird\u{1b}[K"
        );
    }

    #[test]
    fn an_append_of_nothing_writes_nothing() {
        let mut band = Band::new();
        assert!(band.render_append(0, &[], &geometry()).is_empty());
    }

    #[test]
    fn an_append_that_scrolls_nothing_rewrites_the_row_where_it_already_is() {
        // A delta that only lengthened the last row of an answer. Scrolling
        // here would push a blank row into the document for every few
        // characters the model streams, and the answer would come out
        // double-spaced.
        let mut band = Band::new();
        let bytes = band.render_append(0, &["answered".to_string()], &geometry());
        let text = String::from_utf8(bytes).expect("utf-8");
        assert_eq!(
            text, "\u{1b}[21;1Hanswered\u{1b}[K",
            "a scroll of no rows still moved the screen"
        );
    }

    #[test]
    fn an_append_writes_the_whole_tail_but_scrolls_only_what_is_new() {
        // The wrapping case: one row of the answer was already on the screen,
        // so it is repainted where it is, and only the row the wrap added
        // costs a scroll.
        let mut band = Band::new();
        let bytes = band.render_append(1, &["abcd".to_string(), "efgh".to_string()], &geometry());
        let text = String::from_utf8(bytes).expect("utf-8");
        assert_eq!(
            text, "\u{1b}[21;1Habcd\u{1b}[K\u{1b}[24;1H\n\u{1b}[21;1Hefgh\u{1b}[K",
            "the append moved the screen by something other than its new rows"
        );
    }

    #[test]
    fn every_appended_row_is_on_the_screen_before_anything_scrolls_it_off() {
        // The rows of an answer taller than the document area. A batch that
        // scrolled `rows.len()` times and then painted only the surviving
        // suffix would push *blank* rows into native scrollback and lose the
        // beginning of the answer for good -- Phase 1 never repaints a
        // transcript, so what is not in scrollback is gone. Asserted against a
        // screen rather than against bytes, because "the row reached the
        // terminal's document" is a claim about what the bytes *did*.
        let geometry = crate::tui::layout::solve(6, 20, 1).expect("the smallest band");
        assert_eq!(geometry.divider, 4, "three document rows");
        let rows: Vec<String> = (1..=5).map(|row| format!("row{row}")).collect();

        let mut band = Band::new();
        let mut screen = Screen::under_a_painted_band(&geometry);
        screen.feed(&band.render_append(5, &rows, &geometry));

        assert_eq!(
            screen.document(),
            rows,
            "a row of the answer never reached the terminal's document"
        );
        assert_eq!(
            screen.visible_document(),
            vec!["row3", "row4", "row5"],
            "the screen does not end on the tail of the answer"
        );
        assert_eq!(
            screen.scrolled_off.len(),
            5,
            "the append moved the screen by something other than the rows it added"
        );
    }

    /// How many times a rendered append scrolled the screen, and how many rows
    /// it placed. Counted from the bytes rather than from the arguments,
    /// because "the row was written" is the claim, and at these sizes the
    /// `Screen` model above is the wrong instrument -- painting a hundred
    /// thousand rows through it proves nothing the counts do not.
    fn scrolls_and_placements(bytes: &[u8], geometry: &Geometry) -> (usize, usize) {
        let text = std::str::from_utf8(bytes).expect("an append is text and escapes");
        let bottom = format!("\u{1b}[{};1H\n", geometry.rows);
        // Every `place` ends in an erase and nothing else emits one, so the
        // erases are the rows -- which is what makes this count exact rather
        // than a guess from `CUP`s that also aim the scroll.
        (
            text.matches(&bottom).count(),
            text.matches(ERASE_LINE).count(),
        )
    }

    #[test]
    fn an_append_with_more_rows_than_a_u16_scrolls_and_places_every_one_of_them() {
        // A row count is a property of the *text*, not of the screen: an 8 MiB
        // composer submission (`editor::MAX_COMPOSER_BYTES`) wrapped on a
        // narrow terminal is well past 65535 rows. A count narrowed to a `u16`
        // anywhere on this path saturates, `rows.len() - scroll` then names
        // more rows as "already painted" than were ever painted, and the
        // difference is dropped -- the beginning of the answer, silently and
        // permanently, exactly as a batch scroll drops it at screen scale.
        //
        // Asserted at the boundary and past it, and by counting rather than by
        // painting: every row of a fresh append is scrolled in and placed.
        let geometry = crate::tui::layout::solve(24, 80, 1).expect("a band");
        let boundary = usize::from(u16::MAX);
        for count in [
            boundary - 1,
            boundary,
            boundary + 1,
            boundary + 2,
            boundary * 2 + 3,
        ] {
            let rows = vec!["x".to_string(); count];
            let mut band = Band::new();
            let bytes = band.render_append(count, &rows, &geometry);
            assert_eq!(
                scrolls_and_placements(&bytes, &geometry),
                (count, count),
                "an append of {count} rows lost some of them"
            );
        }
    }

    #[test]
    fn an_append_that_grew_past_a_u16_still_repaints_only_what_the_screen_holds() {
        // The other half of the same count. A tail that has outgrown a `u16`
        // is mostly in scrollback, so the settled rows cost nothing to skip --
        // but only the ones the screen no longer holds may be skipped, and the
        // renderer works that out from `rows.len() - scroll`, which is only
        // right if neither number saturated.
        let geometry = crate::tui::layout::solve(24, 80, 1).expect("a band");
        let count = usize::from(u16::MAX) + 10;
        let rows = vec!["x".to_string(); count];
        let mut band = Band::new();
        let bytes = band.render_append(3, &rows, &geometry);
        // Three scrolls for the three new rows, and one placement per row the
        // screen still holds: the whole document area, plus the three that
        // scrolled in under it.
        let area = usize::from(geometry.divider) - 1;
        assert_eq!(
            scrolls_and_placements(&bytes, &geometry),
            (3, area + 3),
            "the settled rows were repainted by some number other than what \
             the screen holds"
        );
        let text = String::from_utf8(bytes).expect("utf-8");
        // and the settled rows are repainted only as far up as the document
        // area reaches -- row 1, never row 0 and never a negative one.
        assert!(
            text.contains("\u{1b}[1;1H"),
            "the document area's first row"
        );
        assert!(!text.contains("\u{1b}[0;1H"), "a row no terminal has");
    }

    #[test]
    fn a_row_the_scroll_carried_the_band_into_is_erased_before_it_is_written() {
        // A scroll moves the band's own rows up into the document area, so the
        // row an appended line lands on holds the divider's rule. Writing a
        // shorter line over it without an erase leaves the rest of the rule
        // behind -- on a document row nothing will ever repaint.
        let geometry = crate::tui::layout::solve(6, 20, 1).expect("the smallest band");
        let mut band = Band::new();
        let mut screen = Screen::under_a_painted_band(&geometry);
        screen.feed(&band.render_append(1, &["ok".to_string()], &geometry));
        assert_eq!(
            screen.visible_document(),
            vec!["", "", "ok"],
            "the band's rule survived into the document"
        );
    }

    #[test]
    fn a_streamed_answer_lands_in_the_terminals_document_exactly_once() {
        // The whole path, end to end and against a screen: a transcript fed in
        // chunks the way a provider cuts them, on a screen far too short to
        // hold the answer. Every row must be in the document exactly once, in
        // order -- which is the one property that a lost scroll, a duplicated
        // repaint, and an off-by-one placement each break differently.
        let geometry = crate::tui::layout::solve(6, 20, 1).expect("the smallest band");
        let mut transcript = crate::tui::transcript::Transcript::new(geometry.cols);
        let mut band = Band::new();
        let mut screen = Screen::under_a_painted_band(&geometry);
        for chunk in [
            "the quick brown ",
            "fox jumps over",
            " the lazy dog\r\n",
            "and then it ",
            "rested",
        ] {
            let append = transcript.push(chunk);
            screen.feed(&band.render_append(append.scroll, &append.rows, &geometry));
        }
        assert_eq!(
            screen.document().join("\n"),
            "the quick brown fox \njumps over the lazy \ndog\nand then it rested",
            "the answer did not survive the screen it was streamed onto"
        );
    }

    #[test]
    fn a_band_that_has_painted_nothing_has_no_row_to_clear_from() {
        let band = Band::new();
        assert_eq!(
            band.painted_top(),
            None,
            "a session that drew nothing would clear a screen it never wrote on"
        );
    }

    #[test]
    fn a_frame_the_screen_refused_still_leaves_a_row_to_clear_from() {
        // The write failed, but not before the terminal saw some of it -- and
        // an exit that cleared from nothing would leave that on the screen.
        struct Refuses;
        impl Write for Refuses {
            fn write(&mut self, _bytes: &[u8]) -> io::Result<usize> {
                Err(io::Error::new(io::ErrorKind::BrokenPipe, "gone"))
            }
            fn flush(&mut self) -> io::Result<()> {
                Err(io::Error::new(io::ErrorKind::BrokenPipe, "gone"))
            }
        }

        let mut band = Band::new();
        band.commit(&mut Refuses, &band_rows(), &geometry(), (23, 2))
            .expect_err("the screen refused the frame");
        assert_eq!(band.painted_top(), Some(22));
    }

    #[test]
    fn a_committed_frame_is_the_rendered_frame_and_one_write() {
        let mut band = Band::new();
        let expected = band.render(&band_rows(), &geometry(), (23, 2));
        let mut screen = Vec::new();
        band.commit(&mut screen, &band_rows(), &geometry(), (23, 2))
            .expect("commit");
        assert_eq!(screen, expected);
        assert_eq!(band.painted_top(), Some(22));
    }

    #[test]
    fn more_rows_than_the_band_owns_are_dropped_rather_than_scrolling_the_document() {
        // A fourth row on a three-row band would be written to row 25 of a
        // 24-row screen, which a terminal answers by scrolling everything up a
        // row -- taking a row of the user's document with it.
        let mut band = Band::new();
        let bytes = band.render(
            &[
                "--".to_string(),
                "> ".to_string(),
                "hint".to_string(),
                "overflow".to_string(),
            ],
            &geometry(),
            (23, 2),
        );
        let text = String::from_utf8(bytes).expect("utf-8");
        assert!(!text.contains("overflow"), "{text:?}");
        assert!(!text.contains("\u{1b}[25;1H"), "{text:?}");
    }
}
