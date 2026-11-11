#!/usr/bin/env python3
# Train 14: train 13's new_scenarios.py, copied; what changed for this train, if anything, is said below this line.
"""Train 13: the pairs of a train sweep's scenarios that the reference sweep
did not take (the units' new scenarios), per scenario: the verdicts, and for
each unresolved pair the causes the sweep named and whether a row no cause
covers was left (`unattributed`).

    python3 -I -B new_scenarios.py <reference-sweep-out> <train-sweep-out>
"""
import collections
import json
import os
import sys

a, b = (json.load(open(os.path.join(p, "pairs.json"), encoding="utf-8")) for p in sys.argv[1:3])
old = {p["scenario"] for p in a}
new = [p for p in b if p["scenario"] not in old]
by = collections.defaultdict(list)
for p in new:
    by[p["scenario"]].append(p)
PASS = ("identical", "normalized-identical")
total = passed = 0
for sc in sorted(by):
    ps = by[sc]
    verdicts = collections.Counter(p["verdict"] for p in ps)
    causes = collections.Counter((tuple(p.get("causes", [])), bool(p.get("unattributed")))
                                 for p in ps if p["verdict"] == "unresolved-defect")
    n = sum(verdicts[v] for v in PASS)
    total, passed = total + len(ps), passed + n
    print(f"{sc}: {len(ps)} pairs, pass {n}; " + ", ".join(f"{v} {c}" for v, c in sorted(verdicts.items())))
    for (cs, loose), c in sorted(causes.items()):
        print(f"    unresolved {c}: causes {', '.join(cs) or 'none'}; a row no cause covers: {'yes' if loose else 'no'}")
print(f"# {len(by)} scenarios the reference did not take, {total} pairs, {passed} pass")
