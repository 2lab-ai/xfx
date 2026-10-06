//! Line-level block recognizers (`block_parse.zig`). Every function reads one
//! line and returns slices of it; none allocates.

use super::text_util as tu;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct CodeFence {
    pub(super) marker: u8,
    /// Number of marker characters in the run; a closing fence needs at
    /// least this many.
    pub(super) run: usize,
    /// Leading spaces before the fence, stripped from the block's code lines.
    pub(super) indent: usize,
}

/// An opening or closing fence of three or more backticks or tildes, after
/// any leading spaces so fences inside list items are recognized.
pub(super) fn parse_code_fence(line: &[u8]) -> Option<CodeFence> {
    let mut i = 0;
    while i < line.len() && (line[i] == b' ' || line[i] == b'\t') {
        i += 1;
    }
    let indent = i;
    if i >= line.len() {
        return None;
    }
    let marker = line[i];
    if marker != b'`' && marker != b'~' {
        return None;
    }
    let mut run = 0;
    while i < line.len() && line[i] == marker {
        i += 1;
        run += 1;
    }
    if run < 3 {
        return None;
    }
    Some(CodeFence {
        marker,
        run,
        indent,
    })
}

pub(super) fn code_fence_marker(line: &[u8]) -> Option<u8> {
    parse_code_fence(line).map(|fence| fence.marker)
}

/// True when `line` closes a block opened by `open`: same marker, a run at
/// least as long, and nothing but whitespace after it.
pub(super) fn closes_code_fence(line: &[u8], open: CodeFence) -> bool {
    let Some(fence) = parse_code_fence(line) else {
        return false;
    };
    if fence.marker != open.marker || fence.run < open.run {
        return false;
    }
    tu::is_blank_markdown_line(&line[fence.indent + fence.run..])
}

fn is_code_fence(line: &[u8]) -> bool {
    code_fence_marker(line).is_some()
}

pub(super) fn code_fence_language(line: &[u8]) -> &[u8] {
    let Some(fence) = parse_code_fence(line) else {
        return &line[..0];
    };
    let info = tu::trim_spaces(&line[fence.indent + fence.run..]);
    let end = info
        .iter()
        .position(|&b| b == b' ' || b == b'\t')
        .unwrap_or(info.len());
    &info[..end]
}

/// Strips up to `indent` leading spaces or tabs so code inside an indented
/// list item renders flush with the fence.
pub(super) fn strip_fence_indent(line: &[u8], indent: usize) -> &[u8] {
    let mut i = 0;
    while i < line.len() && i < indent && (line[i] == b' ' || line[i] == b'\t') {
        i += 1;
    }
    &line[i..]
}

pub(super) fn has_indented_code_prefix(line: &[u8]) -> bool {
    !line.is_empty() && (line[0] == b'\t' || line.starts_with(b"    "))
}

/// Callers check [`has_indented_code_prefix`] first.
pub(super) fn deindent_code_line(line: &[u8]) -> &[u8] {
    if line[0] == b'\t' {
        return &line[1..];
    }
    &line[4..]
}

pub(super) struct ParsedHeader<'a> {
    pub(super) level: usize,
    pub(super) content: &'a [u8],
}

/// ATX heading: up to three leading spaces, one to six `#`, a space, then
/// content with any closing `#` run removed.
pub(super) fn parse_header(line: &[u8]) -> Option<ParsedHeader<'_>> {
    let mut start = 0;
    while start < 3 && start < line.len() && line[start] == b' ' {
        start += 1;
    }
    let mut level = 0;
    while level < 6 && start + level < line.len() && line[start + level] == b'#' {
        level += 1;
    }
    if level == 0 {
        return None;
    }
    let marker_end = start + level;
    if marker_end >= line.len() || line[marker_end] != b' ' {
        return None;
    }
    Some(ParsedHeader {
        level,
        content: without_closing_hashes(&line[marker_end + 1..]),
    })
}

fn without_closing_hashes(content: &[u8]) -> &[u8] {
    let trimmed = tu::trim_end_spaces(content);
    let mut end = trimmed.len();
    while end > 0 && trimmed[end - 1] == b'#' {
        end -= 1;
    }
    if end == trimmed.len() {
        return content;
    }
    if end == 0 {
        return &trimmed[..0];
    }
    if trimmed[end - 1] != b' ' && trimmed[end - 1] != b'\t' {
        return content;
    }
    tu::trim_end_spaces(&trimmed[..end])
}

pub(super) fn parse_setext_underline(line: &[u8]) -> Option<usize> {
    let mut level = None;
    for &byte in line {
        if byte == b' ' || byte == b'\t' {
            continue;
        }
        let next_level = match byte {
            b'=' => 1,
            b'-' => 2,
            _ => return None,
        };
        match level {
            Some(existing) if existing != next_level => return None,
            Some(_) => {}
            None => level = Some(next_level),
        }
    }
    level
}

pub(super) fn is_setext_candidate(line: &[u8]) -> bool {
    if line.is_empty() || line[0] == b' ' || line[0] == b'\t' || line[0] == b':' {
        return false;
    }
    if parse_header(line).is_some() || parse_blockquote(line).is_some() {
        return false;
    }
    if parse_unordered_list(line).is_some() || parse_ordered_list(line).is_some() {
        return false;
    }
    !is_code_fence(line)
        && !is_pipe_line(line)
        && !is_horizontal_rule(line)
        && parse_setext_underline(line).is_none()
}

pub(super) fn definition_marker_body(line: &[u8]) -> Option<&[u8]> {
    if line.len() < 2 || line[0] != b':' {
        return None;
    }
    let mut body_start = 1;
    while body_start < line.len() && (line[body_start] == b' ' || line[body_start] == b'\t') {
        body_start += 1;
    }
    if body_start == 1 || body_start == line.len() {
        return None;
    }
    Some(&line[body_start..])
}

pub(super) struct ParsedFootnoteDefinition<'a> {
    pub(super) label: &'a [u8],
    pub(super) body: &'a [u8],
}

pub(super) fn parse_footnote_definition(line: &[u8]) -> Option<ParsedFootnoteDefinition<'_>> {
    if line.len() < 6 || line[0] != b'[' || line[1] != b'^' {
        return None;
    }
    let close = tu::index_of_scalar_pos(line, 2, b']')?;
    if close == 2 || close + 1 >= line.len() || line[close + 1] != b':' {
        return None;
    }

    let mut body_start = close + 2;
    while body_start < line.len() && (line[body_start] == b' ' || line[body_start] == b'\t') {
        body_start += 1;
    }
    if body_start == line.len() {
        return None;
    }
    Some(ParsedFootnoteDefinition {
        label: &line[2..close],
        body: &line[body_start..],
    })
}

pub(super) fn footnote_continuation_body(line: &[u8]) -> Option<&[u8]> {
    if line.first() == Some(&b'\t') {
        return Some(&line[1..]);
    }
    if line.starts_with(b"  ") {
        return Some(&line[2..]);
    }
    None
}

pub(super) struct ParsedBlockquote<'a> {
    pub(super) indent: &'a [u8],
    pub(super) depth: usize,
    pub(super) content: &'a [u8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct BlockquotePrefix {
    pub(super) indent: usize,
    pub(super) depth: usize,
}

pub(super) fn parse_blockquote(line: &[u8]) -> Option<ParsedBlockquote<'_>> {
    let mut i = 0;
    while i < line.len() && line[i] == b' ' {
        i += 1;
    }
    let indent_end = i;
    if i == line.len() || line[i] != b'>' {
        return None;
    }
    i += 1;
    if i == line.len() {
        return Some(ParsedBlockquote {
            indent: &line[..indent_end],
            depth: 1,
            content: &line[i..],
        });
    }
    if line[i] != b' ' {
        return None;
    }
    i += 1;

    let mut depth = 1;
    while i < line.len() && line[i] == b'>' {
        let after_marker = i + 1;
        if after_marker < line.len() && line[after_marker] != b' ' {
            break;
        }
        depth += 1;
        i = after_marker;
        if i == line.len() {
            break;
        }
        i += 1;
    }
    Some(ParsedBlockquote {
        indent: &line[..indent_end],
        depth,
        content: &line[i..],
    })
}

pub(super) fn is_blockquote_paragraph(line: &[u8]) -> bool {
    is_lazy_blockquote_continuation(line)
}

pub(super) fn is_lazy_blockquote_continuation(line: &[u8]) -> bool {
    if tu::trim_spaces(line).is_empty() {
        return false;
    }
    if parse_blockquote(line).is_some() || parse_header(line).is_some() {
        return false;
    }
    if parse_unordered_list(line).is_some() || parse_ordered_list(line).is_some() {
        return false;
    }
    code_fence_marker(tu::left_trim(line)).is_none()
        && !is_pipe_line(line)
        && !is_horizontal_rule(line)
}

pub(super) struct ParsedUnorderedList<'a> {
    pub(super) indent: &'a [u8],
    pub(super) content: &'a [u8],
}

/// Accepts Markdown `-`, `*`, and `+` markers plus a literal bullet so model
/// output that already uses `•` gets the same styling and wrap continuation.
pub(super) fn parse_unordered_list(line: &[u8]) -> Option<ParsedUnorderedList<'_>> {
    const LITERAL_BULLET: &[u8] = "\u{2022}".as_bytes();
    let mut i = 0;
    while i < line.len() && (line[i] == b' ' || line[i] == b'\t') {
        i += 1;
    }
    let rest = &line[i..];
    let marker_len = if !rest.is_empty() && (rest[0] == b'-' || rest[0] == b'*' || rest[0] == b'+')
    {
        1
    } else if rest.starts_with(LITERAL_BULLET) {
        LITERAL_BULLET.len()
    } else {
        return None;
    };
    if marker_len >= rest.len() || !tu::is_space(rest[marker_len]) {
        return None;
    }
    Some(ParsedUnorderedList {
        indent: &line[..i],
        content: &rest[marker_len + 1..],
    })
}

pub(super) struct ParsedOrderedList<'a> {
    pub(super) indent: &'a [u8],
    pub(super) marker: &'a [u8],
    pub(super) content: &'a [u8],
}

pub(super) fn parse_ordered_list(line: &[u8]) -> Option<ParsedOrderedList<'_>> {
    let mut i = 0;
    while i < line.len() && (line[i] == b' ' || line[i] == b'\t') {
        i += 1;
    }
    let indent_end = i;
    while i < line.len() && line[i].is_ascii_digit() {
        i += 1;
    }
    if i == indent_end || i - indent_end > 9 {
        return None;
    }
    if i + 1 >= line.len() {
        return None;
    }
    if line[i] != b'.' && line[i] != b')' {
        return None;
    }
    if !tu::is_space(line[i + 1]) {
        return None;
    }
    Some(ParsedOrderedList {
        indent: &line[..indent_end],
        marker: &line[indent_end..i + 1],
        content: &line[i + 2..],
    })
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ParsedTaskListItem<'a> {
    pub(super) completed: bool,
    pub(super) has_separator: bool,
    pub(super) content: &'a [u8],
}

pub(super) fn parse_task_list_item(content: &[u8]) -> Option<ParsedTaskListItem<'_>> {
    if content.len() < 3 || content[0] != b'[' || content[2] != b']' {
        return None;
    }
    let completed = match content[1] {
        b' ' => false,
        b'x' | b'X' => true,
        _ => return None,
    };
    if content.len() == 3 {
        return Some(ParsedTaskListItem {
            completed,
            has_separator: false,
            content: &content[3..],
        });
    }
    if content[3] != b' ' {
        return None;
    }
    Some(ParsedTaskListItem {
        completed,
        has_separator: true,
        content: &content[4..],
    })
}

pub(super) fn is_horizontal_rule(line: &[u8]) -> bool {
    let trimmed = tu::left_trim(line);
    if trimmed.len() < 3 {
        return false;
    }
    let rule_char = trimmed[0];
    if rule_char != b'-' && rule_char != b'*' && rule_char != b'_' {
        return false;
    }
    let mut count = 0;
    for &c in trimmed {
        if c == rule_char {
            count += 1;
        } else if c != b' ' && c != b'\t' {
            return false;
        }
    }
    count >= 3
}

pub(super) fn is_pipe_line(line: &[u8]) -> bool {
    let trimmed = tu::left_trim(line);
    if trimmed.is_empty() {
        return false;
    }
    trimmed
        .iter()
        .enumerate()
        .any(|(index, &byte)| byte == b'|' && !tu::is_escaped_punctuation_at(trimmed, index))
}

fn is_separator_line(line: &[u8]) -> bool {
    let trimmed = tu::left_trim(line);
    if trimmed.is_empty() {
        return false;
    }
    let mut seen_dash = false;
    let mut seen_pipe = false;
    for &c in trimmed {
        match c {
            b'|' => seen_pipe = true,
            b':' | b' ' | b'\t' => {}
            b'-' => seen_dash = true,
            _ => return false,
        }
    }
    seen_dash && seen_pipe
}

pub(super) fn is_valid_table(buf: &[u8]) -> bool {
    let mut line_count = 0;
    let mut saw_separator = false;
    let mut start = 0;
    while start < buf.len() {
        let end = tu::index_of_scalar_pos(buf, start, b'\n').unwrap_or(buf.len());
        let line = &buf[start..end];
        if line_count == 1 && is_separator_line(line) {
            saw_separator = true;
        }
        line_count += 1;
        start = end + 1;
    }
    line_count >= 2 && saw_separator
}
