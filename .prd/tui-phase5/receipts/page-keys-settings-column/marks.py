"""Where each capture's mark is: for every snap in fx/, xfx-base/ and xfx/,
each indented row whose text starts bold -- the marked setting, model,
level, speed or `/mcp` line, and under /settings' Model row the list's
marked model as well (a value in a settings row is bold too, but never
first on its row; a header starts at column 0) -- with its screen row. Read with cells.py's
parser, so the attributes are tmux's own.

usage: python3 -I marks.py
"""

import glob
import os
import sys

sys.dont_write_bytecode = True
HERE = os.path.dirname(os.path.realpath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "..", "..", "..", "scripts", "fxcap"))
import cells  # noqa: E402


def bold(shown):
    return dict(shown).get("bold", False)


def mark(path):
    """Every indented row whose first printed cell is bold, as `row N: text`
    with the text cut at its first two blanks -- the marked setting, and
    under the Model row the list's marked model too."""
    with open(path, encoding="utf-8") as f:
        rows = cells.parse(f.read())
    found = []
    for number, row in enumerate(rows, start=1):
        printed = [cell for cell in row if not cell[1].isspace()]
        if number == 1 or not printed:
            continue
        first = printed[0]
        # A list's rows are indented (settings 2, `/mcp` 7, the column 9);
        # a header (`Settings 14`), a rule and the composer start at column 0.
        if first[0] < 2 or not bold(first[2]):
            continue
        text = "".join(ch for col, ch, _ in row if col >= first[0])
        found.append(f"row {number}: {text.strip().split('  ')[0]}")
    return " | ".join(found) or "no mark"


def main():
    for fx in sorted(glob.glob(os.path.join(HERE, "fx", "*.ansi"))):
        name = os.path.basename(fx)
        sides = [fx, os.path.join(HERE, "xfx-base", "x" + name), os.path.join(HERE, "xfx", "x" + name)]
        got = [mark(p) if os.path.exists(p) else "-" for p in sides]
        verdict = "same" if got[0] == got[2] else "DIFFERS"
        print(f"{name[3:-5]:42} {verdict:7} fx {got[0]:40} base {got[1]:40} head {got[2]}")


if __name__ == "__main__":
    main()
