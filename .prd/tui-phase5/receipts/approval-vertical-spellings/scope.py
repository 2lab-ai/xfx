"""Every fx/ snap of this receipt against the same snap under <side>/, cell by
cell with its styles (scripts/fxcap/cells.py's reading), with the rows of the
parity ledger's `approval-grant-scope` cause set aside on each side.

That cause is a pending user decision no unit can resolve: choice 2's words
(fx's `workspace file access`, xfx's `future edits/writes to this file`) and
xfx's two rows of what `2` buys. The rows set aside are exactly those the
ledger's `fx_row`/`xfx_row` match, read from scripts/fxcap/parity-ledger.toml
with the ledger's `session-id` normalization applied first (its regex expects
`<session-id>`). Choice 2's row is compared up to its words, so its marker and
indent still count. Row 1 (the banner) and the blank rows at the bottom are
left out; everything else must be equal, row for row.

usage: python3 -I scope.py <red|xfx>   (reads this directory)
"""

import importlib.util
import os
import re
import sys
import tomllib

here = os.path.dirname(os.path.realpath(__file__))
root = os.path.join(here, "..", "..", "..", "..")
spec = importlib.util.spec_from_file_location("cells", os.path.join(root, "scripts", "fxcap", "cells.py"))
cells = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cells)
ledger = tomllib.load(open(os.path.join(root, "scripts", "fxcap", "parity-ledger.toml"), "rb"))
cause = next(c for c in ledger["cause"] if c["id"] == "approval-grant-scope")
session = next(n for n in ledger["normalization"] if n["id"] == "session-id")
SCOPE = {"fx": re.compile(cause["fx_row"]), "xfx": re.compile(cause["xfx_row"])}
SESSION = {"fx": re.compile(session["fx"]), "xfx": re.compile(session["xfx"])}
CHOICE_2 = re.compile(r"^\s+(?:❯ )?2  Apply \+ allow ")


def kept(path, side):
    """The rows of one capture with the cause's rows set aside: (text, cells)."""
    out = []
    for row in cells.rows(path, side == "fx")[1:]:
        text = SESSION[side].sub(session["replace"], cells.text_of(row))
        if SCOPE[side].search(text):
            m = CHOICE_2.match(text)
            if not m:
                continue
            row = row[: m.end()]
            text = text[: m.end()]
        out.append((text, row))
    while out and not out[-1][0].strip():
        out.pop()
    return out


def main():
    side = sys.argv[1]
    name = "p5-approval-vertical-keys-40x120"
    same = total = 0
    for f in sorted(os.listdir(os.path.join(here, "fx"))):
        if not f.endswith(".ansi"):
            continue
        label = f[len("fx-" + name + "-") : -len(".ansi")]
        fx = kept(os.path.join(here, "fx", f), "fx")
        xfx = kept(os.path.join(here, side, "xfx-%s-%s.ansi" % (name, label)), "xfx")
        total += 1
        rows = [i for i in range(max(len(fx), len(xfx))) if (fx[i][1] if i < len(fx) else None) != (xfx[i][1] if i < len(xfx) else None)]
        if not rows:
            same += 1
            print("SAME  %-13s (%d rows compared)" % (label, len(fx)))
            continue
        print("DIFF  %-13s %d rows differ" % (label, len(rows)))
        for i in rows[:3]:
            print("      fx:  %r" % (fx[i][0] if i < len(fx) else None))
            print("      xfx: %r" % (xfx[i][0] if i < len(xfx) else None))
    print("%d of %d pairs identical once the grant scope's rows are set aside" % (same, total))
    return 0 if same == total else 1


if __name__ == "__main__":
    sys.exit(main())
