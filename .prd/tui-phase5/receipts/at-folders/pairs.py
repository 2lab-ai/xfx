"""Every capture pair in this folder through cells.py (row 1, each product's
banner, left out): fx/ against xfx/ (the head) and against
xfx-base/ (the base, af2b6137), one line per pair with each side's
count of differing rows.

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


def main():
    fx_files = sorted(glob.glob(os.path.join(HERE, "fx", "*.ansi")))
    first = "x" + os.path.basename(fx_files[0])
    print("banners: head", banner(os.path.join(HERE, "xfx", first)), "| base", banner(os.path.join(HERE, "xfx-base", first)))
    print(f"{'pair':58} {'base':>5} {'head':>5}")
    same = 0
    for fx in fx_files:
        name = os.path.basename(fx)
        head = differ(fx, os.path.join(HERE, "xfx", "x" + name))
        base = differ(fx, os.path.join(HERE, "xfx-base", "x" + name))
        same += head == 0
        print(f"{name[3:-5]:58} {base:>5} {head:>5}")
    print(f"pairs: {len(fx_files)}; identical on the head (0 rows differ): {same}")


if __name__ == "__main__":
    main()
