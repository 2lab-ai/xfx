#!/usr/bin/env python3
"""Train 12, join check 4: INPUT-ROW-LIMIT's 8-row scenario at the head, at
the 8x120 its tracked `-short-8x120-` captures were taken at. sweep.sh ran
both captures (`SIZE=8x120`, `irl8-selection.txt`) and then stopped on its
own inventory, which needs fx's `/help` menu and an 8-row screen cannot show
one (`sweep: a help snap shows no menu`, exit 2) -- so no scoreboard; the
pairs are read as the unit's pairs.py reads them: each fx/xfx `.ansi` pair
through scripts/fxcap/cells.py (row 1, each product's banner, left out), its
count of differing rows, then each `.cursor` (the caret) side by side.

    python3 -I -B short8.py <sweep-out>
"""
import glob
import os
import subprocess
import sys

sys.dont_write_bytecode = True
HERE = os.path.dirname(os.path.realpath(__file__))
CELLS = os.path.join(HERE, "..", "..", "..", "..", "scripts", "fxcap", "cells.py")
runs = os.path.join(sys.argv[1], "runs", "p5-input-row-limit-short")
same = 0
fxs = sorted(glob.glob(os.path.join(runs, "fx", "*.ansi")))
for fx in fxs:
    xfx = os.path.join(runs, "xfx", "x" + os.path.basename(fx))
    out = subprocess.run([sys.executable, "-I", CELLS, fx, xfx], capture_output=True, text=True).stdout
    n = int(out.strip().splitlines()[-1].split()[0])
    same += n == 0
    print(f"{os.path.basename(fx)[3:-5]:44} rows differing {n}")
banner = open(glob.glob(os.path.join(runs, "xfx", "*.txt"))[0], encoding="utf-8").readline().split(" · ")[0]
print(f"# {len(fxs)} pairs, {same} with no differing row; xfx's banner: {banner}")
for fc in sorted(glob.glob(os.path.join(runs, "fx", "*.cursor"))):
    xc = os.path.join(runs, "xfx", "x" + os.path.basename(fc))
    a, b = (open(p, encoding="utf-8").read().strip() for p in (fc, xc))
    print(f"caret {os.path.basename(fc)[3:-7]:38} fx {a:6} xfx {b:6} {'same' if a == b else 'DIFFERENT'}")
