#!/usr/bin/env python3
"""Every snap the sweep takes narrower than the widest 0.2.0 banner (74 cells).

    python3 -I -B narrow_snaps.py <scripts/fxcap/scenarios> <run size>[:<scenario glob>]...

Walks each .cap as capture.sh runs it for xfx (capfile.py's reading): the run
starts at the size the sweep was given (`--size`, `40x120` by default) and a
`resize RxC` line changes it. Prints `<run size> <scenario> <snap> <cols>` for
every snap taken narrower than 74 columns, and the count. The run sizes are
the ones the sweep and the receipts take scenarios at: 40x120 (CI and every
full sweep), and the units' own narrow runs -- 24x50 for
`p5-panels-status-short` (P-status, `receipts/info-panel`), 40x60 for
`p5-mcp-menu-new` (MCP-M, `receipts/mcp-menu`) and 40x50 for five
`p5-usage-*` (#62, `receipts/epic-2-integration`). Every other size a receipt
names is 74 columns or wider (30x100, 24x80, 14x120, 12x120).
"""
import fnmatch
import glob
import os
import re
import sys

WIDEST = 74
SNAP = re.compile(r"^(snap|snapfull) (.+)$")
RESIZE = re.compile(r"^resize (\d+)x(\d+)$")

caps, sizes = sys.argv[1], sys.argv[2:]
found = 0
for spec in sizes:
    size, _, only = spec.partition(":")
    for path in sorted(glob.glob(os.path.join(caps, "*.cap"))):
        name = os.path.basename(path)[:-4]
        if name.endswith("-fx") or not fnmatch.fnmatch(name, only or "*"):
            continue
        cols = int(size.split("x")[1])
        for raw in open(path, encoding="utf-8").read().split("\n"):
            if raw.startswith("fx: "):
                continue
            line = raw[5:] if raw.startswith("xfx: ") else raw
            m = RESIZE.match(line)
            if m:
                cols = int(m.group(2))
            m = SNAP.match(line)
            if m and cols < WIDEST:
                found += 1
                print("%s %s %s %d" % (size, name, m.group(2), cols))
print("%d snaps narrower than %d columns" % (found, WIDEST))
