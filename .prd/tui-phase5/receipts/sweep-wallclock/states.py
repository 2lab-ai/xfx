"""What each catalog snap shows, in each sweep of a set (sweeps.sh's run-<i>
directories), on each binary: FAIL (`unable to load models` or `Could not
reach AI Gateway`: the catalog failed), ROWS (`gpt-6-astra`, a row only the
catalog lists here), or other. The snaps whose purpose is one or the other.

usage: python3 -I states.py <label> <sweeps-root>
"""

import glob
import os
import sys

label, root = sys.argv[1], sys.argv[2]
SNAPS = [
    ("model-column-idle", "col-down"), ("model-column-idle", "col-idle"), ("model-column-idle", "col-typed"),
    ("model-column-reask", "col-down"), ("model-column-reask", "col-back"),
    ("model-reask", "menu-failed"), ("model-reask", "reask-midturn"),
    ("p5-input-model-fail", "fail-open"), ("p5-input-model-fail", "after-outage"),
]
runs = sorted((p for p in glob.glob(os.path.join(root, "run-*")) if os.path.isdir(p)), key=lambda p: int(p.rsplit("run-", 1)[1]))
print("# %s: per snap, what it shows in run 1..%d, fx then xfx (FAIL the failure's sentence, ROWS the catalog's rows)" % (label, len(runs)))
for scen, snap in SNAPS:
    cells = []
    for run in runs:
        pair = []
        for side in ("fx", "xfx"):
            path = os.path.join(run, "runs", scen, side, "%s-%s-40x120-%s.txt" % (side, scen, snap))
            try:
                with open(path, encoding="utf-8", errors="replace") as f:
                    text = f.read()
            except OSError:
                pair.append("none")
                continue
            pair.append("FAIL" if ("unable to load models" in text or "Could not reach AI Gateway" in text) else "ROWS" if "gpt-6-astra" in text else "other")
        cells.append("/".join(pair))
    print("%-20s %-14s %s" % (scen, snap, " ".join(cells)))
