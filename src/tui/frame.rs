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
    /// The cursor is moved to the **bottom** row first and the screen is
    /// scrolled with literal newlines, because a linefeed scrolls a terminal
    /// only from the bottom margin -- from anywhere else it merely walks the
    /// cursor down, and the rows that were supposed to enter native scrollback
    /// would still be on the screen (`frame_scroll_plan.zig:8-12`,
    /// `terminal_diff.zig:1348-1397`). The appended rows are then placed, with
    /// `CUP`, on the document rows the scroll made free.
    ///
    /// A row's own carriage returns and linefeeds are removed rather than
    /// written: they are the document's text, and either one would move the
    /// cursor out of the row it was just placed on.
    // Task 7 is the transcript that produces these rows; the mechanics are here
    // because they are the band writer's, and splitting them across two commits
    // would leave the scroll untested for the length of one.
    #[allow(dead_code)]
    pub(crate) fn render_append(&mut self, rows: &[String], geometry: &Geometry) -> Vec<u8> {
        self.buffer.clear();
        let Ok(count) = u16::try_from(rows.len()) else {
            return self.buffer.clone();
        };
        if count == 0 {
            return self.buffer.clone();
        }
        cup(&mut self.buffer, geometry.rows, 1);
        for _ in 0..count {
            self.buffer.push(b'\n');
        }
        // The rows the scroll freed: the `count` document rows immediately
        // above the divider, top first.
        let first = geometry.divider.saturating_sub(count);
        for (offset, row) in rows.iter().enumerate() {
            let Ok(offset) = u16::try_from(offset) else {
                break;
            };
            cup(&mut self.buffer, first.saturating_add(offset), 1);
            self.buffer
                .extend_from_slice(row_text(row, geometry.cols).as_bytes());
        }
        self.buffer.clone()
    }

    /// [`render_append`](Self::render_append) plus exactly one write and one
    /// flush, for the same reason [`commit`](Self::commit) is one of each.
    // Task 7's document append is this function's first caller.
    #[allow(dead_code)]
    pub(crate) fn append_document(
        &mut self,
        out: &mut impl Write,
        rows: &[String],
        geometry: &Geometry,
    ) -> io::Result<()> {
        let appended = self.render_append(rows, geometry);
        if appended.is_empty() {
            return Ok(());
        }
        out.write_all(&appended)?;
        out.flush()
    }
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
        let bytes = band.render_append(&["answered".to_string()], &geometry());
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
        assert!(text.contains("\u{1b}[20;1Hfirstsecond"), "{text:?}");
        assert!(text.contains("\u{1b}[21;1Hthird"), "{text:?}");
    }

    #[test]
    fn an_append_of_nothing_writes_nothing() {
        let mut band = Band::new();
        assert!(band.render_append(&[], &geometry()).is_empty());
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
