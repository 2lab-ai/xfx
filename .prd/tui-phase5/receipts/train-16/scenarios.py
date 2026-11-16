#!/usr/bin/env python3
"""Train 16: train 15's scenarios.py, unchanged but for this line.
Train 15 (new), join checks 1 and 4: named scenarios read in the two
sweeps, pair by pair. For each glob, every pair of a matching scenario, its
verdict at the base (rescored with the head's ledger) and at the head, and
for an unresolved pair the causes the sweep named and whether a row no cause
covers was left (`unattributed`); `--cause <id>`: the pairs that name it, on
each side. A pair the base did not take is `new`.

    python3 -I -B scenarios.py <base-sweep-out> <head-sweep-out> [--cause <id>]... <glob>...
"""
import fnmatch
import json
import os
import sys

base, head = sys.argv[1:3]
args = sys.argv[3:]
causes = [args[i + 1] for i, a in enumerate(args) if a == "--cause"]
globs = [a for i, a in enumerate(args) if a != "--cause" and (i == 0 or args[i - 1] != "--cause")]
A, B = ({(p["scenario"], p["label"]): p for p in json.load(open(os.path.join(d, "pairs.json"), encoding="utf-8"))}
        for d in (base, head))
PASS = ("identical", "normalized-identical")


def say(p):
    if p is None:
        return "new"
    s = p["verdict"]
    if s == "unresolved-defect":
        s += f" [{', '.join(p.get('causes', [])) or 'no cause'}{'; a row no cause covers' if p.get('unattributed') else ''}]"
    return s


for g in globs:
    keys = sorted(k for k in B if fnmatch.fnmatch(k[0], g))
    up = sum(1 for k in keys if B[k]["verdict"] in PASS and (k not in A or A[k]["verdict"] not in PASS))
    down = sum(1 for k in keys if k in A and A[k]["verdict"] in PASS and B[k]["verdict"] not in PASS)
    print(f"## {g}: {len(keys)} pairs at the head ({sum(1 for k in keys if k in A)} at the base too); "
          f"pass at the head {sum(1 for k in keys if B[k]['verdict'] in PASS)}, at the base "
          f"{sum(1 for k in keys if k in A and A[k]['verdict'] in PASS)}; up {up}, down {down}")
    for k in keys:
        print(f"    {k[0]} {k[1]}: base {say(A.get(k))} -> head {say(B[k])}")
for c in causes:
    for name, S in (("base", A), ("head", B)):
        hit = sorted(k for k, p in S.items() if c in p.get("causes", []))
        print(f"## cause {c} at the {name}: {len(hit)} pairs name it" + "".join(f"\n    {k[0]} {k[1]}" for k in hit))
