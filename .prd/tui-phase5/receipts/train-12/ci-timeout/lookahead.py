"""What the base sweep PR #90's run never reached could call a regression.

usage: python3 -I lookahead.py <head scoreboard.json> <head-retry-1 scoreboard.json>
                               <train-12/sweep-main.md> <train-12/sweep-head.md> <checkout at 1b15dad9>

Neither side has every pair's verdict here: the CI artifact keeps each
scenario's counts and its first unresolved pair (scoreboard.json, no
pairs.json), and train 12's sweeps keep their scoreboards' scenario tables,
which name the same first pair. So this compares pass counts per scenario,
then the first unresolved pairs: sweep.py walks a scenario's snaps in the
.cap's order (`for label in u.labels()`) and keeps the first unresolved one,
so a CI first-unresolved snap earlier than main's was not unresolved at main
-- it passed there or was approved -- even where the counts are equal. pass = identical + normalized-identical, as ci_sweep.py's PASS. A
pair the head retried passes if any run passed it, so a retried scenario
counts its better run. `main` is train 12's sweep of df46777e, the PR's
base; `t12 head` is train 12's sweep of its code head 03fb5436.
"""

import json
import sys

head_sb, retry_sb, main_md, t12_md, repo = sys.argv[1:6]
import os  # noqa: E402
sys.path.insert(0, os.path.join(repo, "scripts", "fxcap"))
import capfile  # noqa: E402

ORDER = {u.name: list(u.labels()) for u in capfile.units(os.path.join(repo, "scripts", "fxcap", "scenarios"))}


FIRST = {}


def table(path):
    """The scenario table of a sweep's scoreboard: {scenario: (pass, total)};
    each scenario's first unresolved snap into FIRST[path]."""
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
        first = c[13].split(" ", 1)[0] if len(c) > 13 and c[13] else None
        FIRST.setdefault(path, {})[c[0]] = first
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

# The first unresolved snaps.
mf = FIRST[main_md]
earlier, checked = [], 0
for name, (_, _, s) in sorted(head.items()):
    ci_first = (s.get("first_unresolved") or {}).get("label")
    if name not in main or ci_first is None:
        continue
    checked += 1
    order = ORDER.get(name, [])
    m = mf.get(name)
    if ci_first in order and (m is None or (m in order and order.index(ci_first) < order.index(m))):
        earlier.append((name, ci_first, m))
print()
print("first unresolved snaps: %d CI scenarios have one and are in main's sweep" % checked)
print("CI's first unresolved snap comes before main's (so main passed or approved it): %d" % len(earlier))
for name, c, m in earlier:
    print("  %-34s CI %-22s main %s" % (name, c, m or "(none unresolved)"))
