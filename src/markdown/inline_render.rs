//! Inline markdown within one line (`inline_render.zig`).
//!
//! Rendering runs in three passes over a line:
//!
//! 1. Tokenize. Escapes, entities, code spans, links, images, autolinks,
//!    footnotes, and bare URLs become atomic tokens; runs of `*`, `_`, and
//!    `~~` become delimiter tokens whose ability to open or close is decided
//!    once from the bytes around them.
//! 2. Match delimiters with the CommonMark delimiter stack, so every span is a
//!    real pair and unmatched runs fall out as literal text without any
//!    lookahead or state-dependent rescanning.
//! 3. Emit ANSI, tracking nesting depth so closing an inner span re-opens an
//!    outer span of the same style.

use super::ansi::{self, Styles};
use super::payload::FootnoteSink;
use super::text_util as tu;
use super::theme;
use super::unicode_classes;

/// What inline rendering reads and advances besides the text itself.
///
/// Upstream threads a footnote sink and a pointer to its process-wide OSC 8
/// id counter through every writer, and reads its colours from globals; here
/// the three travel together.
pub(super) struct RenderContext<'a> {
    pub(super) styles: &'a Styles,
    /// The next OSC 8 hyperlink id. Every emitted link takes one and
    /// advances it, wrapping, so links never coalesce across wrapped rows.
    pub(super) link_id: &'a mut u32,
    pub(super) footnotes: Option<&'a mut dyn FootnoteSink>,
}

/// Heading content: emphasis resolves normally, but bold escapes are
/// suppressed because the heading style already owns bold.
pub(super) fn write_inline_no_bold(
    cx: &mut RenderContext<'_>,
    text: &[u8],
    out: &mut Vec<u8>,
    restore_underline_after_link: bool,
) {
    render_inline(
        cx,
        text,
        out,
        InlineOptions {
            restore_underline_after_link,
            suppress_bold: true,
        },
    );
}

pub(super) fn write_inline(
    cx: &mut RenderContext<'_>,
    text: &[u8],
    out: &mut Vec<u8>,
    restore_underline_after_link: bool,
) {
    render_inline(
        cx,
        text,
        out,
        InlineOptions {
            restore_underline_after_link,
            suppress_bold: false,
        },
    );
}

#[derive(Clone, Copy)]
struct InlineOptions {
    restore_underline_after_link: bool,
    suppress_bold: bool,
}

fn render_inline(
    cx: &mut RenderContext<'_>,
    text: &[u8],
    out: &mut Vec<u8>,
    options: InlineOptions,
) {
    let mut tokens = Vec::new();
    tokenize(cx, text, &mut tokens);

    let mut matches = Vec::new();
    match_delimiters(&mut tokens, &mut matches);

    emit_tokens(cx, text, &tokens, &matches, out, options);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Style {
    Bold,
    Italic,
    Strike,
}

impl Style {
    fn slot(self) -> usize {
        match self {
            Style::Bold => 0,
            Style::Italic => 1,
            Style::Strike => 2,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Delimiter {
    marker: u8,
    start: usize,
    orig_len: usize,
    remaining: usize,
    can_open: bool,
    can_close: bool,
    /// Matches this run opens, newest first, so emission in list order puts
    /// the outermost span first.
    open_head: Option<u32>,
    /// Matches this run closes, oldest first, so the innermost span closes
    /// first.
    close_head: Option<u32>,
    close_tail: Option<u32>,
}

enum Token<'t> {
    /// Literal bytes copied from the source line, including the character
    /// produced by a backslash escape.
    Text(&'t [u8]),
    Entity(DecodedEntity),
    Code(&'t [u8]),
    Link {
        link: InlineLink<'t>,
        visible_prefix: Option<&'static str>,
    },
    Footnote(usize),
    Delimiter(Delimiter),
}

struct Match {
    style: Style,
    next_open: Option<u32>,
    next_close: Option<u32>,
}

fn tokenize<'t>(cx: &mut RenderContext<'_>, text: &'t [u8], tokens: &mut Vec<Token<'t>>) {
    let mut i = 0;
    let mut literal_start = 0;
    let mut link_admission_suppressed_until = 0;
    // Every bracket construct starting at `i` ends at the first `]` at or
    // after `i`, so that position is found once and shared: a `[` whose `]`
    // is not followed by `(` cannot be a link or image, and a footnote label
    // ends there too. Without this every `[` in a long run rescans to the
    // same `]`.
    let last_close_bracket = text.iter().rposition(|&b| b == b']');
    let mut next_close_bracket: Option<usize> = None;
    // End of the most recent delimiter run that can open emphasis. A bare
    // URL may follow such a run directly, and the tokenizer owns that
    // decision rather than a second flanking rule inside the URL parser.
    let mut opening_delimiter_end = 0;

    while i < text.len() {
        let c = text[i];
        let after_opening_delimiter = i > 0 && opening_delimiter_end == i;
        if let Some(last) = last_close_bracket {
            if i <= last && next_close_bracket.is_none_or(|next| next < i) {
                next_close_bracket = tu::index_of_scalar_pos(text, i, b']');
            }
        }
        let bracket_link_possible = next_close_bracket
            .is_some_and(|next| next >= i && next + 1 < text.len() && text[next + 1] == b'(');

        if c == b'`' {
            if let Some(span) = code_span_at(text, i) {
                flush_literal(text, tokens, literal_start, i);
                tokens.push(Token::Code(&text[span.content_start..span.content_end]));
                i = span.end;
                literal_start = i;
                continue;
            }
            // A backtick run without a partner is literal text.
            i += backtick_run_length(text, i);
            continue;
        }

        if c == b'&' {
            if let Some(entity) = decode_entity(text, i) {
                flush_literal(text, tokens, literal_start, i);
                i = entity.end;
                tokens.push(Token::Entity(entity));
                literal_start = i;
                continue;
            }
        }

        if c == b'\\' && i + 1 < text.len() && tu::is_escaped_punctuation_at(text, i + 1) {
            flush_literal(text, tokens, literal_start, i);
            if text[i + 1] == b'<' {
                link_admission_suppressed_until =
                    link_admission_suppressed_until.max(angle_autolink_candidate_end(text, i + 1));
            }
            if bracket_link_possible
                && text[i + 1] == b'!'
                && i + 2 < text.len()
                && text[i + 2] == b'['
            {
                if let Some(candidate_end) = malformed_inline_link_candidate_end(text, i + 2) {
                    tokens.push(Token::Text(&text[i + 1..candidate_end]));
                    link_admission_suppressed_until =
                        link_admission_suppressed_until.max(candidate_end);
                    i = candidate_end;
                    literal_start = i;
                    continue;
                }
            }
            if bracket_link_possible && text[i + 1] == b'[' {
                if let Some(candidate_end) = malformed_inline_link_candidate_end(text, i + 1) {
                    link_admission_suppressed_until =
                        link_admission_suppressed_until.max(candidate_end);
                }
            }
            tokens.push(Token::Text(&text[i + 1..i + 2]));
            i += 2;
            literal_start = i;
            continue;
        }

        if bracket_link_possible
            && i >= link_admission_suppressed_until
            && c == b'!'
            && i + 1 < text.len()
            && text[i + 1] == b'['
        {
            if let Some(image) = parse_inline_image(text, i) {
                flush_literal(text, tokens, literal_start, i);
                i = image.end;
                tokens.push(Token::Link {
                    link: image,
                    visible_prefix: Some("\u{25A7} "),
                });
                literal_start = i;
                continue;
            }
            if let Some(candidate_end) = malformed_inline_link_candidate_end(text, i + 1) {
                link_admission_suppressed_until =
                    link_admission_suppressed_until.max(candidate_end);
            }
        }

        if c == b'[' {
            if let (Some(close), Some(sink)) = (next_close_bracket, cx.footnotes.as_mut()) {
                if let Some(reference) = parse_footnote_reference(text, i, close) {
                    flush_literal(text, tokens, literal_start, i);
                    let number = sink.register(reference.label);
                    tokens.push(Token::Footnote(number));
                    i = reference.end;
                    literal_start = i;
                    continue;
                }
            }
        }

        if bracket_link_possible && i >= link_admission_suppressed_until && c == b'[' {
            if let Some(link) = parse_inline_link(text, i) {
                flush_literal(text, tokens, literal_start, i);
                i = link.end;
                tokens.push(Token::Link {
                    link,
                    visible_prefix: None,
                });
                literal_start = i;
                continue;
            }
            if let Some(candidate_end) = malformed_inline_link_candidate_end(text, i) {
                link_admission_suppressed_until =
                    link_admission_suppressed_until.max(candidate_end);
            }
        }

        if i >= link_admission_suppressed_until && c == b'<' {
            if let Some(link) = parse_angle_autolink(text, i) {
                flush_literal(text, tokens, literal_start, i);
                i = link.end;
                tokens.push(Token::Link {
                    link,
                    visible_prefix: None,
                });
                literal_start = i;
                continue;
            }
            link_admission_suppressed_until =
                link_admission_suppressed_until.max(angle_autolink_candidate_end(text, i));
        }

        if i >= link_admission_suppressed_until {
            if let Some(link) = parse_bare_url(text, i, after_opening_delimiter) {
                flush_literal(text, tokens, literal_start, i);
                i = link.end;
                tokens.push(Token::Link {
                    link,
                    visible_prefix: None,
                });
                literal_start = i;
                continue;
            }
        }

        if c == b'*' || c == b'_' || c == b'~' {
            let run_end = marker_run_end(text, i);
            if let Some(delimiter) = delimiter_at(text, i, run_end) {
                flush_literal(text, tokens, literal_start, i);
                tokens.push(Token::Delimiter(delimiter));
                literal_start = run_end;
                if delimiter.can_open {
                    opening_delimiter_end = run_end;
                }
            }
            i = run_end;
            continue;
        }

        i += 1;
    }
    flush_literal(text, tokens, literal_start, text.len());
}

fn flush_literal<'t>(text: &'t [u8], tokens: &mut Vec<Token<'t>>, start: usize, end: usize) {
    if end > start {
        tokens.push(Token::Text(&text[start..end]));
    }
}

/// End of the run of `text[i]` bytes starting at `i`.
fn marker_run_end(text: &[u8], i: usize) -> usize {
    let marker = text[i];
    let mut end = i;
    while end < text.len() && text[end] == marker {
        end += 1;
    }
    end
}

/// Classifies the marker run `text[start..end]` with the CommonMark flanking
/// rules. `None` for a run that can neither open nor close, or for a tilde
/// run that is not exactly two long, so it stays literal text.
fn delimiter_at(text: &[u8], start: usize, end: usize) -> Option<Delimiter> {
    let marker = text[start];
    let len = end - start;
    if marker == b'~' && len != 2 {
        return None;
    }

    let before = codepoint_before(text, start);
    let after = codepoint_at(text, end);
    let before_space = unicode_classes::is_whitespace(before);
    let after_space = unicode_classes::is_whitespace(after);
    let before_punct = unicode_classes::is_punctuation_or_symbol(before);
    let after_punct = unicode_classes::is_punctuation_or_symbol(after);

    let left_flanking = !after_space && (!after_punct || before_space || before_punct);
    let right_flanking = !before_space && (!before_punct || after_space || after_punct);

    let mut can_open = left_flanking;
    let mut can_close = right_flanking;
    if marker == b'_' {
        // Intraword underscores never delimit, so snake_case stays literal.
        can_open = left_flanking && (!right_flanking || before_punct);
        can_close = right_flanking && (!left_flanking || after_punct);
    }
    if !can_open && !can_close {
        return None;
    }
    Some(Delimiter {
        marker,
        start,
        orig_len: len,
        remaining: len,
        can_open,
        can_close,
        open_head: None,
        close_head: None,
        close_tail: None,
    })
}

/// Code point ending just before `index`, or a space at the line start.
fn codepoint_before(text: &[u8], index: usize) -> u32 {
    if index == 0 {
        return u32::from(b' ');
    }
    let mut start = index - 1;
    let mut steps = 0;
    while start > 0 && steps < 3 && text[start] & 0xc0 == 0x80 {
        start -= 1;
        steps += 1;
    }
    single_codepoint(&text[start..index]).unwrap_or(u32::from(text[index - 1]))
}

/// Code point starting at `index`, or a space at the line end.
fn codepoint_at(text: &[u8], index: usize) -> u32 {
    if index >= text.len() {
        return u32::from(b' ');
    }
    let len = match text[index] {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf7 => 4,
        _ => return u32::from(text[index]),
    };
    if index + len > text.len() {
        return u32::from(text[index]);
    }
    single_codepoint(&text[index..index + len]).unwrap_or(u32::from(text[index]))
}

/// The one code point `bytes` encodes, if it encodes exactly one.
fn single_codepoint(bytes: &[u8]) -> Option<u32> {
    let decoded = std::str::from_utf8(bytes).ok()?;
    let mut chars = decoded.chars();
    let c = chars.next()?;
    chars.next().is_none().then_some(u32::from(c))
}

#[derive(Clone, Copy)]
struct StackEntry {
    token: u32,
    seq: u32,
}

/// CommonMark "process emphasis" over an explicit stack of open delimiters.
/// A closer searches the stack from the top for a compatible opener; the pair
/// is recorded and every entry above the opener is popped, since a run
/// between a matched pair can no longer open anything. Each entry is pushed
/// and popped at most once, and `openers_bottom` records, per closer kind,
/// how deep a failed search reached so the same entries are never rescanned
/// for that kind, which keeps the pass linear.
fn match_delimiters(tokens: &mut [Token<'_>], matches: &mut Vec<Match>) {
    let mut stack: Vec<StackEntry> = Vec::new();
    let mut next_seq: u32 = 1;
    // Sequence number of the top entry when a search for this closer kind
    // last failed; entries at or below it stay unusable for that kind.
    let mut openers_bottom = [[[0u32; 3]; 2]; 3];

    for token_index in 0..tokens.len() {
        // Every opener on the stack precedes the closer.
        let (before, rest) = tokens.split_at_mut(token_index);
        let Token::Delimiter(closer) = &mut rest[0] else {
            continue;
        };

        if closer.can_close {
            let bottom = &mut openers_bottom[marker_slot(closer.marker)]
                [usize::from(closer.can_open)][closer.orig_len % 3];
            // Each part of the closer runs its own search: once a match pops
            // the stack, the remainder searches again from the new top, and
            // a failed remainder is cached like any other failed search.
            let mut matched_in_search = false;
            let mut depth = stack.len();
            while closer.remaining > 0 && depth > 0 {
                depth -= 1;
                let entry = stack[depth];
                if entry.seq <= *bottom {
                    break;
                }
                let Token::Delimiter(opener) = &mut before[entry.token as usize] else {
                    continue;
                };
                if opener.marker != closer.marker {
                    continue;
                }
                if opener.marker != b'~' && violates_rule_of_three(opener, closer) {
                    continue;
                }

                let use_len =
                    if opener.marker == b'~' || (opener.remaining >= 2 && closer.remaining >= 2) {
                        2
                    } else {
                        1
                    };
                let style = match opener.marker {
                    b'~' => Style::Strike,
                    _ if use_len == 2 => Style::Bold,
                    _ => Style::Italic,
                };
                let match_index = matches.len() as u32;
                matches.push(Match {
                    style,
                    next_open: opener.open_head,
                    next_close: None,
                });
                opener.open_head = Some(match_index);
                match closer.close_tail {
                    Some(tail) => matches[tail as usize].next_close = Some(match_index),
                    None => closer.close_head = Some(match_index),
                }
                closer.close_tail = Some(match_index);
                opener.remaining -= use_len;
                closer.remaining -= use_len;

                // Everything above the opener sat between the pair.
                let keep = if opener.remaining == 0 {
                    depth
                } else {
                    depth + 1
                };
                stack.truncate(keep);
                depth = stack.len();
                matched_in_search = closer.remaining == 0;
            }
            if !matched_in_search {
                if let Some(top) = stack.last() {
                    *bottom = top.seq;
                }
            }
        }

        if closer.can_open && closer.remaining > 0 {
            stack.push(StackEntry {
                token: token_index as u32,
                seq: next_seq,
            });
            next_seq += 1;
        }
    }
}

fn marker_slot(marker: u8) -> usize {
    match marker {
        b'*' => 0,
        b'_' => 1,
        _ => 2,
    }
}

/// A run that can both open and close only pairs with another run when their
/// combined length is not a multiple of three, unless both lengths are.
fn violates_rule_of_three(opener: &Delimiter, closer: &Delimiter) -> bool {
    if !opener.can_close && !closer.can_open {
        return false;
    }
    let sum = opener.orig_len + closer.orig_len;
    if !sum.is_multiple_of(3) {
        return false;
    }
    !opener.orig_len.is_multiple_of(3) || !closer.orig_len.is_multiple_of(3)
}

fn emit_tokens(
    cx: &mut RenderContext<'_>,
    text: &[u8],
    tokens: &[Token<'_>],
    matches: &[Match],
    out: &mut Vec<u8>,
    options: InlineOptions,
) {
    let mut depth = [0usize; 3];
    for token in tokens {
        match token {
            Token::Text(slice) => out.extend_from_slice(slice),
            Token::Entity(entity) => out.extend_from_slice(&entity.utf8[..entity.len]),
            Token::Code(content) => {
                out.extend_from_slice(cx.styles.inline_code_open().as_bytes());
                if let Some(url_end) = code_span_url_end(content) {
                    let id = take_link_id(cx.link_id);
                    out.extend_from_slice(format!("\x1b]8;id=fx-{id};").as_bytes());
                    out.extend_from_slice(&content[..url_end]);
                    out.extend_from_slice(b"\x1b\\");
                    out.extend_from_slice(&content[..url_end]);
                    out.extend_from_slice(b"\x1b]8;;\x1b\\");
                    out.extend_from_slice(&content[url_end..]);
                } else {
                    out.extend_from_slice(content);
                }
                out.extend_from_slice(cx.styles.inline_code_close().as_bytes());
            }
            Token::Link {
                link,
                visible_prefix,
            } => emit_inline_link(
                cx,
                out,
                link,
                options.restore_underline_after_link,
                *visible_prefix,
            ),
            Token::Footnote(number) => write_footnote_marker(out, *number),
            Token::Delimiter(delimiter) => {
                let mut close = delimiter.close_head;
                while let Some(index) = close {
                    let entry = &matches[index as usize];
                    emit_style(out, entry.style, false, &mut depth, options);
                    close = entry.next_close;
                }
                out.extend_from_slice(
                    &text[delimiter.start..delimiter.start + delimiter.remaining],
                );
                let mut open = delimiter.open_head;
                while let Some(index) = open {
                    let entry = &matches[index as usize];
                    emit_style(out, entry.style, true, &mut depth, options);
                    open = entry.next_open;
                }
            }
        }
    }
}

fn take_link_id(link_id: &mut u32) -> u32 {
    let id = *link_id;
    *link_id = link_id.wrapping_add(1);
    id
}

fn emit_style(
    out: &mut Vec<u8>,
    style: Style,
    opening: bool,
    depth: &mut [usize; 3],
    options: InlineOptions,
) {
    let slot = style.slot();
    let (open_seq, close_seq) = match style {
        Style::Bold => (ansi::BOLD_OPEN, ansi::BOLD_CLOSE),
        Style::Italic => (ansi::ITALIC_OPEN, ansi::ITALIC_CLOSE),
        Style::Strike => (ansi::STRIKE_OPEN, ansi::STRIKE_CLOSE),
    };
    let visible = !(style == Style::Bold && options.suppress_bold);
    if opening {
        depth[slot] += 1;
        if visible {
            out.extend_from_slice(open_seq.as_bytes());
        }
        return;
    }
    depth[slot] -= 1;
    if !visible {
        return;
    }
    out.extend_from_slice(close_seq.as_bytes());
    // Closing an inner span must not switch off an enclosing span of the
    // same style.
    if depth[slot] > 0 {
        out.extend_from_slice(open_seq.as_bytes());
    }
}

struct ParsedFootnoteReference<'t> {
    label: &'t [u8],
    end: usize,
}

/// `close` is the first `]` at or after `start`, found by the tokenizer.
fn parse_footnote_reference(
    text: &[u8],
    start: usize,
    close: usize,
) -> Option<ParsedFootnoteReference<'_>> {
    if start + 4 > text.len() || text[start] != b'[' || text[start + 1] != b'^' {
        return None;
    }
    if close <= start + 2 || close >= text.len() || text[close] != b']' {
        return None;
    }
    if close + 1 < text.len() && text[close + 1] == b':' {
        return None;
    }
    Some(ParsedFootnoteReference {
        label: &text[start + 2..close],
        end: close + 1,
    })
}

fn write_footnote_marker(out: &mut Vec<u8>, number: usize) {
    ansi::write_dim(out, format!("[{number}]").as_bytes());
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LabelMode {
    Escaped,
    Literal,
}

struct InlineLink<'t> {
    text: &'t [u8],
    url: &'t [u8],
    end: usize,
    destination_prefix: &'static [u8],
    label_mode: LabelMode,
}

fn emit_inline_link(
    cx: &mut RenderContext<'_>,
    out: &mut Vec<u8>,
    link: &InlineLink<'_>,
    restore_underline_after_link: bool,
    visible_prefix: Option<&str>,
) {
    let id = take_link_id(cx.link_id);
    out.extend_from_slice(format!("\x1b]8;id=fx-{id};").as_bytes());
    out.extend_from_slice(link.destination_prefix);
    out.extend_from_slice(link.url);
    out.extend_from_slice(b"\x1b\\");
    // Link text carries the theme's link colour in addition to the underline.
    let link_style = cx.styles.link_style();
    out.extend_from_slice(link_style.as_bytes());
    out.extend_from_slice(ansi::UNDERLINE_OPEN.as_bytes());
    if let Some(prefix) = visible_prefix {
        out.extend_from_slice(prefix.as_bytes());
    }
    let visible_text: &[u8] = if link.text.is_empty() && visible_prefix.is_some() {
        b"image"
    } else {
        link.text
    };
    match link.label_mode {
        LabelMode::Escaped => tu::append_escaped_punctuation(out, visible_text),
        LabelMode::Literal => out.extend_from_slice(visible_text),
    }
    out.extend_from_slice(ansi::UNDERLINE_CLOSE.as_bytes());
    out.extend_from_slice(theme::closing_for(link_style).as_bytes());
    out.extend_from_slice(b"\x1b]8;;\x1b\\");
    if restore_underline_after_link {
        out.extend_from_slice(ansi::UNDERLINE_OPEN.as_bytes());
    }
}

/// Rejects control bytes so a URL cannot terminate its OSC 8 wrapper.
fn parse_inline_link(text: &[u8], start: usize) -> Option<InlineLink<'_>> {
    parse_inline_bracket_destination(text, start, false)
}

fn parse_inline_image(text: &[u8], start: usize) -> Option<InlineLink<'_>> {
    if start + 1 >= text.len() || text[start] != b'!' || text[start + 1] != b'[' {
        return None;
    }
    parse_inline_bracket_destination(text, start + 1, true)
}

fn parse_inline_bracket_destination(
    text: &[u8],
    start: usize,
    allow_empty_text: bool,
) -> Option<InlineLink<'_>> {
    if start >= text.len() || text[start] != b'[' {
        return None;
    }
    let mut j = start + 1;
    while j < text.len() && text[j] != b']' && text[j] != b'\n' {
        j += 1;
    }
    if j >= text.len() || text[j] != b']' {
        return None;
    }
    let text_end = j;
    if !allow_empty_text && text_end == start + 1 {
        return None;
    }
    if text_end + 1 >= text.len() || text[text_end + 1] != b'(' {
        return None;
    }
    let destination = parse_link_destination(text, text_end + 2)?;
    if !is_valid_link_url(destination.url) {
        return None;
    }
    Some(InlineLink {
        text: &text[start + 1..text_end],
        url: destination.url,
        end: destination.end,
        destination_prefix: b"",
        label_mode: LabelMode::Escaped,
    })
}

struct LinkDestination<'t> {
    url: &'t [u8],
    /// Index just past the closing `)`.
    end: usize,
}

/// Parses `(destination "optional title")` starting just after the `(`. The
/// destination is either `<...>` or a run without spaces whose parentheses
/// balance; the title is validated and dropped.
fn parse_link_destination(text: &[u8], start: usize) -> Option<LinkDestination<'_>> {
    let mut k = skip_inline_spaces(text, start);
    if k >= text.len() {
        return None;
    }

    let url;
    if text[k] == b'<' {
        let close = tu::index_of_scalar_pos(text, k + 1, b'>')?;
        url = &text[k + 1..close];
        if url.iter().any(|&byte| byte == b'<' || byte == b'\n') {
            return None;
        }
        k = close + 1;
    } else {
        let url_start = k;
        let mut depth = 0usize;
        while k < text.len() {
            let byte = text[k];
            if byte == b'\\' && k + 1 < text.len() {
                k += 2;
                continue;
            }
            if byte <= b' ' || byte == 0x7f {
                break;
            }
            if byte == b'(' {
                depth += 1;
            } else if byte == b')' {
                if depth == 0 {
                    break;
                }
                depth -= 1;
            }
            k += 1;
        }
        if depth != 0 || k == url_start {
            return None;
        }
        url = &text[url_start..k];
    }

    let after_url = k;
    k = skip_inline_spaces(text, k);
    if k > after_url && k < text.len() && text[k] != b')' {
        k = link_title_end(text, k)?;
        k = skip_inline_spaces(text, k);
    }
    if k >= text.len() || text[k] != b')' {
        return None;
    }
    Some(LinkDestination { url, end: k + 1 })
}

fn skip_inline_spaces(text: &[u8], start: usize) -> usize {
    let mut k = start;
    while k < text.len() && (text[k] == b' ' || text[k] == b'\t') {
        k += 1;
    }
    k
}

/// The index just past a `"..."`, `'...'`, or `(...)` link title.
fn link_title_end(text: &[u8], start: usize) -> Option<usize> {
    if start >= text.len() {
        return None;
    }
    let closer = match text[start] {
        b'"' => b'"',
        b'\'' => b'\'',
        b'(' => b')',
        _ => return None,
    };
    let mut k = start + 1;
    while k < text.len() {
        if text[k] == b'\\' && k + 1 < text.len() {
            k += 2;
            continue;
        }
        if text[k] == b'\n' {
            return None;
        }
        if text[k] == closer {
            return Some(k + 1);
        }
        k += 1;
    }
    None
}

struct CodeSpan {
    content_start: usize,
    content_end: usize,
    /// Index just past the closing backtick run.
    end: usize,
}

fn backtick_run_length(text: &[u8], start: usize) -> usize {
    let mut end = start;
    while end < text.len() && text[end] == b'`' {
        end += 1;
    }
    end - start
}

/// The code span opened by the backtick run at `start`: the content ends at
/// the next run of exactly the same length. One leading and one trailing
/// space are stripped when both are present and the content is not all
/// spaces.
fn code_span_at(text: &[u8], start: usize) -> Option<CodeSpan> {
    let run = backtick_run_length(text, start);
    if run == 0 {
        return None;
    }
    let mut k = start + run;
    while k < text.len() {
        if text[k] != b'`' {
            k += 1;
            continue;
        }
        let candidate = backtick_run_length(text, k);
        if candidate == run {
            let mut content_start = start + run;
            let mut content_end = k;
            let content = &text[content_start..content_end];
            if content.len() >= 2
                && content[0] == b' '
                && content[content.len() - 1] == b' '
                && content.iter().any(|&b| b != b' ')
            {
                content_start += 1;
                content_end -= 1;
            }
            return Some(CodeSpan {
                content_start,
                content_end,
                end: k + run,
            });
        }
        k += candidate;
    }
    None
}

struct DecodedEntity {
    utf8: [u8; 4],
    len: usize,
    /// Index just past the terminating `;`.
    end: usize,
}

/// Longest reference body accepted between `&` and `;`: the numeric form
/// `#x10FFFF` is eight characters and every named entity in the table is
/// shorter, so the terminator search never needs to look further.
const MAX_ENTITY_NAME_LEN: usize = 8;

/// Decodes the HTML entities models commonly emit plus numeric references.
fn decode_entity(text: &[u8], start: usize) -> Option<DecodedEntity> {
    if start >= text.len() || text[start] != b'&' {
        return None;
    }
    // Bound the terminator search so a line full of ampersands stays linear.
    let window_end = text.len().min(start + 1 + MAX_ENTITY_NAME_LEN + 1);
    let semicolon = tu::index_of_scalar_pos(&text[..window_end], start + 1, b';')?;
    let name = &text[start + 1..semicolon];
    if name.is_empty() {
        return None;
    }

    let codepoint = if name[0] == b'#' {
        let hex = name.len() > 1 && (name[1] == b'x' || name[1] == b'X');
        let digits = if hex { &name[2..] } else { &name[1..] };
        if digits.is_empty() {
            return None;
        }
        let value = parse_u21(digits, if hex { 16 } else { 10 })?;
        // Control characters would reach the terminal as real control bytes,
        // so the reference stays literal instead of decoding.
        if value != 0 && (value < 0x20 || (0x7f..=0x9f).contains(&value)) {
            return None;
        }
        if value == 0 || (0xd800..=0xdfff).contains(&value) {
            0xfffd
        } else {
            value
        }
    } else {
        const NAMED: [(&[u8], u32); 13] = [
            (b"amp", '&' as u32),
            (b"lt", '<' as u32),
            (b"gt", '>' as u32),
            (b"quot", '"' as u32),
            (b"apos", '\'' as u32),
            (b"nbsp", 0xa0),
            (b"copy", 0xa9),
            (b"reg", 0xae),
            (b"hellip", 0x2026),
            (b"mdash", 0x2014),
            (b"ndash", 0x2013),
            (b"larr", 0x2190),
            (b"rarr", 0x2192),
        ];
        NAMED.iter().find(|(entry, _)| *entry == name)?.1
    };

    let c = char::from_u32(codepoint)?;
    let mut utf8 = [0u8; 4];
    let len = c.encode_utf8(&mut utf8).len();
    Some(DecodedEntity {
        utf8,
        len,
        end: semicolon + 1,
    })
}

/// `std.fmt.parseInt(u21, digits, base)`: an optional sign, `_` separators
/// between digits, and overflow past `u21` rejected. A minus sign only
/// survives on a value of zero.
fn parse_u21(buf: &[u8], base: u32) -> Option<u32> {
    const MAX: u32 = (1 << 21) - 1;
    let (negative, digits) = match buf.first() {
        Some(b'+') => (false, &buf[1..]),
        Some(b'-') => (true, &buf[1..]),
        _ => (false, buf),
    };
    if digits.is_empty() || digits[0] == b'_' || digits[digits.len() - 1] == b'_' {
        return None;
    }
    let mut accumulate: u32 = 0;
    for &c in digits {
        if c == b'_' {
            continue;
        }
        let digit = char::from(c).to_digit(base)?;
        if negative {
            // Any non-zero digit makes the value negative, which a `u21`
            // cannot hold.
            if digit != 0 {
                return None;
            }
            continue;
        }
        accumulate = accumulate.checked_mul(base)?.checked_add(digit)?;
        if accumulate > MAX {
            return None;
        }
    }
    Some(accumulate)
}

fn parse_angle_autolink(text: &[u8], start: usize) -> Option<InlineLink<'_>> {
    if start >= text.len() || text[start] != b'<' {
        return None;
    }

    let end = angle_autolink_candidate_end(text, start);
    if end <= start + 1 || end > text.len() || text[end - 1] != b'>' {
        return None;
    }

    let value = &text[start + 1..end - 1];
    if is_valid_angle_autolink_uri(value) && is_valid_link_url(value) {
        return Some(InlineLink {
            text: value,
            url: value,
            end,
            destination_prefix: b"",
            label_mode: LabelMode::Literal,
        });
    }
    if is_valid_angle_autolink_email(value) && is_valid_link_url_with_prefix(b"mailto:", value) {
        return Some(InlineLink {
            text: value,
            url: value,
            end,
            destination_prefix: b"mailto:",
            label_mode: LabelMode::Literal,
        });
    }
    None
}

fn angle_autolink_candidate_end(text: &[u8], start: usize) -> usize {
    if start >= text.len() || text[start] != b'<' {
        return start;
    }
    let mut end = start + 1;
    while end < text.len() && text[end] != b'>' && text[end] != b'\n' {
        end += 1;
    }
    if end < text.len() && text[end] == b'>' {
        end + 1
    } else {
        end
    }
}

fn is_valid_angle_autolink_uri(value: &[u8]) -> bool {
    let mut colon = 0;
    while colon < value.len() && value[colon] != b':' {
        colon += 1;
    }
    if !(2..=32).contains(&colon) || colon == value.len() || !tu::is_ascii_alpha(value[0]) {
        return false;
    }
    if value[1..colon].iter().any(|&byte| {
        !tu::is_ascii_alpha_numeric(byte) && byte != b'+' && byte != b'-' && byte != b'.'
    }) {
        return false;
    }
    !value[colon + 1..]
        .iter()
        .any(|&byte| byte <= b' ' || byte == b'<' || byte == b'>')
}

fn is_valid_angle_autolink_email(value: &[u8]) -> bool {
    let mut at_index = None;
    for (index, &byte) in value.iter().enumerate() {
        if byte == b'@' {
            if at_index.is_some() {
                return false;
            }
            at_index = Some(index);
        }
    }

    let Some(at) = at_index else {
        return false;
    };
    if at == 0 || at + 1 >= value.len() {
        return false;
    }
    if !value[..at]
        .iter()
        .all(|&byte| is_angle_autolink_email_local_byte(byte))
    {
        return false;
    }

    // Every dot-separated label, the empty ones around a stray dot included.
    value[at + 1..]
        .split(|&byte| byte == b'.')
        .all(is_valid_angle_autolink_email_domain_label)
}

fn is_angle_autolink_email_local_byte(byte: u8) -> bool {
    tu::is_ascii_alpha_numeric(byte)
        || matches!(
            byte,
            b'.' | b'!'
                | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'/'
                | b'='
                | b'?'
                | b'^'
                | b'_'
                | b'`'
                | b'{'
                | b'|'
                | b'}'
                | b'~'
                | b'-'
        )
}

fn is_valid_angle_autolink_email_domain_label(label: &[u8]) -> bool {
    if label.is_empty() || label.len() > 63 {
        return false;
    }
    for (index, &byte) in label.iter().enumerate() {
        if index == 0 || index + 1 == label.len() {
            if !tu::is_ascii_alpha_numeric(byte) {
                return false;
            }
        } else if !tu::is_ascii_alpha_numeric(byte) && byte != b'-' {
            return false;
        }
    }
    true
}

fn code_span_url_end(content: &[u8]) -> Option<usize> {
    let scheme_len = if content.starts_with(b"https://") {
        "https://".len()
    } else if content.starts_with(b"http://") {
        "http://".len()
    } else {
        return None;
    };

    // Only a sentence-final period is excluded; other suffixes can be URI
    // bytes in literal code, including _, *, ~, !, ?, ;, :, and ,.
    let end = if content.len() > scheme_len && content[content.len() - 1] == b'.' {
        content.len() - 1
    } else {
        content.len()
    };
    if end == scheme_len || !is_valid_link_url(&content[..end]) {
        return None;
    }
    if content.iter().any(|&byte| byte <= b' ' || byte == 0x7f) {
        return None;
    }
    Some(end)
}

/// The bare URL starting at `start`. Its extent does not depend on any
/// emphasis state: it ends at whitespace or a closing bracket, and trailing
/// punctuation, including `*`, `_`, and `~`, is left outside the link the
/// way GFM autolinks do, so a delimiter that follows a URL can still close
/// the span that contains it.
fn parse_bare_url(
    text: &[u8],
    start: usize,
    after_opening_delimiter: bool,
) -> Option<InlineLink<'_>> {
    if !is_bare_url_boundary(text, start, after_opening_delimiter) {
        return None;
    }
    let rest = &text[start..];
    let scheme_len = if rest.starts_with(b"https://") {
        "https://".len()
    } else if rest.starts_with(b"http://") {
        "http://".len()
    } else {
        return None;
    };

    let mut end = start + scheme_len;
    while end < text.len() && !is_bare_url_terminator(text[end]) {
        end += 1;
    }
    while end > start + scheme_len && is_trailing_bare_url_byte(text[end - 1]) {
        end -= 1;
    }

    let url = &text[start..end];
    if !is_valid_link_url(url) {
        return None;
    }
    Some(InlineLink {
        text: url,
        url,
        end,
        destination_prefix: b"",
        label_mode: LabelMode::Escaped,
    })
}

fn malformed_inline_link_candidate_end(text: &[u8], start: usize) -> Option<usize> {
    if start >= text.len() || text[start] != b'[' {
        return None;
    }

    let mut label_end = start + 1;
    while label_end < text.len() && text[label_end] != b']' && text[label_end] != b'\n' {
        label_end += 1;
    }
    if label_end >= text.len() || text[label_end] != b']' {
        return None;
    }
    if label_end + 1 >= text.len() || text[label_end + 1] != b'(' {
        return None;
    }

    let mut candidate_end = label_end + 2;
    while candidate_end < text.len() && text[candidate_end] != b')' && text[candidate_end] != b'\n'
    {
        candidate_end += 1;
    }
    if candidate_end < text.len() && text[candidate_end] == b')' {
        return Some(candidate_end + 1);
    }
    Some(candidate_end)
}

fn is_valid_link_url(url: &[u8]) -> bool {
    if url.is_empty() || url.len() > ansi::MAX_LINK_URL_BYTES {
        return false;
    }
    !url.iter().any(|&b| b < 0x20 || b == 0x7f)
}

fn is_valid_link_url_with_prefix(prefix: &[u8], url: &[u8]) -> bool {
    if prefix.len() + url.len() > ansi::MAX_LINK_URL_BYTES {
        return false;
    }
    if prefix.iter().any(|&byte| byte < 0x20 || byte == 0x7f) {
        return false;
    }
    is_valid_link_url(url)
}

/// A bare URL may start at the line start, directly after a delimiter run
/// that can open emphasis, or after a non-word byte other than `<`.
fn is_bare_url_boundary(text: &[u8], start: usize, after_opening_delimiter: bool) -> bool {
    if start == 0 || after_opening_delimiter {
        return true;
    }
    let previous = text[start - 1];
    !tu::is_ascii_word_byte(previous) && previous != b'<'
}

fn is_bare_url_terminator(c: u8) -> bool {
    tu::is_ascii_whitespace(c) || c == b')' || c == b']' || c == b'}' || c == b'>'
}

fn is_trailing_bare_url_byte(c: u8) -> bool {
    tu::is_trailing_url_punctuation(c) || c == b'*' || c == b'_' || c == b'~'
}
