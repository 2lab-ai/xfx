//! Unicode 17.0 emphasis flanking classes (`unicode_classes.zig`).
//!
//! Generated upstream from:
//! https://www.unicode.org/Public/17.0.0/ucd/UnicodeData.txt
//! Source SHA-256: 2e1efc1dcb59c575eedf5ccae60f95229f706ee6d031835247d843c11d96470c
//!
//! Selected properties: general categories P* and S* (punctuation and
//! symbols) and Zs (space separators), which CommonMark uses to decide
//! whether a delimiter run is left or right flanking. Code points below
//! U+0080 are classified directly. The range tables below are transcribed
//! verbatim from upstream at vercel-labs/fx `c1db9191` and packed at compile
//! time, as upstream packs them.
//!
//! Unicode data is used under https://www.unicode.org/license.txt.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Range {
    first: u32,
    last: u32,
}

/// The first code point in the low 21 bits and the span in the high 11
/// (`unicode_classes.zig:19-22`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PackedRange(u32);

const FIRST_BITS: u32 = 21;

impl PackedRange {
    const fn first(self) -> u32 {
        self.0 & ((1 << FIRST_BITS) - 1)
    }

    const fn span(self) -> u32 {
        self.0 >> FIRST_BITS
    }
}

/// CommonMark punctuation: any code point in general category P* or S*.
pub(super) fn is_punctuation_or_symbol(cp: u32) -> bool {
    if cp < 0x80 {
        return cp > 0x20 && cp < 0x7f && !(cp as u8).is_ascii_alphanumeric();
    }
    contains_codepoint(&PUNCTUATION_RANGES, cp)
}

/// CommonMark whitespace: general category Zs plus tab, line feed, form
/// feed, and carriage return.
pub(super) fn is_whitespace(cp: u32) -> bool {
    if cp < 0x80 {
        return cp == u32::from(b' ')
            || cp == u32::from(b'\t')
            || cp == u32::from(b'\n')
            || cp == 0x0c
            || cp == u32::from(b'\r');
    }
    contains_codepoint(&SPACE_RANGES, cp)
}

fn contains_codepoint(ranges: &[PackedRange], cp: u32) -> bool {
    let mut lo = 0;
    let mut hi = ranges.len();
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let range = ranges[mid];
        if cp < range.first() {
            hi = mid;
        } else if cp > range.first() + range.span() {
            lo = mid + 1;
        } else {
            return true;
        }
    }
    false
}

const fn pack_ranges<const N: usize>(source: &[Range; N]) -> [PackedRange; N] {
    let mut encoded = [PackedRange(0); N];
    let mut index = 0;
    while index < N {
        let range = source[index];
        let span = range.last - range.first;
        assert!(range.first < (1 << FIRST_BITS), "range start does not fit");
        assert!(span < (1 << (32 - FIRST_BITS)), "range span does not fit");
        encoded[index] = PackedRange(range.first | (span << FIRST_BITS));
        index += 1;
    }
    encoded
}

static SPACE_RANGES: [PackedRange; SOURCE_SPACE_RANGES.len()] = pack_ranges(&SOURCE_SPACE_RANGES);
static PUNCTUATION_RANGES: [PackedRange; SOURCE_PUNCTUATION_RANGES.len()] =
    pack_ranges(&SOURCE_PUNCTUATION_RANGES);

#[rustfmt::skip]
const SOURCE_SPACE_RANGES: [Range; 6] = [
    Range { first: 0x00A0, last: 0x00A0 },
    Range { first: 0x1680, last: 0x1680 },
    Range { first: 0x2000, last: 0x200A },
    Range { first: 0x202F, last: 0x202F },
    Range { first: 0x205F, last: 0x205F },
    Range { first: 0x3000, last: 0x3000 },
];

#[rustfmt::skip]
const SOURCE_PUNCTUATION_RANGES: [Range; 351] = [
    Range { first: 0x00A1, last: 0x00A9 },
    Range { first: 0x00AB, last: 0x00AC },
    Range { first: 0x00AE, last: 0x00B1 },
    Range { first: 0x00B4, last: 0x00B4 },
    Range { first: 0x00B6, last: 0x00B8 },
    Range { first: 0x00BB, last: 0x00BB },
    Range { first: 0x00BF, last: 0x00BF },
    Range { first: 0x00D7, last: 0x00D7 },
    Range { first: 0x00F7, last: 0x00F7 },
    Range { first: 0x02C2, last: 0x02C5 },
    Range { first: 0x02D2, last: 0x02DF },
    Range { first: 0x02E5, last: 0x02EB },
    Range { first: 0x02ED, last: 0x02ED },
    Range { first: 0x02EF, last: 0x02FF },
    Range { first: 0x0375, last: 0x0375 },
    Range { first: 0x037E, last: 0x037E },
    Range { first: 0x0384, last: 0x0385 },
    Range { first: 0x0387, last: 0x0387 },
    Range { first: 0x03F6, last: 0x03F6 },
    Range { first: 0x0482, last: 0x0482 },
    Range { first: 0x055A, last: 0x055F },
    Range { first: 0x0589, last: 0x058A },
    Range { first: 0x058D, last: 0x058F },
    Range { first: 0x05BE, last: 0x05BE },
    Range { first: 0x05C0, last: 0x05C0 },
    Range { first: 0x05C3, last: 0x05C3 },
    Range { first: 0x05C6, last: 0x05C6 },
    Range { first: 0x05F3, last: 0x05F4 },
    Range { first: 0x0606, last: 0x060F },
    Range { first: 0x061B, last: 0x061B },
    Range { first: 0x061D, last: 0x061F },
    Range { first: 0x066A, last: 0x066D },
    Range { first: 0x06D4, last: 0x06D4 },
    Range { first: 0x06DE, last: 0x06DE },
    Range { first: 0x06E9, last: 0x06E9 },
    Range { first: 0x06FD, last: 0x06FE },
    Range { first: 0x0700, last: 0x070D },
    Range { first: 0x07F6, last: 0x07F9 },
    Range { first: 0x07FE, last: 0x07FF },
    Range { first: 0x0830, last: 0x083E },
    Range { first: 0x085E, last: 0x085E },
    Range { first: 0x0888, last: 0x0888 },
    Range { first: 0x0964, last: 0x0965 },
    Range { first: 0x0970, last: 0x0970 },
    Range { first: 0x09F2, last: 0x09F3 },
    Range { first: 0x09FA, last: 0x09FB },
    Range { first: 0x09FD, last: 0x09FD },
    Range { first: 0x0A76, last: 0x0A76 },
    Range { first: 0x0AF0, last: 0x0AF1 },
    Range { first: 0x0B70, last: 0x0B70 },
    Range { first: 0x0BF3, last: 0x0BFA },
    Range { first: 0x0C77, last: 0x0C77 },
    Range { first: 0x0C7F, last: 0x0C7F },
    Range { first: 0x0C84, last: 0x0C84 },
    Range { first: 0x0D4F, last: 0x0D4F },
    Range { first: 0x0D79, last: 0x0D79 },
    Range { first: 0x0DF4, last: 0x0DF4 },
    Range { first: 0x0E3F, last: 0x0E3F },
    Range { first: 0x0E4F, last: 0x0E4F },
    Range { first: 0x0E5A, last: 0x0E5B },
    Range { first: 0x0F01, last: 0x0F17 },
    Range { first: 0x0F1A, last: 0x0F1F },
    Range { first: 0x0F34, last: 0x0F34 },
    Range { first: 0x0F36, last: 0x0F36 },
    Range { first: 0x0F38, last: 0x0F38 },
    Range { first: 0x0F3A, last: 0x0F3D },
    Range { first: 0x0F85, last: 0x0F85 },
    Range { first: 0x0FBE, last: 0x0FC5 },
    Range { first: 0x0FC7, last: 0x0FCC },
    Range { first: 0x0FCE, last: 0x0FDA },
    Range { first: 0x104A, last: 0x104F },
    Range { first: 0x109E, last: 0x109F },
    Range { first: 0x10FB, last: 0x10FB },
    Range { first: 0x1360, last: 0x1368 },
    Range { first: 0x1390, last: 0x1399 },
    Range { first: 0x1400, last: 0x1400 },
    Range { first: 0x166D, last: 0x166E },
    Range { first: 0x169B, last: 0x169C },
    Range { first: 0x16EB, last: 0x16ED },
    Range { first: 0x1735, last: 0x1736 },
    Range { first: 0x17D4, last: 0x17D6 },
    Range { first: 0x17D8, last: 0x17DB },
    Range { first: 0x1800, last: 0x180A },
    Range { first: 0x1940, last: 0x1940 },
    Range { first: 0x1944, last: 0x1945 },
    Range { first: 0x19DE, last: 0x19FF },
    Range { first: 0x1A1E, last: 0x1A1F },
    Range { first: 0x1AA0, last: 0x1AA6 },
    Range { first: 0x1AA8, last: 0x1AAD },
    Range { first: 0x1B4E, last: 0x1B4F },
    Range { first: 0x1B5A, last: 0x1B6A },
    Range { first: 0x1B74, last: 0x1B7F },
    Range { first: 0x1BFC, last: 0x1BFF },
    Range { first: 0x1C3B, last: 0x1C3F },
    Range { first: 0x1C7E, last: 0x1C7F },
    Range { first: 0x1CC0, last: 0x1CC7 },
    Range { first: 0x1CD3, last: 0x1CD3 },
    Range { first: 0x1FBD, last: 0x1FBD },
    Range { first: 0x1FBF, last: 0x1FC1 },
    Range { first: 0x1FCD, last: 0x1FCF },
    Range { first: 0x1FDD, last: 0x1FDF },
    Range { first: 0x1FED, last: 0x1FEF },
    Range { first: 0x1FFD, last: 0x1FFE },
    Range { first: 0x2010, last: 0x2027 },
    Range { first: 0x2030, last: 0x205E },
    Range { first: 0x207A, last: 0x207E },
    Range { first: 0x208A, last: 0x208E },
    Range { first: 0x20A0, last: 0x20C1 },
    Range { first: 0x2100, last: 0x2101 },
    Range { first: 0x2103, last: 0x2106 },
    Range { first: 0x2108, last: 0x2109 },
    Range { first: 0x2114, last: 0x2114 },
    Range { first: 0x2116, last: 0x2118 },
    Range { first: 0x211E, last: 0x2123 },
    Range { first: 0x2125, last: 0x2125 },
    Range { first: 0x2127, last: 0x2127 },
    Range { first: 0x2129, last: 0x2129 },
    Range { first: 0x212E, last: 0x212E },
    Range { first: 0x213A, last: 0x213B },
    Range { first: 0x2140, last: 0x2144 },
    Range { first: 0x214A, last: 0x214D },
    Range { first: 0x214F, last: 0x214F },
    Range { first: 0x218A, last: 0x218B },
    Range { first: 0x2190, last: 0x2429 },
    Range { first: 0x2440, last: 0x244A },
    Range { first: 0x249C, last: 0x24E9 },
    Range { first: 0x2500, last: 0x2775 },
    Range { first: 0x2794, last: 0x2B73 },
    Range { first: 0x2B76, last: 0x2BFF },
    Range { first: 0x2CE5, last: 0x2CEA },
    Range { first: 0x2CF9, last: 0x2CFC },
    Range { first: 0x2CFE, last: 0x2CFF },
    Range { first: 0x2D70, last: 0x2D70 },
    Range { first: 0x2E00, last: 0x2E2E },
    Range { first: 0x2E30, last: 0x2E5D },
    Range { first: 0x2E80, last: 0x2E99 },
    Range { first: 0x2E9B, last: 0x2EF3 },
    Range { first: 0x2F00, last: 0x2FD5 },
    Range { first: 0x2FF0, last: 0x2FFF },
    Range { first: 0x3001, last: 0x3004 },
    Range { first: 0x3008, last: 0x3020 },
    Range { first: 0x3030, last: 0x3030 },
    Range { first: 0x3036, last: 0x3037 },
    Range { first: 0x303D, last: 0x303F },
    Range { first: 0x309B, last: 0x309C },
    Range { first: 0x30A0, last: 0x30A0 },
    Range { first: 0x30FB, last: 0x30FB },
    Range { first: 0x3190, last: 0x3191 },
    Range { first: 0x3196, last: 0x319F },
    Range { first: 0x31C0, last: 0x31E5 },
    Range { first: 0x31EF, last: 0x31EF },
    Range { first: 0x3200, last: 0x321E },
    Range { first: 0x322A, last: 0x3247 },
    Range { first: 0x3250, last: 0x3250 },
    Range { first: 0x3260, last: 0x327F },
    Range { first: 0x328A, last: 0x32B0 },
    Range { first: 0x32C0, last: 0x33FF },
    Range { first: 0x4DC0, last: 0x4DFF },
    Range { first: 0xA490, last: 0xA4C6 },
    Range { first: 0xA4FE, last: 0xA4FF },
    Range { first: 0xA60D, last: 0xA60F },
    Range { first: 0xA673, last: 0xA673 },
    Range { first: 0xA67E, last: 0xA67E },
    Range { first: 0xA6F2, last: 0xA6F7 },
    Range { first: 0xA700, last: 0xA716 },
    Range { first: 0xA720, last: 0xA721 },
    Range { first: 0xA789, last: 0xA78A },
    Range { first: 0xA828, last: 0xA82B },
    Range { first: 0xA836, last: 0xA839 },
    Range { first: 0xA874, last: 0xA877 },
    Range { first: 0xA8CE, last: 0xA8CF },
    Range { first: 0xA8F8, last: 0xA8FA },
    Range { first: 0xA8FC, last: 0xA8FC },
    Range { first: 0xA92E, last: 0xA92F },
    Range { first: 0xA95F, last: 0xA95F },
    Range { first: 0xA9C1, last: 0xA9CD },
    Range { first: 0xA9DE, last: 0xA9DF },
    Range { first: 0xAA5C, last: 0xAA5F },
    Range { first: 0xAA77, last: 0xAA79 },
    Range { first: 0xAADE, last: 0xAADF },
    Range { first: 0xAAF0, last: 0xAAF1 },
    Range { first: 0xAB5B, last: 0xAB5B },
    Range { first: 0xAB6A, last: 0xAB6B },
    Range { first: 0xABEB, last: 0xABEB },
    Range { first: 0xFB29, last: 0xFB29 },
    Range { first: 0xFBB2, last: 0xFBD2 },
    Range { first: 0xFD3E, last: 0xFD4F },
    Range { first: 0xFD90, last: 0xFD91 },
    Range { first: 0xFDC8, last: 0xFDCF },
    Range { first: 0xFDFC, last: 0xFDFF },
    Range { first: 0xFE10, last: 0xFE19 },
    Range { first: 0xFE30, last: 0xFE52 },
    Range { first: 0xFE54, last: 0xFE66 },
    Range { first: 0xFE68, last: 0xFE6B },
    Range { first: 0xFF01, last: 0xFF0F },
    Range { first: 0xFF1A, last: 0xFF20 },
    Range { first: 0xFF3B, last: 0xFF40 },
    Range { first: 0xFF5B, last: 0xFF65 },
    Range { first: 0xFFE0, last: 0xFFE6 },
    Range { first: 0xFFE8, last: 0xFFEE },
    Range { first: 0xFFFC, last: 0xFFFD },
    Range { first: 0x10100, last: 0x10102 },
    Range { first: 0x10137, last: 0x1013F },
    Range { first: 0x10179, last: 0x10189 },
    Range { first: 0x1018C, last: 0x1018E },
    Range { first: 0x10190, last: 0x1019C },
    Range { first: 0x101A0, last: 0x101A0 },
    Range { first: 0x101D0, last: 0x101FC },
    Range { first: 0x1039F, last: 0x1039F },
    Range { first: 0x103D0, last: 0x103D0 },
    Range { first: 0x1056F, last: 0x1056F },
    Range { first: 0x10857, last: 0x10857 },
    Range { first: 0x10877, last: 0x10878 },
    Range { first: 0x1091F, last: 0x1091F },
    Range { first: 0x1093F, last: 0x1093F },
    Range { first: 0x10A50, last: 0x10A58 },
    Range { first: 0x10A7F, last: 0x10A7F },
    Range { first: 0x10AC8, last: 0x10AC8 },
    Range { first: 0x10AF0, last: 0x10AF6 },
    Range { first: 0x10B39, last: 0x10B3F },
    Range { first: 0x10B99, last: 0x10B9C },
    Range { first: 0x10D6E, last: 0x10D6E },
    Range { first: 0x10D8E, last: 0x10D8F },
    Range { first: 0x10EAD, last: 0x10EAD },
    Range { first: 0x10ED0, last: 0x10ED8 },
    Range { first: 0x10F55, last: 0x10F59 },
    Range { first: 0x10F86, last: 0x10F89 },
    Range { first: 0x11047, last: 0x1104D },
    Range { first: 0x110BB, last: 0x110BC },
    Range { first: 0x110BE, last: 0x110C1 },
    Range { first: 0x11140, last: 0x11143 },
    Range { first: 0x11174, last: 0x11175 },
    Range { first: 0x111C5, last: 0x111C8 },
    Range { first: 0x111CD, last: 0x111CD },
    Range { first: 0x111DB, last: 0x111DB },
    Range { first: 0x111DD, last: 0x111DF },
    Range { first: 0x11238, last: 0x1123D },
    Range { first: 0x112A9, last: 0x112A9 },
    Range { first: 0x113D4, last: 0x113D5 },
    Range { first: 0x113D7, last: 0x113D8 },
    Range { first: 0x1144B, last: 0x1144F },
    Range { first: 0x1145A, last: 0x1145B },
    Range { first: 0x1145D, last: 0x1145D },
    Range { first: 0x114C6, last: 0x114C6 },
    Range { first: 0x115C1, last: 0x115D7 },
    Range { first: 0x11641, last: 0x11643 },
    Range { first: 0x11660, last: 0x1166C },
    Range { first: 0x116B9, last: 0x116B9 },
    Range { first: 0x1173C, last: 0x1173F },
    Range { first: 0x1183B, last: 0x1183B },
    Range { first: 0x11944, last: 0x11946 },
    Range { first: 0x119E2, last: 0x119E2 },
    Range { first: 0x11A3F, last: 0x11A46 },
    Range { first: 0x11A9A, last: 0x11A9C },
    Range { first: 0x11A9E, last: 0x11AA2 },
    Range { first: 0x11B00, last: 0x11B09 },
    Range { first: 0x11BE1, last: 0x11BE1 },
    Range { first: 0x11C41, last: 0x11C45 },
    Range { first: 0x11C70, last: 0x11C71 },
    Range { first: 0x11EF7, last: 0x11EF8 },
    Range { first: 0x11F43, last: 0x11F4F },
    Range { first: 0x11FD5, last: 0x11FF1 },
    Range { first: 0x11FFF, last: 0x11FFF },
    Range { first: 0x12470, last: 0x12474 },
    Range { first: 0x12FF1, last: 0x12FF2 },
    Range { first: 0x16A6E, last: 0x16A6F },
    Range { first: 0x16AF5, last: 0x16AF5 },
    Range { first: 0x16B37, last: 0x16B3F },
    Range { first: 0x16B44, last: 0x16B45 },
    Range { first: 0x16D6D, last: 0x16D6F },
    Range { first: 0x16E97, last: 0x16E9A },
    Range { first: 0x16FE2, last: 0x16FE2 },
    Range { first: 0x1BC9C, last: 0x1BC9C },
    Range { first: 0x1BC9F, last: 0x1BC9F },
    Range { first: 0x1CC00, last: 0x1CCEF },
    Range { first: 0x1CCFA, last: 0x1CCFC },
    Range { first: 0x1CD00, last: 0x1CEB3 },
    Range { first: 0x1CEBA, last: 0x1CED0 },
    Range { first: 0x1CEE0, last: 0x1CEF0 },
    Range { first: 0x1CF50, last: 0x1CFC3 },
    Range { first: 0x1D000, last: 0x1D0F5 },
    Range { first: 0x1D100, last: 0x1D126 },
    Range { first: 0x1D129, last: 0x1D164 },
    Range { first: 0x1D16A, last: 0x1D16C },
    Range { first: 0x1D183, last: 0x1D184 },
    Range { first: 0x1D18C, last: 0x1D1A9 },
    Range { first: 0x1D1AE, last: 0x1D1EA },
    Range { first: 0x1D200, last: 0x1D241 },
    Range { first: 0x1D245, last: 0x1D245 },
    Range { first: 0x1D300, last: 0x1D356 },
    Range { first: 0x1D6C1, last: 0x1D6C1 },
    Range { first: 0x1D6DB, last: 0x1D6DB },
    Range { first: 0x1D6FB, last: 0x1D6FB },
    Range { first: 0x1D715, last: 0x1D715 },
    Range { first: 0x1D735, last: 0x1D735 },
    Range { first: 0x1D74F, last: 0x1D74F },
    Range { first: 0x1D76F, last: 0x1D76F },
    Range { first: 0x1D789, last: 0x1D789 },
    Range { first: 0x1D7A9, last: 0x1D7A9 },
    Range { first: 0x1D7C3, last: 0x1D7C3 },
    Range { first: 0x1D800, last: 0x1D9FF },
    Range { first: 0x1DA37, last: 0x1DA3A },
    Range { first: 0x1DA6D, last: 0x1DA74 },
    Range { first: 0x1DA76, last: 0x1DA83 },
    Range { first: 0x1DA85, last: 0x1DA8B },
    Range { first: 0x1E14F, last: 0x1E14F },
    Range { first: 0x1E2FF, last: 0x1E2FF },
    Range { first: 0x1E5FF, last: 0x1E5FF },
    Range { first: 0x1E95E, last: 0x1E95F },
    Range { first: 0x1ECAC, last: 0x1ECAC },
    Range { first: 0x1ECB0, last: 0x1ECB0 },
    Range { first: 0x1ED2E, last: 0x1ED2E },
    Range { first: 0x1EEF0, last: 0x1EEF1 },
    Range { first: 0x1F000, last: 0x1F02B },
    Range { first: 0x1F030, last: 0x1F093 },
    Range { first: 0x1F0A0, last: 0x1F0AE },
    Range { first: 0x1F0B1, last: 0x1F0BF },
    Range { first: 0x1F0C1, last: 0x1F0CF },
    Range { first: 0x1F0D1, last: 0x1F0F5 },
    Range { first: 0x1F10D, last: 0x1F1AD },
    Range { first: 0x1F1E6, last: 0x1F202 },
    Range { first: 0x1F210, last: 0x1F23B },
    Range { first: 0x1F240, last: 0x1F248 },
    Range { first: 0x1F250, last: 0x1F251 },
    Range { first: 0x1F260, last: 0x1F265 },
    Range { first: 0x1F300, last: 0x1F6D8 },
    Range { first: 0x1F6DC, last: 0x1F6EC },
    Range { first: 0x1F6F0, last: 0x1F6FC },
    Range { first: 0x1F700, last: 0x1F7D9 },
    Range { first: 0x1F7E0, last: 0x1F7EB },
    Range { first: 0x1F7F0, last: 0x1F7F0 },
    Range { first: 0x1F800, last: 0x1F80B },
    Range { first: 0x1F810, last: 0x1F847 },
    Range { first: 0x1F850, last: 0x1F859 },
    Range { first: 0x1F860, last: 0x1F887 },
    Range { first: 0x1F890, last: 0x1F8AD },
    Range { first: 0x1F8B0, last: 0x1F8BB },
    Range { first: 0x1F8C0, last: 0x1F8C1 },
    Range { first: 0x1F8D0, last: 0x1F8D8 },
    Range { first: 0x1F900, last: 0x1FA57 },
    Range { first: 0x1FA60, last: 0x1FA6D },
    Range { first: 0x1FA70, last: 0x1FA7C },
    Range { first: 0x1FA80, last: 0x1FA8A },
    Range { first: 0x1FA8E, last: 0x1FAC6 },
    Range { first: 0x1FAC8, last: 0x1FAC8 },
    Range { first: 0x1FACD, last: 0x1FADC },
    Range { first: 0x1FADF, last: 0x1FAEA },
    Range { first: 0x1FAEF, last: 0x1FAF8 },
    Range { first: 0x1FB00, last: 0x1FB92 },
    Range { first: 0x1FB94, last: 0x1FBEF },
    Range { first: 0x1FBFA, last: 0x1FBFA },
];
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flanking_classes_follow_unicode_general_categories() {
        assert!(is_punctuation_or_symbol(0x2014)); // em dash, Pd
        assert!(is_punctuation_or_symbol(0x201C)); // left double quote, Pi
        assert!(is_punctuation_or_symbol(0x00D7)); // multiplication sign, Sm
        assert!(is_punctuation_or_symbol(0xFF0C)); // fullwidth comma, Po
        assert!(is_punctuation_or_symbol(0x1F600)); // grinning face, So
        assert!(is_punctuation_or_symbol(0x1F979)); // face holding back tears, So, Unicode 14
        assert!(!is_punctuation_or_symbol(0x00B5)); // micro sign, Ll
        assert!(!is_punctuation_or_symbol(0x00AA)); // feminine ordinal, Lo
        assert!(!is_punctuation_or_symbol(0x3031)); // vertical kana repeat mark, Lm
        assert!(!is_punctuation_or_symbol(0x4E2D)); // CJK ideograph, Lo
        assert!(!is_punctuation_or_symbol(0x2460)); // circled digit one, No
        assert!(
            is_punctuation_or_symbol(u32::from(b'*')) && !is_punctuation_or_symbol(u32::from(b'a'))
        );
        assert!(is_whitespace(0x00A0) && is_whitespace(0x3000) && is_whitespace(0x2003));
        assert!(
            is_whitespace(u32::from(b' '))
                && is_whitespace(u32::from(b'\t'))
                && !is_whitespace(0x200B)
        );
    }

    #[test]
    fn packed_flanking_ranges_preserve_generated_endpoints() {
        assert_eq!(std::mem::size_of::<PackedRange>(), 4);
        for (source, encoded) in SOURCE_SPACE_RANGES.iter().zip(SPACE_RANGES.iter()) {
            assert_eq!(source.first, encoded.first());
            assert_eq!(source.last, encoded.first() + encoded.span());
        }
        for (source, encoded) in SOURCE_PUNCTUATION_RANGES
            .iter()
            .zip(PUNCTUATION_RANGES.iter())
        {
            assert_eq!(source.first, encoded.first());
            assert_eq!(source.last, encoded.first() + encoded.span());
        }
    }
}
