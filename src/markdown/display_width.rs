//! Terminal cell widths, measured per display unit (`display_width.zig`).
//!
//! A display unit is what a terminal paints as one glyph: an ASCII byte, a
//! code point, a text- or emoji-presentation variation sequence, or a whole
//! RGI emoji sequence (keycaps, flags, tag sequences, modifier and ZWJ
//! sequences). Every function takes bytes, not `str`, because upstream
//! measures whatever bytes it is handed: an invalid byte is one cell of
//! U+FFFD, never a panic. Measuring a `&str` is `visible_width(s.as_bytes())`.

use super::unicode_display_data as data;

/// A range packed the way upstream packs it (`display_width.zig:4-7`): the
/// first code point in the low 18 bits and the span in the high 14, so a
/// range wider than the span field is split across entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PackedRange(u32);

const FIRST_BITS: u32 = 18;
const MAX_SPAN: u32 = (1 << 14) - 1;

impl PackedRange {
    const fn first(self) -> u32 {
        self.0 & ((1 << FIRST_BITS) - 1)
    }

    const fn span(self) -> u32 {
        self.0 >> FIRST_BITS
    }
}

const fn packed_range_count(source: &[data::Range]) -> usize {
    let mut count = 0;
    let mut i = 0;
    while i < source.len() {
        count += ((source[i].last - source[i].first) / (MAX_SPAN + 1)) as usize + 1;
        i += 1;
    }
    count
}

const fn pack_ranges<const N: usize>(source: &[data::Range]) -> [PackedRange; N] {
    let mut encoded = [PackedRange(0); N];
    let mut index = 0;
    let mut i = 0;
    while i < source.len() {
        let mut first = source[i].first;
        while first <= source[i].last {
            let remaining = source[i].last - first;
            let span = if remaining < MAX_SPAN {
                remaining
            } else {
                MAX_SPAN
            };
            assert!(
                first < (1 << FIRST_BITS),
                "range start does not fit the packed field"
            );
            encoded[index] = PackedRange(first | (span << FIRST_BITS));
            index += 1;
            first += span + 1;
        }
        i += 1;
    }
    assert!(index == N, "packed range count mismatch");
    encoded
}

static WIDE_RANGES: [PackedRange; packed_range_count(&data::WIDE_RANGES)] =
    pack_ranges(&data::WIDE_RANGES);
static EMOJI_PRESENTATION_RANGES: [PackedRange;
    packed_range_count(&data::EMOJI_PRESENTATION_RANGES)] =
    pack_ranges(&data::EMOJI_PRESENTATION_RANGES);
static EMOJI_MODIFIER_RANGES: [PackedRange; packed_range_count(&data::EMOJI_MODIFIER_RANGES)] =
    pack_ranges(&data::EMOJI_MODIFIER_RANGES);
static VARIATION_BASES: [PackedRange; packed_range_count(&data::VARIATION_BASES)] =
    pack_ranges(&data::VARIATION_BASES);

use data::RGI_DATA;

fn rgi_node_edge_start(node: u32) -> usize {
    (node & ((1 << 13) - 1)) as usize
}

fn rgi_node_edge_len(node: u32) -> usize {
    ((node >> 13) & 0xff) as usize
}

fn rgi_node_terminal(node: u32) -> bool {
    node & (1 << 21) != 0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodedRune {
    pub len: usize,
    pub codepoint: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayUnit {
    pub byte_len: usize,
    pub cell_width: usize,
}

/// Cells `text` occupies.
pub fn visible_width(text: &[u8]) -> usize {
    let mut width = 0usize;
    let mut i = 0;
    while i < text.len() {
        let unit = display_unit_at(text, i);
        width = width.saturating_add(unit.cell_width);
        i += unit.byte_len;
    }
    width
}

/// Same as [`visible_width`], but skips ANSI escape sequences so callers can
/// measure the rendered width of text that already carries styling.
pub fn visible_width_ignoring_ansi(text: &[u8]) -> usize {
    let mut width = 0usize;
    let mut i = 0;
    while i < text.len() {
        if text[i] == 0x1b {
            i = ansi_sequence_end(text, i);
            continue;
        }
        let unit = display_unit_at(text, i);
        width = width.saturating_add(unit.cell_width);
        i += unit.byte_len;
    }
    width
}

/// The first variant whose width fits `max_width`, falling back to the last
/// (shortest) so callers always get something to clip.
///
/// # Panics
///
/// When `variants` is empty, as upstream asserts.
pub fn widest_fitting<'a>(variants: &[&'a [u8]], max_width: usize) -> &'a [u8] {
    assert!(
        !variants.is_empty(),
        "widest_fitting needs at least one variant"
    );
    for variant in variants {
        if visible_width(variant) <= max_width {
            return variant;
        }
    }
    variants[variants.len() - 1]
}

pub fn prefix_by_width(text: &[u8], max_width: usize) -> &[u8] {
    if max_width == 0 {
        return &text[..0];
    }
    let mut width = 0;
    let mut i = 0;
    while i < text.len() {
        let unit = display_unit_at(text, i);
        if unit.cell_width > max_width - width {
            break;
        }
        width += unit.cell_width;
        i += unit.byte_len;
    }
    &text[..i]
}

/// Same as [`prefix_by_width`], but skips ANSI escape sequences so they do not
/// eat into the width budget.
pub fn prefix_by_width_ignoring_ansi(text: &[u8], max_width: usize) -> &[u8] {
    if max_width == 0 {
        return &text[..0];
    }
    let mut width = 0;
    let mut i = 0;
    while i < text.len() {
        if text[i] == 0x1b {
            i = ansi_sequence_end(text, i);
            continue;
        }
        let unit = display_unit_at(text, i);
        if unit.cell_width > max_width - width {
            break;
        }
        width += unit.cell_width;
        i += unit.byte_len;
    }
    &text[..i]
}

pub fn suffix_by_width(text: &[u8], max_width: usize) -> &[u8] {
    if max_width == 0 {
        return &text[text.len()..];
    }
    let mut remaining = visible_width(text);
    if remaining <= max_width {
        return text;
    }
    let mut i = 0;
    while i < text.len() && remaining > max_width {
        let unit = display_unit_at(text, i);
        remaining -= unit.cell_width;
        i += unit.byte_len;
    }
    &text[i..]
}

/// Same as [`suffix_by_width`], but ANSI-aware. Walks forward so escape
/// sequences stay attached to the visible rune that follows them; splitting an
/// OSC 8 open/close pair would leak a dangling hyperlink into the terminal.
pub fn suffix_by_width_ignoring_ansi(text: &[u8], max_width: usize) -> &[u8] {
    if max_width == 0 {
        return &text[text.len()..];
    }
    let total = visible_width_ignoring_ansi(text);
    if total <= max_width {
        return text;
    }
    let mut remaining = total;
    let mut i = 0;
    while i < text.len() && remaining > max_width {
        if text[i] == 0x1b {
            i = ansi_sequence_end(text, i);
            continue;
        }
        let unit = display_unit_at(text, i);
        remaining -= unit.cell_width;
        i += unit.byte_len;
    }
    &text[i..]
}

/// The display unit starting at `index` (`display_width.zig:234-263`).
pub fn display_unit_at(text: &[u8], index: usize) -> DisplayUnit {
    if index >= text.len() {
        return DisplayUnit {
            byte_len: 0,
            cell_width: 0,
        };
    }

    let first_byte = text[index];
    let may_start_keycap =
        (first_byte == b'#' || first_byte == b'*' || first_byte.is_ascii_digit())
            && has_rgi_keycap_suffix(text, index + 1);
    if first_byte < 0x80 && !may_start_keycap {
        let cell_width = if first_byte < 32 || first_byte == 0x7f {
            0
        } else {
            1
        };
        return DisplayUnit {
            byte_len: 1,
            cell_width,
        };
    }

    let rgi_len = match_rgi_sequence(text, index);
    if rgi_len != 0 {
        return DisplayUnit {
            byte_len: rgi_len,
            cell_width: 2,
        };
    }

    let first = decode_next_rune(text, index);
    let next_index = index + first.len;
    if next_index < text.len() && is_in_ranges(first.codepoint, &VARIATION_BASES) {
        let selector = decode_next_rune(text, next_index);
        if selector.codepoint == 0xfe0e {
            let cell_width = if is_in_ranges(first.codepoint, &WIDE_RANGES) {
                2
            } else {
                1
            };
            return DisplayUnit {
                byte_len: first.len + selector.len,
                cell_width,
            };
        }
        if selector.codepoint == 0xfe0f {
            return DisplayUnit {
                byte_len: first.len + selector.len,
                cell_width: 2,
            };
        }
    }

    DisplayUnit {
        byte_len: first.len,
        cell_width: rune_width(first.codepoint),
    }
}

fn has_rgi_keycap_suffix(text: &[u8], suffix_start: usize) -> bool {
    const VARIATION_SELECTOR_16: &[u8] = "\u{fe0f}".as_bytes();
    const COMBINING_KEYCAP: &[u8] = "\u{20e3}".as_bytes();
    let mut index = suffix_start;
    if text[index..].starts_with(VARIATION_SELECTOR_16) {
        index += VARIATION_SELECTOR_16.len();
    }
    text[index..].starts_with(COMBINING_KEYCAP)
}

fn rune_width(codepoint: u32) -> usize {
    if codepoint == 0 {
        return 0;
    }
    if codepoint < 32 || (0x7f..0xa0).contains(&codepoint) {
        return 0;
    }
    if is_zero_width_continuation(codepoint) {
        return 0;
    }
    if is_in_ranges(codepoint, &WIDE_RANGES) || is_in_ranges(codepoint, &EMOJI_PRESENTATION_RANGES)
    {
        return 2;
    }
    1
}

/// Whether width `w` at one-based `col` overflows `cols` with autowrap off.
/// Requires non-zero inputs; the arithmetic is widened so it cannot overflow.
pub fn should_wrap_at(col: u16, w: u16, cols: u16) -> bool {
    u32::from(col) + u32::from(w) - 1 > u32::from(cols)
}

/// The next fixed eight-column tab stop, clamped to the right margin.
/// Columns are one-based.
///
/// # Panics
///
/// When `col` or `cols` is zero, as upstream asserts.
pub fn next_tab_stop_column(col: u16, cols: u16) -> u16 {
    assert!(col > 0, "columns are one-based");
    assert!(cols > 0, "columns are one-based");
    if col >= cols {
        return cols;
    }
    let zero_based = u32::from(col) - 1;
    let next_stop = ((zero_based / 8) + 1) * 8 + 1;
    // `next_stop` is clamped to `cols`, which is a `u16`.
    next_stop.min(u32::from(cols)) as u16
}

/// The code point at `index`; an invalid or truncated sequence is one byte of
/// U+FFFD.
pub fn decode_next_rune(text: &[u8], index: usize) -> DecodedRune {
    const REPLACEMENT: DecodedRune = DecodedRune {
        len: 1,
        codepoint: 0xfffd,
    };
    if index >= text.len() {
        return DecodedRune {
            len: 0,
            codepoint: 0,
        };
    }
    let first = text[index];
    if first < 0x80 {
        return DecodedRune {
            len: 1,
            codepoint: u32::from(first),
        };
    }
    // `std.unicode.utf8ByteSequenceLength`.
    let len = match first {
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf7 => 4,
        _ => return REPLACEMENT,
    };
    if index + len > text.len() {
        return REPLACEMENT;
    }
    let Ok(decoded) = std::str::from_utf8(&text[index..index + len]) else {
        return REPLACEMENT;
    };
    match decoded.chars().next() {
        Some(c) => DecodedRune {
            len,
            codepoint: u32::from(c),
        },
        None => REPLACEMENT,
    }
}

/// The index just past the escape sequence starting at `index`.
pub fn ansi_sequence_end(text: &[u8], index: usize) -> usize {
    if index >= text.len() || text[index] != 0x1b {
        return index;
    }
    if index + 1 >= text.len() {
        return text.len();
    }

    let kind = text[index + 1];
    if kind == b'[' {
        for (i, &b) in text.iter().enumerate().skip(index + 2) {
            if (b'@'..=b'~').contains(&b) {
                return i + 1;
            }
        }
        return text.len();
    }

    if kind == b']' {
        let mut i = index + 2;
        while i < text.len() {
            if text[i] == 0x07 {
                return i + 1;
            }
            if text[i] == 0x1b && i + 1 < text.len() && text[i + 1] == b'\\' {
                return i + 2;
            }
            i += 1;
        }
        return text.len();
    }

    (index + 2).min(text.len())
}

pub fn previous_rune_start(text: &[u8], end: usize) -> usize {
    if end == 0 {
        return 0;
    }
    let mut i = end - 1;
    while i > 0 && (text[i] & 0b1100_0000) == 0b1000_0000 {
        i -= 1;
    }
    i
}

/// The byte length of a leading status glyph and its space, or zero.
pub fn status_prefix_end(label: &[u8]) -> usize {
    if label.is_empty() {
        return 0;
    }
    let first = display_unit_at(label, 0);
    if first.byte_len == 0 || first.byte_len >= label.len() {
        return 0;
    }
    if label[first.byte_len] == b' ' {
        first.byte_len + 1
    } else {
        0
    }
}

/// The longest prefix within `max_width`, cut at its last space when it has
/// one past the start.
pub fn wrap_cut_ignoring_ansi(text: &[u8], max_width: usize) -> &[u8] {
    let prefix = prefix_by_width_ignoring_ansi(text, max_width);
    if prefix.len() == text.len() {
        return prefix;
    }

    let mut last_space = None;
    let mut index = 0;
    while index < prefix.len() {
        // A space inside an escape, such as an OSC 8 URL, is not a break.
        if prefix[index] == 0x1b {
            index = ansi_sequence_end(prefix, index);
            continue;
        }
        let unit = display_unit_at(prefix, index);
        if prefix[index] == b' ' || prefix[index] == b'\t' {
            last_space = Some(index);
        }
        index += unit.byte_len;
    }
    if let Some(space_index) = last_space {
        if space_index > 0 {
            return &text[..space_index];
        }
    }
    prefix
}

pub fn trim_break_whitespace(text: &[u8]) -> &[u8] {
    let mut index = 0;
    while index < text.len() && (text[index] == b' ' || text[index] == b'\t') {
        index += 1;
    }
    &text[index..]
}

fn is_combining(codepoint: u32) -> bool {
    (0x0300..=0x036f).contains(&codepoint)
        || (0x1ab0..=0x1aff).contains(&codepoint)
        || (0x1dc0..=0x1dff).contains(&codepoint)
        || (0x20d0..=0x20ff).contains(&codepoint)
        || (0xfe20..=0xfe2f).contains(&codepoint)
        || (0xfe00..=0xfe0f).contains(&codepoint)
}

fn is_zero_width_continuation(codepoint: u32) -> bool {
    is_combining(codepoint)
        || is_in_ranges(codepoint, &EMOJI_MODIFIER_RANGES)
        || codepoint == 0x200c
        || codepoint == 0x200d
        || (0x200b..=0x200f).contains(&codepoint)
        || (0x202a..=0x202e).contains(&codepoint)
        || (0x2060..=0x206f).contains(&codepoint)
        || codepoint == 0xfeff
        || (0xe0001..=0xe007f).contains(&codepoint)
        || (0xe0100..=0xe01ef).contains(&codepoint)
}

fn is_in_ranges(codepoint: u32, ranges: &[PackedRange]) -> bool {
    let mut low = 0;
    let mut high = ranges.len();
    while low < high {
        let middle = low + (high - low) / 2;
        let range = ranges[middle];
        if codepoint < range.first() {
            high = middle;
        } else if codepoint > range.first() + range.span() {
            low = middle + 1;
        } else {
            return true;
        }
    }
    false
}

fn match_rgi_sequence(text: &[u8], index: usize) -> usize {
    let mut node_index = 0usize;
    let mut cursor = index;
    let mut longest = 0;
    let mut depth = 0;

    while cursor < text.len() && depth < data::MAX_RGI_SEQUENCE_CODEPOINTS {
        let rune = decode_next_rune(text, cursor);
        let Some(child) = find_trie_child(node_index, rune.codepoint) else {
            break;
        };
        node_index = child;
        cursor += rune.len;
        if rgi_node_terminal(RGI_DATA.nodes[node_index]) {
            longest = cursor - index;
        }
        depth += 1;
    }

    longest
}

fn find_trie_child(node_index: usize, codepoint: u32) -> Option<usize> {
    let node = RGI_DATA.nodes[node_index];
    let edge_start = rgi_node_edge_start(node);
    let mut low = edge_start;
    let mut high = edge_start + rgi_node_edge_len(node);
    while low < high {
        let middle = low + (high - low) / 2;
        let edge_codepoint = RGI_DATA.codepoints[usize::from(RGI_DATA.edge_codepoint_ids[middle])];
        if codepoint < edge_codepoint {
            high = middle;
        } else if codepoint > edge_codepoint {
            low = middle + 1;
        } else {
            return Some(middle + 1);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_in_source_ranges(codepoint: u32, ranges: &[data::Range]) -> bool {
        let mut low = 0;
        let mut high = ranges.len();
        while low < high {
            let middle = low + (high - low) / 2;
            let range = ranges[middle];
            if codepoint < range.first {
                high = middle;
            } else if codepoint > range.last {
                low = middle + 1;
            } else {
                return true;
            }
        }
        false
    }

    fn b(text: &str) -> &[u8] {
        text.as_bytes()
    }

    #[test]
    fn rgi_runtime_lookup_data_stays_within_the_measured_size_budget() {
        assert!(std::mem::size_of::<data::RgiData>() <= 22_272);
    }

    #[test]
    fn packed_display_ranges_preserve_every_scalar_classification() {
        assert_eq!(std::mem::size_of::<PackedRange>(), 4);
        for codepoint in 0..0x110000u32 {
            assert_eq!(
                is_in_source_ranges(codepoint, &data::WIDE_RANGES),
                is_in_ranges(codepoint, &WIDE_RANGES)
            );
            assert_eq!(
                is_in_source_ranges(codepoint, &data::EMOJI_PRESENTATION_RANGES),
                is_in_ranges(codepoint, &EMOJI_PRESENTATION_RANGES)
            );
            assert_eq!(
                is_in_source_ranges(codepoint, &data::EMOJI_MODIFIER_RANGES),
                is_in_ranges(codepoint, &EMOJI_MODIFIER_RANGES)
            );
            assert_eq!(
                is_in_source_ranges(codepoint, &data::VARIATION_BASES),
                is_in_ranges(codepoint, &VARIATION_BASES)
            );
        }
    }

    #[test]
    fn prefix_by_width_avoids_cutting_emoji_bytes() {
        let text = b("a\u{1F600}b");
        assert_eq!(prefix_by_width(text, 2), b"a");
        assert_eq!(prefix_by_width(text, 3), b("a\u{1F600}"));
    }

    #[test]
    fn suffix_by_width_keeps_whole_runes() {
        assert_eq!(suffix_by_width(b("ab\u{1F600}cd"), 4), b("\u{1F600}cd"));
    }

    #[test]
    fn visible_width_counts_emoji_as_two_cells() {
        assert_eq!(visible_width(b("a\u{1F600}b")), 4);
    }

    #[test]
    fn visible_width_ignoring_ansi_skips_csi_sequences() {
        assert_eq!(visible_width_ignoring_ansi(b"\x1b[1mhello\x1b[22m"), 5);
        assert_eq!(visible_width_ignoring_ansi(b"\x1b[48;5;236mfoo\x1b[49m"), 3);
        assert_eq!(visible_width_ignoring_ansi(b"\x1b[1mEnfoque\x1b[22m"), 7);
    }

    #[test]
    fn should_wrap_at_is_strictly_overflow_based() {
        assert!(!should_wrap_at(10, 1, 10));
        assert!(should_wrap_at(10, 2, 10));
        assert!(!should_wrap_at(1, 1, 1));

        // Start at c = 2 so c - 1 stays >= 1; cols = 0 is excluded by the
        // helper precondition and guarded by current callers.
        for c in 2..=256u16 {
            assert!(!should_wrap_at(c, 1, c));
            assert!(should_wrap_at(c, 1, c - 1));
        }

        assert!(should_wrap_at(65535, 2, 65535));
    }

    #[test]
    fn next_tab_stop_column_uses_absolute_stops_and_clamps_safely() {
        assert_eq!(next_tab_stop_column(1, 80), 9);
        assert_eq!(next_tab_stop_column(8, 80), 9);
        assert_eq!(next_tab_stop_column(9, 80), 17);
        assert_eq!(next_tab_stop_column(1, 8), 8);
        assert_eq!(next_tab_stop_column(10, 10), 10);
        assert_eq!(next_tab_stop_column(11, 10), 10);
        assert_eq!(next_tab_stop_column(65535, 65535), 65535);
    }

    #[test]
    fn prefix_by_width_ignoring_ansi_keeps_opening_osc_8_attached_to_visible_bytes() {
        // OSC-8 open does not consume any visible cells; the first two bytes
        // of the hyperlink body do. Budget of 2 keeps the open + 2 cells.
        let text = b"\x1b]8;;file:///x\x1b\\ab\x1b]8;;\x1b\\";
        let out = prefix_by_width_ignoring_ansi(text, 2);
        assert_eq!(visible_width_ignoring_ansi(out), 2);
        let open = b"\x1b]8;;file:///x\x1b\\";
        assert!(out.windows(open.len()).any(|window| window == open));
    }

    #[test]
    fn prefix_by_width_ignoring_ansi_returns_empty_on_zero_budget() {
        assert_eq!(
            prefix_by_width_ignoring_ansi(b"\x1b[31mhello\x1b[0m", 0),
            b""
        );
    }

    #[test]
    fn suffix_by_width_ignoring_ansi_tolerates_ansi_around_multibyte_runes() {
        let text = b("a\x1b]8;;file:///\u{a0}.png\x1b\\[Image 1]\x1b]8;;\x1b\\");
        let out = suffix_by_width_ignoring_ansi(text, 9);
        assert_eq!(visible_width_ignoring_ansi(out), 9);
    }

    #[test]
    fn suffix_by_width_ignoring_ansi_returns_full_text_when_it_fits() {
        let text = b"\x1b[31mhi\x1b[0m";
        assert_eq!(suffix_by_width_ignoring_ansi(text, 5), text);
    }

    #[test]
    fn decode_next_rune_advances_through_invalid_utf_8() {
        let text = b"\xf0\x28\x8c\x28";
        let first = decode_next_rune(text, 0);
        assert_eq!(first.len, 1);
        assert_eq!(first.codepoint, 0xfffd);

        let second = decode_next_rune(text, first.len);
        assert_eq!(second.len, 1);
        assert_eq!(second.codepoint, u32::from(b'('));
    }

    #[test]
    fn ansi_sequence_end_handles_osc_terminated_by_esc_backslash() {
        let text = b"\x1b]8;;file:///x\x1b\\body";
        assert_eq!(ansi_sequence_end(text, 0), 16);
        assert_eq!(visible_width_ignoring_ansi(text), 4);
    }

    #[test]
    fn rune_width_preserves_current_width_table_semantics() {
        assert_eq!(rune_width(0), 0);
        assert_eq!(rune_width(0x07), 0);
        assert_eq!(rune_width(0x1b), 0);
        assert_eq!(rune_width(0x80), 0);
        assert_eq!(rune_width(0x9f), 0);
        assert_eq!(rune_width(0x0301), 0);
        assert_eq!(rune_width(0x1ab0), 0);
        assert_eq!(rune_width(0x4e00), 2);
        assert_eq!(rune_width(0x1f600), 2);
        assert_eq!(rune_width(0xff66), 1);
        assert_eq!(rune_width(u32::from(b'a')), 1);
        assert_eq!(rune_width(u32::from(b' ')), 1);
        assert_eq!(rune_width(0x09), 0);
    }

    #[test]
    fn decode_next_rune_preserves_boundary_and_invalid_byte_behavior() {
        let empty = DecodedRune {
            len: 0,
            codepoint: 0,
        };
        let replacement = DecodedRune {
            len: 1,
            codepoint: 0xfffd,
        };
        assert_eq!(decode_next_rune(b"", 0), empty);
        assert_eq!(decode_next_rune(b"a", 1), empty);
        assert_eq!(decode_next_rune(b"\xff", 0), replacement);
        assert_eq!(decode_next_rune(b"\xf0", 0), replacement);
        assert_eq!(decode_next_rune(b"\xc0\x80", 0), replacement);
    }

    #[test]
    fn ansi_sequence_end_preserves_current_csi_osc_and_esc_boundaries() {
        assert_eq!(ansi_sequence_end(b"\x1b[m", 0), 3);
        assert_eq!(ansi_sequence_end(b"\x1b[1;31m", 0), 7);
        assert_eq!(ansi_sequence_end(b"\x1b[K", 0), 3);

        let unterminated_csi = b"\x1b[";
        assert_eq!(
            ansi_sequence_end(unterminated_csi, 0),
            unterminated_csi.len()
        );

        assert_eq!(ansi_sequence_end(b"\x1b]8;;url\x07", 0), 9);
        assert_eq!(ansi_sequence_end(b"\x1b]8;;url\x1b\\", 0), 10);

        let unterminated_osc = b"\x1b]8;;url";
        assert_eq!(
            ansi_sequence_end(unterminated_osc, 0),
            unterminated_osc.len()
        );

        assert_eq!(ansi_sequence_end(b"\x1bA", 0), 2);

        let bare_escape = b"\x1b";
        assert_eq!(ansi_sequence_end(bare_escape, 0), bare_escape.len());
    }

    #[test]
    fn previous_rune_start_preserves_byte_boundary_behavior() {
        assert_eq!(previous_rune_start(b"hello", 5), 4);
        assert_eq!(previous_rune_start(b("a\u{1F600}b"), 5), 1);
        assert_eq!(previous_rune_start(b"", 0), 0);
        assert_eq!(previous_rune_start(b"a", 1), 0);
    }

    #[test]
    fn prefix_by_width_and_suffix_by_width_preserve_boundary_behavior() {
        assert_eq!(prefix_by_width(b"ab", 0), b"");
        assert_eq!(prefix_by_width(b"ab", 1), b"a");
        assert_eq!(prefix_by_width(b"ab", 2), b"ab");
        assert_eq!(prefix_by_width(b"ab", 100), b"ab");

        let suffix_zero = suffix_by_width(b"ab", 0);
        assert_eq!(suffix_zero.len(), 0);
        assert_eq!(suffix_by_width(b"ab", 1), b"b");
        assert_eq!(suffix_by_width(b("a\u{1F600}b"), 3), b("\u{1F600}b"));
    }

    #[test]
    fn should_wrap_at_preserves_documented_edge_behavior() {
        assert!(should_wrap_at(1, 2, 1));
        assert!(!should_wrap_at(1, 1, 80));
        assert!(!should_wrap_at(80, 1, 80));
        assert!(should_wrap_at(80, 2, 80));
    }

    #[test]
    fn status_prefix_end_detects_a_leading_status_rune_and_space() {
        assert_eq!(status_prefix_end(b("\u{25B2} error")), 4);
        assert_eq!(status_prefix_end(b"x error"), 2);
        assert_eq!(status_prefix_end(b"error text"), 0);
        assert_eq!(status_prefix_end(b("\u{25B2}")), 0);
        assert_eq!(status_prefix_end(b""), 0);
    }

    #[test]
    fn wrap_cut_ignoring_ansi_prefers_the_last_space_inside_the_budget() {
        assert_eq!(wrap_cut_ignoring_ansi(b"aaaa bbbb cccc", 10), b"aaaa bbbb");
        assert_eq!(wrap_cut_ignoring_ansi(b"aaaaabbbbbcc", 5), b"aaaaa");
        assert_eq!(wrap_cut_ignoring_ansi(b" abcd", 2), b" a");
        assert_eq!(
            wrap_cut_ignoring_ansi(b"aaaa bbbb cccc", 20),
            b"aaaa bbbb cccc"
        );
        assert_eq!(
            wrap_cut_ignoring_ansi(b"\x1b[31maaaa bbbb\x1b[0m", 6),
            b"\x1b[31maaaa"
        );
        assert_eq!(
            wrap_cut_ignoring_ansi(b"see \x1b]8;id=1;https://x.io/a b\x1b\\docs more", 8),
            b"see"
        );
    }

    #[test]
    fn trim_break_whitespace_strips_leading_spaces_and_tabs() {
        assert_eq!(trim_break_whitespace(b" \t abc"), b"abc");
        assert_eq!(trim_break_whitespace(b"abc "), b"abc ");
        assert_eq!(trim_break_whitespace(b"  "), b"");
    }

    #[test]
    fn widest_fitting_picks_the_first_variant_that_fits_and_falls_back_to_the_last() {
        let variants: [&[u8]; 3] = [b"long variant here", b"medium one", b"short"];
        assert_eq!(widest_fitting(&variants, 40), b"long variant here");
        assert_eq!(widest_fitting(&variants, 17), b"long variant here");
        assert_eq!(widest_fitting(&variants, 16), b"medium one");
        assert_eq!(widest_fitting(&variants, 9), b"short");
        assert_eq!(widest_fitting(&variants, 2), b"short");
        let wide: [&[u8]; 2] = [b("\u{6F22}\u{6F22}\u{6F22}"), b("\u{6F22}")];
        assert_eq!(widest_fitting(&wide, 5), b("\u{6F22}"));
    }

    #[test]
    fn display_unit_widths_cover_presentation_and_rgi_sequences() {
        let cases: [(&[u8], usize); 15] = [
            (b"A", 1),
            (b("\u{754C}"), 2),
            (b("a\u{0301}"), 1),
            (b"\xff", 1),
            (b("\u{2705}"), 2),
            (b("\u{274C}"), 2),
            (b("\u{2600}\u{FE0E}"), 1),
            (b("\u{231A}\u{FE0E}"), 2),
            (b("\u{26A1}\u{FE0E}"), 2),
            (b("\u{2600}\u{FE0F}"), 2),
            (b("\u{1F44D}\u{1F3FD}"), 2),
            (b("\u{1F1FA}\u{1F1F8}"), 2),
            (b("#\u{FE0F}\u{20E3}"), 2),
            (
                b("\u{1F3F4}\u{E0067}\u{E0062}\u{E0065}\u{E006E}\u{E0067}\u{E007F}"),
                2,
            ),
            (b("\u{1F469}\u{200D}\u{1F4BB}"), 2),
        ];
        for (text, width) in cases {
            assert_eq!(visible_width(text), width, "{text:?}");
        }
    }

    #[test]
    fn display_unit_scanner_returns_exact_sequence_byte_spans() {
        let cases: [(&[u8], usize); 13] = [
            (b"A", 1),
            (b("\u{754C}"), 2),
            (b"\xff", 1),
            (b("\u{2705}"), 2),
            (b("\u{2600}\u{FE0E}"), 1),
            (b("\u{231A}\u{FE0E}"), 2),
            (b("\u{26A1}\u{FE0E}"), 2),
            (b("\u{2600}\u{FE0F}"), 2),
            (b("\u{1F44D}\u{1F3FD}"), 2),
            (b("\u{1F1FA}\u{1F1F8}"), 2),
            (b("#\u{FE0F}\u{20E3}"), 2),
            (
                b("\u{1F3F4}\u{E0067}\u{E0062}\u{E0065}\u{E006E}\u{E0067}\u{E007F}"),
                2,
            ),
            (b("\u{1F469}\u{200D}\u{1F4BB}"), 2),
        ];
        for (text, cell_width) in cases {
            assert_eq!(
                display_unit_at(text, 0),
                DisplayUnit {
                    byte_len: text.len(),
                    cell_width
                },
                "{text:?}"
            );
        }

        assert_eq!(
            display_unit_at(b"", 0),
            DisplayUnit {
                byte_len: 0,
                cell_width: 0
            }
        );
    }

    #[test]
    fn display_unit_scanner_rejects_sequence_lookalikes() {
        let stray_selector = b("A\u{FE0F}");
        assert_eq!(
            display_unit_at(stray_selector, 0),
            DisplayUnit {
                byte_len: 1,
                cell_width: 1
            }
        );
        assert_eq!(visible_width(stray_selector), 1);

        let invalid_flag = b("\u{1F1E6}\u{1F1E6}");
        assert_eq!(visible_width(invalid_flag), 4);
        assert_eq!(display_unit_at(invalid_flag, 0).byte_len, 4);

        let incomplete_keycap = b("#\u{20E3}");
        assert_eq!(display_unit_at(incomplete_keycap, 0).byte_len, 1);
        assert_eq!(visible_width(incomplete_keycap), 1);

        let incomplete_tag = b("\u{1F3F4}\u{E0067}\u{E0062}\u{E0065}\u{E006E}\u{E0067}");
        assert_eq!(display_unit_at(incomplete_tag, 0).byte_len, 4);

        let invalid_zwj = b("\u{1F469}\u{200D}A");
        assert_eq!(display_unit_at(invalid_zwj, 0).byte_len, 4);
        assert_eq!(visible_width(invalid_zwj), 3);
    }

    #[test]
    fn ordinary_ascii_keycap_candidates_stay_on_the_single_byte_path() {
        let text = b"0123456789#*";
        for index in 0..text.len() {
            assert_eq!(
                display_unit_at(text, index),
                DisplayUnit {
                    byte_len: 1,
                    cell_width: 1
                }
            );
        }
    }

    #[test]
    fn display_unit_clipping_preserves_sequence_boundaries() {
        let sequence = "\u{1F44D}\u{1F3FD}";
        let text = format!("a{sequence}b");
        let text = text.as_bytes();
        assert_eq!(prefix_by_width(text, 2), b"a");
        assert_eq!(prefix_by_width(text, 3), format!("a{sequence}").as_bytes());
        assert_eq!(suffix_by_width(text, 1), b"b");
        assert_eq!(suffix_by_width(text, 3), format!("{sequence}b").as_bytes());

        let styled = format!("\x1b[31m{sequence}\x1b[0m");
        let styled = styled.as_bytes();
        assert_eq!(prefix_by_width_ignoring_ansi(styled, 2), styled);
        assert_eq!(suffix_by_width_ignoring_ansi(styled, 2), styled);
        assert_eq!(
            status_prefix_end(format!("{sequence} active").as_bytes()),
            sequence.len() + 1
        );
    }

    /// Upstream fuzzes this property; without a fuzzer its test runs the
    /// corpus, which is what this does.
    #[test]
    fn fuzz_display_unit_boundaries() {
        let corpus: [&[u8]; 6] = [
            b"",
            b"plain text",
            b"\xff\xf0\x28\x8c\x28",
            b("\u{2600}\u{FE0E}\u{2600}\u{FE0F}"),
            b("\u{1F44D}\u{1F3FD}\u{1F1FA}\u{1F1F8}"),
            b("#\u{FE0F}\u{20E3}\u{1F469}\u{200D}\u{1F4BB}"),
        ];
        for text in corpus {
            check_display_unit_boundaries(text);
        }
    }

    fn check_display_unit_boundaries(text: &[u8]) {
        let mut expected_width = 0usize;
        let mut index = 0;
        while index < text.len() {
            let unit = display_unit_at(text, index);
            assert!(unit.byte_len > 0);
            assert!(unit.byte_len <= text.len() - index);
            assert!(unit.cell_width <= 2);
            assert_eq!(unit, display_unit_at(text, index));
            expected_width = expected_width.saturating_add(unit.cell_width);
            index += unit.byte_len;
        }
        assert_eq!(text.len(), index);
        assert_eq!(expected_width, visible_width(text));

        let budgets = [
            0,
            (expected_width / 2).min(16),
            expected_width.saturating_add(2).min(16),
        ];
        for budget in budgets {
            let prefix = prefix_by_width(text, budget);
            let suffix = suffix_by_width(text, budget);
            assert!(is_display_unit_boundary(text, prefix.len()));
            assert!(is_display_unit_boundary(text, text.len() - suffix.len()));
            assert!(visible_width(prefix) <= budget);
            assert!(visible_width(suffix) <= budget);
        }
    }

    fn is_display_unit_boundary(text: &[u8], target: usize) -> bool {
        if target > text.len() {
            return false;
        }
        let mut index = 0;
        while index < target {
            let unit = display_unit_at(text, index);
            if unit.byte_len == 0 || unit.byte_len > target - index {
                return false;
            }
            index += unit.byte_len;
        }
        index == target
    }
}
