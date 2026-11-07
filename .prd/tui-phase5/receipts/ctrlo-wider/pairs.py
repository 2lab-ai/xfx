"""The p5-ctrlo-wider scenarios' pairs, base against head: each snap's
verdict in two sweeps' `pairs.json`, and the entries that cover its rows.

usage: python3 -I pairs.py <base sweep out> <head sweep out> [<base label> <head label>]
"""
import json
import os
import sys


def load(out):
    with open(os.path.join(out, "pairs.json"), encoding="utf-8") as f:
        return {(p["scenario"], p["label"]): p for p in json.load(f) if p["scenario"].startswith("p5-ctrlo-wider-")}


base, head = (load(p) for p in sys.argv[1:3])
labels = sys.argv[3:5] if len(sys.argv) > 4 else ["e6cde019", "0b4807bc"]
print("| scenario | snap | base (%s), rows differing | head (%s), rows differing | head's rows covered by |" % tuple(labels))
print("|---|---|---|---|---|")
for key in sorted(set(base) | set(head)):
    b, h = base.get(key), head.get(key)
    ids = sorted({i for r in (h or {}).get("rows", []) for i in r.get("matched") or []})
    print("| %s | %s | %s (%d) | %s (%d) | %s |" % (
        key[0], key[1],
        b["verdict"] if b else "-", len(b["rows"]) if b else 0,
        h["verdict"] if h else "-", len(h["rows"]) if h else 0,
        ", ".join(ids) or "-"))
