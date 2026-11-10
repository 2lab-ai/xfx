"""The sample's per-scenario pass counts beside train 12's head sweep's.

usage: python3 -I sample_vs_head.py <sweep-result.json> <train-12/sweep-head.md>"""
import json
import re
import sys

res = json.load(open(sys.argv[1]))
head = {}
for line in open(sys.argv[2]):
    cells = [c.strip().strip("`") for c in line.strip().strip("|").split("|")]
    if len(cells) > 6 and re.fullmatch(r"\d+", cells[2] or "") and re.fullmatch(r"\d+", cells[5] or ""):
        head[cells[0]] = (int(cells[5]), int(cells[2]))
PASS = ("identical", "normalized-identical")
counts = {}
for pair in res["pairs"]:
    c = counts.setdefault(pair["scenario"], [0, 0])
    c[1] += 1
    c[0] += pair["head"] in PASS
same = 0
for name, (passed, pairs) in counts.items():
    h = head.get(name)
    ok = h == (passed, pairs)
    same += ok
    print("%-28s sample %2d/%-2d  train-12 head %-6s %s" % (name, passed, pairs, "%d/%d" % h if h else "-", "same" if ok else "DIFFERS"))
print("%d of %d scenarios the same; sample %d/%d" % (same, len(counts), sum(c[0] for c in counts.values()), sum(c[1] for c in counts.values())))
