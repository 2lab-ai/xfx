"""AT-HEIGHT's band check, beside the cell comparison (cells.py), for the
pairs whose cells cannot agree for reasons outside this unit.

- p5-input-at's `@st` snaps (at-st, at-st-down12, at-st-up6): xfx ranks `@st`
  with c1db919's score until U2 lands, so the list rows hold other paths. What
  this unit owns is the band's shape -- where the composer, the two rules and
  the hint row are, how many rows the list has, which of them the bar is on --
  and that each side's window is the edge-scrolled window over its own
  ranking: a row of one snap and a row of another that stand for the same
  rank hold the same path. Checked on both sides, so fx's captures check the
  rule too.
- p5-input-at's at-src: fx lists `src/` itself first (I-U3), so the rows
  differ; the band's shape is compared.
- p5-input-at-resize after the resize: fx clears the screen and writes the
  document again from the top, xfx keeps its rows and solves the band at the
  bottom (RESIZE-REPAINT; parity-ledger.toml `resize-document-repaint`). The
  band itself, from the composer row to the hint row, is compared cell by
  cell (cells.py's reading).

usage: python3 -I band.py <fx-dir> <xfx-dir> <size>...
Prints one line per check and `band: N checks, M failed`; exits 1 on a failure.
"""

import os
import sys

sys.dont_write_bytecode = True
HERE = os.path.dirname(os.path.realpath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "..", "..", "..", "scripts", "fxcap"))

import cells  # noqa: E402

RULE = "─"


def text(row):
    return cells.text_of(row)


def band(path):
    """The band in a capture: (first row number, rows), from the composer row
    (`┃ @`) to the hint row -- the row under the second rule after it."""
    with open(path, encoding="utf-8") as f:
        rows = cells.parse(f.read())
    first = next(i for i, row in enumerate(rows) if text(row).startswith("┃ @"))
    rules = [i for i in range(first + 1, len(rows)) if text(rows[i]) and set(text(rows[i])) == {RULE}]
    last = rules[1] + 1
    return first + 1, rows[first : last + 1]


def shape(path):
    """The composer's row number, the list's rows (text) and the bar's index
    in them (the row on background 239), and the hint row's text."""
    top, rows = band(path)
    listed = rows[2:-2]
    bars = [i for i, row in enumerate(listed) if any(dict(c[2]).get("bg") == 239 for c in row)]
    return {
        "composer_row": top,
        "list_rows": len(listed),
        "bar": bars[0] if len(bars) == 1 else None,
        "paths": [text(row).strip() for row in listed],
        "hint": text(rows[-1]),
    }


def edge(start, count, selected, rows):
    """`updateEdgeStart` (list_window.zig:55-68@481104fd)."""
    if count <= rows:
        return 0
    start = min(start, count - rows)
    if selected < start:
        return selected
    if selected >= start + rows:
        return selected - rows + 1
    return start


checks, failed = 0, 0


def check(ok, what):
    global checks, failed
    checks += 1
    failed += 0 if ok else 1
    print(("ok   " if ok else "FAIL ") + what)


def at_st(fx_dir, xfx_dir, size):
    snaps = {"at-st": 0, "at-st-down12": 12, "at-st-up6": 6}
    sides = {}
    for side, d in (("fx", fx_dir), ("xfx", xfx_dir)):
        sides[side] = {s: shape(os.path.join(d, f"{side}-p5-input-at-{size}-{s}.ansi")) for s in snaps}
    for s in snaps:
        f, x = sides["fx"][s], sides["xfx"][s]
        for key in ("composer_row", "list_rows", "bar", "hint"):
            check(f[key] == x[key], f"p5-input-at {size} {s}: {key} fx={f[key]!r} xfx={x[key]!r}")
    # The window each snap shows, over each side's own ranking: the bar on the
    # marked rank, the window edge-scrolled over the rows the list has.
    for side in ("fx", "xfx"):
        shown = sides[side]
        rows = shown["at-st"]["list_rows"]
        start, windows = 0, {}
        for s, selected in snaps.items():
            start = edge(start, 30, selected, rows)
            windows[s] = start
            check(shown[s]["bar"] == selected - start, f"{side} {size} {s}: bar on rank {selected}, window from rank {start}")
        by_rank = {}
        for s, start in windows.items():
            for i, path in enumerate(shown[s]["paths"]):
                by_rank.setdefault(start + i, set()).add(path)
        clash = {rank: paths for rank, paths in by_rank.items() if len(paths) > 1}
        check(not clash, f"{side} {size}: every rank holds one path across the three snaps {clash or ''}")


def at_src(fx_dir, xfx_dir, size):
    """`@src/`: fx lists the folder itself first (I-U3), so the rows differ;
    the band's shape does not."""
    f = shape(os.path.join(fx_dir, f"fx-p5-input-at-{size}-at-src.ansi"))
    x = shape(os.path.join(xfx_dir, f"xfx-p5-input-at-{size}-at-src.ansi"))
    for key in ("composer_row", "list_rows", "bar", "hint"):
        check(f[key] == x[key], f"p5-input-at {size} at-src: {key} fx={f[key]!r} xfx={x[key]!r}")


def resize(fx_dir, xfx_dir, size):
    for s in ("short", "short-up1", "short-up7", "tall", "tall-down1"):
        f_path = os.path.join(fx_dir, f"fx-p5-input-at-resize-{size}-{s}.ansi")
        x_path = os.path.join(xfx_dir, f"xfx-p5-input-at-resize-{size}-{s}.ansi")
        (_, f_rows), (_, x_rows) = band(f_path), band(x_path)
        same = len(f_rows) == len(x_rows) and all(a == b for a, b in zip(f_rows, x_rows))
        check(same, f"p5-input-at-resize {size} {s}: the band, {len(f_rows)} rows on fx and {len(x_rows)} on xfx, cell for cell")


def main():
    fx_dir, xfx_dir, sizes = sys.argv[1], sys.argv[2], sys.argv[3:]
    for size in sizes:
        at_st(fx_dir, xfx_dir, size)
        at_src(fx_dir, xfx_dir, size)
    if os.path.exists(os.path.join(fx_dir, "fx-p5-input-at-resize-40x120-short.ansi")):
        resize(fx_dir, xfx_dir, "40x120")
    print(f"band: {checks} checks, {failed} failed")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
