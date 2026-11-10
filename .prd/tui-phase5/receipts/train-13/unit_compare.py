#!/usr/bin/env python3
"""Train 12: the head's sweep against a unit's own sweep, from the files the
unit's receipt commits -- none of the three units' sweeps left a pairs.json
here, so this reads what each committed (train 10's verdict-level compare,
widened to three formats). Each file is read from git, `<commit>:<path>`.

    python3 -I -B unit_compare.py <repo> <head-sweep-out> runs <commit>:<file>...
        SWEEP-WALLCLOCK-F's `*-verdicts.txt`: per pair its verdict in each of
        its runs (`ID`, `NI`, `UD[...]`); the pair passes on the unit when it
        passed in every run, flips when some runs passed. A later file's pairs
        replace an earlier's (round 2's `green-r2` after `green`).
    python3 -I -B unit_compare.py <repo> <head-sweep-out> tsv <commit>:<file> <column>
        LOCAL-PATHS's `verdicts.tsv`: per pair its verdict in <column>.
    python3 -I -B unit_compare.py <repo> <head-sweep-out> board <commit>:<file>
        a committed scoreboard.md (INPUT-ROW-LIMIT's 41x120 one): its "By
        scenario" table, per scenario the pairs that pass (identical +
        normalized-identical) -- read per scenario, since a scoreboard holds
        no pair's verdict.

"pass" is identical or normalized-identical, the sweep's own measure. Prints
each pair (or scenario) whose verdict moved, and the counts; exit 1 when one
that passes on the unit does not on the head.
"""
import collections
import json
import os
import subprocess
import sys

PASS = ("identical", "normalized-identical")
SHORT = {"ID": "identical", "NI": "normalized-identical", "UD": "unresolved-defect",
         "AD": "approved-divergence", "ADC": "approved-divergence-cascade", "F": "failed", "T": "timeout"}


def show(repo, spec):
    commit, path = spec.split(":", 1)
    return subprocess.run(["git", "-C", repo, "show", f"{commit}:{path}"], check=True, capture_output=True, text=True).stdout


repo, out, kind = sys.argv[1:4]
with open(os.path.join(out, "pairs.json"), encoding="utf-8") as f:
    head = {(p["scenario"], p["label"]): p["verdict"] for p in json.load(f)}
dropped = []
if kind in ("runs", "tsv"):
    unit = {}
    if kind == "runs":
        for spec in sys.argv[4:]:
            for line in show(repo, spec).splitlines():
                if line.startswith("#") or " pass " not in line:
                    continue
                cells = line.split()
                runs = [SHORT.get(c.split("[")[0], c) for c in cells[2:cells.index("pass")]]
                verdict = runs[0] if len(set(runs)) == 1 else "flips " + "/".join(runs)
                unit[(cells[0], cells[1])] = verdict
    else:
        spec, column = sys.argv[4:6]
        rows = show(repo, spec).splitlines()
        at = rows[0].split("\t").index(column)
        for row in rows[1:]:
            cells = row.split("\t")
            unit[(cells[0], cells[1])] = cells[at]
    shared = sorted(set(unit) & set(head))
    missing = sorted(set(unit) - set(head))
    print(f"pairs: unit {len(unit)}, head {len(head)}, shared {len(shared)}; the unit's not in the head's sweep: {len(missing)}"
          + "".join(f"\n    {s} {l}" for s, l in missing))
    for name, side in (("unit", unit), ("head", head)):
        counts = collections.Counter(side[k] for k in shared)
        print(f"{name}: pass {sum(counts[v] for v in PASS)} / {len(shared)}; "
              + ", ".join(f"{v} {n}" for v, n in sorted(counts.items())))
    for key in shared:
        u, h = unit[key], head[key]
        if u != h:
            print(f"moved: {key[0]} {key[1]}: {u} -> {h}")
        if u in PASS and h not in PASS:
            dropped.append(key)
else:
    lines = show(repo, sys.argv[4]).splitlines()
    start = lines.index("## By scenario")
    header = [c.strip() for c in lines[start + 2].strip("|").split("|")]
    unit = {}
    for line in lines[start + 4:]:
        if not line.startswith("|"):
            break
        cells = dict(zip(header, (c.strip() for c in line.strip("|").split("|"))))
        unit[cells["scenario"]] = (int(cells["total"]), int(cells["identical"]) + int(cells["normalized-identical"]))
    heads = collections.defaultdict(lambda: [0, 0])
    for (scenario, _), verdict in head.items():
        heads[scenario][0] += 1
        heads[scenario][1] += verdict in PASS
    print(f"scenarios: unit {len(unit)}, head {len(heads)}, shared {len(set(unit) & set(heads))}")
    tu = th = 0
    for scenario in sorted(unit):
        (n, u), (m, h) = unit[scenario], heads.get(scenario, (0, 0))
        tu, th = tu + u, th + h
        print(f"  {scenario}: unit {u} / {n}, head {h} / {m}{'' if u == h and n == m else '  MOVED'}")
        if h < u:
            dropped.append((scenario, f"{u} -> {h}"))
    print(f"pass: unit {tu}, head {th}")
print(f"passed on the unit, not on the head: {len(dropped)}" + "".join(f"\n    {a} {b}" for a, b in dropped))
sys.exit(1 if dropped else 0)
