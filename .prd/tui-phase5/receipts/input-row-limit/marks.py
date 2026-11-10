"""Where the page put the mark, read off each menu capture in this folder: the
screen row of the marked list row (shaded in the @ list, bold in a menu), its text, and the slash menu's
`a–b` counter, for fx/, xfx-base/ and xfx/ (the head). A pair whose list
fx and xfx fill differently (/help and the slash menu: 38 commands against
34) can still show the page landed on the same position.

usage: python3 -I marks.py
"""

import glob
import os
import re
import sys

sys.dont_write_bytecode = True
HERE = os.path.dirname(os.path.realpath(__file__))
SGR = re.compile(r"\x1b\[[0-9;:]*m")
SNAPS = ("at", "at-pgdn", "help", "help-pgdn", "slash", "slash-pgdn", "model", "model-pgdn", "effort", "effort-pgdn")


def mark(path):
    rows = open(path, encoding="utf-8").read().split("\n")
    counter, shaded, bold = "", None, None
    for i, row in enumerate(rows[1:], 2):
        text = SGR.sub("", row)
        found = re.search(r"(\d+)–(\d+)\s*$", text)
        if found and "type to filter" in text:
            counter = found.group(0).strip()
        if not (text.startswith("  ") and text.strip()):
            continue
        # The @ list marks its row with a background (its bold is the
        # match); the menus mark theirs bold.
        if shaded is None and "\x1b[48;" in row:
            shaded = (i, text.strip()[:44])
        if bold is None and "\x1b[1m" in row:
            bold = (i, text.strip()[:44])
    return shaded or bold, counter


def main():
    for fx in sorted(glob.glob(os.path.join(HERE, "fx", "*.ansi"))):
        name = os.path.basename(fx)
        label = re.sub(r"^fx-p5-input-row-limit(-menus|-short)?-\d+x\d+-", "", name)[:-5]
        if label not in SNAPS:
            continue
        print(name[3:-5])
        for side, path in (("fx", fx), ("base", os.path.join(HERE, "xfx-base", "x" + name)), ("head", os.path.join(HERE, "xfx", "x" + name))):
            m, c = mark(path)
            print(f"  {side:5} row {m[0] if m else '-':>3}  {c:7} {m[1] if m else ''}")


if __name__ == "__main__":
    main()
