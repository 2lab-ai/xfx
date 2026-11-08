#!/usr/bin/env python3
"""Every cancelled-marker row of a pair, cell by cell: fx's against xfx's.

    python3 -I -B marker_cells.py <fx dir> <xfx dir> <scenario/label>...

A marker row is a row whose first printed cell is U+25AA or U+25A0 (the
turn's line, a call's own line, a question batch's `Cancelled`). The k-th
marker row of fx's capture is paired with the k-th of xfx's -- by order, not
by index, so rows another difference pushed down (a taller approval, a Ctrl-O
page read from the tail) still meet -- and their glyph cell and `Cancelled`
word (column 0 and columns 2-10, each with the attributes in force on it,
`scripts/fxcap/cells.py`'s reading) are compared. A side with fewer marker
rows than the other is a difference. The rest of the row is the product's name (`fx`/`xfx`) and the
call's argument, which the sweep compares under its normalizations.

Prints one line per pair: `same` when every marker row's cells match with
their attributes; `chars` when the characters match and only the attributes
differ (a truecolor pair, whose greys xfx spells 38;2 where fx spells 38;5:
the ledger's `truecolor-grey-notation`, TC-NOTATION's); else `DIFFER` and the
rows. Exit 0 when no pair is `DIFFER`.
"""
import os
import sys

HERE = os.path.dirname(os.path.realpath(__file__))
sys.dont_write_bytecode = True
sys.path.insert(0, os.path.join(HERE, "..", "..", "..", "..", "scripts", "fxcap"))
import cells  # noqa: E402

MARKS = ("▪", "■")


def rows(path):
    with open(path, encoding="utf-8") as f:
        return cells.parse(f.read())


def first(row):
    return row[0][1] if row else ""


def main():
    fx_dir, xfx_dir, pairs = sys.argv[1], sys.argv[2], sys.argv[3:]
    bad = 0
    for pair in pairs:
        scenario, label = pair.split("/", 1)
        name = "%s-40x120-%s.ansi" % (scenario, label)
        fx = rows(os.path.join(fx_dir, "fx-" + name))
        xfx = rows(os.path.join(xfx_dir, "xfx-" + name))
        marked = [i for i, row in enumerate(fx) if first(row) in MARKS]
        theirs = [i for i, row in enumerate(xfx) if first(row) in MARKS]
        problems, attrs_only = [], False
        if len(marked) != len(theirs):
            problems.append("fx has %d marker row(s), xfx %d (%s)" % (
                len(marked), len(theirs), ", ".join(str(i + 1) for i in theirs) or "none"))
        for i, j in zip(marked, theirs):
            want = [c for c in fx[i] if c[0] == 0 or 2 <= c[0] <= 10]
            got = [c for c in xfx[j] if c[0] == 0 or 2 <= c[0] <= 10]
            if want == got:
                continue
            if [c[:2] for c in want] == [c[:2] for c in got]:
                attrs_only = True
                continue
            problems.append("fx row %d / xfx row %d: fx %r / xfx %r" % (i + 1, j + 1, want[:3], got[:3]))
        if not marked:
            problems.append("fx has no marker row")
        verdict = "DIFFER" if problems else ("chars" if attrs_only else "same")
        bad += bool(problems)
        print("%s %s: %d marker row(s) %s" % (verdict, pair, len(marked),
              ", ".join(str(i + 1) for i in marked)))
        for p in problems:
            print("    " + p)
    print("pairs: %d, DIFFER: %d" % (len(pairs), bad))
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
