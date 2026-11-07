"""The four new scenarios' pairs, base against head: each snap's verdict in
two sweeps' `pairs.json`, and the entries that cover its rows.

usage: python3 -I pairs.py <base sweep out> <head sweep out>
"""
import json
import os
import sys


def load(out):
    with open(os.path.join(out, "pairs.json"), encoding="utf-8") as f:
        return {(p["scenario"], p["label"]): p for p in json.load(f) if p["scenario"].startswith("p5-ctrlo-wider-")}


base, head = (load(p) for p in sys.argv[1:3])
print("| scenario | snap | base (e6cde019), rows differing | head (0b4807bc), rows differing | head's rows covered by |")
print("|---|---|---|---|---|")
for key in sorted(set(base) | set(head)):
    b, h = base.get(key), head.get(key)
    ids = sorted({i for r in (h or {}).get("rows", []) for i in r.get("matched") or []})
    print("| %s | %s | %s (%d) | %s (%d) | %s |" % (
        key[0], key[1],
        b["verdict"] if b else "-", len(b["rows"]) if b else 0,
        h["verdict"] if h else "-", len(h["rows"]) if h else 0,
        ", ".join(ids) or "-"))
