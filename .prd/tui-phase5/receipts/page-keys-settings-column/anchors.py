"""Where each list of `p5-page-keys-anchor` starts, on every side: for each
snap in fx/, xfx-base/, xfx-r1/ and xfx/, the one-based column of the first
printed cell of the list's first row -- the first row with text between the
rule under the composer and the next rule, past a `Results N` header -- with
that row's text, and the snap's caret (`x,y`). The column is what
projectedAnchorColumn decides; the row's text is shown so a column that
agrees over rows that do not is visible.

usage: python3 -I anchors.py
"""

import glob
import os
import sys

sys.dont_write_bytecode = True
HERE = os.path.dirname(os.path.realpath(__file__))
SIDES = ("fx", "xfx-base", "xfx-r1", "xfx")
RULE = "─" * 10


def first_row(path):
    with open(path, encoding="utf-8") as f:
        rows = f.read().split("\n")
    rules = [i for i, row in enumerate(rows) if row.startswith(RULE)]
    if len(rules) < 2:
        return "-", ""
    for row in rows[rules[0] + 1 : rules[1]]:
        text = row.rstrip()
        if not text or text.startswith("Results "):
            continue
        return len(text) - len(text.lstrip()) + 1, text.strip()[:34]
    return "-", ""


def caret(path):
    path = path.replace(".txt", ".cursor")
    return open(path, encoding="utf-8").read().strip() if os.path.exists(path) else ""


def main():
    names = sorted(os.path.basename(p)[3:-4] for p in glob.glob(os.path.join(HERE, "fx", "fx-p5-page-keys-anchor-*.txt")))
    print(f"{'snap':40} " + "  ".join(f"{side:>14}" for side in SIDES) + "  row (fx | head)")
    for name in names:
        got = []
        for side in SIDES:
            prefix = "fx-" if side == "fx" else "xfx-"
            path = os.path.join(HERE, side, prefix + name + ".txt")
            if not os.path.exists(path):
                got.append(("-", "", ""))
                continue
            column, text = first_row(path)
            got.append((column, text, caret(path)))
        cells = [f"{c}" + (f" @{k}" if k else "") for c, _, k in got]
        verdict = "same" if (got[0][0], got[0][2]) == (got[3][0], got[3][2]) else "DIFFERS"
        print(f"{name:40} " + "  ".join(f"{c:>14}" for c in cells) + f"  {verdict}  {got[0][1]!r} | {got[3][1]!r}")


if __name__ == "__main__":
    main()
