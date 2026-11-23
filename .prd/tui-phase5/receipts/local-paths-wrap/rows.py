"""Which rows of a capture hold a piece of a root, and what leads each.

usage: python3 -I rows.py <capture .txt> <root>

For each row that holds 4 bytes or more of <root> in one piece, prints the
row's number, what stands before the piece on the row -- its code points
when they are all spaces, tabs, U+2500-U+259F or a mark (U+21B3, U+2191),
otherwise only how many characters of other text -- and which of the root's
bytes the piece is. Never the row's text, never the root.
"""
import difflib
import sys

cap, root = sys.argv[1:3]
rows = open(cap, encoding="utf-8").read().split("\n")


def gutter(c):
    return c in " \t↳↑" or "─" <= c <= "▟"


held = 0
for i, row in enumerate(rows, 1):
    m = difflib.SequenceMatcher(None, row, root, autojunk=False).find_longest_match(0, len(row), 0, len(root))
    if m.size < 4:
        continue
    held += 1
    lead = row[: m.a]
    if not lead:
        said = "nothing"
    elif all(gutter(c) for c in lead):
        said = " ".join("U+%04X" % ord(c) for c in lead)
    else:
        said = "%d characters of other text" % len(lead)
    where = "first" if m.b == 0 else "last" if m.b + m.size == len(root) else "middle"
    print("row %d, led by %s: the root's %s %d of its %d bytes" % (i, said, where, m.size, len(root.encode())))
print("rows holding 4 bytes or more of the root in one piece: %d" % held)
