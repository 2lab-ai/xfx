"""Every pair's verdicts across repeat.sh's runs: which vary from run to run,
and which ci_sweep.py could call a regression.

usage: python3 -I verdicts.py <repeat out>

A pair passes as ci_sweep.py's PASS has it (identical, normalized-identical).
It varies when its runs on one build do not all pass or all fail. A run pair
(head-i, base-j) would be a regression for it when base-j passes it and
head-i does not (ci_sweep.py:427); the count is over every such pairing.
"""
import collections
import glob
import json
import os
import sys

PASS = ("identical", "normalized-identical")
out = sys.argv[1]
runs = collections.defaultdict(dict)  # build -> run -> {(scen, label): verdict}
for d in sorted(glob.glob(os.path.join(out, "*-*"))):
    f = os.path.join(d, "pairs.json")
    if not os.path.isfile(f):
        continue
    build, i = os.path.basename(d).rsplit("-", 1)
    runs[build][int(i)] = {(p["scenario"], p["label"]): p["verdict"] for p in json.load(open(f))}
keys = sorted({k for b in runs.values() for r in b.values() for k in r})
print("runs: %s" % ", ".join("%s %d" % (b, len(r)) for b, r in sorted(runs.items())))
scen_pass = collections.defaultdict(lambda: collections.Counter())
varying, regress = [], []
for k in keys:
    p = {b: [r[i].get(k) in PASS for i in sorted(r)] for b, r in runs.items()}
    for b, v in p.items():
        scen_pass[(k[0], b)]["pass"] += sum(v)
        scen_pass[(k[0], b)]["pairs"] += len(v)
        if 0 < sum(v) < len(v):
            varying.append((k, b, sum(v), len(v)))
    if "head" in p and "base" in p:
        n = sum(1 for h in p["head"] for bb in p["base"] if bb and not h)
        if n:
            regress.append((k, n, len(p["head"]) * len(p["base"])))
print("\nper scenario and build: pairs passed over all runs")
for (s, b), c in sorted(scen_pass.items()):
    print("  %-26s %-5s %4d of %4d" % (s, b, c["pass"], c["pairs"]))
print("\npairs whose verdict varies across one build's runs: %d" % len(varying))
for k, b, n, m in varying:
    print("  %s %s on %s: passes %d of %d" % (k[0], k[1], b, n, m))
print("\npairs a (head run, base run) pairing would call a regression: %d" % len(regress))
for k, n, m in regress:
    print("  %s %s: %d of %d pairings" % (k[0], k[1], n, m))
