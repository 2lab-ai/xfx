#!/usr/bin/env python3
"""Base against head, pair by pair, over the model scenarios: each pair's
verdict in the base sweep and the head sweep, what moved. A scenario a sweep
did not run (a new one) is read from the extra sweep named after it.
usage: compare.py <base-sweep> <head-sweep> [<extra-base> <extra-head>]"""
import json, sys
PASS = ("identical", "normalized-identical")
def load(*dirs):
    out = {}
    for d in dirs:
        for x in json.load(open(f"{d}/pairs.json")):
            out[(x["scenario"], x["label"])] = x
    return out
base = load(sys.argv[1], *sys.argv[3:4]); head = load(sys.argv[2], *sys.argv[4:5])
keys = sorted(set(base) | set(head))
moved = {"up": [], "down": [], "both": [], "fail-both": []}
for k in keys:
    b = base.get(k, {}).get("verdict", "absent"); h = head.get(k, {}).get("verdict", "absent")
    pb, ph = b in PASS, h in PASS
    key = "up" if ph and not pb else "down" if pb and not ph else "both" if pb else "fail-both"
    moved[key].append((k, b, h, head.get(k, {}).get("rows", [])))
print(f"pairs {len(keys)}; base pass {sum(v.get('verdict') in PASS for v in base.values())}; head pass {sum(v.get('verdict') in PASS for v in head.values())}")
for name in ("up", "down", "fail-both"):
    print(f"## {name}: {len(moved[name])}")
    for (sc, lab), b, h, rows in moved[name]:
        diff = "; ".join(f"row {r['row']}: fx `{r['fx']}` / xfx `{r['xfx']}`" for r in rows[:1])
        print(f"- {sc} {lab}: {b} -> {h}" + (f"  ({diff})" if diff and h not in PASS else ""))
print(f"## pass both: {len(moved['both'])}")
