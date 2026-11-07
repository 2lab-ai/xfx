"""A truecolor pair, read two ways, row 1 (the banner) left out.

usage: python3 -I notation.py <cells.py dir> <fx.ansi> <xfx.ansi> [<xfx drawn.ansi>]

fx's builtins are 256-colour at every depth; xfx spells its own greys in the
direct notation when COLORTERM claims truecolor. So a strict reading
(`cells.py`) differs on those cells by notation alone. This prints:

  ramp:     the rows that still differ once each direct grey on xterm's ramp
            (38;2;L;L;L with L = 8 + 10k, k in 0..=23) is read as the index
            232 + k it spells -- the colours compared, not their spelling;
  colour:   every cell that still differs after it, grouped (fx attrs, xfx
            attrs as read) with a count and whether its text is the same;
  notation: every cell that differs strictly and not after the ramp reading,
            grouped (fx attrs, xfx attrs) with a count;
  kept:     with a fourth argument, a snap of the same run taken before a
            theme report: the cells (same row, column and character) whose
            foreground is spelt in another notation than in that snap. A
            report that matched one notation and repainted in the other shows
            here, where the ramp reading alone would pass it.

Exit 1 if `ramp` or `kept` is not 0.
"""

import collections
import sys

sys.dont_write_bytecode = True
sys.path.insert(0, sys.argv[1])
import cells  # noqa: E402


def ramp(value):
    if isinstance(value, tuple) and len(value) == 3 and all(isinstance(v, int) for v in value):
        level = value[0]
        if value == (level, level, level) and (level - 8) % 10 == 0 and 0 <= (level - 8) // 10 <= 23:
            return 232 + (level - 8) // 10
    return value


def read(cell):
    col, ch, shown = cell
    return (col, ch, tuple((k, ramp(v)) if k in ("fg", "bg") else (k, v) for k, v in shown))


def notation(shown):
    fg = dict(shown).get("fg")
    if isinstance(fg, int):
        return "indexed"
    if isinstance(fg, tuple) and all(isinstance(v, int) for v in fg):
        return "direct"
    return fg


def load(path, product_dir):
    return cells.rows(path, product_dir)


fx, xfx = load(sys.argv[2], True), load(sys.argv[3], False)
differ, groups, colours = 0, collections.Counter(), collections.Counter()
for r in range(1, max(len(fx), len(xfx))):
    a = fx[r] if r < len(fx) else []
    b = xfx[r] if r < len(xfx) else []
    if [read(c) for c in a] != [read(c) for c in b]:
        differ += 1
        print("ramp row %d: fx %r" % (r + 1, cells.text_of(a)[:60]))
        print("ramp row %d: xfx %r" % (r + 1, cells.text_of(b)[:60]))
    for i in range(max(len(a), len(b))):
        x = read(a[i]) if i < len(a) else None
        y = read(b[i]) if i < len(b) else None
        if x != y:
            colours[(x[2] if x else None, y[2] if y else None, (x[1] if x else None) == (y[1] if y else None))] += 1
        elif a[i] != b[i]:
            groups[(a[i][2], b[i][2])] += 1
print("ramp: %d rows differ (row 1 not compared)" % differ)
for (x, y, same), n in sorted(colours.items(), key=str):
    print("colour: %d fx %s xfx %s %s" % (n, x, y, "same-text" if same else "TEXT-DIFFERS"))
for (x, y), n in sorted(groups.items(), key=str):
    print("notation: %d fx %s xfx %s" % (n, x, y))
print("notation: %d cells in all" % sum(groups.values()))
kept = 0
if len(sys.argv) > 4:
    drawn = load(sys.argv[4], False)
    for r in range(1, min(len(drawn), len(xfx))):
        before = {(c[0], c[1]): notation(c[2]) for c in drawn[r]}
        for col, ch, shown in xfx[r]:
            was = before.get((col, ch))
            if was in ("indexed", "direct") and notation(shown) in ("indexed", "direct") and was != notation(shown):
                kept += 1
                print("kept row %d col %d %r: drawn %s, now %s" % (r + 1, col, ch, was, notation(shown)))
    print("kept: %d cells changed notation since the drawn snap" % kept)
sys.exit(1 if differ or kept else 0)
