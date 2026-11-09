#!/usr/bin/env python3
"""The two sweeps' pairs side by side: every snap pair's verdict on the base's
build and on the head's, and each one that moved.

    python3 -I -B cmp_sweeps.py <base-sweep-dir> <head-sweep-dir>

A pair `drops` when its head verdict is worse than its base verdict in the
order pass (identical, normalized-identical) < approved-divergence(-cascade)
< replaced-in-next-pin < unresolved-defect < failed < timeout. A move between
identical and normalized-identical is printed and is not a drop: both pass.
"""
import json
import os
import sys

ORDER = ["identical", "normalized-identical", "approved-divergence", "approved-divergence-cascade",
         "replaced-in-next-pin", "unresolved-defect", "failed", "timeout"]
RANK = {"identical": 0, "normalized-identical": 0, "approved-divergence": 1, "approved-divergence-cascade": 1,
        "replaced-in-next-pin": 2, "unresolved-defect": 3, "failed": 4, "timeout": 5}


def pairs(d):
    return {(p["scenario"], p["label"]): p for p in json.load(open(os.path.join(d, "pairs.json")))}


base, head = pairs(sys.argv[1]), pairs(sys.argv[2])
drops = moved = 0
for key in sorted(set(base) | set(head)):
    b = base.get(key, {}).get("verdict", "absent")
    h = head.get(key, {}).get("verdict", "absent")
    if b == h:
        continue
    moved += 1
    worse = b == "absent" or h == "absent" or RANK[h] > RANK[b]
    drops += worse
    tag = "DROP" if worse else ("pass" if RANK[h] == RANK[b] else "up")
    print("%-6s %s/%s: %s -> %s" % (tag, key[0], key[1], b, h))
same = sum(1 for k in base if k in head and base[k]["verdict"] == head[k]["verdict"])
print("%d pairs on both; %d the same verdict, %d moved, %d dropped" % (len(set(base) & set(head)), same, moved, drops))
for name, d in (("base", base), ("head", head)):
    counts = {}
    for p in d.values():
        counts[p["verdict"]] = counts.get(p["verdict"], 0) + 1
    print("%s: %s" % (name, ", ".join("%s %d" % (v, counts[v]) for v in ORDER if v in counts)))
sys.exit(1 if drops else 0)
