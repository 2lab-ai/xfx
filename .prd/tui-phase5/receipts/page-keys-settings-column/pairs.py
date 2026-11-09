"""Every capture pair in this folder through cells.py (row 1, each product's
banner, left out): fx/ against xfx/ (the head), xfx-r1/ (round 1's head)
and xfx-base/ (the base), one line per pair with each side's count of
differing rows; then each `.cursor` (the caret, `x,y`) the same way.

usage: python3 -I pairs.py
"""

import glob
import os
import subprocess
import sys

sys.dont_write_bytecode = True
HERE = os.path.dirname(os.path.realpath(__file__))
CELLS = os.path.join(HERE, "..", "..", "..", "..", "scripts", "fxcap", "cells.py")
SIDES = ("xfx-base", "xfx-r1", "xfx")


def differ(fx, xfx):
    if not os.path.exists(xfx):
        return "-"
    out = subprocess.run([sys.executable, "-I", CELLS, fx, xfx], capture_output=True, text=True).stdout
    last = out.strip().splitlines()[-1]
    return int(last.split()[0])


def banner(path):
    with open(path.replace(".ansi", ".txt"), encoding="utf-8") as f:
        return f.readline().split(" · ")[0].strip()


def read(path):
    if not os.path.exists(path):
        return "-"
    with open(path, encoding="utf-8") as f:
        return f.read().strip()


def main():
    fx_files = sorted(glob.glob(os.path.join(HERE, "fx", "*.ansi")))
    for side in ("fx",) + SIDES:
        names = sorted({banner(p) for p in glob.glob(os.path.join(HERE, side, "*.ansi"))})
        print(f"banners {side}: {', '.join(names)}")
    print(f"{'pair':52} {'base':>5} {'r1':>5} {'head':>5}")
    same = 0
    differs = {side: 0 for side in SIDES}
    for fx in fx_files:
        name = os.path.basename(fx)
        counts = [differ(fx, os.path.join(HERE, side, "x" + name)) for side in SIDES]
        for side, n in zip(SIDES, counts):
            differs[side] += n not in (0, "-")
        same += counts[-1] == 0
        print(f"{name[3:-5]:52} " + " ".join(f"{n:>5}" for n in counts))
    print(f"pairs: {len(fx_files)}; identical on the head (0 rows differ): {same}; "
          + "; ".join(f"differing on {side}: {n}" for side, n in differs.items()))
    print(f"\n{'caret':52} {'fx':>7} {'base':>7} {'r1':>7} {'head':>7}")
    for fx in sorted(glob.glob(os.path.join(HERE, "fx", "*.cursor"))):
        name = os.path.basename(fx)
        f = read(fx)
        got = [read(os.path.join(HERE, side, "x" + name)) for side in SIDES]
        print(f"{name[3:-7]:52} {f:>7} " + " ".join(f"{g:>7}" for g in got) + f"  {'same' if got[-1] == f else 'DIFFERS'}")


if __name__ == "__main__":
    main()
