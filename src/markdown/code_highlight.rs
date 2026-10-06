//! Syntax colouring for code block bodies (`code_highlight.zig`).
//!
//! A small per-profile tokenizer, not a grammar: comments, quoted strings,
//! numbers, keyword and literal words, and for shell the command, variable,
//! flag, and operator shapes. The presenter itself never calls it --
//! upstream's transcript does, when it paints a code block payload -- so it
//! is exported for that caller. The palette comes from the theme passed in;
//! upstream reads the same slots from its process-wide active theme.

use super::code_highlight_languages::{self as languages, KeywordCase, Profile};
use super::text_util as tu;
use super::theme::{self, SyntaxPalette, Theme};

/// Which builtin palette a block asks for (upstream's `Theme` enum).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    Dark,
    Light,
}

fn palette_for_variant(variant: Variant, active: &Theme) -> SyntaxPalette {
    // When the requested variant matches the active theme, a custom theme
    // contributes its syntax palette; otherwise the builtin variant renders.
    if active.light == (variant == Variant::Light) {
        return active.syntax.clone();
    }
    match variant {
        Variant::Dark => theme::FX_DARK.syntax,
        Variant::Light => theme::FX_LIGHT.syntax,
    }
}

/// Colours `source` with `profile`'s tokenizer.
///
/// When `base` is set the span opens with it and every token close restores
/// it, so untokenized text keeps the caller's ambient colour. `None` leaves
/// plain text at the terminal default.
pub fn highlight(
    source: &str,
    profile: &Profile,
    variant: Variant,
    base: Option<&str>,
    active: &Theme,
) -> String {
    let source = source.as_bytes();
    let mut styled = Vec::new();
    let palette = palette_for_variant(variant, active);
    if let Some(base_style) = base {
        styled.extend_from_slice(base_style.as_bytes());
    }
    // A theme can disable syntax highlighting entirely; the span then keeps
    // only the caller's base, byte-identical to a token-free source.
    if !palette.enabled {
        styled.extend_from_slice(source);
        return tu::into_string(styled);
    }

    let mut index = 0;
    // Command-position state, used only by command_words profiles (shell):
    // the next word is a command name unless a token says otherwise.
    let mut command_position = profile.command_words;
    while index < source.len() {
        let byte = source[index];
        if byte == b'\n' {
            styled.push(b'\n');
            command_position = profile.command_words;
            index += 1;
            continue;
        }
        if let Some(end) = block_comment_end(source, index, profile) {
            append_styled(
                &mut styled,
                &palette.comment_style,
                &source[index..end],
                base,
            );
            command_position = false;
            index = end;
            continue;
        }
        if let Some(end) = line_comment_end(source, index, profile.line_comments) {
            append_styled(
                &mut styled,
                &palette.comment_style,
                &source[index..end],
                base,
            );
            command_position = false;
            index = end;
            continue;
        }
        if profile.quotes.contains(&byte) {
            let end = quoted_end(source, index);
            if byte == b'"' && profile.dollar_vars {
                append_double_quoted(&mut styled, &palette, &source[index..end], base);
            } else {
                append_styled(
                    &mut styled,
                    &palette.string_style,
                    &source[index..end],
                    base,
                );
            }
            command_position = false;
            index = end;
            continue;
        }
        if is_number_start(source, index) {
            let end = number_end(source, index);
            // Shell: bare number arguments stay plain (a run id is not a
            // literal); only file descriptors glued to a redirect colour.
            if profile.bare_numbers || fd_context(source, index, end) {
                append_styled(
                    &mut styled,
                    &palette.number_style,
                    &source[index..end],
                    base,
                );
            } else {
                styled.extend_from_slice(&source[index..end]);
            }
            command_position = false;
            index = end;
            continue;
        }
        if profile.dollar_vars && byte == b'$' {
            // Command substitution reopens command position for its contents.
            if index + 1 < source.len() && source[index + 1] == b'(' {
                append_styled(&mut styled, &palette.operator_style, b"$(", base);
                command_position = true;
                index += 2;
                continue;
            }
            if let Some(end) = dollar_var_end(source, index) {
                append_styled(
                    &mut styled,
                    &palette.variable_style,
                    &source[index..end],
                    base,
                );
                command_position = false;
                index = end;
                continue;
            }
        }
        if profile.dollar_vars && byte == b'~' && tilde_start(source, index, profile.operators) {
            append_styled(&mut styled, &palette.variable_style, b"~", base);
            command_position = false;
            index += 1;
            continue;
        }
        if profile.dash_flags && byte == b'-' {
            if let Some(end) = flag_end(source, index, profile.operators) {
                append_styled(
                    &mut styled,
                    &palette.number_style,
                    &source[index..end],
                    base,
                );
                command_position = false;
                index = end;
                continue;
            }
        }
        if profile.operators.contains(&byte) {
            let end = operator_run_end(source, index, profile.operators);
            let run = &source[index..end];
            append_styled(&mut styled, &palette.operator_style, run, base);
            // Redirect targets are paths, not commands; `2>&1`-style runs
            // too.
            command_position = !run.contains(&b'<') && !run.contains(&b'>');
            index = end;
            continue;
        }
        if profile.command_words && byte == b'`' {
            // Backticks parse as code; their contents reopen command
            // position.
            styled.push(byte);
            command_position = true;
            index += 1;
            continue;
        }
        if is_identifier_start(byte) {
            let end = identifier_end(source, index);
            let token = &source[index..end];
            // A word glued to a path separator is a path segment, not
            // syntax: /dev/null keeps "null" plain.
            let after_separator = index > 0 && source[index - 1] == b'/';
            let mut styled_word = false;
            if profile.command_words {
                if command_position && !after_separator {
                    // Control keywords read as keywords; other command words
                    // as functions, matching the grammar's scopes.
                    let style = if is_command_prefix(token) {
                        &palette.keyword_style
                    } else {
                        &palette.function_style
                    };
                    append_styled(&mut styled, style, token, base);
                    styled_word = true;
                }
            } else if !after_separator
                && (!profile.keywords.is_empty() || !profile.literals.is_empty())
            {
                let token_hash = languages::packed_word_hash(token);
                if in_packed_list(token, token_hash, profile.keywords, profile.keyword_case) {
                    append_styled(&mut styled, &palette.keyword_style, token, base);
                    styled_word = true;
                } else if in_packed_list(token, token_hash, profile.literals, profile.keyword_case)
                {
                    append_styled(&mut styled, &palette.number_style, token, base);
                    styled_word = true;
                }
            }
            if !styled_word {
                styled.extend_from_slice(token);
            }
            // Control keywords are followed by the command they govern; an
            // ordinary command word is followed by its arguments.
            if profile.command_words {
                command_position = styled_word && command_position && is_command_prefix(token);
            }
            index = end;
            continue;
        }
        styled.push(byte);
        if !tu::is_ascii_whitespace(byte) {
            command_position = false;
        }
        index += 1;
    }

    tu::into_string(styled)
}

/// Control keywords after which the next word is again a command.
const COMMAND_PREFIXES: [&[u8]; 7] = [b"if", b"then", b"elif", b"else", b"while", b"until", b"do"];

fn is_command_prefix(token: &[u8]) -> bool {
    COMMAND_PREFIXES.contains(&token)
}

/// File descriptors glued to a redirect keep the number colour when bare
/// number arguments stay plain: the 2 in `2>` and the 1 in `>&1`.
fn fd_context(source: &[u8], start: usize, end: usize) -> bool {
    if end < source.len() && (source[end] == b'>' || source[end] == b'<') {
        return true;
    }
    if start > 0 && (source[start - 1] == b'>' || source[start - 1] == b'<') {
        return true;
    }
    start > 1
        && source[start - 1] == b'&'
        && (source[start - 2] == b'>' || source[start - 2] == b'<')
}

/// A tilde opens a home path at a word boundary when a path or name follows.
fn tilde_start(source: &[u8], index: usize, operators: &[u8]) -> bool {
    if index > 0 {
        let prev = source[index - 1];
        if !tu::is_ascii_whitespace(prev)
            && !operators.contains(&prev)
            && prev != b'('
            && prev != b'`'
        {
            return false;
        }
    }
    let next = index + 1;
    next < source.len()
        && (source[next] == b'/'
            || is_identifier_start(source[next])
            || source[next].is_ascii_digit())
}

/// Double-quoted spans interpolate in shell: `$name`, `${name}`, and `$(`
/// take the variable colour while the rest keeps the string colour.
fn append_double_quoted(
    out: &mut Vec<u8>,
    palette: &SyntaxPalette,
    text: &[u8],
    base: Option<&str>,
) {
    let inner_end = text.len() - 1;
    // The opening quote rides the first text chunk so the pair stays one
    // span.
    let mut chunk_start = 0;
    let mut i = 1;
    while i < inner_end {
        if text[i] == b'$' && text[i - 1] != b'\\' {
            let var_end = if i + 1 < inner_end && text[i + 1] == b'(' {
                Some(i + 2)
            } else {
                dollar_var_end_within(text, i, inner_end)
            };
            if let Some(end) = var_end {
                if chunk_start < i {
                    append_styled(out, &palette.string_style, &text[chunk_start..i], base);
                }
                append_styled(out, &palette.variable_style, &text[i..end], base);
                i = end;
                chunk_start = end;
                continue;
            }
        }
        i += 1;
    }
    if chunk_start < inner_end {
        append_styled(
            out,
            &palette.string_style,
            &text[chunk_start..inner_end],
            base,
        );
    }
    append_styled(out, &palette.string_style, &text[inner_end..], base);
}

fn append_styled(out: &mut Vec<u8>, style: &str, text: &[u8], base: Option<&str>) {
    out.extend_from_slice(style.as_bytes());
    out.extend_from_slice(text);
    // Close whatever the slot opened: fg-only slots keep the plain reset,
    // themed slots carrying bold/italic get those reset too.
    out.extend_from_slice(theme::closing_for(style).as_bytes());
    if let Some(base_style) = base {
        out.extend_from_slice(base_style.as_bytes());
    }
}

/// Line-oriented diff painting: `+`/`-` lines take the caller's
/// capability-resolved diff marker colours, `@@` hunks the keyword colour,
/// and file metadata lines the comment colour. Honours the theme's syntax
/// switch.
pub fn highlight_diff(
    source: &str,
    variant: Variant,
    added: &str,
    removed: &str,
    active: &Theme,
) -> String {
    let palette = palette_for_variant(variant, active);
    if !palette.enabled {
        return source.to_owned();
    }

    let mut styled = String::new();
    let mut lines = source.split('\n').peekable();
    while let Some(line) = lines.next() {
        let style: Option<&str> = if line.starts_with("+++") || line.starts_with("---") {
            Some(&palette.comment_style)
        } else if line.starts_with('+') {
            Some(added)
        } else if line.starts_with('-') {
            Some(removed)
        } else if line.starts_with("@@") {
            Some(&palette.keyword_style)
        } else if line.starts_with("diff ")
            || line.starts_with("index ")
            || line.starts_with("new file")
            || line.starts_with("deleted file")
            || line.starts_with("similarity")
            || line.starts_with("rename ")
        {
            Some(&palette.comment_style)
        } else {
            None
        };
        match style {
            Some(line_style) => {
                styled.push_str(line_style);
                styled.push_str(line);
                styled.push_str(theme::closing_for(line_style));
            }
            None => styled.push_str(line),
        }
        if lines.peek().is_some() {
            styled.push('\n');
        }
    }
    styled
}

fn block_comment_end(source: &[u8], index: usize, profile: &Profile) -> Option<usize> {
    let comment = profile.block_comment?;
    if !source[index..].starts_with(comment.start.as_bytes()) {
        return None;
    }
    let content_start = index + comment.start.len();
    match tu::index_of_pos(source, content_start, comment.end.as_bytes()) {
        Some(close_start) => Some(close_start + comment.end.len()),
        None => Some(source.len()),
    }
}

fn line_comment_end(source: &[u8], index: usize, prefixes: &[&str]) -> Option<usize> {
    prefixes
        .iter()
        .any(|prefix| source[index..].starts_with(prefix.as_bytes()))
        .then(|| tu::index_of_scalar_pos(source, index, b'\n').unwrap_or(source.len()))
}

fn quoted_end(source: &[u8], start: usize) -> usize {
    let quote = source[start];
    let mut index = start + 1;
    while index < source.len() {
        if source[index] == b'\n' {
            return index;
        }
        if source[index] == b'\\' && index + 1 < source.len() {
            index += 2;
            continue;
        }
        if source[index] == quote {
            return index + 1;
        }
        index += 1;
    }
    source.len()
}

fn is_number_start(source: &[u8], index: usize) -> bool {
    if !source[index].is_ascii_digit() {
        return false;
    }
    if index == 0 {
        return true;
    }
    let prev = source[index - 1];
    if is_identifier_continue(prev) {
        return false;
    }
    // A digit run glued to a word by a dash is a name segment, not a number:
    // paths like build-20260918 stay plain while flags like -80 still colour.
    !(prev == b'-' && index >= 2 && is_identifier_continue(source[index - 2]))
}

fn number_end(source: &[u8], start: usize) -> usize {
    let mut index = start;
    while index < source.len()
        && (source[index].is_ascii_alphanumeric() || source[index] == b'.' || source[index] == b'_')
    {
        index += 1;
    }
    index
}

fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte == b'$'
}

fn operator_run_end(source: &[u8], start: usize, operators: &[u8]) -> usize {
    let mut end = start;
    while end < source.len() && operators.contains(&source[end]) {
        end += 1;
    }
    end
}

/// `$` opens a variable when a name, braced name, positional digit, or
/// special parameter follows; a bare `$` stays plain text.
fn dollar_var_end(source: &[u8], start: usize) -> Option<usize> {
    dollar_var_end_within(source, start, source.len())
}

fn dollar_var_end_within(source: &[u8], start: usize, limit: usize) -> Option<usize> {
    let next = start + 1;
    if next >= limit {
        return None;
    }
    let b = source[next];
    if b == b'{' {
        let close = tu::index_of_scalar_pos(source, next + 1, b'}')?;
        return (close < limit).then_some(close + 1);
    }
    if b.is_ascii_alphabetic() || b == b'_' {
        let mut end = next;
        while end < limit && is_identifier_continue(source[end]) {
            end += 1;
        }
        return Some(end);
    }
    if b.is_ascii_digit() || b"?#@*!$".contains(&b) {
        return Some(next + 1);
    }
    None
}

/// A dash opens a flag token only at a word boundary (after whitespace, an
/// operator, or the start) with a letter, digit, or second dash next. Dashes
/// inside words, like date suffixes in paths, stay plain.
fn flag_end(source: &[u8], start: usize, operators: &[u8]) -> Option<usize> {
    if start > 0 {
        let prev = source[start - 1];
        if !tu::is_ascii_whitespace(prev) && !operators.contains(&prev) {
            return None;
        }
    }
    let next = start + 1;
    if next >= source.len() {
        return None;
    }
    let b = source[next];
    if !b.is_ascii_alphanumeric() && b != b'-' {
        return None;
    }
    let mut end = next;
    while end < source.len() && (source[end].is_ascii_alphanumeric() || source[end] == b'-') {
        end += 1;
    }
    Some(end)
}

fn is_identifier_continue(byte: u8) -> bool {
    is_identifier_start(byte) || byte.is_ascii_digit()
}

fn identifier_end(source: &[u8], start: usize) -> usize {
    let mut index = start + 1;
    while index < source.len() && is_identifier_continue(source[index]) {
        index += 1;
    }
    index
}

fn in_packed_list(
    token: &[u8],
    target_hash: u32,
    options: &[u8],
    keyword_case: KeywordCase,
) -> bool {
    let mut offset = 0;
    while offset < options.len() {
        let hash = u32::from_le_bytes([
            options[offset],
            options[offset + 1],
            options[offset + 2],
            options[offset + 3],
        ]);
        offset += 4;
        let len = usize::from(options[offset]);
        offset += 1;
        let end = offset + len;
        let option = &options[offset..end];
        if hash == target_hash {
            let matches = match keyword_case {
                KeywordCase::Sensitive => token == option,
                KeywordCase::AsciiInsensitive => token.eq_ignore_ascii_case(option),
            };
            if matches {
                return true;
            }
        }
        offset = end;
    }
    false
}

#[cfg(test)]
mod tests;
