"""Compares two `capture-pane -e` files cell by cell, row 1 (each product's
banner) left out.

usage: python3 -I cells.py <fx.ansi> <xfx.ansi>

Every printed character is compared with its column and the attributes in
force on it, the SGR state carried from row to row as tmux writes it. Blank
cells past a row's last printed character are not compared: tmux trims them,
and a space's foreground and weight do not show -- so where tmux happens to
put the reset that ends a row's run (on that row, or in front of the next)
is not a difference. A space under a background, reverse or underline is
compared like any other character. `/.fx/` in the fx file is read as
`/.xfx/` -- `~/.fx/mcp.json` and `~/.xfx/mcp.json` are each product's
profile -- the same rule as the banner.
"""

import re
import sys
import unicodedata

SGR = re.compile(r"\x1b\[([0-9;]*)m")
OTHER = re.compile(r"\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)|\x1b\[[0-9;?]*[A-Za-z]")
SHOWS_ON_A_SPACE = ("bg", "reverse", "underline")


def apply(state, params):
    codes = [int(p) if p else 0 for p in params.split(";")] if params else [0]
    i = 0
    while i < len(codes):
        c = codes[i]
        if c == 0:
            state.clear()
        elif c in (38, 48) and i + 2 < len(codes) and codes[i + 1] == 5:
            state["fg" if c == 38 else "bg"] = codes[i + 2]
            i += 2
        elif c in (38, 48) and i + 4 < len(codes) and codes[i + 1] == 2:
            state["fg" if c == 38 else "bg"] = tuple(codes[i + 2 : i + 5])
            i += 4
        elif c == 39:
            state.pop("fg", None)
        elif c == 49:
            state.pop("bg", None)
        elif c == 22:
            state.pop("bold", None)
            state.pop("dim", None)
        elif c in (23, 24, 27):
            state.pop({23: "italic", 24: "underline", 27: "reverse"}[c], None)
        elif c in (1, 2, 3, 4, 7):
            state[{1: "bold", 2: "dim", 3: "italic", 4: "underline", 7: "reverse"}[c]] = True
        elif 30 <= c <= 37 or 90 <= c <= 97:
            state["fg"] = ("ansi", c)
        elif 40 <= c <= 47 or 100 <= c <= 107:
            state["bg"] = ("ansi", c)
        else:
            state["sgr%d" % c] = True
        i += 1


def width(ch):
    if unicodedata.combining(ch):
        return 0
    return 2 if unicodedata.east_asian_width(ch) in "WF" else 1


def rows(path, product_dir):
    text = open(path, encoding="utf-8").read()
    if product_dir:
        text = text.replace("/.fx/", "/.xfx/")
    state, out = {}, []
    for line in text.split("\n"):
        cells, col, pos = [], 0, 0
        while pos < len(line):
            m = SGR.match(line, pos)
            if m:
                apply(state, m.group(1))
                pos = m.end()
                continue
            m = OTHER.match(line, pos)
            if m:
                pos = m.end()
                continue
            ch = line[pos]
            pos += 1
            shown = tuple(sorted(state.items(), key=str))
            if ch == " " and not any(k in state for k in SHOWS_ON_A_SPACE):
                shown = ()
            cells.append((col, ch, shown))
            col += width(ch)
        while cells and cells[-1][1] == " " and cells[-1][2] == ():
            cells.pop()
        out.append(cells)
    return out


def main():
    fx, xfx = rows(sys.argv[1], True), rows(sys.argv[2], False)
    differ = 0
    for i in range(1, max(len(fx), len(xfx))):
        a = fx[i] if i < len(fx) else []
        b = xfx[i] if i < len(xfx) else []
        if a != b:
            differ += 1
            at = next((n for n, (p, q) in enumerate(zip(a, b)) if p != q), min(len(a), len(b)))
            print("row %d: fx %r" % (i + 1, a[at : at + 4]))
            print("row %d: xfx %r" % (i + 1, b[at : at + 4]))
    print("%d rows differ in their cells (row 1 not compared)" % differ)
    return 1 if differ else 0


if __name__ == "__main__":
    sys.exit(main())
