//! Byte-level predicates shared by the block and inline parsers
//! (`text_util.zig`).

pub(super) fn left_trim(line: &[u8]) -> &[u8] {
    let mut i = 0;
    while i < line.len() && (line[i] == b' ' || line[i] == b'\t') {
        i += 1;
    }
    &line[i..]
}

/// Trims spaces and tabs from both ends (`std.mem.trim(u8, _, " \t")`).
pub(super) fn trim_spaces(text: &[u8]) -> &[u8] {
    trim_end_spaces(left_trim(text))
}

/// Trims spaces and tabs from the end (`std.mem.trimEnd(u8, _, " \t")`).
pub(super) fn trim_end_spaces(text: &[u8]) -> &[u8] {
    let mut end = text.len();
    while end > 0 && (text[end - 1] == b' ' || text[end - 1] == b'\t') {
        end -= 1;
    }
    &text[..end]
}

pub(super) fn is_space(c: u8) -> bool {
    c == b' ' || c == b'\t'
}

pub(super) fn is_blank_markdown_line(line: &[u8]) -> bool {
    trim_spaces(line).is_empty()
}

pub(super) fn is_ascii_alpha(c: u8) -> bool {
    c.is_ascii_alphabetic()
}

pub(super) fn is_ascii_alpha_numeric(c: u8) -> bool {
    c.is_ascii_alphanumeric()
}

pub(super) fn is_ascii_word_byte(c: u8) -> bool {
    is_ascii_alpha_numeric(c) || c == b'_'
}

/// Zig's ASCII whitespace, which unlike [`u8::is_ascii_whitespace`] includes
/// the vertical tab.
pub(super) fn is_ascii_whitespace(c: u8) -> bool {
    c == b' ' || c == b'\t' || c == b'\n' || c == b'\r' || c == 0x0b || c == 0x0c
}

pub(super) fn is_trailing_url_punctuation(c: u8) -> bool {
    matches!(c, b'.' | b',' | b';' | b':' | b'!' | b'?')
}

fn is_escapable_punctuation(byte: u8) -> bool {
    matches!(
        byte,
        b'!' | b'"'
            | b'#'
            | b'$'
            | b'%'
            | b'&'
            | b'\''
            | b'('
            | b')'
            | b'*'
            | b'+'
            | b','
            | b'-'
            | b'.'
            | b'/'
            | b':'
            | b';'
            | b'<'
            | b'='
            | b'>'
            | b'?'
            | b'@'
            | b'['
            | b'\\'
            | b']'
            | b'^'
            | b'_'
            | b'`'
            | b'{'
            | b'|'
            | b'}'
            | b'~'
    )
}

pub(super) fn is_escaped_punctuation_at(text: &[u8], index: usize) -> bool {
    if index == 0 || index >= text.len() || !is_escapable_punctuation(text[index]) {
        return false;
    }
    let mut slash_count = 0usize;
    let mut cursor = index;
    while cursor > 0 && text[cursor - 1] == b'\\' {
        slash_count += 1;
        cursor -= 1;
    }
    slash_count % 2 == 1
}

pub(super) fn append_escaped_punctuation(out: &mut Vec<u8>, text: &[u8]) {
    let mut i = 0;
    while i < text.len() {
        if text[i] == b'\\' && i + 1 < text.len() && is_escaped_punctuation_at(text, i + 1) {
            out.push(text[i + 1]);
            i += 2;
            continue;
        }
        out.push(text[i]);
        i += 1;
    }
}

pub(super) fn without_terminal_hard_break_marker(line: &[u8], line_has_lf: bool) -> &[u8] {
    if !line_has_lf || line.last() != Some(&b'\\') {
        return line;
    }
    let mut slash_start = line.len();
    while slash_start > 0 && line[slash_start - 1] == b'\\' {
        slash_start -= 1;
    }
    if (line.len() - slash_start).is_multiple_of(2) {
        return line;
    }
    &line[..line.len() - 1]
}

pub(super) fn nth_line(buf: &[u8], n: usize) -> Option<&[u8]> {
    let mut idx = 0;
    let mut start = 0;
    while start < buf.len() {
        let end = index_of_scalar_pos(buf, start, b'\n').unwrap_or(buf.len());
        if idx == n {
            return Some(&buf[start..end]);
        }
        idx += 1;
        start = end + 1;
    }
    None
}

/// `std.mem.indexOfScalarPos`.
pub(super) fn index_of_scalar_pos(haystack: &[u8], start: usize, needle: u8) -> Option<usize> {
    if start >= haystack.len() {
        return None;
    }
    haystack[start..]
        .iter()
        .position(|&b| b == needle)
        .map(|offset| start + offset)
}

/// `std.mem.indexOfPos`.
pub(super) fn index_of_pos(haystack: &[u8], start: usize, needle: &[u8]) -> Option<usize> {
    if start > haystack.len() {
        return None;
    }
    if needle.is_empty() {
        return Some(start);
    }
    haystack[start..]
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|offset| start + offset)
}

/// `std.mem.indexOf`.
pub(super) fn index_of(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    index_of_pos(haystack, 0, needle)
}

/// Rendered bytes as a `String`.
///
/// The presenter only ever cuts its `&str` input at ASCII bytes (line feeds,
/// markers, delimiters, spaces) and only inserts ASCII escapes and whole
/// UTF-8 glyphs, so its output is valid UTF-8 whenever its input is. The
/// lossy fallback exists so that claim failing would show as U+FFFD rather
/// than a panic in the paint path.
pub(super) fn into_string(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes)
        .unwrap_or_else(|error| String::from_utf8_lossy(error.as_bytes()).into_owned())
}
