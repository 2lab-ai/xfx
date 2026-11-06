//! A tool group's rows as the full transcript shows them: word-wrapped under
//! their tree rail instead of clipped (`vercel-labs/fx@481104fd`, 427d481f and
//! 6f071dea).
//!
//! Upstream fits each row of the expanded group to the screen with
//! `materializeExpandedOverride` (`src/ui/transcript/tool_group_projection.zig:
//! 1061-1092@481104fd`), which hands it to `wrapFullToolStatus`
//! (`src/ui/render_engine/transcript_blocks.zig:904-958@481104fd`) with the
//! continuation `expandedContinuationPrefix` picks (`tool_group_projection.zig:
//! 1094-1100@481104fd`): `│ ` under a `├` row or its `│` continuation, two
//! spaces under the group's header and its last `└` row.
//!
//! Every continuation row starts with that prefix in the default style and
//! then reopens the attributes and the foreground in force where the row
//! broke. The reopen comes from the tracked state ([`CellSgr`], upstream's
//! `assistant_pacer.SgrState`), not from a replay of every sequence before the
//! break, so a long highlighted command costs rows in proportion to its length
//! (6f071dea: a 14 KB heredoc's replay grew to megabytes). Styles the state
//! does not track -- a background -- are not reopened, as upstream's are not.
//!
//! Upstream's other full-transcript wrap, `formatFullToolStatus`
//! (`transcript_blocks.zig:871-902@481104fd`), keeps the full replay for a
//! **raw** tool status (an ask row, 4e4b599e). xfx's screen has no raw status:
//! every call is drawn through its group, as upstream's expanded override, and
//! a question is its resolution's rows (`super::Body::Block`), which this
//! module never sees.
//!
//! One difference in how the rows are handed back: upstream writes one string
//! with a line break between rows; this returns the rows, and a row that
//! leaves a style open where it broke is closed with a reset, as every row of
//! this screen is (the frame writes rows one by one and resets nothing
//! between them). The continuation row opens with upstream's own reset, so the
//! cells are the same.

use crate::markdown::display_width as dw;
use crate::tui::blocks::CellSgr;

const ESC: u8 = 0x1b;
const RESET: &[u8] = b"\x1b[0m";

/// Where one row of a status ends and the next begins
/// (`ToolStatusLine`, `transcript_blocks.zig:470-475@481104fd`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StatusLine {
    end: usize,
    next: usize,
    hard_break: bool,
    omitted_rune: bool,
}

impl StatusLine {
    fn at(end: usize, next: usize) -> Self {
        Self {
            end,
            next,
            hard_break: false,
            omitted_rune: false,
        }
    }
}

/// The continuation a row of the expanded group wraps under, chosen by the
/// tree glyph it starts with -- read past any colour it opens with, since
/// xfx's header is painted before its `●` where upstream styles it after
/// choosing (`expandedContinuationPrefix`, `tool_group_projection.zig:
/// 1094-1100@481104fd`).
pub(super) fn continuation_prefix(row: &str) -> &'static str {
    let mut rest = row;
    while let Some(len) = crate::tui::pacer::escape_at(rest) {
        rest = &rest[len..];
    }
    if rest.starts_with('\u{251c}') || rest.starts_with('\u{2502}') {
        "\u{2502} "
    } else {
        "  "
    }
}

/// One row of the expanded group, fitted to `cols` without hiding any of it
/// (`materializeExpandedOverride`, `tool_group_projection.zig:1064-1092@481104fd`).
pub(super) fn expanded_rows(row: &str, cols: u16) -> Vec<String> {
    wrap(row, cols, continuation_prefix(row))
}

/// `wrapFullToolStatus` (`transcript_blocks.zig:912-958@481104fd`): `text`
/// word-wrapped to `cols`, each continuation row under `prefix` in the default
/// style and then the attributes and foreground in force at the break.
///
/// A prefix that leaves no room for content is left off, and a wide character
/// that cannot fit a row at all is a `?` in its place. Empty text, or no
/// columns, is one empty row: upstream writes nothing, and the line it was
/// given stays a row.
pub(super) fn wrap(text: &str, cols: u16, prefix: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    if cols == 0 || bytes.is_empty() {
        return vec![String::new()];
    }
    let cols = usize::from(cols);
    let prefix_width = dw::visible_width_ignoring_ansi(prefix.as_bytes());
    let prefix = if prefix_width < cols { prefix } else { "" };
    let mut style = CellSgr::default();
    let mut styled = false;
    let mut rows = Vec::new();
    let mut row: Vec<u8> = Vec::new();
    let mut start = 0;
    let mut continuation = false;
    while start < bytes.len() {
        let indent = if continuation && !prefix.is_empty() {
            prefix_width
        } else {
            0
        };
        let line = scan(bytes, start, cols - indent);
        if continuation {
            if styled {
                row.extend_from_slice(RESET);
            }
            rows.push(std::mem::take(&mut row));
            if styled {
                row.extend_from_slice(RESET);
            }
            row.extend_from_slice(prefix.as_bytes());
            if style.is_active() {
                style.write_opens(&mut row);
            }
        }
        row.extend_from_slice(&bytes[start..line.end]);
        if line.omitted_rune {
            row.push(b'?');
        }
        let next = if line.next > start {
            line.next
        } else if line.end > start {
            line.end
        } else {
            row.push(b'?');
            start + dw::display_unit_at(bytes, start).byte_len.max(1)
        };
        if track(&mut style, &bytes[start..next]) {
            styled = true;
        }
        start = next;
        continuation = true;
        if line.hard_break && start == bytes.len() {
            rows.push(std::mem::take(&mut row));
        }
    }
    rows.push(row);
    rows.into_iter().map(into_string).collect()
}

/// Folds every SGR sequence in `text` into `style`; whether there was one
/// (`trackToolStatusStyle`, `transcript_blocks.zig:960-975@481104fd`).
fn track<'a>(style: &mut CellSgr<'a>, text: &'a [u8]) -> bool {
    let mut styled = false;
    let mut index = 0;
    while let Some(offset) = text[index..].iter().position(|&byte| byte == ESC) {
        let escape = index + offset;
        let end = dw::ansi_sequence_end(text, escape);
        let sequence = &text[escape..end];
        if sequence.len() > 2 && sequence[1] == b'[' && sequence[sequence.len() - 1] == b'm' {
            styled = true;
            style.apply(sequence);
        }
        index = end.max(escape + 1);
    }
    styled
}

/// `skipToolStatusWrapWhitespace` (`transcript_blocks.zig:477-481@481104fd`).
fn skip_whitespace(text: &[u8], start: usize) -> usize {
    let mut index = start;
    while index < text.len() && matches!(text[index], b' ' | b'\t') {
        index += 1;
    }
    index
}

/// Whether the word at `start` is wider than a whole row, so a row breaks
/// inside it rather than before it (`toolStatusTokenExceedsWidth`,
/// `transcript_blocks.zig:483-509@481104fd`).
fn token_exceeds_width(text: &[u8], start: usize, max_width: usize) -> bool {
    if max_width == 0 {
        return false;
    }
    let mut index = start;
    let mut width = 0;
    while index < text.len() {
        let byte = text[index];
        if byte == ESC {
            let end = dw::ansi_sequence_end(text, index);
            if end <= index {
                return false;
            }
            index = end;
            continue;
        }
        if matches!(byte, b' ' | b'\t' | b'\n' | b'\r') {
            return false;
        }
        if byte < 32 {
            index += 1;
            continue;
        }
        let unit = dw::display_unit_at(text, index);
        if width + unit.cell_width > max_width {
            return true;
        }
        width += unit.cell_width;
        index += unit.byte_len.max(1);
    }
    false
}

/// The row of `text` that starts at `start` and fits `max_width` cells
/// (`scanToolStatusLine`, `transcript_blocks.zig:511-573@481104fd`): broken at
/// the last blank that fits, inside a word only when the word is wider than a
/// row, and at a line break wherever there is one.
fn scan(text: &[u8], start: usize, max_width: usize) -> StatusLine {
    if start >= text.len() || max_width == 0 {
        return StatusLine::at(start, start);
    }
    let mut index = start;
    let mut width = 0;
    let mut last_wrap_end: Option<usize> = None;
    let mut last_wrap_next = start;
    while index < text.len() {
        let byte = text[index];
        if byte == ESC {
            let end = dw::ansi_sequence_end(text, index);
            if end <= index {
                return StatusLine {
                    omitted_rune: true,
                    ..StatusLine::at(index, text.len())
                };
            }
            index = end;
            continue;
        }
        if byte == b'\n' || byte == b'\r' {
            let next = if byte == b'\r' && text.get(index + 1) == Some(&b'\n') {
                index + 2
            } else {
                index + 1
            };
            return StatusLine {
                hard_break: true,
                ..StatusLine::at(index, next)
            };
        }
        if byte == b' ' || byte == b'\t' {
            let next = skip_whitespace(text, index);
            let cell_width = usize::from(byte == b' ');
            if width + cell_width > max_width {
                return StatusLine::at(index, next);
            }
            last_wrap_end = Some(index);
            last_wrap_next = next;
            width += cell_width;
            index += 1;
            continue;
        }
        if byte < 32 {
            index += 1;
            continue;
        }
        let unit = dw::display_unit_at(text, index);
        if width + unit.cell_width > max_width {
            if let Some(end) = last_wrap_end {
                if token_exceeds_width(text, last_wrap_next, max_width) {
                    return StatusLine::at(index, index);
                }
                return StatusLine::at(end, last_wrap_next);
            }
            if width == 0 {
                return StatusLine {
                    omitted_rune: true,
                    ..StatusLine::at(index, index + unit.byte_len.max(1))
                };
            }
            return StatusLine::at(index, index);
        }
        width += unit.cell_width;
        index += unit.byte_len.max(1);
    }
    StatusLine::at(index, index)
}

/// A row is cut only at ASCII bytes and display-unit boundaries, so it is
/// valid UTF-8 whenever the text was; an escape that runs into a multi-byte
/// character is the one way it is not, shown as U+FFFD rather than panicking
/// in the paint path (as `super::super::blocks`' `into_string`).
fn into_string(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes)
        .unwrap_or_else(|error| String::from_utf8_lossy(error.as_bytes()).into_owned())
}

#[cfg(test)]
mod tests {
    //! Upstream's tests of `wrapFullToolStatus` (`transcript_blocks.zig:
    //! 3918-3984@481104fd`), with their inputs and expectations; where they
    //! read a terminal emulator's cells, these read the same cells off this
    //! crate's grid ([`crate::tui::grid::Grid`]).

    use super::*;
    use crate::tui::grid::{Cell, Grid};
    use crate::tui::pacer::SgrState;

    /// `rows` placed one per line, from the top, on a grid `cols` wide.
    fn painted(rows: &[String], cols: u16) -> Grid {
        let lines = u16::try_from(rows.len()).expect("rows");
        // A screen's geometry only bounds the row; the grid is `cols` wide.
        let geometry = crate::tui::layout::solve(40, 120, 1).expect("a band");
        let mut grid = Grid::blank(lines, cols);
        for (index, row) in rows.iter().enumerate() {
            grid.place_row(u16::try_from(index + 1).expect("line"), row, &geometry);
        }
        grid
    }

    fn glyph(grid: &Grid, line: u16, column: u16) -> (String, SgrState) {
        match grid.cell(line, column) {
            Some(Cell::Lead { grapheme, sgr, .. }) => (grapheme.clone(), sgr.clone()),
            other => panic!("row {line}, column {column}: {other:?}"),
        }
    }

    fn indexed(sgr: &SgrState) -> Option<u8> {
        match sgr.color().expect("a colour this crate writes") {
            crate::tui::check::Color::Indexed(index) => Some(index),
            _ => None,
        }
    }

    fn plain_rows(rows: &[String]) -> Vec<String> {
        rows.iter()
            .map(|row| super::super::unpainted(row))
            .collect()
    }

    #[test]
    fn a_continuation_prefix_is_painted_unstyled_before_the_styles_are_reopened() {
        // `transcript_blocks.zig:3918-3946@481104fd`.
        let source =
            "\u{251c} Ran \x1b[38;5;250m'every word of this quoted argument stays visible'\x1b[39m";
        let rows = wrap(source, 20, "\u{2502} ");
        assert!(rows.len() >= 3, "{rows:?}");
        for row in &rows {
            assert!(
                dw::visible_width_ignoring_ansi(row.as_bytes()) <= 20,
                "{row:?}"
            );
        }
        assert!(!rows.concat().contains('\u{2026}'));
        let grid = painted(&rows, 20);
        for line in 2..=u16::try_from(rows.len()).expect("rows") {
            let (rail, style) = glyph(&grid, line, 1);
            assert_eq!(rail, "\u{2502}");
            assert_eq!(indexed(&style), None, "the rail is in the default colour");
            assert_eq!(indexed(&glyph(&grid, line, 3).1), Some(250), "row {line}");
        }
        assert!(plain_rows(&rows)
            .last()
            .expect("rows")
            .ends_with("visible'"));
    }

    #[test]
    fn a_continuation_prefix_that_leaves_no_room_is_left_off() {
        // `transcript_blocks.zig:3948-3953@481104fd`.
        assert_eq!(wrap("abcdef", 2, "\u{2502} ").join("\n"), "ab\ncd\nef");
    }

    #[test]
    fn a_long_highlighted_row_reopens_a_bounded_style_on_every_continuation() {
        // `transcript_blocks.zig:3955-3977@481104fd`: replaying every earlier
        // sequence on each row would grow with the square of the length.
        let source = format!(
            "\u{251c} Ran {}",
            "\x1b[38;5;250m'quoted token'\x1b[39m ".repeat(400)
        );
        let rows = wrap(&source, 20, "\u{2502} ");
        assert!(rows.len() > 100, "{}", rows.len());
        let bytes: usize = rows.iter().map(String::len).sum::<usize>() + rows.len() - 1;
        // Upstream's bound, and a row's closing reset on top of it.
        assert!(
            bytes <= source.len() + rows.len() * (32 + RESET.len()),
            "{bytes} bytes for {} source bytes in {} rows",
            source.len(),
            rows.len()
        );
        let grid = painted(&rows, 20);
        for line in 2..=u16::try_from(rows.len()).expect("rows") {
            let (rail, style) = glyph(&grid, line, 1);
            assert_eq!(rail, "\u{2502}");
            assert_eq!(indexed(&style), None);
            assert_eq!(indexed(&glyph(&grid, line, 3).1), Some(250), "row {line}");
        }
    }

    #[test]
    fn a_wide_character_that_cannot_fit_is_a_question_mark() {
        // `transcript_blocks.zig:3979-3984@481104fd`.
        assert_eq!(wrap("\u{65e5}\u{672c}", 1, "").join("\n"), "?\n?");
    }

    #[test]
    fn a_fourteen_kilobyte_heredoc_wraps_in_time_and_space_linear_in_its_length() {
        // 6f071dea's case: a long command whose every word is coloured. The
        // replay it replaced wrote every earlier sequence again on each row.
        let mut source =
            String::from("\u{2514} Running \x1b[38;5;252mcat\x1b[39m <<'EOF' > notes.md ");
        let mut token = 0;
        while source.len() < 14 * 1024 {
            source.push_str(&format!(
                "\x1b[38;5;252mword{token}\x1b[39m \x1b[1mbold{token}\x1b[22m "
            ));
            token += 1;
        }
        source.push_str("EOF\x1b[0m");
        let started = std::time::Instant::now();
        let rows = wrap(&source, 80, "  ");
        let elapsed = started.elapsed();
        assert!(rows.len() > 50, "{}", rows.len());
        let bytes: usize = rows.iter().map(String::len).sum();
        assert!(
            bytes <= source.len() + rows.len() * (32 + RESET.len()),
            "{bytes} bytes for {} source bytes in {} rows",
            source.len(),
            rows.len()
        );
        assert!(
            elapsed < std::time::Duration::from_secs(2),
            "{elapsed:?} for {} rows",
            rows.len()
        );
        // Nothing is hidden: the rows hold every word, in order.
        let plain: String = plain_rows(&rows)
            .iter()
            .map(|row| row.trim_start())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(plain.contains(&format!("word{} bold{} EOF", token - 1, token - 1)));
        assert!(!plain.contains('\u{2026}'));
    }

    #[test]
    fn the_header_and_the_last_row_indent_past_their_glyph_and_the_others_keep_the_rail() {
        // `expandedContinuationPrefix`, `tool_group_projection.zig:1097-1100@481104fd`.
        assert_eq!(continuation_prefix("\u{251c} Read a.txt"), "\u{2502} ");
        assert_eq!(continuation_prefix("\u{2502} more"), "\u{2502} ");
        assert_eq!(continuation_prefix("\u{2514} Ran ls"), "  ");
        assert_eq!(
            continuation_prefix("\x1b[38;5;255m\u{25cf}\x1b[0m 2 tool calls"),
            "  "
        );
    }

    #[test]
    fn a_line_break_ends_a_row_and_a_trailing_one_leaves_an_empty_row() {
        // `scanToolStatusLine`'s hard break, and `wrapFullToolStatus`'s
        // trailing one (`transcript_blocks.zig:526-531,955@481104fd`).
        assert_eq!(wrap("one\ntwo\n", 20, "  "), ["one", "  two", ""]);
        assert_eq!(wrap("", 20, "  "), [""]);
        assert_eq!(wrap("abc", 0, "  "), [""]);
    }

    #[test]
    fn a_row_that_breaks_inside_a_colour_is_closed_and_the_next_reopens_it() {
        let rows = wrap("\x1b[38;5;250maaa bbb ccc\x1b[39m", 5, "  ");
        assert_eq!(
            rows,
            [
                "\x1b[38;5;250maaa\x1b[0m",
                "\x1b[0m  \x1b[38;5;250mbbb\x1b[0m",
                "\x1b[0m  \x1b[38;5;250mccc\x1b[39m",
            ]
        );
    }
}
