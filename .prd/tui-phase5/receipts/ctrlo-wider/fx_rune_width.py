"""fx v0.0.16's count of one code point, read from its own source, not from xfx's port.

usage: python3 -I fx_rune_width.py <unicode_display_data.zig> <scalars.txt> > fx.txt

<unicode_display_data.zig> is `git show 481104fd:src/core/shared/unicode_display_data.zig`
(vercel-labs/fx v0.0.16). The ranges are parsed from it; the rule is
`runeWidth`, `src/core/shared/display_width.zig:275-282@481104fd`:

    if (codepoint == 0) return 0;
    if (codepoint < 32 or (codepoint >= 0x7f and codepoint < 0xa0)) return 0;
    if (isZeroWidthContinuation(codepoint)) return 0;
    if (isInRanges(codepoint, &wide_ranges) or
        isInRanges(codepoint, &emoji_presentation_ranges)) return 2;
    return 1;

with `isCombining` (`:400-407`) and `isZeroWidthContinuation` (`:409-420`)
transcribed below. Writes `hex cells` per line of <scalars.txt>.
"""
import bisect
import re
import sys


def ranges(text, name):
    body = re.search(r"pub const %s = \[_\]Range\{(.*?)\n\};" % name, text, re.S)
    if body is None:
        sys.exit("no %s in the data file" % name)
    pairs = re.findall(r"\.first = 0x([0-9A-Fa-f]+), \.last = 0x([0-9A-Fa-f]+)", body.group(1))
    out = sorted((int(a, 16), int(b, 16)) for a, b in pairs)
    if not out:
        sys.exit("%s is empty" % name)
    return out


def member(table):
    firsts = [first for first, _ in table]

    def test(code):
        at = bisect.bisect_right(firsts, code) - 1
        return at >= 0 and table[at][0] <= code <= table[at][1]

    return test


def main():
    data = open(sys.argv[1], encoding="utf-8").read()
    wide = member(ranges(data, "wide_ranges"))
    presentation = member(ranges(data, "emoji_presentation_ranges"))
    modifier = member(ranges(data, "emoji_modifier_ranges"))

    # display_width.zig:400-407@481104fd
    def is_combining(c):
        return (
            0x0300 <= c <= 0x036F
            or 0x1AB0 <= c <= 0x1AFF
            or 0x1DC0 <= c <= 0x1DFF
            or 0x20D0 <= c <= 0x20FF
            or 0xFE20 <= c <= 0xFE2F
            or 0xFE00 <= c <= 0xFE0F
        )

    # display_width.zig:409-420@481104fd
    def zero_width_continuation(c):
        return (
            is_combining(c)
            or modifier(c)
            or c == 0x200C
            or c == 0x200D
            or 0x200B <= c <= 0x200F
            or 0x202A <= c <= 0x202E
            or 0x2060 <= c <= 0x206F
            or c == 0xFEFF
            or 0xE0001 <= c <= 0xE007F
            or 0xE0100 <= c <= 0xE01EF
        )

    # display_width.zig:275-282@481104fd
    def rune_width(c):
        if c == 0:
            return 0
        if c < 32 or 0x7F <= c < 0xA0:
            return 0
        if zero_width_continuation(c):
            return 0
        if wide(c) or presentation(c):
            return 2
        return 1

    out = []
    for line in open(sys.argv[2]):
        code = line.strip()
        if code:
            out.append("%s %d" % (code, rune_width(int(code, 16))))
    sys.stdout.write("\n".join(out) + "\n")


main()
