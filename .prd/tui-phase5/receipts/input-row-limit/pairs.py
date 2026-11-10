"""Every capture pair in this folder through cells.py (row 1, each product's
banner, left out): fx/ against xfx/ (the head) and against xfx-base/ (the
base), one line per pair with each side's count of differing rows, then each
`.cursor` (the caret, `x,y`) the same way.

usage: python3 -I pairs.py
"""

import glob
import os
import subprocess
import sys

sys.dont_write_bytecode = True
HERE = os.path.dirname(os.path.realpath(__file__))
CELLS = os.path.join(HERE, "..", "..", "..", "..", "scripts", "fxcap", "cells.py")


def differ(fx, xfx):
    out = subprocess.run([sys.executable, "-I", CELLS, fx, xfx], capture_output=True, text=True).stdout
    last = out.strip().splitlines()[-1]
    return int(last.split()[0])


def banner(path):
    with open(path.replace(".ansi", ".txt"), encoding="utf-8") as f:
        return f.readline().split(" · ")[0]


def read(path):
    with open(path, encoding="utf-8") as f:
        return f.read().strip()


def main():
    fx_files = sorted(glob.glob(os.path.join(HERE, "fx", "*.ansi")))
    for side in ("xfx", "xfx-base"):
        names = sorted({banner(p) for p in glob.glob(os.path.join(HERE, side, "*.ansi"))})
        print(f"banners {side}: {', '.join(names)}")
    print(f"{'pair':48} {'base':>5} {'head':>5}")
    same = 0
    for fx in fx_files:
        name = os.path.basename(fx)
        head = differ(fx, os.path.join(HERE, "xfx", "x" + name))
        base = differ(fx, os.path.join(HERE, "xfx-base", "x" + name))
        same += head == 0
        print(f"{name[3:-5]:48} {base:>5} {head:>5}")
    print(f"pairs: {len(fx_files)}; identical on the head (0 rows differ): {same}")
    print(f"\n{'caret':48} {'fx':>7} {'base':>7} {'head':>7}")
    for fx in sorted(glob.glob(os.path.join(HERE, "fx", "*.cursor"))):
        name = os.path.basename(fx)
        f, b, h = read(fx), read(os.path.join(HERE, "xfx-base", "x" + name)), read(os.path.join(HERE, "xfx", "x" + name))
        print(f"{name[3:-7]:48} {f:>7} {b:>7} {h:>7}  {'same' if h == f else 'DIFFERS'}")


if __name__ == "__main__":
    main()
