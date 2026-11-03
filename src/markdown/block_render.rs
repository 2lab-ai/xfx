//! Block-level writers: headings, quotes, lists, tables, footnotes, and code
//! block payloads (`block_render.zig`).

use super::ansi::{self, Styles};
use super::block_parse as bp;
use super::display_width;
use super::inline_render::{self, RenderContext};
use super::payload::{CodeBlockPayload, TableColumnAlign, TablePayload, TableRow};
use super::text_util as tu;

pub(super) fn write_heading(
    cx: &mut RenderContext<'_>,
    level: usize,
    content: &[u8],
    out: &mut Vec<u8>,
) {
    // `parse_header` and the Setext underline only produce levels 1 to 6.
    let (open, close): (&[&str], &[&str]) = match level {
        1 => (
            &[ansi::BOLD_OPEN, ansi::UNDERLINE_OPEN],
            &[ansi::UNDERLINE_CLOSE, ansi::BOLD_CLOSE],
        ),
        2 => (&[ansi::BOLD_OPEN], &[ansi::BOLD_CLOSE]),
        3 => (&[ansi::UNDERLINE_OPEN], &[ansi::UNDERLINE_CLOSE]),
        4 => (&[ansi::BOLD_OPEN, ansi::DIM_OPEN], &[ansi::BOLD_CLOSE]),
        5 => (
            &[ansi::DIM_OPEN, ansi::UNDERLINE_OPEN],
            &[ansi::UNDERLINE_CLOSE, ansi::DIM_CLOSE],
        ),
        _ => (&[ansi::DIM_OPEN], &[ansi::BOLD_CLOSE]),
    };
    for sequence in open {
        out.extend_from_slice(sequence.as_bytes());
    }
    inline_render::write_inline_no_bold(cx, content, out, level == 1 || level == 3 || level == 5);
    for sequence in close {
        out.extend_from_slice(sequence.as_bytes());
    }
}

pub(super) fn write_blockquote_line(
    cx: &mut RenderContext<'_>,
    blockquote: bp::BlockquotePrefix,
    content: &[u8],
    line_has_lf: bool,
    out: &mut Vec<u8>,
) {
    out.resize(out.len() + blockquote.indent, b' ');
    for _ in 0..blockquote.depth {
        ansi::write_dim(out, ansi::VERTICAL_RULE_PREFIX.as_bytes());
    }
    write_quoted_block_content(cx, content, line_has_lf, out);
}

/// Renders headings and list items inside a blockquote with their usual
/// styling; everything else is inline prose.
fn write_quoted_block_content(
    cx: &mut RenderContext<'_>,
    content: &[u8],
    line_has_lf: bool,
    out: &mut Vec<u8>,
) {
    if let Some(header) =
        bp::parse_header(tu::without_terminal_hard_break_marker(content, line_has_lf))
    {
        write_heading(cx, header.level, header.content, out);
        return;
    }
    if write_list_line(cx, content, line_has_lf, out) {
        return;
    }
    inline_render::write_inline(
        cx,
        tu::without_terminal_hard_break_marker(content, line_has_lf),
        out,
        false,
    );
}

/// Renders an unordered, ordered, or task list line without a trailing
/// newline. False when `line` is not a list item.
pub(super) fn write_list_line(
    cx: &mut RenderContext<'_>,
    line: &[u8],
    line_has_lf: bool,
    out: &mut Vec<u8>,
) -> bool {
    if let Some(parsed) = bp::parse_unordered_list(line) {
        out.extend_from_slice(parsed.indent);
        if let Some(task) = bp::parse_task_list_item(parsed.content) {
            write_task_list_marker(cx.styles, out, task);
            inline_render::write_inline(
                cx,
                tu::without_terminal_hard_break_marker(task.content, line_has_lf),
                out,
                false,
            );
            return true;
        }
        ansi::write_dim(out, ansi::BULLET_MARKER.as_bytes());
        inline_render::write_inline(
            cx,
            tu::without_terminal_hard_break_marker(parsed.content, line_has_lf),
            out,
            false,
        );
        return true;
    }
    if let Some(parsed) = bp::parse_ordered_list(line) {
        out.extend_from_slice(parsed.indent);
        ansi::write_dim(out, parsed.marker);
        out.push(b' ');
        if let Some(task) = bp::parse_task_list_item(parsed.content) {
            write_task_list_marker(cx.styles, out, task);
            inline_render::write_inline(
                cx,
                tu::without_terminal_hard_break_marker(task.content, line_has_lf),
                out,
                false,
            );
            return true;
        }
        inline_render::write_inline(
            cx,
            tu::without_terminal_hard_break_marker(parsed.content, line_has_lf),
            out,
            false,
        );
        return true;
    }
    false
}

pub(super) fn write_definition_line(
    cx: &mut RenderContext<'_>,
    body: &[u8],
    line_has_lf: bool,
    out: &mut Vec<u8>,
) {
    ansi::write_dim(out, b"  ");
    inline_render::write_inline(
        cx,
        tu::without_terminal_hard_break_marker(body, line_has_lf),
        out,
        false,
    );
    out.push(b'\n');
}

fn write_task_list_marker(styles: &Styles, out: &mut Vec<u8>, task: bp::ParsedTaskListItem<'_>) {
    if task.completed {
        out.extend_from_slice(styles.task_completed_open().as_bytes());
        out.extend_from_slice(ansi::TASK_COMPLETED_MARKER.as_bytes());
        out.extend_from_slice(styles.task_completed_close().as_bytes());
        if task.has_separator {
            out.push(b' ');
        }
        return;
    }

    out.extend_from_slice(ansi::DIM_OPEN.as_bytes());
    out.extend_from_slice(ansi::TASK_PENDING_MARKER.as_bytes());
    if task.has_separator {
        out.push(b' ');
    }
    out.extend_from_slice(ansi::DIM_CLOSE.as_bytes());
}

/// A rendered cell; its width excludes ANSI escape bytes so table dividers
/// stay aligned.
struct RenderedCell {
    bytes: Vec<u8>,
    width: usize,
}

pub(super) fn render_table(cx: &mut RenderContext<'_>, buf: &[u8], out: &mut Vec<u8>) {
    let table = parse_and_render_rows(cx, buf);

    let col_count = table.iter().map(Vec::len).max().unwrap_or(0);
    if table.is_empty() || col_count == 0 {
        return;
    }

    let mut widths = vec![0usize; col_count];
    for row in &table {
        for (col, cell) in row.iter().enumerate() {
            widths[col] = widths[col].max(cell.width);
        }
    }

    let mut alignments = vec![TableColumnAlign::Left; col_count];
    parse_column_alignments(buf, &mut alignments);

    for (row_idx, row) in table.iter().enumerate() {
        let is_header = row_idx == 0;
        write_table_row(row, &widths, &alignments, is_header, out);
        if is_header {
            write_table_separator(&widths, out);
        }
    }
}

fn parse_and_render_rows(cx: &mut RenderContext<'_>, buf: &[u8]) -> Vec<Vec<RenderedCell>> {
    let mut table = Vec::new();
    for (line_idx, line) in table_lines(buf).enumerate() {
        if line_idx == 1 {
            continue;
        }
        let row = split_cells(line)
            .into_iter()
            .map(|raw_cell| {
                let mut bytes = Vec::new();
                inline_render::write_inline(cx, raw_cell, &mut bytes, false);
                let width = display_width::visible_width_ignoring_ansi(&bytes);
                RenderedCell { bytes, width }
            })
            .collect();
        table.push(row);
    }
    table
}

/// The `\n`-separated lines of a pipe buffer, without the empty piece after a
/// trailing newline.
fn table_lines(buf: &[u8]) -> impl Iterator<Item = &[u8]> {
    let mut start = 0;
    std::iter::from_fn(move || {
        if start >= buf.len() {
            return None;
        }
        let end = tu::index_of_scalar_pos(buf, start, b'\n').unwrap_or(buf.len());
        let line = &buf[start..end];
        start = end + 1;
        Some(line)
    })
}

fn split_cells(line: &[u8]) -> Vec<&[u8]> {
    let mut rest = tu::left_trim(line);
    if rest.first() == Some(&b'|') {
        rest = &rest[1..];
    }
    rest = tu::trim_end_spaces(rest);
    if rest.last() == Some(&b'|') && !tu::is_escaped_punctuation_at(rest, rest.len() - 1) {
        rest = &rest[..rest.len() - 1];
    }

    let mut cells = Vec::new();
    let mut cell_start = 0;
    for i in 0..=rest.len() {
        if i == rest.len() || (rest[i] == b'|' && !tu::is_escaped_punctuation_at(rest, i)) {
            cells.push(tu::trim_spaces(&rest[cell_start..i]));
            cell_start = i + 1;
        }
    }
    cells
}

/// ANSI escape bytes do not contribute to table column width.
fn write_table_row(
    cells: &[RenderedCell],
    widths: &[usize],
    alignments: &[TableColumnAlign],
    is_header: bool,
    out: &mut Vec<u8>,
) {
    for (col, &col_width) in widths.iter().enumerate() {
        if col > 0 {
            out.extend_from_slice(ansi::TABLE_COLUMN_SEP.as_bytes());
        }
        let col_align = if is_header {
            TableColumnAlign::Left
        } else {
            alignments[col]
        };
        match cells.get(col) {
            Some(cell) => write_padded_cell(
                &cell.bytes,
                cell.width,
                col_width,
                col_align,
                is_header,
                out,
            ),
            None => out.resize(out.len() + col_width, b' '),
        }
    }
    out.push(b'\n');
}

fn write_padded_cell(
    cell: &[u8],
    cell_width: usize,
    col_width: usize,
    col_align: TableColumnAlign,
    is_header: bool,
    out: &mut Vec<u8>,
) {
    let pad = col_width.saturating_sub(cell_width);
    let left_pad = match col_align {
        TableColumnAlign::Left => 0,
        TableColumnAlign::Right => pad,
        TableColumnAlign::Center => pad / 2,
    };
    out.resize(out.len() + left_pad, b' ');
    if is_header {
        write_table_header_cell(out, cell);
    } else {
        out.extend_from_slice(cell);
    }
    out.resize(out.len() + (pad - left_pad), b' ');
}

/// Bolds a header cell, re-opening bold after every bold close inside it so
/// an inline strong span does not end the header's bold early.
pub fn write_table_header_cell(out: &mut Vec<u8>, cell: &[u8]) {
    let close = ansi::BOLD_CLOSE.as_bytes();
    out.extend_from_slice(ansi::BOLD_OPEN.as_bytes());
    let mut remaining = cell;
    while let Some(close_index) = tu::index_of(remaining, close) {
        out.extend_from_slice(&remaining[..close_index]);
        out.extend_from_slice(close);
        out.extend_from_slice(ansi::BOLD_OPEN.as_bytes());
        remaining = &remaining[close_index + close.len()..];
    }
    out.extend_from_slice(remaining);
    out.extend_from_slice(close);
}

fn write_table_separator(widths: &[usize], out: &mut Vec<u8>) {
    for (col, &col_width) in widths.iter().enumerate() {
        if col > 0 {
            out.extend_from_slice(ansi::TABLE_JUNCTION.as_bytes());
        }
        for _ in 0..col_width {
            out.extend_from_slice(ansi::TABLE_HORIZ.as_bytes());
        }
    }
    out.push(b'\n');
}

fn parse_column_alignments(buf: &[u8], alignments: &mut [TableColumnAlign]) {
    alignments.fill(TableColumnAlign::Left);
    let Some(sep_line) = tu::nth_line(buf, 1) else {
        return;
    };

    for (col, cell) in split_cells(sep_line).into_iter().enumerate() {
        if col >= alignments.len() {
            break;
        }
        let trimmed = tu::trim_spaces(cell);
        if trimmed.is_empty() {
            continue;
        }
        let starts = trimmed[0] == b':';
        let ends = trimmed[trimmed.len() - 1] == b':';
        alignments[col] = if starts && ends {
            TableColumnAlign::Center
        } else if ends {
            TableColumnAlign::Right
        } else {
            TableColumnAlign::Left
        };
    }
}

/// Renders a table payload the way the presenter renders a table inline: a
/// bold header row, a rule with junctions under every column separator, and
/// cells padded to their column's widest cell.
pub fn render_table_payload(table: &TablePayload, out: &mut Vec<u8>) {
    if table.rows.is_empty() || table.column_count == 0 {
        return;
    }

    let mut widths = vec![0usize; table.column_count];
    for row in &table.rows {
        for (col, cell) in row.cells.iter().enumerate() {
            widths[col] =
                widths[col].max(display_width::visible_width_ignoring_ansi(cell.as_bytes()));
        }
    }

    for (row_idx, row) in table.rows.iter().enumerate() {
        let is_header = row_idx == 0;
        write_table_payload_row(&row.cells, &widths, &table.alignments, is_header, out);
        if is_header {
            write_table_separator(&widths, out);
        }
    }
}

fn write_table_payload_row(
    cells: &[String],
    widths: &[usize],
    alignments: &[TableColumnAlign],
    is_header: bool,
    out: &mut Vec<u8>,
) {
    for (col, &col_width) in widths.iter().enumerate() {
        if col > 0 {
            out.extend_from_slice(ansi::TABLE_COLUMN_SEP.as_bytes());
        }
        let col_align = if is_header {
            TableColumnAlign::Left
        } else {
            alignments[col]
        };
        match cells.get(col) {
            Some(cell) => {
                let cell = cell.as_bytes();
                let width = display_width::visible_width_ignoring_ansi(cell);
                write_padded_cell(cell, width, col_width, col_align, is_header, out);
            }
            None => out.resize(out.len() + col_width, b' '),
        }
    }
    out.push(b'\n');
}

/// Parses a pipe buffer into a payload whose cells are already rendered.
pub(super) fn parse_table_payload(cx: &mut RenderContext<'_>, buf: &[u8]) -> TablePayload {
    let mut rows = Vec::new();
    for (line_idx, line) in table_lines(buf).enumerate() {
        if line_idx == 1 {
            continue;
        }
        let cells = split_cells(line)
            .into_iter()
            .map(|raw_cell| {
                let mut rendered = Vec::new();
                inline_render::write_inline(cx, raw_cell, &mut rendered, false);
                tu::into_string(rendered)
            })
            .collect();
        rows.push(TableRow { cells });
    }

    let column_count = rows.iter().map(|row| row.cells.len()).max().unwrap_or(0);
    let mut alignments = vec![TableColumnAlign::Left; column_count];
    parse_column_alignments(buf, &mut alignments);
    TablePayload {
        rows,
        alignments,
        column_count,
    }
}

pub(super) fn write_footnote_definition_marker(out: &mut Vec<u8>, number: usize) {
    ansi::write_dim(out, format!("[{number}] ").as_bytes());
}

pub(super) fn write_footnote_body(
    cx: &mut RenderContext<'_>,
    body: &[u8],
    out: &mut Vec<u8>,
    number: usize,
) {
    let continuation = " ".repeat(format!("[{number}] ").len());
    let mut start = 0;
    let mut is_first = true;
    while start <= body.len() {
        let end = tu::index_of_scalar_pos(body, start, b'\n').unwrap_or(body.len());
        if !is_first {
            out.push(b'\n');
            ansi::write_dim(out, continuation.as_bytes());
        }
        inline_render::write_inline(cx, &body[start..end], out, false);
        if end == body.len() {
            break;
        }
        is_first = false;
        start = end + 1;
    }
    out.push(b'\n');
}

/// Renders a code block payload the way the presenter renders code inline:
/// each line behind a dim `│ ` rule, with no language label and no
/// highlighting.
pub fn render_code_block_payload(block: &CodeBlockPayload, out: &mut Vec<u8>) {
    let code = block.code.as_bytes();
    let mut start = 0;
    while start < code.len() {
        let end = tu::index_of_scalar_pos(code, start, b'\n').unwrap_or(code.len());
        ansi::write_dim(out, ansi::VERTICAL_RULE_PREFIX.as_bytes());
        out.extend_from_slice(&code[start..end]);
        out.push(b'\n');
        if end == code.len() {
            break;
        }
        start = end + 1;
    }
}
