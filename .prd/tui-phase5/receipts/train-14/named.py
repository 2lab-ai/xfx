#!/usr/bin/env python3
"""Train 14 (new; train 13's named.sh ran its named tests again, this reads
the gate's own runs): each test the join checks name, found in the gate's
two full `cargo test` logs (gates.sh's `test` and `test-fault` steps), with
its result line in each. A name found in neither, or found failing, makes
the exit 1.

    python3 -I -B named.py <gate-log-dir> <check>:<test name>...
"""
import glob
import os
import re
import sys

logs = sys.argv[1]
runs = {}
for step in ("test", "test-fault"):
    hits = glob.glob(os.path.join(logs, f"*-{step}.log"))
    runs[step] = open(hits[0], encoding="utf-8", errors="replace").read() if len(hits) == 1 else ""
bad = 0
for spec in sys.argv[2:]:
    check, name = spec.split(":", 1)
    out = []
    for step, text in runs.items():
        m = re.findall(r"^test (\S*::)?" + re.escape(name) + r" \.\.\. (\w+)$", text, re.M)
        out.append(f"{step} {m[0][1] if len(m) == 1 else ('not found' if not m else f'{len(m)} matches')}")
        bad += not (len(m) == 1 and m[0][1] == "ok")
    print(f"{check:10} {name}: {'; '.join(out)}")
print(f"# {len(sys.argv) - 2} tests named; {'every one ok in both runs' if not bad else f'{bad} results not ok or not found'}")
sys.exit(1 if bad else 0)
