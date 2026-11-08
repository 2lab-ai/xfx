#!/usr/bin/env python3
"""The sweep's pairs per family, base against head, from each scoreboard's
per-scenario rows (total pairs, pass %): scenarios, pairs, passing pairs on
each side, and how many scenarios pass fewer pairs on the head.

    python3 -I families.py sweep-base.md sweep-head.md > families.txt
"""
import re
import sys

ROW = re.compile(r"\| ([\w-]+) \| ran \| (\d+) \|.*\| ([\d.]+)% \|")


def scenarios(path):
    out = {}
    for line in open(path, encoding="utf-8"):
        m = ROW.match(line)
        if m:
            total, pct = int(m.group(2)), float(m.group(3))
            out[m.group(1)] = (total, round(total * pct / 100))
    return out


def family(name):
    for prefix in ("p5-transcript", "p5-ctrlo-wider", "p5-toolgroup", "p5-inline-code"):
        if name.startswith(prefix):
            return prefix + "-*"
    if name.startswith(("interrupt", "question", "compact-cancel")):
        return "cancel: interrupt*, question*, compact-cancel"
    return "wraps, resize, fulltranscript*, toolgroup* (added)"


base, head = scenarios(sys.argv[1]), scenarios(sys.argv[2])
print(f"scenarios: base {len(base)}, head {len(head)}, only one side: {sorted(set(base) ^ set(head))}")
print("| family | scenarios | pairs | pass, base | pass, head | scenarios passing fewer on the head |")
print("|---|---|---|---|---|---|")
rows = {}
for name, (total, passed) in base.items():
    row = rows.setdefault(family(name), [0, 0, 0, 0, 0])
    head_passed = head.get(name, (0, 0))[1]
    row[0] += 1
    row[1] += total
    row[2] += passed
    row[3] += head_passed
    row[4] += head_passed < passed
for name, row in rows.items():
    print(f"| {name} | " + " | ".join(str(x) for x in row) + " |")
print("| **all** | " + " | ".join(str(sum(r[i] for r in rows.values())) for i in range(5)) + " |")
