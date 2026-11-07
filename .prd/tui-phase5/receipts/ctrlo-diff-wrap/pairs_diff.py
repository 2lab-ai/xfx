"""Base sweep against head sweep, pair by pair: which verdicts moved.

usage: python3 -I pairs_diff.py <base pairs.json> <head pairs.json>
"""
import json, sys
PASS = ("identical", "normalized-identical")
RANK = {"identical": 0, "normalized-identical": 0, "approved-divergence": 1, "approved-divergence-cascade": 1,
        "replaced-in-next-pin": 1, "unresolved-defect": 2, "failed": 3, "timeout": 3}
base = {(p["scenario"], p["label"]): p["verdict"] for p in json.load(open(sys.argv[1]))}
head = {(p["scenario"], p["label"]): p["verdict"] for p in json.load(open(sys.argv[2]))}
keys = sorted(set(base) | set(head))
moved, dropped = [], []
for k in keys:
    a, b = base.get(k, "(absent)"), head.get(k, "(absent)")
    if a != b:
        moved.append((k, a, b))
        if a in RANK and (b not in RANK or RANK[b] > RANK[a]):
            dropped.append((k, a, b))
n = lambda d, v: sum(1 for x in d.values() if x in v)  # noqa: E731
print("pairs: base %d, head %d" % (len(base), len(head)))
print("pass: base %d / %d, head %d / %d" % (n(base, PASS), len(base), n(head, PASS), len(head)))
print("verdicts that moved: %d" % len(moved))
for (s, l), a, b in moved:
    print("  %s/%s: %s -> %s" % (s, l, a, b))
print("dropped: %d" % len(dropped))
for (s, l), a, b in dropped:
    print("  DROP %s/%s: %s -> %s" % (s, l, a, b))
sys.exit(1 if dropped else 0)
