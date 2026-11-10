"""What the base sweep PR #90's run never reached could call a regression.

usage: python3 -I lookahead.py <head scoreboard.json> <head-retry-1 scoreboard.json>
                               <train-12/sweep-main.md> <train-12/sweep-head.md>

Neither side has per-pair verdicts here: the CI artifact keeps each
scenario's counts (scoreboard.json, no pairs.json), and train 12's sweeps
keep their scoreboards' scenario tables. So this compares pass counts per
scenario. pass = identical + normalized-identical, as ci_sweep.py's PASS. A
pair the head retried passes if any run passed it, so a retried scenario
counts its better run. `main` is train 12's sweep of df46777e, the PR's
base; `t12 head` is train 12's sweep of its code head 03fb5436.
"""

import json
import sys

head_sb, retry_sb, main_md, t12_md = sys.argv[1:5]


def table(path):
    """The scenario table of a sweep's scoreboard: {scenario: (pass, total)}."""
    out, on = {}, False
    for line in open(path, encoding="utf-8"):
        if line.startswith("| scenario | status |"):
            on = True
            continue
        if not on or not line.startswith("| ") or line.startswith("|---"):
            continue
        c = [x.strip() for x in line.strip().strip("|").split("|")]
        if len(c) < 13 or c[1] not in ("ran", "failed", "timeout"):
            continue
        out[c[0]] = (int(c[4]) + int(c[5]), int(c[2]))
    return out


def ci(path):
    d = json.load(open(path))
    return {s["scenario"]: (s["counts"]["identical"] + s["counts"]["normalized-identical"], s["counts"]["total"], s) for s in d["scenarios"]}


head, retry = ci(head_sb), ci(retry_sb)
main, t12 = table(main_md), table(t12_md)
print("CI head scenarios: %d; in train 12's main sweep: %d; in its head sweep: %d"
      % (len(head), sum(1 for n in head if n in main), sum(1 for n in head if n in t12)))
fewer, same, more, absent = [], 0, 0, []
for name, (p, total, s) in sorted(head.items()):
    if name in retry:
        p = max(p, retry[name][0])
    if name not in main:
        absent.append(name)
        continue
    mp, mt = main[name]
    if p < mp:
        first = s.get("first_unresolved") or {}
        fewer.append((name, p, total, mp, mt, t12.get(name), first))
    elif p == mp:
        same += 1
    else:
        more += 1
print("against main: %d scenario(s) pass as many pairs, %d more, %d fewer; %d not in main's sweep: %s"
      % (same, more, len(fewer), len(absent), " ".join(absent) or "-"))
print("pairs main passed that the CI head does not, at least: %d" % sum(f[3] - f[1] for f in fewer))
print()
print("%-34s %-9s %-9s %-9s %s" % ("scenario", "CI head", "main", "t12 head", "the CI head's first unresolved pair"))
for name, p, total, mp, mt, th, first in fewer:
    where = ""
    if first:
        where = "%s row %s: fx %r / xfx %r" % (first.get("label"), first.get("row"), (first.get("fx") or "")[:60], (first.get("xfx") or "")[:60])
    print("%-34s %-9s %-9s %-9s %s" % (name, "%d/%d" % (p, total), "%d/%d" % (mp, mt), "%d/%d" % th if th else "-", where))
