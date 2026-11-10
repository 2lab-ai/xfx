"""The sample's per-scenario pass counts beside train 12's head sweep's.

usage: python3 -I sample_vs_head.py <sweep-result.json> <train-12/sweep-head.md>"""
import json
import sys

res = json.load(open(sys.argv[1]))
# The scoreboard's scenario table only (its group table has rows of the same
# shape): scenario | status | total | ran | identical | normalized-identical
# | ...; a pass is either of those two. (Its first form read only the
# normalized column, from any table; the result held because no row of these
# 11 had an identical pair, and a group named like its one scenario,
# `startup`, has the same counts.)
head, on = {}, False
for line in open(sys.argv[2]):
    if line.startswith("| scenario | status |"):
        on = True
        continue
    cells = [c.strip() for c in line.strip().strip("|").split("|")]
    if on and len(cells) >= 13 and cells[1] in ("ran", "failed", "timeout"):
        head[cells[0]] = (int(cells[4]) + int(cells[5]), int(cells[2]))
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
