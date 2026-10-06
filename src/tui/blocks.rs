//! The assistant blocks the transcript lays out whole: a finished table, a
//! fenced code block, and a thematic rule (`transcript_blocks.zig` and
//! `assistant_wrap.zig` at vercel-labs/fx `c1db9191`).
//!
//! The markdown presenter holds these three back until they are complete and
//! hands them over as payloads ([`crate::markdown::Event`]); this module is
//! what upstream's transcript does with them. A table and a code block render
//! at the screen less the assistant gutter, and then every row that shows
//! something is pushed right by that gutter ([`table_row_bytes`],
//! [`code_block_row_bytes`], the call shape at `transcript_blocks.zig:1912-1930`).
//! A thematic rule draws its own gutter ([`render_thematic_rule`]).
//!
//! The bytes are upstream's, byte for byte. Where Zig would trip a safety
//! check on a malformed payload (a ragged alignment list, an empty table
//! handed to the vertical layout) this port reads past it instead, because a
//! paint path must not panic; a payload the presenter produces never reaches
//! those branches.
//!
//! Two carriers of SGR state live here, and neither is
//! [`super::pacer::SgrState`]. A code row carries only a foreground
//! ([`CodeStyle`], as upstream does). A table cell carries upstream's
//! `assistant_pacer.SgrState` ([`CellSgr`]): five attributes and a
//! foreground, re-opened in a fixed order. The pacer's own tracker models nine
//! slots, background included, and replays them in the order they were
//! opened, so reusing it would re-open what upstream drops, in another order.

#![cfg_attr(not(test), allow(dead_code))]

use crate::markdown::code_highlight::{self, Variant};
use crate::markdown::code_highlight_languages as languages;
use crate::markdown::display_width as dw;
use crate::markdown::theme::{sgr_has_param, Theme};
use crate::markdown::{write_table_header_cell, CodeBlockPayload, TableColumnAlign, TablePayload};

const ESC: u8 = 0x1b;
const DIM_OPEN: &[u8] = b"\x1b[2m";
const DIM_CLOSE: &[u8] = b"\x1b[22m";
const RESET: &[u8] = b"\x1b[0m";
const RULE: &[u8] = "\u{2500}".as_bytes();

/// Upstream's `ui_render.diff_added_marker_style` and
/// `diff_removed_marker_style` (`render.zig:40-41`): empty until a theme is
/// pinned by name or variant (`render.zig:93-105`), which leaves `+`/`-` lines
/// monochrome. xfx has no theme pin yet, so the empty default is the one
/// upstream state there is to port.
const DIFF_ADDED_MARKER: &str = "";
const DIFF_REMOVED_MARKER: &str = "";

// ---------------------------------------------------------------------------
// Gutter (`assistant_wrap.zig:386-407,790-805`)
// ---------------------------------------------------------------------------

/// How many columns the assistant's rows are indented by at `cols`
/// (`assistant_wrap.zig:386-388`).
pub(crate) fn gutter_width(cols: u16) -> u16 {
    if cols >= 3 {
        2
    } else if cols == 2 {
        1
    } else {
        0
    }
}

/// Indents every row of `bytes` that shows something by `gutter` spaces
/// (`assistant_wrap.zig:390-407`). A blank row, and a row that opens with a
/// control byte before anything visible, stays where it is.
pub(crate) fn prefix_structural_rows(bytes: &str, gutter: u16) -> String {
    if gutter == 0 || bytes.is_empty() {
        return bytes.to_owned();
    }
    let raw = bytes.as_bytes();
    let mut out = String::with_capacity(bytes.len() + bytes.len() / 8);
    let mut line_start = 0;
    while line_start < raw.len() {
        let newline = index_of_scalar_pos(raw, line_start, b'\n');
        let line_end = newline.unwrap_or(raw.len());
        if first_visible_width(&raw[line_start..line_end]).is_some() {
            out.extend(std::iter::repeat_n(' ', usize::from(gutter)));
        }
        // Both ends sit on a line feed or the end of the text, so the slice
        // is on character boundaries.
        out.push_str(&bytes[line_start..line_end]);
        if newline.is_some() {
            out.push('\n');
        }
        line_start = line_end + 1;
    }
    out
}

/// The width of the first visible unit of `text`, or `None` when a control
/// byte or the end comes first (`assistant_wrap.zig:790-805`).
fn first_visible_width(text: &[u8]) -> Option<usize> {
    let mut i = 0;
    while i < text.len() {
        if text[i] == ESC {
            let end = dw::ansi_sequence_end(text, i);
            if end <= i {
                return None;
            }
            i = end;
            continue;
        }
        if text[i] < 32 {
            return None;
        }
        let unit = dw::display_unit_at(text, i);
        if unit.cell_width > 0 {
            return Some(unit.cell_width);
        }
        i += unit.byte_len;
    }
    None
}

// ---------------------------------------------------------------------------
// Transcript call shapes (`transcript_blocks.zig:1912-1930`)
// ---------------------------------------------------------------------------

/// A completed table as transcript rows at `cols`: rendered at the screen
/// less the gutter, then indented by it.
pub(crate) fn table_row_bytes(table: &TablePayload, cols: u16) -> String {
    let gutter = gutter_width(cols);
    let rendered = render_table(table, cols.saturating_sub(gutter));
    prefix_structural_rows(&rendered, gutter)
}

/// A completed code block as transcript rows at `cols`: rendered at the
/// screen less the gutter, then indented by it.
pub(crate) fn code_block_row_bytes(block: &CodeBlockPayload, cols: u16, theme: &Theme) -> String {
    let gutter = gutter_width(cols);
    let rendered = render_code_block(block, cols.saturating_sub(gutter), theme);
    prefix_structural_rows(&rendered, gutter)
}

// ---------------------------------------------------------------------------
// Thematic rule (`transcript_blocks.zig:444-457`)
// ---------------------------------------------------------------------------

/// A dim rule across the assistant's width, its gutter included and no line
/// feed after it.
pub(crate) fn render_thematic_rule(cols: u16) -> String {
    if cols == 0 {
        return String::new();
    }
    let gutter = gutter_width(cols);
    let rule_width = cols.saturating_sub(gutter);
    let mut out = Vec::new();
    out.resize(usize::from(gutter), b' ');
    out.extend_from_slice(DIM_OPEN);
    for _ in 0..rule_width {
        out.extend_from_slice(RULE);
    }
    out.extend_from_slice(DIM_CLOSE);
    into_string(out)
}

// ---------------------------------------------------------------------------
// Code block (`transcript_blocks.zig:1084-1370`)
// ---------------------------------------------------------------------------

/// A code block between two dim rules, the first naming its language
/// (`renderCodeBlockForTranscriptWithTheme`, `transcript_blocks.zig:1092-1147`).
///
/// The panel is as wide as the longest **source** line -- measured before
/// highlighting, so the colour escapes never widen it -- at least six cells
/// and at most `cols`. Long lines wrap inside it and keep their indent. At five
/// columns or fewer there is no panel, only the wrapped code.
///
/// `theme` picks the palette: its `light` flag is upstream's
/// `code_highlight_theme` variant, and its syntax slots are the active theme
/// upstream reads globally.
pub(crate) fn render_code_block(block: &CodeBlockPayload, cols: u16, theme: &Theme) -> String {
    if cols == 0 {
        return String::new();
    }
    let profile = if block.language.is_empty() {
        languages::infer(&block.code)
    } else {
        languages::resolve(&block.language)
    };
    let language: &str = if !block.language.is_empty() {
        &block.language
    } else {
        profile.map_or("", |inferred| inferred.label)
    };
    let variant = if theme.light {
        Variant::Light
    } else {
        Variant::Dark
    };
    let styled = profile.map(|resolved| {
        if resolved.diff_lines {
            code_highlight::highlight_diff(
                &block.code,
                variant,
                DIFF_ADDED_MARKER,
                DIFF_REMOVED_MARKER,
                theme,
            )
        } else {
            code_highlight::highlight(&block.code, resolved, variant, None, theme)
        }
    });
    let code = styled.as_deref().unwrap_or(&block.code).as_bytes();

    let max_code_width = max_code_line_width(block.code.as_bytes());
    let mut out = Vec::new();
    if cols <= 5 {
        render_unboxed_code(code, cols, &mut out);
        return into_string(out);
    }

    let panel_width = code_panel_width(max_code_width, language.as_bytes(), cols);
    if language.is_empty() {
        append_code_panel_rule(&mut out, panel_width);
    } else {
        append_code_panel_header(&mut out, panel_width, language.as_bytes());
    }

    let mut start = 0;
    let mut emitted_line = false;
    while start < code.len() {
        let end = index_of_scalar_pos(code, start, b'\n').unwrap_or(code.len());
        append_code_panel_line(&mut out, &code[start..end], panel_width);
        emitted_line = true;
        if end == code.len() {
            break;
        }
        start = end + 1;
    }
    if !emitted_line {
        append_code_panel_line(&mut out, b"", panel_width);
    }
    append_code_panel_rule(&mut out, panel_width);
    into_string(out)
}

/// The foreground a code row leaves open (`transcript_blocks.zig:1149-1176`).
///
/// Highlighting only ever colours the foreground, so that is all a wrapped
/// row has to re-open on the row after it.
#[derive(Debug, Clone, Copy, Default)]
struct CodeStyle<'a> {
    foreground: Option<&'a [u8]>,
}

impl<'a> CodeStyle<'a> {
    fn apply(&mut self, text: &'a [u8]) {
        let mut index = 0;
        while index < text.len() {
            if text[index] == ESC {
                let end = dw::ansi_sequence_end(text, index);
                if end > index {
                    self.apply_sequence(&text[index..end]);
                    index = end;
                    continue;
                }
            }
            index += dw::decode_next_rune(text, index).len;
        }
    }

    fn apply_sequence(&mut self, sequence: &'a [u8]) {
        if sequence.len() < 4
            || sequence[0] != ESC
            || sequence[1] != b'['
            || sequence[sequence.len() - 1] != b'm'
        {
            return;
        }
        // A CSI begins and ends on ASCII bytes, so a slice of valid text is
        // valid text.
        let Ok(text) = std::str::from_utf8(sequence) else {
            return;
        };
        if sgr_has_param(text, "38") {
            self.foreground = Some(sequence);
        } else if sequence == b"\x1b[39m" || sequence == RESET {
            self.foreground = None;
        }
    }
}

/// A display unit that takes at least one cell, located in its text.
#[derive(Debug, Clone, Copy)]
struct VisibleUnit {
    start: usize,
    len: usize,
    width: usize,
}

/// The first unit of `text` that takes a cell, escapes skipped
/// (`transcript_blocks.zig:1184-1199`).
fn first_code_glyph(text: &[u8]) -> Option<VisibleUnit> {
    let mut index = 0;
    while index < text.len() {
        if text[index] == ESC {
            let end = dw::ansi_sequence_end(text, index);
            if end > index {
                index = end;
                continue;
            }
        }
        let unit = dw::display_unit_at(text, index);
        if unit.cell_width > 0 {
            return Some(VisibleUnit {
                start: index,
                len: unit.byte_len,
                width: unit.cell_width,
            });
        }
        index += unit.byte_len;
    }
    None
}

/// The widest line of the raw source (`transcript_blocks.zig:1201-1211`).
fn max_code_line_width(code: &[u8]) -> usize {
    let mut max_width = 0;
    let mut start = 0;
    while start < code.len() {
        let end = index_of_scalar_pos(code, start, b'\n').unwrap_or(code.len());
        max_width = max_width.max(dw::visible_width(&code[start..end]));
        if end == code.len() {
            break;
        }
        start = end + 1;
    }
    max_width
}

/// `cols` is at least six here (`transcript_blocks.zig:1213-1219`).
fn code_panel_width(max_code_width: usize, language: &[u8], cols: u16) -> usize {
    let cols = usize::from(cols);
    let label_width = if language.is_empty() {
        0
    } else {
        dw::visible_width(language).min(cols - 4)
    };
    cols.min(6.max(max_code_width.max(label_width + 4)))
}

/// `─ label ───`, the label cut to fit (`transcript_blocks.zig:1221-1238`).
fn append_code_panel_header(out: &mut Vec<u8>, panel_width: usize, language: &[u8]) {
    let label_limit = panel_width - 4;
    let label_prefix = dw::prefix_by_width(language, label_limit);
    let label: &[u8] = if label_prefix.is_empty() {
        b"?"
    } else {
        label_prefix
    };
    let label_width = dw::visible_width(label);

    out.extend_from_slice(DIM_OPEN);
    out.extend_from_slice(RULE);
    out.push(b' ');
    out.extend_from_slice(label);
    out.push(b' ');
    for _ in 0..panel_width.saturating_sub(3 + label_width) {
        out.extend_from_slice(RULE);
    }
    out.extend_from_slice(DIM_CLOSE);
    out.push(b'\n');
}

fn append_code_panel_rule(out: &mut Vec<u8>, panel_width: usize) {
    out.extend_from_slice(DIM_OPEN);
    for _ in 0..panel_width {
        out.extend_from_slice(RULE);
    }
    out.extend_from_slice(DIM_CLOSE);
    out.push(b'\n');
}

/// One source line, wrapped to the panel (`transcript_blocks.zig:1251-1292`).
///
/// A continuation row keeps the line's leading indent while that leaves room,
/// re-opens the foreground the previous row left open, and closes it again
/// before the row ends. A glyph wider than the room left becomes `?`.
fn append_code_panel_line(out: &mut Vec<u8>, line: &[u8], panel_width: usize) {
    let indent = leading_code_indent(line);
    let indent_width = dw::visible_width(indent);
    let mut remaining = line;
    let mut continuation = false;
    let mut style = CodeStyle::default();
    if remaining.is_empty() {
        out.push(b'\n');
        return;
    }
    while !remaining.is_empty() {
        let continuation_indent: &[u8] = if continuation && indent_width < panel_width {
            indent
        } else {
            b""
        };
        let available_width = panel_width - dw::visible_width(continuation_indent);
        let mut prefix = dw::prefix_by_width_ignoring_ansi(remaining, available_width);
        if first_code_glyph(prefix).is_none() {
            let Some(rune) = first_code_glyph(remaining) else {
                style.apply(remaining);
                break;
            };
            if rune.width > available_width {
                let mut fallback_style = style;
                fallback_style.apply(&remaining[..rune.start]);
                append_code_row(
                    out,
                    continuation_indent,
                    fallback_style,
                    b"?",
                    fallback_style,
                );
                style = fallback_style;
                remaining = &remaining[rune.start + rune.len..];
                continuation = true;
                continue;
            }
            prefix = &remaining[..rune.start + rune.len];
        }
        let row_style = style;
        style.apply(prefix);
        append_code_row(out, continuation_indent, row_style, prefix, style);
        remaining = &remaining[prefix.len()..];
        continuation = true;
    }
}

/// The run of spaces and tabs a line opens with
/// (`transcript_blocks.zig:1294-1298`).
fn leading_code_indent(line: &[u8]) -> &[u8] {
    let index = line
        .iter()
        .position(|&byte| byte != b' ' && byte != b'\t')
        .unwrap_or(line.len());
    &line[..index]
}

fn append_code_row(
    out: &mut Vec<u8>,
    leading: &[u8],
    before: CodeStyle<'_>,
    content: &[u8],
    after: CodeStyle<'_>,
) {
    out.extend_from_slice(leading);
    if let Some(foreground) = before.foreground {
        out.extend_from_slice(foreground);
    }
    out.extend_from_slice(content);
    if after.foreground.is_some() {
        out.extend_from_slice(RESET);
    }
    out.push(b'\n');
}

/// The code alone, wrapped to `cols`, for screens too narrow for a panel
/// (`transcript_blocks.zig:1315-1369`).
fn render_unboxed_code(code: &[u8], cols: u16, out: &mut Vec<u8>) {
    let cols = usize::from(cols);
    let mut start = 0;
    while start < code.len() {
        let end = index_of_scalar_pos(code, start, b'\n').unwrap_or(code.len());
        let line = &code[start..end];
        let indent = leading_code_indent(line);
        let indent_width = dw::visible_width(indent);
        let mut remaining = line;
        let mut continuation = false;
        let mut style = CodeStyle::default();
        if remaining.is_empty() {
            out.push(b'\n');
        }
        while !remaining.is_empty() {
            let continuation_indent: &[u8] = if continuation && indent_width < cols {
                indent
            } else {
                b""
            };
            let available_width = cols - dw::visible_width(continuation_indent);
            let mut prefix = dw::prefix_by_width_ignoring_ansi(remaining, available_width);
            if first_code_glyph(prefix).is_none() {
                let Some(rune) = first_code_glyph(remaining) else {
                    style.apply(remaining);
                    break;
                };
                if rune.width > available_width {
                    let mut fallback_style = style;
                    fallback_style.apply(&remaining[..rune.start]);
                    out.extend_from_slice(continuation_indent);
                    if let Some(foreground) = fallback_style.foreground {
                        out.extend_from_slice(foreground);
                    }
                    out.push(b'?');
                    if fallback_style.foreground.is_some() {
                        out.extend_from_slice(RESET);
                    }
                    style = fallback_style;
                    remaining = &remaining[rune.start + rune.len..];
                    out.push(b'\n');
                    continuation = true;
                    continue;
                }
                prefix = &remaining[..rune.start + rune.len];
            }
            let row_style = style;
            style.apply(prefix);
            out.extend_from_slice(continuation_indent);
            if let Some(foreground) = row_style.foreground {
                out.extend_from_slice(foreground);
            }
            out.extend_from_slice(prefix);
            if style.foreground.is_some() {
                out.extend_from_slice(RESET);
            }
            out.push(b'\n');
            remaining = &remaining[prefix.len()..];
            continuation = true;
        }
        if end == code.len() {
            break;
        }
        start = end + 1;
    }
}

// ---------------------------------------------------------------------------
// Table (`transcript_blocks.zig:905-1083,1371-1613`)
// ---------------------------------------------------------------------------

/// A table as a boxed grid whose cells wrap onto extra lines when the
/// natural layout is wider than `cols` (`renderTableForTranscript`,
/// `transcript_blocks.zig:905-927`). Stacked `header: value` fields are used
/// only when even the narrowest legible grid cannot fit, and unboxed fields at
/// two columns or fewer.
pub(crate) fn render_table(table: &TablePayload, cols: u16) -> String {
    let mut out = Vec::new();
    if cols <= 2 {
        render_unboxed_table(table, cols, &mut out);
    } else {
        let mut measures = vec![TableColumnMeasure::default(); table.column_count];
        let mut widths = vec![0; table.column_count];
        measure_table_columns(table, &mut measures);
        if table.column_count > 0
            && fit_table_column_widths(&measures, usize::from(cols), &mut widths)
        {
            render_boxed_grid(table, &widths, &mut out);
        } else {
            render_boxed_vertical(table, cols, &mut out);
        }
    }
    into_string(out)
}

/// A wrapped column keeps at least this many cells, or its natural width when
/// narrower, so its text stays legible and every two-cell rune still fits.
const TABLE_MIN_COLUMN_WIDTH: usize = 3;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct TableColumnMeasure {
    /// Width of the widest cell laid out on one line.
    natural: usize,
    /// Width of the widest run between spaces.
    word: usize,
}

fn measure_table_columns(table: &TablePayload, measures: &mut [TableColumnMeasure]) {
    measures.fill(TableColumnMeasure::default());
    for row in &table.rows {
        for (cell, measure) in row.cells.iter().zip(measures.iter_mut()) {
            let cell = cell.as_bytes();
            measure.natural = measure.natural.max(dw::visible_width_ignoring_ansi(cell));
            measure.word = measure.word.max(longest_table_word_width(cell));
        }
    }
}

fn longest_table_word_width(text: &[u8]) -> usize {
    let mut longest = 0;
    let mut current = 0;
    let mut index = 0;
    while index < text.len() {
        if text[index] == ESC {
            index = dw::ansi_sequence_end(text, index);
            continue;
        }
        if text[index] == b' ' || text[index] == b'\t' {
            longest = longest.max(current);
            current = 0;
            index += 1;
            continue;
        }
        let unit = dw::display_unit_at(text, index);
        current += unit.cell_width;
        index += unit.byte_len;
    }
    longest.max(current)
}

/// Marks a column whose wrapped width is still being chosen.
const TABLE_COLUMN_OPEN: usize = usize::MAX;

/// Fractional bits of the shared scale applied to natural widths.
const TABLE_SCALE_BITS: u32 = 16;

/// Chooses content widths so the boxed grid fits within `cols`
/// (`transcript_blocks.zig:976-1053`). Columns that fit within an even share
/// keep their natural width. The remaining columns wrap and grow by one shared
/// factor of their natural width, keeping whole words when every column can.
/// Returns false when the minimum legible widths do not fit.
fn fit_table_column_widths(
    measures: &[TableColumnMeasure],
    cols: usize,
    widths: &mut [usize],
) -> bool {
    debug_assert_eq!(measures.len(), widths.len());
    let border_width = measures.len() * 3 + 1;
    if cols <= border_width {
        return false;
    }
    let available = cols - border_width;

    let mut natural_total = 0;
    let mut floor_total = 0;
    for (measure, width) in measures.iter().zip(widths.iter_mut()) {
        *width = measure.natural;
        natural_total += measure.natural;
        floor_total += measure.natural.min(TABLE_MIN_COLUMN_WIDTH);
    }
    if natural_total <= available {
        return true;
    }
    // This is the only reason to stack. A column settles below only while its
    // natural width fits an even share, so the open floors always fit.
    if floor_total > available {
        return false;
    }

    // Settle every column that fits on one line within an even share of what
    // remains. A settled column takes no more than that share, so the share
    // only grows for the columns still open.
    widths.fill(TABLE_COLUMN_OPEN);
    let mut settled_total = 0;
    let mut open_count = measures.len();
    let mut settled_any = true;
    while settled_any && open_count > 0 {
        settled_any = false;
        let share = (available - settled_total) / open_count;
        for (measure, width) in measures.iter().zip(widths.iter_mut()) {
            if *width != TABLE_COLUMN_OPEN || measure.natural > share {
                continue;
            }
            *width = measure.natural;
            settled_total += measure.natural;
            open_count -= 1;
            settled_any = true;
        }
    }

    let mut word_total = settled_total;
    for (measure, &width) in measures.iter().zip(widths.iter()) {
        if width == TABLE_COLUMN_OPEN {
            word_total += wrapped_column_span(*measure, false).base;
        }
    }
    let break_words = word_total > available;

    // Find the largest shared scale that fits. At `high` every open column
    // reaches its target, which overflows `available`, so the answer is lower.
    let mut low: u64 = 0;
    let mut high: u64 = (available as u64 + 1) << TABLE_SCALE_BITS;
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        if settled_total + open_columns_width(measures, widths, break_words, middle) <= available {
            low = middle;
        } else {
            high = middle;
        }
    }
    let mut assigned = settled_total;
    for (measure, width) in measures.iter().zip(widths.iter_mut()) {
        if *width != TABLE_COLUMN_OPEN {
            continue;
        }
        *width = scaled_column_width(*measure, break_words, low);
        assigned += *width;
    }
    // Settled columns already sit at or above their span target, and the open
    // targets sum above `available`, so every pass finds room. The progress
    // check only turns a broken invariant into a narrower grid rather than a
    // hung paint; upstream has no counterpart because it cannot fire.
    while assigned < available {
        let before = assigned;
        for (measure, width) in measures.iter().zip(widths.iter_mut()) {
            if assigned == available {
                break;
            }
            if *width >= wrapped_column_span(*measure, break_words).target {
                continue;
            }
            *width += 1;
            assigned += 1;
        }
        if assigned == before {
            break;
        }
    }
    true
}

fn open_columns_width(
    measures: &[TableColumnMeasure],
    widths: &[usize],
    break_words: bool,
    scale: u64,
) -> usize {
    measures
        .iter()
        .zip(widths)
        .filter(|(_, &width)| width == TABLE_COLUMN_OPEN)
        .map(|(measure, _)| scaled_column_width(*measure, break_words, scale))
        .sum()
}

fn scaled_column_width(measure: TableColumnMeasure, break_words: bool, scale: u64) -> usize {
    let span = wrapped_column_span(measure, break_words);
    let scaled = (measure.natural as u128 * u128::from(scale)) >> TABLE_SCALE_BITS;
    if scaled >= span.target as u128 {
        return span.target;
    }
    // Below `target`, which is a `usize`.
    span.base.max(scaled as usize)
}

#[derive(Debug, Clone, Copy)]
struct TableColumnSpan {
    base: usize,
    target: usize,
}

/// The width a wrapped column starts from and the width it grows toward.
/// Columns first keep their longest word and grow toward one line; when
/// words must break, they start at the legible minimum and grow toward it.
fn wrapped_column_span(measure: TableColumnMeasure, break_words: bool) -> TableColumnSpan {
    let floor = measure.natural.min(TABLE_MIN_COLUMN_WIDTH);
    let word = floor.max(measure.word.min(measure.natural));
    if break_words {
        TableColumnSpan {
            base: floor,
            target: word,
        }
    } else {
        TableColumnSpan {
            base: word,
            target: measure.natural,
        }
    }
}

fn append_table_border(out: &mut Vec<u8>, left: &str, middle: &str, right: &str, widths: &[usize]) {
    out.extend_from_slice(left.as_bytes());
    for (index, &width) in widths.iter().enumerate() {
        if index > 0 {
            out.extend_from_slice(middle.as_bytes());
        }
        for _ in 0..width + 2 {
            out.extend_from_slice(RULE);
        }
    }
    out.extend_from_slice(right.as_bytes());
    out.push(b'\n');
}

fn render_boxed_grid(table: &TablePayload, widths: &[usize], out: &mut Vec<u8>) {
    let mut cell_lines: Vec<Vec<TableCellLine<'_>>> = vec![Vec::new(); widths.len()];

    append_table_border(out, "┌", "┬", "┐", widths);
    for (row_index, row) in table.rows.iter().enumerate() {
        let mut row_height = 1;
        for (col, (&width, lines)) in widths.iter().zip(cell_lines.iter_mut()).enumerate() {
            let cell = row.cells.get(col).map_or(&b""[..], |cell| cell.as_bytes());
            wrap_table_cell(cell, width, lines);
            row_height = row_height.max(lines.len());
        }
        for line_index in 0..row_height {
            out.extend_from_slice("│".as_bytes());
            for (col, (&width, lines)) in widths.iter().zip(cell_lines.iter()).enumerate() {
                out.push(b' ');
                if let Some(line) = lines.get(line_index) {
                    let pad = width.saturating_sub(line.width);
                    let alignment = if row_index == 0 {
                        TableColumnAlign::Left
                    } else {
                        table
                            .alignments
                            .get(col)
                            .copied()
                            .unwrap_or(TableColumnAlign::Left)
                    };
                    let left_pad = match alignment {
                        TableColumnAlign::Left => 0,
                        TableColumnAlign::Right => pad,
                        TableColumnAlign::Center => pad / 2,
                    };
                    out.resize(out.len() + left_pad, b' ');
                    append_table_cell_line(out, line, row_index == 0);
                    out.resize(out.len() + (pad - left_pad), b' ');
                } else {
                    out.resize(out.len() + width, b' ');
                }
                out.extend_from_slice(" │".as_bytes());
            }
            out.push(b'\n');
        }
        if row_index == 0 && table.rows.len() > 1 {
            append_table_border(out, "├", "┼", "┤", widths);
        }
        if row_index > 0 && row_index + 1 < table.rows.len() {
            append_table_border(out, "├", "┼", "┤", widths);
        }
    }
    append_table_border(out, "└", "┴", "┘", widths);
}

/// One physical line of a wrapped cell. `text` borrows the cell bytes; the
/// style fields let the line be drawn on its own between borders.
#[derive(Debug, Clone, Copy)]
struct TableCellLine<'a> {
    text: &'a [u8],
    width: usize,
    /// Styles and link still open from earlier lines of the cell.
    style: CellSgr<'a>,
    hyperlink: Option<&'a [u8]>,
    /// Whether the line must reset styles or close a link before its padding.
    close_style: bool,
    close_hyperlink: bool,
}

/// Splits a rendered cell into lines no wider than `width`, breaking at
/// spaces where possible and inside a word only when it cannot fit
/// (`transcript_blocks.zig:1440-1477`). Callers choose widths no narrower than
/// the cell's widest display unit.
fn wrap_table_cell<'a>(cell: &'a [u8], width: usize, lines: &mut Vec<TableCellLine<'a>>) {
    lines.clear();
    let mut style = CellSgr::default();
    let mut hyperlink: Option<&'a [u8]> = None;
    let mut remaining = cell;
    while let Some(glyph) = first_code_glyph(remaining) {
        let mut text = table_cell_cut(remaining, width);
        if !has_table_cell_text(text) {
            // A leading space is a break like any other, not a line of its own.
            // Skipping from the space itself always consumes input, even when
            // zero-width units precede it.
            if remaining[glyph.start] == b' ' {
                track_table_cell_escapes(&remaining[..glyph.start], &mut style, &mut hyperlink);
                let next =
                    skip_table_cell_break(&remaining[glyph.start..], &mut style, &mut hyperlink);
                debug_assert!(next.len() < remaining.len());
                remaining = next;
                continue;
            }
            text = &remaining[..glyph.start + glyph.len];
        }
        debug_assert!(!text.is_empty());
        let mut line = TableCellLine {
            text,
            width: dw::visible_width_ignoring_ansi(text),
            style,
            hyperlink,
            close_style: false,
            close_hyperlink: false,
        };
        let styled = track_table_cell_escapes(text, &mut style, &mut hyperlink);
        line.close_style = styled || style.is_active();
        line.close_hyperlink = hyperlink.is_some();
        lines.push(line);
        remaining = skip_table_cell_break(&remaining[text.len()..], &mut style, &mut hyperlink);
    }
    if lines.is_empty() {
        lines.push(TableCellLine {
            text: b"",
            width: 0,
            style: CellSgr::default(),
            hyperlink: None,
            close_style: false,
            close_hyperlink: false,
        });
    }
}

fn has_table_cell_text(text: &[u8]) -> bool {
    let mut index = 0;
    while let Some(glyph) = first_code_glyph(&text[index..]) {
        if text[index + glyph.start] != b' ' {
            return true;
        }
        index += glyph.start + glyph.len;
    }
    false
}

/// Cuts at the last space that fits, or at the width itself when the text
/// continues with a space there.
fn table_cell_cut(text: &[u8], width: usize) -> &[u8] {
    let prefix = dw::prefix_by_width_ignoring_ansi(text, width);
    if prefix.len() < text.len() && (text[prefix.len()] == b' ' || text[prefix.len()] == b'\t') {
        return prefix;
    }
    dw::wrap_cut_ignoring_ansi(text, width)
}

/// Drops the spaces at a break. Escapes among them only update the carried
/// state, which the next line reopens.
fn skip_table_cell_break<'a>(
    text: &'a [u8],
    style: &mut CellSgr<'a>,
    hyperlink: &mut Option<&'a [u8]>,
) -> &'a [u8] {
    let mut index = 0;
    while index < text.len() {
        if text[index] == b' ' || text[index] == b'\t' {
            index += 1;
        } else if text[index] == ESC {
            let end = dw::ansi_sequence_end(text, index);
            track_table_cell_escapes(&text[index..end], style, hyperlink);
            index = end;
        } else {
            break;
        }
    }
    &text[index..]
}

/// Returns whether `text` sets SGR state. Lines that do are reset before the
/// border even when the tracker cannot restore the style they set.
fn track_table_cell_escapes<'a>(
    text: &'a [u8],
    style: &mut CellSgr<'a>,
    hyperlink: &mut Option<&'a [u8]>,
) -> bool {
    let mut styled = false;
    let mut index = 0;
    while let Some(start) = index_of_scalar_pos(text, index, ESC) {
        let end = dw::ansi_sequence_end(text, start);
        let sequence = &text[start..end];
        if sequence.len() > 2 && sequence[1] == b'[' && sequence[sequence.len() - 1] == b'm' {
            styled = true;
        }
        style.apply(sequence);
        update_hyperlink_state(sequence, hyperlink);
        index = end;
    }
    styled
}

fn append_table_cell_line(out: &mut Vec<u8>, line: &TableCellLine<'_>, header: bool) {
    if line.style.is_active() {
        line.style.write_opens(out);
    }
    if let Some(open) = line.hyperlink {
        out.extend_from_slice(open);
    }
    if header {
        write_table_header_cell(out, line.text);
    } else {
        out.extend_from_slice(line.text);
    }
    if line.close_style {
        out.extend_from_slice(RESET);
    }
    if line.close_hyperlink {
        out.extend_from_slice(OSC8_CLOSE);
    }
}

fn append_vertical_line(out: &mut Vec<u8>, content: &[u8], inner_width: usize) {
    let mut remaining = content;
    if remaining.is_empty() {
        out.extend_from_slice("│".as_bytes());
        out.resize(out.len() + inner_width, b' ');
        out.extend_from_slice("│\n".as_bytes());
        return;
    }
    while !remaining.is_empty() {
        let mut prefix = dw::prefix_by_width_ignoring_ansi(remaining, inner_width);
        let mut consumed = prefix.len();
        if prefix.is_empty() {
            prefix = b"?";
            consumed = dw::display_unit_at(remaining, 0).byte_len;
        }
        let visible = dw::visible_width_ignoring_ansi(prefix);
        out.extend_from_slice("│".as_bytes());
        out.extend_from_slice(prefix);
        out.resize(out.len() + inner_width.saturating_sub(visible), b' ');
        out.extend_from_slice("│\n".as_bytes());
        remaining = &remaining[consumed..];
    }
}

/// `header: value` fields in one box, a rule between body rows
/// (`transcript_blocks.zig:1569-1597`).
fn render_boxed_vertical(table: &TablePayload, cols: u16, out: &mut Vec<u8>) {
    let inner_width = usize::from(cols) - 2;
    out.extend_from_slice("┌".as_bytes());
    for _ in 0..inner_width {
        out.extend_from_slice(RULE);
    }
    out.extend_from_slice("┐\n".as_bytes());
    let body = table.rows.get(1..).unwrap_or_default();
    for (row_index, row) in body.iter().enumerate() {
        for col in 0..table.alignments.len() {
            let mut field = Vec::new();
            field.extend_from_slice(table_cell(table, 0, col));
            field.extend_from_slice(b": ");
            field.extend_from_slice(row.cells.get(col).map_or(&b""[..], |c| c.as_bytes()));
            append_vertical_line(out, &field, inner_width);
        }
        if row_index + 1 < body.len() {
            out.extend_from_slice("├".as_bytes());
            for _ in 0..inner_width {
                out.extend_from_slice(RULE);
            }
            out.extend_from_slice("┤\n".as_bytes());
        }
    }
    out.extend_from_slice("└".as_bytes());
    for _ in 0..inner_width {
        out.extend_from_slice(RULE);
    }
    out.extend_from_slice("┘\n".as_bytes());
}

/// `header: value` lines cut to `cols`, no box (`transcript_blocks.zig:1599-1613`).
fn render_unboxed_table(table: &TablePayload, cols: u16, out: &mut Vec<u8>) {
    let width = usize::from(cols);
    for row in table.rows.get(1..).unwrap_or_default() {
        for col in 0..table.alignments.len() {
            let mut field = Vec::new();
            field.extend_from_slice(table_cell(table, 0, col));
            field.extend_from_slice(b": ");
            field.extend_from_slice(row.cells.get(col).map_or(&b""[..], |c| c.as_bytes()));
            out.extend_from_slice(dw::prefix_by_width_ignoring_ansi(&field, width));
            out.push(b'\n');
        }
    }
}

/// The cell at `row`, `col`, or nothing for a ragged row.
fn table_cell(table: &TablePayload, row: usize, col: usize) -> &[u8] {
    table
        .rows
        .get(row)
        .and_then(|row| row.cells.get(col))
        .map_or(&b""[..], |cell| cell.as_bytes())
}

// ---------------------------------------------------------------------------
// Cell SGR carry (`assistant/pacer.zig:42-163`) and OSC 8 state
// (`transcript_blocks.zig:1620-1643`)
// ---------------------------------------------------------------------------

const OSC8_PREFIX: &[u8] = b"\x1b]8;";
const OSC8_CLOSE: &[u8] = b"\x1b]8;;\x1b\\";

/// Upstream's `assistant_pacer.SgrState`: which of bold, dim, italic,
/// underline and strike are on, and the foreground open, as a wrapped cell
/// must re-open them on its next line.
///
/// Upstream stores a foreground as one of the theme's inline-code or link
/// slots, re-resolved when it is written, or as up to 24 bytes of anything
/// else (`pacer.zig:101-110`). Within one render the theme cannot change, so
/// the slot re-resolves to the bytes that set it; this keeps those bytes,
/// which needs no theme here. The one place that differs: a foreground open
/// longer than 24 bytes that is *not* a theme slot. Upstream ignores it and
/// keeps the previous foreground; this carries it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct CellSgr<'a> {
    bold: bool,
    dim: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    fg: Option<&'a [u8]>,
}

/// Upstream's `writeOpens` buffer (`transcript_blocks.zig:1530`): an open that
/// would not fit is skipped.
const REOPEN_CAPACITY: usize = 64;

impl<'a> CellSgr<'a> {
    fn is_active(&self) -> bool {
        self.bold || self.dim || self.italic || self.underline || self.strike || self.fg.is_some()
    }

    /// Updates from one complete escape sequence; anything but an SGR is
    /// ignored (`pacer.zig:64-117`).
    fn apply(&mut self, seq: &'a [u8]) {
        if seq.len() < 3 || seq[0] != ESC || seq[1] != b'[' || seq[seq.len() - 1] != b'm' {
            return;
        }
        let body = &seq[2..seq.len() - 1];

        if body.is_empty() || body == b"0" {
            *self = Self::default();
            return;
        }

        // Exact full-body match against the known single-parameter codes the
        // markdown renderer emits; multi-parameter SGRs (e.g. "1;38;5;245")
        // count for every attribute and the colour they carry.
        match body {
            b"1" => self.bold = true,
            b"2" => self.dim = true,
            b"3" => self.italic = true,
            b"4" => self.underline = true,
            b"9" => self.strike = true,
            b"22" => {
                self.bold = false;
                self.dim = false;
            }
            b"23" => self.italic = false,
            b"24" => self.underline = false,
            b"29" => self.strike = false,
            b"39" => self.fg = None,
            _ => {
                if body.contains(&b';') {
                    let mut params = body.split(|&byte| byte == b';');
                    while let Some(param) = params.next() {
                        if param == b"38" || param == b"48" {
                            // Skip the colour space and value parameters so
                            // their digits are not mistaken for attributes.
                            if let Some(space) = params.next() {
                                let skip = if space == b"2" { 3 } else { 1 };
                                for _ in 0..skip {
                                    params.next();
                                }
                            }
                            continue;
                        }
                        match param {
                            b"1" => self.bold = true,
                            b"2" => self.dim = true,
                            b"3" => self.italic = true,
                            b"4" => self.underline = true,
                            b"9" => self.strike = true,
                            _ => {}
                        }
                    }
                }
                if has_foreground_color(body) {
                    self.fg = Some(seq);
                }
            }
        }
    }

    /// The opens that restore this state, bold first and the foreground last
    /// (`pacer.zig:132-162`).
    fn write_opens(&self, out: &mut Vec<u8>) {
        let mut written = 0;
        let mut append = |src: &[u8]| {
            if written + src.len() <= REOPEN_CAPACITY {
                out.extend_from_slice(src);
                written += src.len();
            }
        };
        if self.bold {
            append(b"\x1b[1m");
        }
        if self.dim {
            append(b"\x1b[2m");
        }
        if self.italic {
            append(b"\x1b[3m");
        }
        if self.underline {
            append(b"\x1b[4m");
        }
        if self.strike {
            append(b"\x1b[9m");
        }
        if let Some(fg) = self.fg {
            append(fg);
        }
    }
}

/// True when an SGR body sets a foreground: a `38` extended colour or a basic
/// 30-37/90-97, combined forms included (`pacer.zig:121-129`).
fn has_foreground_color(body: &[u8]) -> bool {
    body.split(|&byte| byte == b';').any(|param| {
        param == b"38"
            || zig_parse_u8(param)
                .is_some_and(|value| (30..=37).contains(&value) || (90..=97).contains(&value))
    })
}

/// `std.fmt.parseInt(u8, param, 10)` as Zig 0.16 reads it: an optional sign,
/// decimal digits with `_` separators between them, and overflow as failure.
/// A negative value parses only when it is zero.
fn zig_parse_u8(param: &[u8]) -> Option<u8> {
    let (negative, digits) = match param.first() {
        Some(b'+') => (false, &param[1..]),
        Some(b'-') => (true, &param[1..]),
        _ => (false, param),
    };
    if digits.is_empty() || digits[0] == b'_' || digits[digits.len() - 1] == b'_' {
        return None;
    }
    let mut value: u8 = 0;
    for &byte in digits {
        if byte == b'_' {
            continue;
        }
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add(byte - b'0')?;
    }
    if negative && value != 0 {
        return None;
    }
    Some(value)
}

/// Tracks the OSC 8 link `fragment` leaves open: an open with a target sets
/// it, an open with an empty target closes it (`updateNoticeHyperlinkState`).
fn update_hyperlink_state<'a>(fragment: &'a [u8], active: &mut Option<&'a [u8]>) {
    let mut offset = 0;
    while let Some(start) = index_of_pos(fragment, offset, OSC8_PREFIX) {
        let sequence_end = dw::ansi_sequence_end(fragment, start);
        if sequence_end <= start || sequence_end > fragment.len() {
            return;
        }
        let sequence = &fragment[start..sequence_end];
        let last = sequence[sequence.len() - 1];
        let terminator_len = if last == 0x07 {
            1
        } else if sequence.len() >= 2 && sequence[sequence.len() - 2] == ESC && last == b'\\' {
            2
        } else {
            return;
        };
        let payload = &sequence[OSC8_PREFIX.len()..sequence.len() - terminator_len];
        let Some(separator) = payload.iter().position(|&byte| byte == b';') else {
            return;
        };
        *active = if separator + 1 == payload.len() {
            None
        } else {
            Some(sequence)
        };
        offset = sequence_end;
    }
}

// ---------------------------------------------------------------------------
// Byte helpers
// ---------------------------------------------------------------------------

fn index_of_scalar_pos(text: &[u8], start: usize, needle: u8) -> Option<usize> {
    text.get(start..)?
        .iter()
        .position(|&byte| byte == needle)
        .map(|offset| start + offset)
}

fn index_of_pos(text: &[u8], start: usize, needle: &[u8]) -> Option<usize> {
    text.get(start..)?
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|offset| start + offset)
}

/// The renderers cut their `&str` input only at ASCII bytes and at display
/// unit boundaries, and insert only ASCII and whole glyphs, so the output is
/// valid UTF-8 whenever the input is. The one exception is an escape that runs
/// into a multi-byte character (`ESC` followed by a non-ASCII byte, which
/// `ansi_sequence_end` ends one byte in); the lossy fallback shows that as
/// U+FFFD rather than panicking in the paint path.
fn into_string(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes)
        .unwrap_or_else(|error| String::from_utf8_lossy(error.as_bytes()).into_owned())
}

#[cfg(test)]
mod tests {
    //! Upstream's tests for these renderers are ported first, with their
    //! inputs and expected bytes unchanged (`transcript_blocks.zig:3380-3640,
    //! 4557-4934`); the two that read a terminal emulator's cells read the
    //! same colours through [`super::super::pacer::SgrState`] instead.
    //!
    //! The rest compare against upstream's own output. [`FX_GOLDENS`] and
    //! the [`UPSTREAM_SWEEPS`] hashes were produced by fx `c1db9191` itself:
    //! a test added to a scratch copy of `transcript_blocks.zig` rendered
    //! these exact inputs with `renderCodeBlockForTranscriptWithTheme`,
    //! `renderTableForTranscript`, `renderThematicRuleForTranscript`, and
    //! `assistant_wrap.prefixStructuralRows`, under Zig 0.16 with upstream's
    //! default (dark, unpinned) theme.

    use super::*;

    use crate::markdown::theme::{FX_DARK, FX_LIGHT};
    use crate::markdown::{parse_table_payload, Styles};
    use crate::tui::check::Color;
    use crate::tui::pacer::SgrState;

    const LONG_ZIG: &str = "fn main() void {\n    const greeting = \"hello\"; // greet the world before returning to the caller\n\tconst wide = \"\u{6f22}\u{5b57}\u{304b}\u{306a}\";\n\n    return;\n}\n";
    const SHORT_ZIG: &str = "fn f() void {\n    x = \"ab\"; // note\n\ty = \"\u{6f22}\";\n}\n";
    const DIFF_SRC: &str = "diff --git a/x b/x\nindex 1..2 100644\n--- a/x\n+++ b/x\n@@ -1,2 +1,2 @@\n-old line\n+new line\n context\n";
    const CJK_SRC: &str = " //abcdefghijklmnop\u{6f22}";
    const READY_SRC: &str = "  const value = \"ready\"; // comment \u{6f22}";
    const TS_SRC: &str =
        "const hook = await resumeHook(token, { cleanup: true } as CleanupSignal);";
    const UNLABELED_SRC: &str = "plain words\n  indented line\n";
    const PREFIX_SRC: &str = "a\n\n\tb\n\x1b[2m\x1b[22m\n c\x1b[0m";

    const LINK_SRC: &str = concat!(
        "| Name | Notes |\n",
        "|------|-------|\n",
        "| api | see `alpha beta gamma delta` and [the project documentation](https://example.com/docs) ",
        "or [spaced reference](<https://example.com/a b>) now |\n",
    );
    const DECISION_SRC: &str = concat!(
        "| # | Choice | Direct consequences |\n",
        "|---|--------|---------------------|\n",
        "| 1 | **Change:** on reopen, an unfinished turn gets `turn_interrupted{reason: crash}`; it's never cut | `recovery.json` and `recovery.asked` go away. The adapter handles a turn that ends in a tool call with no result |\n",
        "| 8 | **Keep:** forks start only at turn boundaries (or turn 0) | Mid-turn forks deferred |\n",
    );
    const ALIGN_SRC: &str =
        "| Name | Count | Mid |\n|:--|--:|:-:|\n| api | 7 | x |\n| web service | 12345 | yy |\n";
    const BOLD_SRC: &str = "| prefix __strong__ suffix |\n|------|\n| value |\n";
    const CJK_TABLE_SRC: &str = "| \u{540d}\u{524d} | \u{8aac}\u{660e} |\n|---|---|\n| \u{6f22}\u{5b57} | \u{9577}\u{3044}\u{8aac}\u{660e}\u{306e}\u{30c6}\u{30ad}\u{30b9}\u{30c8}\u{3067}\u{3059} |\n";
    const RAGGED_SRC: &str = "| Name | Notes |\n|------|-------|\n| a | `foo bar` x |\n| b |\n";
    const ZWJ_SRC: &str = "| S |\n|---|\n| \u{1F469}\u{200D}\u{1F4BB} |\n";
    const HEADER_ONLY_SRC: &str = "| Only |\n|------|\n";

    /// Every code fixture, with its fence label.
    const CODE_CASES: [(&str, &str); 12] = [
        ("zig", LONG_ZIG),
        ("zig", SHORT_ZIG),
        ("diff", DIFF_SRC),
        ("zig", CJK_SRC),
        ("", UNLABELED_SRC),
        ("zig", ""),
        ("Zig", READY_SRC),
        ("", TS_SRC),
        ("python", "def ready():\n    return True"),
        ("typescript", "x"),
        ("\u{6f22}", "x"),
        ("brainfuck", "+++[>+++<-]"),
    ];

    /// Every table fixture, in the order upstream's dump parsed them: the
    /// one with links first, so its OSC 8 ids start where upstream's counter
    /// does.
    const TABLE_SOURCES: [&str; 8] = [
        LINK_SRC,
        DECISION_SRC,
        ALIGN_SRC,
        BOLD_SRC,
        CJK_TABLE_SRC,
        RAGGED_SRC,
        ZWJ_SRC,
        HEADER_ONLY_SRC,
    ];

    fn code(language: &str, code: &str) -> CodeBlockPayload {
        CodeBlockPayload {
            language: language.to_owned(),
            code: code.to_owned(),
        }
    }

    fn table(source: &str) -> TablePayload {
        parse_table_payload(source, &Styles::default(), &mut 1)
    }

    /// [`TABLE_SOURCES`] parsed with one link counter starting at 1.
    fn fixture_tables() -> Vec<TablePayload> {
        let styles = Styles::default();
        let mut link_id = 1;
        TABLE_SOURCES
            .iter()
            .map(|source| parse_table_payload(source, &styles, &mut link_id))
            .collect()
    }

    fn rows(rendered: &str) -> std::str::Split<'_, char> {
        rendered.split('\n')
    }

    fn width(row: &str) -> usize {
        dw::visible_width_ignoring_ansi(row.as_bytes())
    }

    fn assert_rows_within(rendered: &str, cols: u16, what: &str) {
        for row in rows(rendered) {
            assert!(
                width(row) <= usize::from(cols),
                "{what} at {cols}: {row:?} is {} wide",
                width(row)
            );
        }
    }

    // -----------------------------------------------------------------------
    // Code block: upstream's tests (`transcript_blocks.zig:3380-3640`)
    // -----------------------------------------------------------------------

    #[test]
    fn code_block_dims_solid_horizontal_rules_without_side_rails() {
        assert_eq!(
            render_code_block(&code("zig", "x"), 80, &FX_DARK),
            concat!(
                "\x1b[2m─ zig ─\x1b[22m\n",
                "x\n",
                "\x1b[2m───────\x1b[22m\n",
            )
        );
        assert_eq!(
            render_code_block(&code("", "x"), 80, &FX_DARK),
            concat!("\x1b[2m──────\x1b[22m\n", "x\n", "\x1b[2m──────\x1b[22m\n",)
        );
        assert_eq!(
            render_code_block(&code("typescript", "x"), 8, &FX_DARK),
            concat!(
                "\x1b[2m─ type ─\x1b[22m\n",
                "x\n",
                "\x1b[2m────────\x1b[22m\n",
            )
        );
        assert_eq!(
            render_code_block(&code("\u{6f22}", "x"), 6, &FX_DARK),
            concat!("\x1b[2m─ 漢 ─\x1b[22m\n", "x\n", "\x1b[2m──────\x1b[22m\n",)
        );
    }

    #[test]
    fn code_block_bounds_highlighted_rows_across_every_supported_width() {
        let block = code("Zig", READY_SRC);
        for cols in 1..=80 {
            assert_rows_within(&render_code_block(&block, cols, &FX_DARK), cols, "ready");
        }
    }

    #[test]
    fn code_block_highlights_registered_profiles_without_styling_unknown_labels() {
        let highlighted = render_code_block(&code("Zig", "const value = 1;"), 80, &FX_DARK);
        assert!(highlighted.contains("\x1b[38;5;252mconst\x1b[39m"));

        let python = render_code_block(
            &code("python", "def ready():\n    return True"),
            80,
            &FX_DARK,
        );
        assert!(python.contains("\x1b[2m─ python ─"));
        assert!(python.contains("\x1b[38;5;252mdef\x1b[39m"));

        let raw = render_code_block(&code("brainfuck", "+++[>+++<-]"), 80, &FX_DARK);
        assert!(!raw.contains("\x1b[38;5;"));
        assert!(raw.contains("+++[>+++<-]"));
    }

    #[test]
    fn semantic_code_blocks_use_the_light_syntax_palette_when_requested() {
        let rendered = code_block_row_bytes(
            &code("zig", "const value = \"ready\"; // comment"),
            80,
            &FX_LIGHT,
        );
        assert!(rendered.contains("\x1b[38;5;238mconst\x1b[39m"));
        assert!(rendered.contains("\x1b[38;5;241m\"ready\"\x1b[39m"));
        assert!(rendered.contains("\x1b[38;5;243m// comment\x1b[39m"));
    }

    #[test]
    fn code_block_infers_registered_high_confidence_code_blocks() {
        let unlabeled = render_code_block(&code("", TS_SRC), 100, &FX_DARK);
        assert!(unlabeled.contains("\x1b[2m─ ts ─"));
        assert!(unlabeled.contains("\x1b[38;5;252mconst\x1b[39m"));
        assert!(unlabeled.contains("\x1b[38;5;252mawait\x1b[39m"));

        let json = render_code_block(&code("", "{\"ready\": true}"), 100, &FX_DARK);
        assert!(json.contains("\x1b[2m─ json ─"));
        assert!(json.contains("\x1b[38;5;250m\"ready\"\x1b[39m"));

        let ambiguous = render_code_block(&code("", "const value = 1;"), 100, &FX_DARK);
        assert!(!ambiguous.contains("\x1b[2mts\x1b[22m"));
        assert!(!ambiguous.contains("\x1b[38;5;"));

        let explicit_unknown = render_code_block(&code("brainfuck", TS_SRC), 100, &FX_DARK);
        assert!(explicit_unknown.contains("\x1b[2m─ brainfuck ─"));
        assert!(!explicit_unknown.contains("\x1b[38;5;"));
    }

    /// Plays `rendered` through an SGR tracker the way a terminal would and
    /// calls `cell` with the colour each display unit is painted in, then
    /// checks that no row leaves a colour open past its end.
    fn paint(rendered: &str, mut cell: impl FnMut(&str, Color)) {
        let mut sgr = SgrState::default();
        for row in rows(rendered) {
            let bytes = row.as_bytes();
            let mut index = 0;
            while index < bytes.len() {
                if bytes[index] == ESC {
                    let end = dw::ansi_sequence_end(bytes, index);
                    sgr.observe(&row[index..end]);
                    index = end;
                    continue;
                }
                let unit = dw::display_unit_at(bytes, index);
                let color = sgr.color().expect("a colour this crate emits");
                cell(&row[index..index + unit.byte_len], color);
                index += unit.byte_len;
            }
            assert_eq!(
                sgr.color().ok(),
                Some(Color::Default),
                "{row:?} leaks a colour"
            );
        }
    }

    /// Upstream feeds both renders and a trailing `z` to its terminal
    /// emulator and reads the cells; this reads the same colours per unit.
    #[test]
    fn code_block_contains_cjk_fallback_color_in_ruled_and_unboxed_rows() {
        let block = code("zig", CJK_SRC);

        let mut ruled_wide_rune = None;
        paint(&render_code_block(&block, 6, &FX_DARK), |unit, color| {
            if unit == "\u{6f22}" {
                ruled_wide_rune = Some(color);
            }
            if unit == " " || unit == "─" {
                assert_eq!(color, Color::Default, "{unit:?}");
            }
        });
        assert_eq!(ruled_wide_rune, Some(Color::Indexed(245)));

        let mut unboxed_fallback = None;
        paint(&render_code_block(&block, 1, &FX_DARK), |unit, color| {
            if unit == "?" {
                unboxed_fallback = Some(color);
            }
        });
        assert_eq!(unboxed_fallback, Some(Color::Indexed(245)));
    }

    /// `renderEntriesToBytes keeps semantic code as its own assistant entry`
    /// (`transcript_blocks.zig:4936-4962`), the code rows alone.
    #[test]
    fn code_block_rows_keep_the_labelled_rule_and_fit_narrow_screens() {
        let block = code("zig", "  const value = 1;\n");
        let wide = code_block_row_bytes(&block, 80, &FX_DARK);
        assert!(wide.contains("\x1b[2m─ zig ─"));
        assert!(!wide.contains("\x1b[2mzig\x1b[22m\n─"));

        let narrow = code_block_row_bytes(&block, 6, &FX_DARK);
        assert!(!narrow.contains('┌'));
        assert_rows_within(&narrow, 6, "narrow");
    }

    // -----------------------------------------------------------------------
    // Code block: the two traps
    // -----------------------------------------------------------------------

    /// The panel is measured on the raw source: a highlighted block and an
    /// unstyled one of the same source draw the same rules.
    #[test]
    fn code_panel_width_comes_from_the_source_not_the_highlighted_bytes() {
        let source = "const value = 1;";
        let highlighted = render_code_block(&code("zig", source), 80, &FX_DARK);
        let plain = render_code_block(&code("brainfuck", source), 80, &FX_DARK);
        assert!(highlighted.contains("\x1b[38;5;"));
        assert!(!plain.contains("\x1b[38;5;"));
        let closing = |rendered: &str| rows(rendered).rev().nth(1).unwrap().to_owned();
        assert_eq!(closing(&highlighted), closing(&plain));
        assert_eq!(closing(&highlighted).matches('─').count(), source.len());
    }

    /// A wrapped comment re-opens its colour on every continuation row,
    /// after the line's indent, and closes it before the row ends.
    #[test]
    fn code_rows_carry_the_foreground_across_wraps_after_the_indent() {
        let rendered = render_code_block(&code("zig", CJK_SRC), 6, &FX_DARK);
        let body: Vec<&str> = rows(&rendered).skip(1).take(4).collect();
        assert_eq!(
            body,
            [
                " \x1b[38;5;245m//abc\x1b[0m",
                " \x1b[38;5;245mdefgh\x1b[0m",
                " \x1b[38;5;245mijklm\x1b[0m",
                " \x1b[38;5;245mnop漢\x1b[39m",
            ]
        );
    }

    // -----------------------------------------------------------------------
    // Table: upstream's tests (`transcript_blocks.zig:4669-4934`)
    // -----------------------------------------------------------------------

    #[test]
    fn table_reasserts_bold_after_an_inline_strong_header_span() {
        let out = render_table(&table(BOLD_SRC), 80);
        assert!(out.contains("\x1b[1mprefix \x1b[1mstrong\x1b[22m\x1b[1m suffix\x1b[22m"));
    }

    /// Every row fits `cols` and the grid's rows are all one width.
    fn expect_grid_lines(out: &str, cols: u16) -> usize {
        let mut grid_width = None;
        let mut count = 0;
        for line in out.trim_end_matches('\n').split('\n') {
            let line_width = width(line);
            assert!(line_width <= usize::from(cols), "{line:?}");
            match grid_width {
                Some(expected) => assert_eq!(line_width, expected, "{line:?}"),
                None => grid_width = Some(line_width),
            }
            count += 1;
        }
        count
    }

    #[test]
    fn table_wraps_long_cells_inside_the_grid_instead_of_stacking_fields() {
        let cols = 100;
        let out = render_table(&table(DECISION_SRC), cols);

        assert!(out.starts_with("┌───┬"));
        assert!(!out.contains("Choice: "));
        assert!(!out.contains("#: "));
        // Borders, header, and two rows need seven lines; wrapped cells add more.
        assert!(expect_grid_lines(&out, cols) > 7);
        assert!(out.contains("\n│ 1 │ "));
        assert!(out.contains("\n│   │ "));
        assert!(out.contains("\n│ 8 │ "));
        for word in [
            "unfinished",
            "turn_interrupted{reason:",
            "recovery.asked",
            "boundaries",
            "consequences",
        ] {
            assert!(out.contains(word), "{word}");
        }
    }

    #[test]
    fn table_closes_and_reopens_inline_styles_across_wrapped_cell_lines() {
        let cols = 30;
        let out = render_table(&table(LINK_SRC), cols);
        assert!(expect_grid_lines(&out, cols) > 5);

        let inline_code = FX_DARK.inline_code_open.as_bytes();
        for line in out.trim_end_matches('\n').split('\n') {
            let bytes = line.as_bytes();
            let mut style = CellSgr::default();
            let mut hyperlink = None;
            let mut index = 0;
            while index < bytes.len() {
                if bytes[index] == ESC {
                    let end = dw::ansi_sequence_end(bytes, index);
                    style.apply(&bytes[index..end]);
                    update_hyperlink_state(&bytes[index..end], &mut hyperlink);
                    index = end;
                    continue;
                }
                let word_end = bytes[index..]
                    .iter()
                    .position(|&byte| byte == b' ' || byte == ESC)
                    .map_or(bytes.len(), |offset| index + offset);
                let word = &line[index..word_end];
                if ["alpha", "beta", "gamma", "delta"].contains(&word) {
                    assert_eq!(style.fg, Some(inline_code), "{word} in {line:?}");
                }
                if ["project", "documentation", "spaced", "reference"].contains(&word) {
                    assert!(hyperlink.is_some(), "{word} in {line:?}");
                }
                index = word_end.max(index + 1);
            }
            assert!(!style.is_active(), "{line:?}");
            assert!(hyperlink.is_none(), "{line:?}");
        }
    }

    fn texts<'a>(lines: &[TableCellLine<'a>]) -> Vec<&'a [u8]> {
        lines.iter().map(|line| line.text).collect()
    }

    #[test]
    fn wrap_table_cell_breaks_at_the_column_edge_and_never_emits_a_blank_line() {
        let mut lines = Vec::new();

        wrap_table_cell(b"A very long header that wraps", 11, &mut lines);
        assert_eq!(
            texts(&lines),
            [&b"A very long"[..], b"header that", b"wraps"]
        );

        // A code span ending in a space breaks after its last word.
        let cell = format!("{}foo \x1b[39m bar", FX_DARK.inline_code_open);
        wrap_table_cell(cell.as_bytes(), 3, &mut lines);
        assert_eq!(lines.len(), 2);
        assert!(lines[0].close_style);
        assert_eq!(lines[1].text, b"bar");
        assert!(!lines[1].style.is_active());

        // A code span that starts with spaces drops them at the first break and
        // keeps its style on every line.
        let leading = format!("{}  configuration\x1b[39m", FX_DARK.inline_code_open);
        wrap_table_cell(leading.as_bytes(), 7, &mut lines);
        assert_eq!(texts(&lines), [&b"configu"[..], b"ration\x1b[39m"]);
        for line in &lines {
            assert_eq!(line.style.fg, Some(FX_DARK.inline_code_open.as_bytes()));
        }
        for narrow in 1..17 {
            wrap_table_cell(leading.as_bytes(), narrow, &mut lines);
            for line in &lines {
                assert!(has_table_cell_text(line.text), "{narrow}: {:?}", line.text);
            }
        }
    }

    #[test]
    fn wrap_table_cell_advances_past_zero_width_units_at_a_break() {
        let mut lines = Vec::new();

        wrap_table_cell("foo \u{200B} bar".as_bytes(), 3, &mut lines);
        assert_eq!(texts(&lines), [&b"foo"[..], b"bar"]);

        for cell in [
            "foo \u{FE0F} bar",
            "\u{200B} supercalifragilistic",
            "\u{200E} word and more",
            "a\u{200B} \u{200B} b",
        ] {
            for width in 1..12 {
                wrap_table_cell(cell.as_bytes(), width, &mut lines);
                for line in &lines {
                    assert!(line.width <= width, "{cell:?} at {width}");
                }
            }
        }
    }

    #[test]
    fn wrap_table_cell_resets_a_style_it_cannot_restore_before_the_padding() {
        let mut lines = Vec::new();
        wrap_table_cell(b"\x1b[48;5;236mfoo bar\x1b[49m", 3, &mut lines);
        assert_eq!(lines.len(), 2);
        assert!(!lines[1].style.is_active());
        assert!(lines[0].close_style);
    }

    #[test]
    fn fit_table_column_widths_keeps_short_columns_whole_and_gives_longer_text_more_room() {
        let measure = |natural, word| TableColumnMeasure { natural, word };
        let measures = [measure(1, 1), measure(90, 24), measure(170, 14)];
        let mut widths = [0; 3];

        let wide = [measure(1, 1), measure(20, 8), measure(30, 9)];
        assert!(fit_table_column_widths(&wide, 70, &mut widths));
        assert_eq!(widths, [1, 20, 30]);

        // Proportional shares are 31 and 58 of the 89 open cells.
        assert!(fit_table_column_widths(&measures, 100, &mut widths));
        assert_eq!(widths, [1, 31, 58]);

        // The proportional share of the second column is below its longest word.
        assert!(fit_table_column_widths(&measures, 60, &mut widths));
        assert_eq!(widths, [1, 24, 25]);

        // Whole words no longer fit, so both wrapped columns break words.
        assert!(fit_table_column_widths(&measures, 30, &mut widths));
        assert_eq!(widths, [1, 6, 13]);

        assert!(fit_table_column_widths(&measures, 17, &mut widths));
        assert_eq!(widths, [1, 3, 3]);
        assert!(!fit_table_column_widths(&measures, 16, &mut widths));
    }

    #[test]
    fn table_keeps_a_grid_at_every_width_that_fits_minimum_columns() {
        let decision = table(DECISION_SRC);
        // Borders take ten cells; "#" needs one and each wrapped column three.
        for cols in 3..=120 {
            let out = render_table(&decision, cols);
            let grid = out.contains('┬');
            assert_eq!(cols >= 17, grid, "{cols}");
            if grid {
                expect_grid_lines(&out, cols);
            } else {
                assert_rows_within(&out, cols, "stacked");
            }
        }
    }

    #[test]
    fn unicode_display_units_remain_atomic_in_three_column_vertical_tables() {
        for sequence in [
            "\u{2600}\u{FE0F}",
            "\u{231A}\u{FE0E}",
            "\u{1F44D}\u{1F3FD}",
            "\u{1F1FA}\u{1F1F8}",
            "#\u{FE0F}\u{20E3}",
            "\u{1F3F4}\u{E0067}\u{E0062}\u{E0065}\u{E006E}\u{E0067}\u{E007F}",
            "\u{1F469}\u{200D}\u{1F4BB}",
        ] {
            let source = format!("| S |\n|---|\n| {sequence} |\n");
            let out = render_table(&table(&source), 3);
            assert!(!out.contains(sequence), "{sequence:?}");
            assert!(out.contains('?'), "{sequence:?}");
            assert_rows_within(&out, 3, sequence);
        }
    }

    // -----------------------------------------------------------------------
    // Thematic rule (`transcript_blocks.zig:4557-4590`)
    // -----------------------------------------------------------------------

    #[test]
    fn thematic_rule_fills_the_assistant_width_after_its_gutter() {
        assert_eq!(render_thematic_rule(0), "");
        for cols in [1, 2, 3, 48, 80, 120] {
            let out = render_thematic_rule(cols);
            assert!(!out.contains('\n'));
            assert_eq!(width(&out), usize::from(cols));
            assert!(out.ends_with("\x1b[22m"));
            assert_eq!(
                out.matches('─').count(),
                usize::from(cols - gutter_width(cols))
            );
        }
        assert_eq!(render_thematic_rule(1), "\x1b[2m─\x1b[22m");
        assert_eq!(render_thematic_rule(2), " \x1b[2m─\x1b[22m");
        assert_eq!(render_thematic_rule(3), "  \x1b[2m─\x1b[22m");
    }

    // -----------------------------------------------------------------------
    // Gutter
    // -----------------------------------------------------------------------

    #[test]
    fn gutter_is_two_columns_from_three_one_at_two_and_none_below() {
        assert_eq!([0, 1, 2, 3, 4, 80].map(gutter_width), [0, 0, 1, 2, 2, 2]);
    }

    /// Upstream indents a row only when a visible unit comes before any
    /// control byte, so a row that opens with a tab keeps column zero.
    #[test]
    fn structural_rows_skip_blank_and_control_led_rows() {
        assert_eq!(
            prefix_structural_rows(PREFIX_SRC, 2),
            "  a\n\n\tb\n\x1b[2m\x1b[22m\n   c\x1b[0m"
        );
        assert_eq!(prefix_structural_rows(PREFIX_SRC, 0), PREFIX_SRC);
        assert_eq!(prefix_structural_rows("", 2), "");
        let rendered = code_block_row_bytes(&code("zig", SHORT_ZIG), 80, &FX_DARK);
        assert!(rendered.contains("\n\ty = "));
        assert!(rendered.contains("\n      x = "));
    }

    // -----------------------------------------------------------------------
    // Every row fits
    // -----------------------------------------------------------------------

    #[test]
    fn every_row_of_every_block_fits_its_columns() {
        for (language, source) in CODE_CASES {
            let block = code(language, source);
            for theme in [&FX_DARK, &FX_LIGHT] {
                for cols in 0..=120 {
                    assert_rows_within(&render_code_block(&block, cols, theme), cols, source);
                    assert_rows_within(&code_block_row_bytes(&block, cols, theme), cols, source);
                }
            }
        }
        for (source, parsed) in TABLE_SOURCES.iter().zip(fixture_tables()) {
            for cols in 0..=120 {
                assert_rows_within(&render_table(&parsed, cols), cols, source);
                assert_rows_within(&table_row_bytes(&parsed, cols), cols, source);
            }
        }
        for cols in 0..=120 {
            assert_eq!(width(&render_thematic_rule(cols)), usize::from(cols));
        }
    }

    // -----------------------------------------------------------------------
    // Upstream's own bytes
    // -----------------------------------------------------------------------

    /// The payload as the dump spelled it: cells split by 0x1f, rows by 0x1e,
    /// then 0x1d and one letter per column alignment.
    fn cells_dump(table: &TablePayload) -> String {
        let rows: Vec<String> = table
            .rows
            .iter()
            .map(|row| row.cells.join("\u{1f}"))
            .collect();
        let alignments: String = table
            .alignments
            .iter()
            .map(|alignment| match alignment {
                TableColumnAlign::Left => 'l',
                TableColumnAlign::Right => 'r',
                TableColumnAlign::Center => 'c',
            })
            .collect();
        format!("{}\u{1d}{alignments}", rows.join("\u{1e}"))
    }

    /// What this port renders for one named upstream dump case.
    fn fx_case(name: &str) -> String {
        let tables = fixture_tables();
        let [link, decision, align, bold, cjk, ..] = &tables[..] else {
            unreachable!("eight fixture tables");
        };
        if let Some(which) = name.strip_prefix("cells_") {
            return cells_dump(match which {
                "link" => link,
                "decision" => decision,
                "align" => align,
                "bold" => bold,
                "cjk" => cjk,
                _ => panic!("unknown case {name}"),
            });
        }
        let (stem, cols) = name.rsplit_once('_').expect("case has a width");
        let cols: u16 = cols.parse().expect("width is a number");
        match stem {
            "code_long" => render_code_block(&code("zig", LONG_ZIG), cols, &FX_DARK),
            "code_long_light" => render_code_block(&code("zig", LONG_ZIG), cols, &FX_LIGHT),
            "code_short" => render_code_block(&code("zig", SHORT_ZIG), cols, &FX_DARK),
            "code_diff" => render_code_block(&code("diff", DIFF_SRC), cols, &FX_DARK),
            "code_cjk" => render_code_block(&code("zig", CJK_SRC), cols, &FX_DARK),
            "code_unlabeled" => render_code_block(&code("", UNLABELED_SRC), cols, &FX_DARK),
            "code_empty" => render_code_block(&code("zig", ""), cols, &FX_DARK),
            "rows_code_short" => code_block_row_bytes(&code("zig", SHORT_ZIG), cols, &FX_DARK),
            "table_link" => render_table(link, cols),
            "table_decision" => render_table(decision, cols),
            "table_align" => render_table(align, cols),
            "table_bold" => render_table(bold, cols),
            "table_cjk" => render_table(cjk, cols),
            "rows_table_align" => table_row_bytes(align, cols),
            "rule" => render_thematic_rule(cols),
            "prefix" => prefix_structural_rows(PREFIX_SRC, cols),
            _ => panic!("unknown case {name}"),
        }
    }

    #[test]
    fn matches_upstream_bytes_case_by_case() {
        let mismatches: Vec<String> = FX_GOLDENS
            .iter()
            .filter_map(|&(name, expected)| {
                let got = fx_case(name);
                (got != expected).then(|| format!("{name}\n  fx:   {expected:?}\n  xfx:  {got:?}"))
            })
            .collect();
        assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
    }

    /// The widths upstream renders identically to a case above.
    #[test]
    fn matches_upstream_where_a_wider_screen_changes_nothing() {
        let long = code("zig", LONG_ZIG);
        assert_eq!(
            render_code_block(&long, 120, &FX_DARK),
            fx_case("code_long_80")
        );
        let diff = code("diff", DIFF_SRC);
        assert_eq!(
            render_code_block(&diff, 40, &FX_DARK),
            fx_case("code_diff_80")
        );
    }

    /// FNV-1a 64 over every output in order, each followed by 0xff (a byte
    /// UTF-8 never contains), as the upstream dump hashed them.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct Fnv {
        hash: u64,
        count: usize,
    }

    impl Fnv {
        fn new() -> Self {
            Self {
                hash: 0xcbf2_9ce4_8422_2325,
                count: 0,
            }
        }

        fn feed(&mut self, bytes: &str) {
            for &byte in bytes.as_bytes().iter().chain(&[0xff]) {
                self.hash ^= u64::from(byte);
                self.hash = self.hash.wrapping_mul(0x0100_0000_01b3);
            }
            self.count += 1;
        }
    }

    /// Upstream's hashes over every fixture at every width 0..=120: code
    /// panels and code rows (twelve blocks, dark and light), tables and table
    /// rows (eight tables), and the rule.
    const UPSTREAM_SWEEPS: [(&str, u64, usize); 5] = [
        ("code", 0x46fb_06fe_6a79_bf99, 2904),
        ("code_rows", 0xbb3b_b2c9_486b_6137, 2904),
        ("table", 0xc81f_6f34_efb7_e671, 968),
        ("table_rows", 0x663b_90e3_dc42_98bf, 968),
        ("rule", 0x917b_e065_e3f0_a13a, 121),
    ];

    #[test]
    fn matches_upstream_bytes_at_every_width() {
        let mut panels = Fnv::new();
        let mut code_rows = Fnv::new();
        for (language, source) in CODE_CASES {
            let block = code(language, source);
            for theme in [&FX_DARK, &FX_LIGHT] {
                for cols in 0..=120 {
                    panels.feed(&render_code_block(&block, cols, theme));
                    code_rows.feed(&code_block_row_bytes(&block, cols, theme));
                }
            }
        }
        let mut tables = Fnv::new();
        let mut table_rows = Fnv::new();
        for parsed in fixture_tables() {
            for cols in 0..=120 {
                tables.feed(&render_table(&parsed, cols));
                table_rows.feed(&table_row_bytes(&parsed, cols));
            }
        }
        let mut rules = Fnv::new();
        for cols in 0..=120 {
            rules.feed(&render_thematic_rule(cols));
        }
        let got = [panels, code_rows, tables, table_rows, rules];
        for ((name, hash, count), sweep) in UPSTREAM_SWEEPS.iter().zip(got) {
            assert_eq!(
                (sweep.hash, sweep.count),
                (*hash, *count),
                "{name} differs from upstream"
            );
        }
    }

    /// Upstream's output for the named cases, byte for byte.
    const FX_GOLDENS: &[(&str, &str)] = &[
        (
            "code_long_80",
            concat!(
                "\x1b[2m─ zig ─────────────────────────────────────────────────────────────────────────\x1b[22m\n",
                "\x1b[38;5;252mfn\x1b[39m main() \x1b[38;5;252mvoid\x1b[39m {\n",
                "    \x1b[38;5;252mconst\x1b[39m greeting = \x1b[38;5;250m\"hello\"\x1b[39m; \x1b[38;5;245m// greet the world before returning to the caller\x1b[39m\n",
                "\t\x1b[38;5;252mconst\x1b[39m wide = \x1b[38;5;250m\"漢字かな\"\x1b[39m;\n",
                "\n",
                "    \x1b[38;5;252mreturn\x1b[39m;\n",
                "}\n",
                "\x1b[2m───────────────────────────────────────────────────────────────────────────────\x1b[22m\n",
            ),
        ),
        (
            "code_long_40",
            concat!(
                "\x1b[2m─ zig ──────────────────────────────────\x1b[22m\n",
                "\x1b[38;5;252mfn\x1b[39m main() \x1b[38;5;252mvoid\x1b[39m {\n",
                "    \x1b[38;5;252mconst\x1b[39m greeting = \x1b[38;5;250m\"hello\"\x1b[39m; \x1b[38;5;245m// greet t\x1b[0m\n",
                "    \x1b[38;5;245mhe world before returning to the cal\x1b[0m\n",
                "    \x1b[38;5;245mler\x1b[39m\n",
                "\t\x1b[38;5;252mconst\x1b[39m wide = \x1b[38;5;250m\"漢字かな\"\x1b[39m;\n",
                "\n",
                "    \x1b[38;5;252mreturn\x1b[39m;\n",
                "}\n",
                "\x1b[2m────────────────────────────────────────\x1b[22m\n",
            ),
        ),
        (
            "code_long_light_80",
            concat!(
                "\x1b[2m─ zig ─────────────────────────────────────────────────────────────────────────\x1b[22m\n",
                "\x1b[38;5;238mfn\x1b[39m main() \x1b[38;5;238mvoid\x1b[39m {\n",
                "    \x1b[38;5;238mconst\x1b[39m greeting = \x1b[38;5;241m\"hello\"\x1b[39m; \x1b[38;5;243m// greet the world before returning to the caller\x1b[39m\n",
                "\t\x1b[38;5;238mconst\x1b[39m wide = \x1b[38;5;241m\"漢字かな\"\x1b[39m;\n",
                "\n",
                "    \x1b[38;5;238mreturn\x1b[39m;\n",
                "}\n",
                "\x1b[2m───────────────────────────────────────────────────────────────────────────────\x1b[22m\n",
            ),
        ),
        (
            "code_short_1",
            concat!(
                "\x1b[38;5;252mf\x1b[0m\n",
                "\x1b[38;5;252mn\x1b[39m\n",
                " \n",
                "f\n",
                "(\n",
                ")\n",
                " \x1b[38;5;252m\x1b[0m\n",
                "\x1b[38;5;252mv\x1b[0m\n",
                "\x1b[38;5;252mo\x1b[0m\n",
                "\x1b[38;5;252mi\x1b[0m\n",
                "\x1b[38;5;252md\x1b[39m\n",
                " \n",
                "{\n",
                " \n",
                " \n",
                " \n",
                " \n",
                "x\n",
                " \n",
                "=\n",
                " \x1b[38;5;250m\x1b[0m\n",
                "\x1b[38;5;250m\"\x1b[0m\n",
                "\x1b[38;5;250ma\x1b[0m\n",
                "\x1b[38;5;250mb\x1b[0m\n",
                "\x1b[38;5;250m\"\x1b[39m\n",
                ";\n",
                " \x1b[38;5;245m\x1b[0m\n",
                "\x1b[38;5;245m/\x1b[0m\n",
                "\x1b[38;5;245m/\x1b[0m\n",
                "\x1b[38;5;245m \x1b[0m\n",
                "\x1b[38;5;245mn\x1b[0m\n",
                "\x1b[38;5;245mo\x1b[0m\n",
                "\x1b[38;5;245mt\x1b[0m\n",
                "\x1b[38;5;245me\x1b[39m\n",
                "\ty\n",
                "\t \n",
                "\t=\n",
                "\t \x1b[38;5;250m\x1b[0m\n",
                "\t\x1b[38;5;250m\"\x1b[0m\n",
                "\t\x1b[38;5;250m?\x1b[0m\n",
                "\t\x1b[38;5;250m\"\x1b[39m\n",
                "\t;\n",
                "}\n",
            ),
        ),
        (
            "code_short_2",
            concat!(
                "\x1b[38;5;252mfn\x1b[39m\n",
                " f\n",
                "()\n",
                " \x1b[38;5;252mv\x1b[0m\n",
                "\x1b[38;5;252moi\x1b[0m\n",
                "\x1b[38;5;252md\x1b[39m \n",
                "{\n",
                "  \n",
                "  \n",
                "x \n",
                "= \x1b[38;5;250m\x1b[0m\n",
                "\x1b[38;5;250m\"a\x1b[0m\n",
                "\x1b[38;5;250mb\"\x1b[39m\n",
                "; \x1b[38;5;245m\x1b[0m\n",
                "\x1b[38;5;245m//\x1b[0m\n",
                "\x1b[38;5;245m n\x1b[0m\n",
                "\x1b[38;5;245mot\x1b[0m\n",
                "\x1b[38;5;245me\x1b[39m\n",
                "\ty \n",
                "\t= \x1b[38;5;250m\x1b[0m\n",
                "\t\x1b[38;5;250m\"\x1b[0m\n",
                "\t\x1b[38;5;250m漢\x1b[0m\n",
                "\t\x1b[38;5;250m\"\x1b[39m;\n",
                "}\n",
            ),
        ),
        (
            "code_short_3",
            concat!(
                "\x1b[38;5;252mfn\x1b[39m \n",
                "f()\n",
                " \x1b[38;5;252mvo\x1b[0m\n",
                "\x1b[38;5;252mid\x1b[39m \n",
                "{\n",
                "   \n",
                " x \n",
                "= \x1b[38;5;250m\"\x1b[0m\n",
                "\x1b[38;5;250mab\"\x1b[39m\n",
                "; \x1b[38;5;245m/\x1b[0m\n",
                "\x1b[38;5;245m/ n\x1b[0m\n",
                "\x1b[38;5;245mote\x1b[39m\n",
                "\ty =\n",
                "\t \x1b[38;5;250m\"\x1b[0m\n",
                "\t\x1b[38;5;250m漢\"\x1b[39m\n",
                "\t;\n",
                "}\n",
            ),
        ),
        (
            "code_short_4",
            concat!(
                "\x1b[38;5;252mfn\x1b[39m f\n",
                "() \x1b[38;5;252mv\x1b[0m\n",
                "\x1b[38;5;252moid\x1b[39m \n",
                "{\n",
                "    \n",
                "x = \x1b[38;5;250m\x1b[0m\n",
                "\x1b[38;5;250m\"ab\"\x1b[39m\n",
                "; \x1b[38;5;245m//\x1b[0m\n",
                "\x1b[38;5;245m not\x1b[0m\n",
                "\x1b[38;5;245me\x1b[39m\n",
                "\ty = \x1b[38;5;250m\x1b[0m\n",
                "\t\x1b[38;5;250m\"漢\"\x1b[39m\n",
                "\t;\n",
                "}\n",
            ),
        ),
        (
            "code_short_5",
            concat!(
                "\x1b[38;5;252mfn\x1b[39m f(\n",
                ") \x1b[38;5;252mvoi\x1b[0m\n",
                "\x1b[38;5;252md\x1b[39m {\n",
                "    x\n",
                "     \n",
                "    =\n",
                "     \x1b[38;5;250m\x1b[0m\n",
                "    \x1b[38;5;250m\"\x1b[0m\n",
                "    \x1b[38;5;250ma\x1b[0m\n",
                "    \x1b[38;5;250mb\x1b[0m\n",
                "    \x1b[38;5;250m\"\x1b[39m\n",
                "    ;\n",
                "     \x1b[38;5;245m\x1b[0m\n",
                "    \x1b[38;5;245m/\x1b[0m\n",
                "    \x1b[38;5;245m/\x1b[0m\n",
                "    \x1b[38;5;245m \x1b[0m\n",
                "    \x1b[38;5;245mn\x1b[0m\n",
                "    \x1b[38;5;245mo\x1b[0m\n",
                "    \x1b[38;5;245mt\x1b[0m\n",
                "    \x1b[38;5;245me\x1b[39m\n",
                "\ty = \x1b[38;5;250m\"\x1b[0m\n",
                "\t\x1b[38;5;250m漢\"\x1b[39m;\n",
                "}\n",
            ),
        ),
        (
            "code_short_6",
            concat!(
                "\x1b[2m─ zi ─\x1b[22m\n",
                "\x1b[38;5;252mfn\x1b[39m f()\n",
                " \x1b[38;5;252mvoid\x1b[39m \n",
                "{\n",
                "    x \n",
                "    = \x1b[38;5;250m\x1b[0m\n",
                "    \x1b[38;5;250m\"a\x1b[0m\n",
                "    \x1b[38;5;250mb\"\x1b[39m\n",
                "    ; \x1b[38;5;245m\x1b[0m\n",
                "    \x1b[38;5;245m//\x1b[0m\n",
                "    \x1b[38;5;245m n\x1b[0m\n",
                "    \x1b[38;5;245mot\x1b[0m\n",
                "    \x1b[38;5;245me\x1b[39m\n",
                "\ty = \x1b[38;5;250m\"\x1b[0m\n",
                "\t\x1b[38;5;250m漢\"\x1b[39m;\n",
                "}\n",
                "\x1b[2m──────\x1b[22m\n",
            ),
        ),
        (
            "code_short_7",
            concat!(
                "\x1b[2m─ zig ─\x1b[22m\n",
                "\x1b[38;5;252mfn\x1b[39m f() \x1b[38;5;252m\x1b[0m\n",
                "\x1b[38;5;252mvoid\x1b[39m {\n",
                "    x =\n",
                "     \x1b[38;5;250m\"a\x1b[0m\n",
                "    \x1b[38;5;250mb\"\x1b[39m;\n",
                "     \x1b[38;5;245m//\x1b[0m\n",
                "    \x1b[38;5;245m no\x1b[0m\n",
                "    \x1b[38;5;245mte\x1b[39m\n",
                "\ty = \x1b[38;5;250m\"漢\x1b[0m\n",
                "\t\x1b[38;5;250m\"\x1b[39m;\n",
                "}\n",
                "\x1b[2m───────\x1b[22m\n",
            ),
        ),
        (
            "code_short_8",
            concat!(
                "\x1b[2m─ zig ──\x1b[22m\n",
                "\x1b[38;5;252mfn\x1b[39m f() \x1b[38;5;252mv\x1b[0m\n",
                "\x1b[38;5;252moid\x1b[39m {\n",
                "    x = \x1b[38;5;250m\x1b[0m\n",
                "    \x1b[38;5;250m\"ab\"\x1b[39m\n",
                "    ; \x1b[38;5;245m//\x1b[0m\n",
                "    \x1b[38;5;245m not\x1b[0m\n",
                "    \x1b[38;5;245me\x1b[39m\n",
                "\ty = \x1b[38;5;250m\"漢\"\x1b[39m\n",
                "\t;\n",
                "}\n",
                "\x1b[2m────────\x1b[22m\n",
            ),
        ),
        (
            "code_diff_80",
            concat!(
                "\x1b[2m─ diff ───────────\x1b[22m\n",
                "\x1b[38;5;245mdiff --git a/x b/x\x1b[39m\n",
                "\x1b[38;5;245mindex 1..2 100644\x1b[39m\n",
                "\x1b[38;5;245m--- a/x\x1b[39m\n",
                "\x1b[38;5;245m+++ b/x\x1b[39m\n",
                "\x1b[38;5;252m@@ -1,2 +1,2 @@\x1b[39m\n",
                "-old line\x1b[39m\n",
                "+new line\x1b[39m\n",
                " context\n",
                "\x1b[2m──────────────────\x1b[22m\n",
            ),
        ),
        (
            "code_diff_5",
            concat!(
                "\x1b[38;5;245mdiff \x1b[0m\n",
                "\x1b[38;5;245m--git\x1b[0m\n",
                "\x1b[38;5;245m a/x \x1b[0m\n",
                "\x1b[38;5;245mb/x\x1b[39m\n",
                "\x1b[38;5;245mindex\x1b[0m\n",
                "\x1b[38;5;245m 1..2\x1b[0m\n",
                "\x1b[38;5;245m 1006\x1b[0m\n",
                "\x1b[38;5;245m44\x1b[39m\n",
                "\x1b[38;5;245m--- a\x1b[0m\n",
                "\x1b[38;5;245m/x\x1b[39m\n",
                "\x1b[38;5;245m+++ b\x1b[0m\n",
                "\x1b[38;5;245m/x\x1b[39m\n",
                "\x1b[38;5;252m@@ -1\x1b[0m\n",
                "\x1b[38;5;252m,2 +1\x1b[0m\n",
                "\x1b[38;5;252m,2 @@\x1b[39m\n",
                "-old \n",
                "line\x1b[39m\n",
                "+new \n",
                "line\x1b[39m\n",
                " cont\n",
                " ext\n",
            ),
        ),
        (
            "code_cjk_6",
            concat!(
                "\x1b[2m─ zi ─\x1b[22m\n",
                " \x1b[38;5;245m//abc\x1b[0m\n",
                " \x1b[38;5;245mdefgh\x1b[0m\n",
                " \x1b[38;5;245mijklm\x1b[0m\n",
                " \x1b[38;5;245mnop漢\x1b[39m\n",
                "\x1b[2m──────\x1b[22m\n",
            ),
        ),
        (
            "code_cjk_1",
            concat!(
                " \x1b[38;5;245m\x1b[0m\n",
                "\x1b[38;5;245m/\x1b[0m\n",
                "\x1b[38;5;245m/\x1b[0m\n",
                "\x1b[38;5;245ma\x1b[0m\n",
                "\x1b[38;5;245mb\x1b[0m\n",
                "\x1b[38;5;245mc\x1b[0m\n",
                "\x1b[38;5;245md\x1b[0m\n",
                "\x1b[38;5;245me\x1b[0m\n",
                "\x1b[38;5;245mf\x1b[0m\n",
                "\x1b[38;5;245mg\x1b[0m\n",
                "\x1b[38;5;245mh\x1b[0m\n",
                "\x1b[38;5;245mi\x1b[0m\n",
                "\x1b[38;5;245mj\x1b[0m\n",
                "\x1b[38;5;245mk\x1b[0m\n",
                "\x1b[38;5;245ml\x1b[0m\n",
                "\x1b[38;5;245mm\x1b[0m\n",
                "\x1b[38;5;245mn\x1b[0m\n",
                "\x1b[38;5;245mo\x1b[0m\n",
                "\x1b[38;5;245mp\x1b[0m\n",
                "\x1b[38;5;245m?\x1b[0m\n",
            ),
        ),
        (
            "code_cjk_7",
            concat!(
                "\x1b[2m─ zig ─\x1b[22m\n",
                " \x1b[38;5;245m//abcd\x1b[0m\n",
                " \x1b[38;5;245mefghij\x1b[0m\n",
                " \x1b[38;5;245mklmnop\x1b[0m\n",
                " \x1b[38;5;245m漢\x1b[39m\n",
                "\x1b[2m───────\x1b[22m\n",
            ),
        ),
        (
            "code_unlabeled_80",
            concat!(
                "\x1b[2m───────────────\x1b[22m\n",
                "plain words\n",
                "  indented line\n",
                "\x1b[2m───────────────\x1b[22m\n",
            ),
        ),
        (
            "code_unlabeled_3",
            concat!(
                "pla\n",
                "in \n",
                "wor\n",
                "ds\n",
                "  i\n",
                "  n\n",
                "  d\n",
                "  e\n",
                "  n\n",
                "  t\n",
                "  e\n",
                "  d\n",
                "   \n",
                "  l\n",
                "  i\n",
                "  n\n",
                "  e\n",
            ),
        ),
        (
            "code_empty_80",
            concat!(
                "\x1b[2m─ zig ─\x1b[22m\n",
                "\n",
                "\x1b[2m───────\x1b[22m\n",
            ),
        ),
        (
            "rows_code_short_80",
            concat!(
                "  \x1b[2m─ zig ───────────────\x1b[22m\n",
                "  \x1b[38;5;252mfn\x1b[39m f() \x1b[38;5;252mvoid\x1b[39m {\n",
                "      x = \x1b[38;5;250m\"ab\"\x1b[39m; \x1b[38;5;245m// note\x1b[39m\n",
                "\ty = \x1b[38;5;250m\"漢\"\x1b[39m;\n",
                "  }\n",
                "  \x1b[2m─────────────────────\x1b[22m\n",
            ),
        ),
        (
            "rows_code_short_10",
            concat!(
                "  \x1b[2m─ zig ──\x1b[22m\n",
                "  \x1b[38;5;252mfn\x1b[39m f() \x1b[38;5;252mv\x1b[0m\n",
                "  \x1b[38;5;252moid\x1b[39m {\n",
                "      x = \x1b[38;5;250m\x1b[0m\n",
                "      \x1b[38;5;250m\"ab\"\x1b[39m\n",
                "      ; \x1b[38;5;245m//\x1b[0m\n",
                "      \x1b[38;5;245m not\x1b[0m\n",
                "      \x1b[38;5;245me\x1b[39m\n",
                "\ty = \x1b[38;5;250m\"漢\"\x1b[39m\n",
                "\t;\n",
                "  }\n",
                "  \x1b[2m────────\x1b[22m\n",
            ),
        ),
        (
            "rows_code_short_3",
            concat!(
                "  \x1b[38;5;252mf\x1b[0m\n",
                "  \x1b[38;5;252mn\x1b[39m\n",
                "   \n",
                "  f\n",
                "  (\n",
                "  )\n",
                "   \x1b[38;5;252m\x1b[0m\n",
                "  \x1b[38;5;252mv\x1b[0m\n",
                "  \x1b[38;5;252mo\x1b[0m\n",
                "  \x1b[38;5;252mi\x1b[0m\n",
                "  \x1b[38;5;252md\x1b[39m\n",
                "   \n",
                "  {\n",
                "   \n",
                "   \n",
                "   \n",
                "   \n",
                "  x\n",
                "   \n",
                "  =\n",
                "   \x1b[38;5;250m\x1b[0m\n",
                "  \x1b[38;5;250m\"\x1b[0m\n",
                "  \x1b[38;5;250ma\x1b[0m\n",
                "  \x1b[38;5;250mb\x1b[0m\n",
                "  \x1b[38;5;250m\"\x1b[39m\n",
                "  ;\n",
                "   \x1b[38;5;245m\x1b[0m\n",
                "  \x1b[38;5;245m/\x1b[0m\n",
                "  \x1b[38;5;245m/\x1b[0m\n",
                "  \x1b[38;5;245m \x1b[0m\n",
                "  \x1b[38;5;245mn\x1b[0m\n",
                "  \x1b[38;5;245mo\x1b[0m\n",
                "  \x1b[38;5;245mt\x1b[0m\n",
                "  \x1b[38;5;245me\x1b[39m\n",
                "\ty\n",
                "\t \n",
                "\t=\n",
                "\t \x1b[38;5;250m\x1b[0m\n",
                "\t\x1b[38;5;250m\"\x1b[0m\n",
                "\t\x1b[38;5;250m?\x1b[0m\n",
                "\t\x1b[38;5;250m\"\x1b[39m\n",
                "\t;\n",
                "  }\n",
            ),
        ),
        (
            "rows_code_short_2",
            concat!(
                " \x1b[38;5;252mf\x1b[0m\n",
                " \x1b[38;5;252mn\x1b[39m\n",
                "  \n",
                " f\n",
                " (\n",
                " )\n",
                "  \x1b[38;5;252m\x1b[0m\n",
                " \x1b[38;5;252mv\x1b[0m\n",
                " \x1b[38;5;252mo\x1b[0m\n",
                " \x1b[38;5;252mi\x1b[0m\n",
                " \x1b[38;5;252md\x1b[39m\n",
                "  \n",
                " {\n",
                "  \n",
                "  \n",
                "  \n",
                "  \n",
                " x\n",
                "  \n",
                " =\n",
                "  \x1b[38;5;250m\x1b[0m\n",
                " \x1b[38;5;250m\"\x1b[0m\n",
                " \x1b[38;5;250ma\x1b[0m\n",
                " \x1b[38;5;250mb\x1b[0m\n",
                " \x1b[38;5;250m\"\x1b[39m\n",
                " ;\n",
                "  \x1b[38;5;245m\x1b[0m\n",
                " \x1b[38;5;245m/\x1b[0m\n",
                " \x1b[38;5;245m/\x1b[0m\n",
                " \x1b[38;5;245m \x1b[0m\n",
                " \x1b[38;5;245mn\x1b[0m\n",
                " \x1b[38;5;245mo\x1b[0m\n",
                " \x1b[38;5;245mt\x1b[0m\n",
                " \x1b[38;5;245me\x1b[39m\n",
                "\ty\n",
                "\t \n",
                "\t=\n",
                "\t \x1b[38;5;250m\x1b[0m\n",
                "\t\x1b[38;5;250m\"\x1b[0m\n",
                "\t\x1b[38;5;250m?\x1b[0m\n",
                "\t\x1b[38;5;250m\"\x1b[39m\n",
                "\t;\n",
                " }\n",
            ),
        ),
        (
            "rows_code_short_1",
            concat!(
                "\x1b[38;5;252mf\x1b[0m\n",
                "\x1b[38;5;252mn\x1b[39m\n",
                " \n",
                "f\n",
                "(\n",
                ")\n",
                " \x1b[38;5;252m\x1b[0m\n",
                "\x1b[38;5;252mv\x1b[0m\n",
                "\x1b[38;5;252mo\x1b[0m\n",
                "\x1b[38;5;252mi\x1b[0m\n",
                "\x1b[38;5;252md\x1b[39m\n",
                " \n",
                "{\n",
                " \n",
                " \n",
                " \n",
                " \n",
                "x\n",
                " \n",
                "=\n",
                " \x1b[38;5;250m\x1b[0m\n",
                "\x1b[38;5;250m\"\x1b[0m\n",
                "\x1b[38;5;250ma\x1b[0m\n",
                "\x1b[38;5;250mb\x1b[0m\n",
                "\x1b[38;5;250m\"\x1b[39m\n",
                ";\n",
                " \x1b[38;5;245m\x1b[0m\n",
                "\x1b[38;5;245m/\x1b[0m\n",
                "\x1b[38;5;245m/\x1b[0m\n",
                "\x1b[38;5;245m \x1b[0m\n",
                "\x1b[38;5;245mn\x1b[0m\n",
                "\x1b[38;5;245mo\x1b[0m\n",
                "\x1b[38;5;245mt\x1b[0m\n",
                "\x1b[38;5;245me\x1b[39m\n",
                "\ty\n",
                "\t \n",
                "\t=\n",
                "\t \x1b[38;5;250m\x1b[0m\n",
                "\t\x1b[38;5;250m\"\x1b[0m\n",
                "\t\x1b[38;5;250m?\x1b[0m\n",
                "\t\x1b[38;5;250m\"\x1b[39m\n",
                "\t;\n",
                "}\n",
            ),
        ),
        (
            "table_decision_80",
            concat!(
                "┌───┬─────────────────────────────────┬────────────────────────────────────────┐\n",
                "│ \x1b[1m#\x1b[22m │ \x1b[1mChoice\x1b[22m                          │ \x1b[1mDirect consequences\x1b[22m                    │\n",
                "├───┼─────────────────────────────────┼────────────────────────────────────────┤\n",
                "│ 1 │ \x1b[1mChange:\x1b[22m on reopen, an\x1b[0m           │ \x1b[38;5;245mrecovery.json\x1b[39m and \x1b[38;5;245mrecovery.asked\x1b[39m go\x1b[0m    │\n",
                "│   │ unfinished turn gets            │ away. The adapter handles a turn that  │\n",
                "│   │ \x1b[38;5;245mturn_interrupted{reason:\x1b[0m        │ ends in a tool call with no result     │\n",
                "│   │ \x1b[38;5;245mcrash}\x1b[39m; it's never cut\x1b[0m          │                                        │\n",
                "├───┼─────────────────────────────────┼────────────────────────────────────────┤\n",
                "│ 8 │ \x1b[1mKeep:\x1b[22m forks start only at turn\x1b[0m  │ Mid-turn forks deferred                │\n",
                "│   │ boundaries (or turn 0)          │                                        │\n",
                "└───┴─────────────────────────────────┴────────────────────────────────────────┘\n",
            ),
        ),
        (
            "table_decision_40",
            concat!(
                "┌───┬─────────────────┬────────────────┐\n",
                "│ \x1b[1m#\x1b[22m │ \x1b[1mChoice\x1b[22m          │ \x1b[1mDirect\x1b[22m         │\n",
                "│   │                 │ \x1b[1mconsequences\x1b[22m   │\n",
                "├───┼─────────────────┼────────────────┤\n",
                "│ 1 │ \x1b[1mChange:\x1b[22m on\x1b[0m      │ \x1b[38;5;245mrecovery.json\x1b[39m\x1b[0m  │\n",
                "│   │ reopen, an      │ and            │\n",
                "│   │ unfinished turn │ \x1b[38;5;245mrecovery.asked\x1b[39m\x1b[0m │\n",
                "│   │ gets            │ go away. The   │\n",
                "│   │ \x1b[38;5;245mturn_interrupte\x1b[0m │ adapter        │\n",
                "│   │ \x1b[38;5;245md{reason:\x1b[0m       │ handles a turn │\n",
                "│   │ \x1b[38;5;245mcrash}\x1b[39m; it's\x1b[0m    │ that ends in a │\n",
                "│   │ never cut       │ tool call with │\n",
                "│   │                 │ no result      │\n",
                "├───┼─────────────────┼────────────────┤\n",
                "│ 8 │ \x1b[1mKeep:\x1b[22m forks\x1b[0m     │ Mid-turn forks │\n",
                "│   │ start only at   │ deferred       │\n",
                "│   │ turn boundaries │                │\n",
                "│   │ (or turn 0)     │                │\n",
                "└───┴─────────────────┴────────────────┘\n",
            ),
        ),
        (
            "table_decision_16",
            concat!(
                "┌──────────────┐\n",
                "│#: 1          │\n",
                "│Choice: \x1b[1mChange│\n",
                "│:\x1b[22m on reopen, a│\n",
                "│n unfinished t│\n",
                "│urn gets \x1b[38;5;245mturn_│\n",
                "│interrupted{re│\n",
                "│ason: crash}\x1b[39m; │\n",
                "│it's never cut│\n",
                "│Direct consequ│\n",
                "│ences: \x1b[38;5;245mrecover│\n",
                "│y.json\x1b[39m and \x1b[38;5;245mrec│\n",
                "│overy.asked\x1b[39m go│\n",
                "│ away. The ada│\n",
                "│pter handles a│\n",
                "│ turn that end│\n",
                "│s in a tool ca│\n",
                "│ll with no res│\n",
                "│ult           │\n",
                "├──────────────┤\n",
                "│#: 8          │\n",
                "│Choice: \x1b[1mKeep:\x1b[22m │\n",
                "│forks start on│\n",
                "│ly at turn bou│\n",
                "│ndaries (or tu│\n",
                "│rn 0)         │\n",
                "│Direct consequ│\n",
                "│ences: Mid-tur│\n",
                "│n forks deferr│\n",
                "│ed            │\n",
                "└──────────────┘\n",
            ),
        ),
        (
            "table_decision_2",
            concat!(
                "#:\n",
                "Ch\n",
                "Di\n",
                "#:\n",
                "Ch\n",
                "Di\n",
            ),
        ),
        (
            "table_decision_1",
            concat!(
                "#\n",
                "C\n",
                "D\n",
                "#\n",
                "C\n",
                "D\n",
            ),
        ),
        (
            "table_link_30",
            concat!(
                "┌──────┬─────────────────────┐\n",
                "│ \x1b[1mName\x1b[22m │ \x1b[1mNotes\x1b[22m               │\n",
                "├──────┼─────────────────────┤\n",
                "│ api  │ see \x1b[38;5;245malpha beta\x1b[0m      │\n",
                "│      │ \x1b[38;5;245mgamma delta\x1b[39m and \x1b]8;id=fx-1;https://example.com/docs\x1b\\\x1b[38;5;75m\x1b[4mthe\x1b[0m\x1b]8;;\x1b\\ │\n",
                "│      │ \x1b[4m\x1b[38;5;75m\x1b]8;id=fx-1;https://example.com/docs\x1b\\project\x1b[0m\x1b]8;;\x1b\\             │\n",
                "│      │ \x1b[4m\x1b[38;5;75m\x1b]8;id=fx-1;https://example.com/docs\x1b\\documentation\x1b[24m\x1b[39m\x1b]8;;\x1b\\ or\x1b[0m    │\n",
                "│      │ \x1b[4m\x1b[38;5;75m\x1b]8;id=fx-2;https://example.com/a b\x1b\\spaced reference\x1b[24m\x1b[39m\x1b]8;;\x1b\\\x1b[0m    │\n",
                "│      │ now                 │\n",
                "└──────┴─────────────────────┘\n",
            ),
        ),
        (
            "table_align_80",
            concat!(
                "┌─────────────┬───────┬─────┐\n",
                "│ \x1b[1mName\x1b[22m        │ \x1b[1mCount\x1b[22m │ \x1b[1mMid\x1b[22m │\n",
                "├─────────────┼───────┼─────┤\n",
                "│ api         │     7 │  x  │\n",
                "├─────────────┼───────┼─────┤\n",
                "│ web service │ 12345 │ yy  │\n",
                "└─────────────┴───────┴─────┘\n",
            ),
        ),
        (
            "table_align_20",
            concat!(
                "┌──────┬─────┬─────┐\n",
                "│ \x1b[1mName\x1b[22m │ \x1b[1mCou\x1b[22m │ \x1b[1mMid\x1b[22m │\n",
                "│      │ \x1b[1mnt\x1b[22m  │     │\n",
                "├──────┼─────┼─────┤\n",
                "│ api  │   7 │  x  │\n",
                "├──────┼─────┼─────┤\n",
                "│ web  │ 123 │ yy  │\n",
                "│ serv │  45 │     │\n",
                "│ ice  │     │     │\n",
                "└──────┴─────┴─────┘\n",
            ),
        ),
        (
            "table_align_14",
            concat!(
                "┌────────────┐\n",
                "│Name: api   │\n",
                "│Count: 7    │\n",
                "│Mid: x      │\n",
                "├────────────┤\n",
                "│Name: web se│\n",
                "│rvice       │\n",
                "│Count: 12345│\n",
                "│Mid: yy     │\n",
                "└────────────┘\n",
            ),
        ),
        (
            "rows_table_align_80",
            concat!(
                "  ┌─────────────┬───────┬─────┐\n",
                "  │ \x1b[1mName\x1b[22m        │ \x1b[1mCount\x1b[22m │ \x1b[1mMid\x1b[22m │\n",
                "  ├─────────────┼───────┼─────┤\n",
                "  │ api         │     7 │  x  │\n",
                "  ├─────────────┼───────┼─────┤\n",
                "  │ web service │ 12345 │ yy  │\n",
                "  └─────────────┴───────┴─────┘\n",
            ),
        ),
        (
            "rows_table_align_3",
            concat!(
                "  N\n",
                "  C\n",
                "  M\n",
                "  N\n",
                "  C\n",
                "  M\n",
            ),
        ),
        (
            "rows_table_align_2",
            concat!(
                " N\n",
                " C\n",
                " M\n",
                " N\n",
                " C\n",
                " M\n",
            ),
        ),
        (
            "table_bold_80",
            concat!(
                "┌──────────────────────┐\n",
                "│ \x1b[1mprefix \x1b[1mstrong\x1b[22m\x1b[1m suffix\x1b[22m\x1b[0m │\n",
                "├──────────────────────┤\n",
                "│ value                │\n",
                "└──────────────────────┘\n",
            ),
        ),
        (
            "table_cjk_80",
            concat!(
                "┌──────┬────────────────────────┐\n",
                "│ \x1b[1m名前\x1b[22m │ \x1b[1m説明\x1b[22m                   │\n",
                "├──────┼────────────────────────┤\n",
                "│ 漢字 │ 長い説明のテキストです │\n",
                "└──────┴────────────────────────┘\n",
            ),
        ),
        (
            "table_cjk_14",
            concat!(
                "┌─────┬──────┐\n",
                "│ \x1b[1m名\x1b[22m  │ \x1b[1m説明\x1b[22m │\n",
                "│ \x1b[1m前\x1b[22m  │      │\n",
                "├─────┼──────┤\n",
                "│ 漢  │ 長い │\n",
                "│ 字  │ 説明 │\n",
                "│     │ のテ │\n",
                "│     │ キス │\n",
                "│     │ トで │\n",
                "│     │ す   │\n",
                "└─────┴──────┘\n",
            ),
        ),
        (
            "table_cjk_10",
            concat!(
                "┌────────┐\n",
                "│名前: 漢│\n",
                "│字      │\n",
                "│説明: 長│\n",
                "│い説明の│\n",
                "│テキスト│\n",
                "│です    │\n",
                "└────────┘\n",
            ),
        ),
        ("rule_0", ""),
        ("rule_1", "\x1b[2m─\x1b[22m"),
        ("rule_2", " \x1b[2m─\x1b[22m"),
        ("rule_3", "  \x1b[2m─\x1b[22m"),
        ("rule_80", "  \x1b[2m──────────────────────────────────────────────────────────────────────────────\x1b[22m"),
        (
            "prefix_0",
            concat!(
                "a\n",
                "\n",
                "\tb\n",
                "\x1b[2m\x1b[22m\n",
                " c\x1b[0m",
            ),
        ),
        (
            "prefix_1",
            concat!(
                " a\n",
                "\n",
                "\tb\n",
                "\x1b[2m\x1b[22m\n",
                "  c\x1b[0m",
            ),
        ),
        (
            "prefix_2",
            concat!(
                "  a\n",
                "\n",
                "\tb\n",
                "\x1b[2m\x1b[22m\n",
                "   c\x1b[0m",
            ),
        ),
        ("cells_link", "Name\u{1f}Notes\u{1e}api\u{1f}see \x1b[38;5;245malpha beta gamma delta\x1b[39m and \x1b]8;id=fx-1;https://example.com/docs\x1b\\\x1b[38;5;75m\x1b[4mthe project documentation\x1b[24m\x1b[39m\x1b]8;;\x1b\\ or \x1b]8;id=fx-2;https://example.com/a b\x1b\\\x1b[38;5;75m\x1b[4mspaced reference\x1b[24m\x1b[39m\x1b]8;;\x1b\\ now\u{1d}ll"),
        ("cells_decision", "#\u{1f}Choice\u{1f}Direct consequences\u{1e}1\u{1f}\x1b[1mChange:\x1b[22m on reopen, an unfinished turn gets \x1b[38;5;245mturn_interrupted{reason: crash}\x1b[39m; it's never cut\u{1f}\x1b[38;5;245mrecovery.json\x1b[39m and \x1b[38;5;245mrecovery.asked\x1b[39m go away. The adapter handles a turn that ends in a tool call with no result\u{1e}8\u{1f}\x1b[1mKeep:\x1b[22m forks start only at turn boundaries (or turn 0)\u{1f}Mid-turn forks deferred\u{1d}lll"),
        ("cells_align", "Name\u{1f}Count\u{1f}Mid\u{1e}api\u{1f}7\u{1f}x\u{1e}web service\u{1f}12345\u{1f}yy\u{1d}lrc"),
        ("cells_bold", "prefix \x1b[1mstrong\x1b[22m suffix\u{1e}value\u{1d}l"),
        ("cells_cjk", "名前\u{1f}説明\u{1e}漢字\u{1f}長い説明のテキストです\u{1d}ll"),
    ];
}
