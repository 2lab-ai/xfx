"""Draws a `capture-pane -e` capture as a PNG, for the parity sheets.

usage: python3 -I render.py <capture.ansi> <out.png> [--rows a-b]

Each cell is drawn where `scripts/fxcap/cells.py` reads it -- the same parse
the pairs are compared with -- in Menlo, on a dark background: the default
foreground light grey, xterm's 256 colours for `38;5;N`, bold in Menlo Bold.
`--rows` keeps one range of rows (1 is the top row); without it the blank
rows under the last printed one are left off.
"""

import argparse
import os
import runpy

from PIL import Image, ImageDraw, ImageFont

HERE = os.path.dirname(os.path.realpath(__file__))
CELLS = runpy.run_path(os.path.join(HERE, "..", "..", "..", "..", "scripts", "fxcap", "cells.py"))
FONT = "/System/Library/Fonts/Menlo.ttc"
SIZE = 16
BG = (18, 18, 22)
FG = (220, 220, 220)


def xterm(n):
    """xterm's 256-colour palette."""
    base = [(0, 0, 0), (205, 0, 0), (0, 205, 0), (205, 205, 0), (0, 0, 238), (205, 0, 205),
            (0, 205, 205), (229, 229, 229), (127, 127, 127), (255, 0, 0), (0, 255, 0),
            (255, 255, 0), (92, 92, 255), (255, 0, 255), (0, 255, 255), (255, 255, 255)]
    if n < 16:
        return base[n]
    if n < 232:
        n -= 16
        steps = [0, 95, 135, 175, 215, 255]
        return (steps[n // 36], steps[(n // 6) % 6], steps[n % 6])
    level = 8 + (n - 232) * 10
    return (level, level, level)


def colour(value, default):
    if value is None:
        return default
    if isinstance(value, int):
        return xterm(value)
    if isinstance(value, tuple) and value and value[0] == "ansi":
        code = value[1]
        index = code - 30 if code < 40 else code - 90 + 8 if code >= 90 else code - 40
        return xterm(index % 16)
    return tuple(value)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("capture")
    ap.add_argument("out")
    ap.add_argument("--rows")
    a = ap.parse_args()
    rows = CELLS["parse"](open(a.capture, encoding="utf-8").read())
    if rows and rows[-1] == []:
        rows = rows[:-1]
    if a.rows:
        first, last = (int(x) for x in a.rows.split("-"))
        rows = rows[first - 1 : last]
    else:
        while rows and not rows[-1]:
            rows = rows[:-1]
    regular = ImageFont.truetype(FONT, SIZE, index=0)
    bold = ImageFont.truetype(FONT, SIZE, index=1)
    cell_w = int(regular.getlength("M"))
    cell_h = SIZE + 6
    cols = max([c + 1 for row in rows for c, _, _ in row] + [40])
    image = Image.new("RGB", (cols * cell_w + 16, len(rows) * cell_h + 16), BG)
    draw = ImageDraw.Draw(image)
    for y, row in enumerate(rows):
        for col, ch, shown in row:
            attrs = dict(shown)
            fg = colour(attrs.get("fg"), FG)
            bg = attrs.get("bg")
            if attrs.get("reverse"):
                fg, bg = colour(bg, BG), attrs.get("fg") or FG
            x0, y0 = 8 + col * cell_w, 8 + y * cell_h
            if bg is not None:
                draw.rectangle([x0, y0, x0 + cell_w, y0 + cell_h], fill=colour(bg, BG))
            if ch != " ":
                font = bold if attrs.get("bold") else regular
                draw.text((x0, y0 + 2), ch, fill=fg, font=font)
    image.save(a.out)


if __name__ == "__main__":
    main()
