#!/usr/bin/env python3
"""Train 16 (new): the pairs with a row no cause covers (`unattributed` in
pairs.json), in two sweeps, and the difference both ways; for each pair the
head alone has, its diff's first differing cell, read from the head sweep's
`diffs/`; and, for the version-row class, the same cell in the base sweep's
`p5-panels-sweep-a` pairs (the base's own `* version:` row).

    python3 -I -B uncovered.py <base-sweep-out> <head-sweep-out>
"""
import json
import os
import re
import sys

base, head = sys.argv[1:3]


def un(d):
    return {(p["scenario"], p["label"]) for p in json.load(open(os.path.join(d, "pairs.json"), encoding="utf-8"))
            if p.get("unattributed")}


a, b = un(base), un(head)
print(f"# pairs with a row no cause covers: base {len(a)}, head {len(b)}")
print(f"# the base's alone: {sorted(a - b)}")
print(f"# the head's alone: {sorted(b - a)}")


def first_cell(d, sc, label):
    f = os.path.join(d, "diffs", sc, f"{label}.diff")
    if not os.path.exists(f):
        return "no diff"
    text = open(f, encoding="utf-8").read()
    rows = re.findall(r"^  (fx |xfx): (.*)$", text, re.M)[:2]
    cell = re.search(r"^  first differing cell .*$", text, re.M)
    return f"{rows}; {cell.group(0).strip() if cell else ''}"


for sc, label in sorted(b - a):
    print(f"  head {sc} {label}: {first_cell(head, sc, label)}")
for label in ("version", "alias"):
    print(f"  base p5-panels-sweep-a {label}: {first_cell(base, 'p5-panels-sweep-a', label)}")
