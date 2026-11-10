#!/usr/bin/env python3
"""Train 14 (new), join check 2's reading: for each sweep output directory
given (joins.sh's `walk` runs of `p5-input-row-limit`), the `at` snap's
verdict (pairs.json) and its row 5 on fx's side and on xfx's (the `.txt`
capture, its fifth line as captured, trailing blanks dropped), and whether
xfx's row 5 is the index's `indexing files...` placeholder.

    python3 -I -B walk_rows.py <sweep-out>...
"""
import glob
import json
import os
import sys

for out in sys.argv[1:]:
    pairs = {p["label"]: p for p in json.load(open(os.path.join(out, "pairs.json"))) if p["scenario"] == "p5-input-row-limit"}
    rows = {}
    for side in ("fx", "xfx"):
        hits = glob.glob(os.path.join(out, "runs", "p5-input-row-limit", side, f"{side}-p5-input-row-limit-*-at.txt"))
        lines = open(hits[0], encoding="utf-8").read().split("\n") if len(hits) == 1 else []
        rows[side] = lines[4].rstrip() if len(lines) > 4 else None
    at = pairs.get("at", {})
    print(f"{os.path.basename(out)}: at {at.get('verdict')}; row 5 fx {rows['fx']!r}; xfx {rows['xfx']!r}; "
          f"xfx says indexing: {'indexing files' in (rows['xfx'] or '')}; at-pgdn {pairs.get('at-pgdn', {}).get('verdict')}")
