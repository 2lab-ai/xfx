"""Every pair's verdict in each sweep of a set (sweeps.sh's run-<i>
directories), the pairs whose verdict is not the same in every run (a flip),
and, per run and binary, the catalog asks the fake gateway logged for each
scenario that keeps its log (`gwlog`): 5 = 503 (the outage), 2 = 200, and
. for a read asked and not yet answered when the log was copied.

usage: python3 -I verdicts.py <label> <sweeps-root>
Prints a plain-text table; reads pairs.json (the sweep's raw classification)
and the captured gateway logs, and prints no path.
"""

import glob
import json
import os
import re
import sys

label, root = sys.argv[1], sys.argv[2]
runs = sorted(glob.glob(os.path.join(root, "run-*", "pairs.json")), key=lambda p: int(p.split("run-")[-1].split(os.sep)[0]))
table = {}
for i, path in enumerate(runs, 1):
    with open(path, encoding="utf-8") as f:
        for p in json.load(f):
            key = (p["scenario"], p["label"])
            # The causes the sweep gave the pair's rows, and `?` when some row has none.
            causes = ",".join(p.get("causes") or []) + ("?" if p.get("unattributed") else "")
            table.setdefault(key, {})[i] = p["verdict"] + ("[" + causes + "]" if causes else "")
short = {"identical": "ID", "normalized-identical": "NI", "unresolved-defect": "UD", "failed": "FAIL", "timeout": "TO"}
n = len(runs)
print("# %s: %d sweeps; per pair the verdict in run 1..%d (ID identical, NI normalized-identical, UD unresolved-defect[its causes; ? = a row no cause covers])" % (label, n, n))
flips = 0
for (scen, lab), by in sorted(table.items()):
    vs = [by.get(i, "-") for i in range(1, n + 1)]
    plain = [v.split("[")[0] for v in vs]
    passed = sum(v in ("identical", "normalized-identical") for v in plain)
    flip = len(set(plain)) > 1
    flips += flip
    print("%-28s %-16s %s  pass %d/%d%s" % (scen, lab, " ".join(short.get(v.split("[")[0], v.split("[")[0]) + (("[" + v.split("[", 1)[1]) if "[" in v else "") for v in vs), passed, n, "  FLIP" if flip else ""))
print("# pairs %d, flipping %d" % (len(table), flips))
print("# catalog asks per run, from each capture's gateway log, in order answered (5 = 503 down, 2 = 200; . = asked, not answered when the log was copied)")
for scen in sorted({s for s, _ in table}):
    for side in ("fx", "xfx"):
        seqs = []
        for i in range(1, n + 1):
            logs = glob.glob(os.path.join(root, "run-%d" % i, "runs", scen, side, "%s-%s-*-gateway.log" % (side, scen)))
            if not logs:
                seqs = None
                break
            with open(logs[0], encoding="utf-8", errors="replace") as f:
                text = f.read()
            got = re.findall(r"request GET /coding-agent/v1/models at=\d+ms -> (\d+)", text)
            asked = len(re.findall(r"\] GET /coding-agent/v1/models bearer=", text))
            seqs.append(("".join(c[0] for c in got) + "." * max(0, asked - len(got))) or "none")
        if seqs:
            print("%-28s %-4s %s" % (scen, side, " ".join(seqs)))
