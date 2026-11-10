#!/usr/bin/env python3
"""Train 12, join check 4: every pair of a sweep (its pairs.json) whose
scenario starts with <prefix>, with its verdict and, for a pair that is not
identical, how many rows differ -- for INPUT-ROW-LIMIT's p5-input-row-limit*
pairs at the head, read against the unit's own sweep and pairs.txt.

    python3 -I -B pairs_of.py <sweep-out> <prefix>
"""
import json
import os
import sys

out, prefix = sys.argv[1:3]
with open(os.path.join(out, "pairs.json"), encoding="utf-8") as f:
    pairs = [p for p in json.load(f) if p["scenario"].startswith(prefix)]
for p in pairs:
    rows = len(p.get("rows", []))
    print(f"{p['scenario']:28} {p['label']:14} {p['verdict']}{'' if p['verdict'] == 'identical' else f' ({rows} rows differ)'}")
passed = sum(p["verdict"] in ("identical", "normalized-identical") for p in pairs)
print(f"# {len(pairs)} pairs, {passed} pass")
